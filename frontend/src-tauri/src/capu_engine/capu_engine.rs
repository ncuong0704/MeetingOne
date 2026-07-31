use super::edits::apply_actions;
use super::tokenizer::CapuTokenizer;
use super::vocabulary::{load_action_labels, Action};
use crate::config::{CAPU_MAX_ITERATIONS, CAPU_MAX_SEQ_LEN, CAPU_TRAILING_CONTEXT_WORDS};
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

pub struct CapuEngine {
    session: Session,
    tokenizer: CapuTokenizer,
    labels: Vec<Action>,
}

impl CapuEngine {
    pub fn load(model_path: &Path, vocab_path: &Path, labels_path: &Path) -> Result<Self> {
        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| anyhow!("Non-UTF8 model path: {:?}", model_path))?;
        let session = Session::builder()
            .map_err(|e| anyhow!("Failed to create ONNX session builder: {}", e))?
            .commit_from_file(model_path_str)
            .map_err(|e| anyhow!("Failed to load CAPU model {:?}: {}", model_path, e))?;

        let tokenizer = CapuTokenizer::from_vocab_file(vocab_path)?;
        let labels = load_action_labels(labels_path)?;

        Ok(Self {
            session,
            tokenizer,
            labels,
        })
    }

    /// Runs one forward pass over `words` and returns one `Action` per word (already
    /// skipping the CLS/SEP sentinel offsets — see the offset-handling note at the top
    /// of this plan).
    fn infer_once(&mut self, words: &[String]) -> Result<Vec<Action>> {
        let encoding = self.tokenizer.encode_words(words)?;
        if encoding.input_ids.len() > CAPU_MAX_SEQ_LEN {
            return Err(anyhow!(
                "Tokenized sequence too long ({} > {}); caller should truncate words",
                encoding.input_ids.len(),
                CAPU_MAX_SEQ_LEN
            ));
        }

        let seq_len = encoding.input_ids.len();
        let num_offsets = encoding.input_offsets.len();

        let ids_tensor = TensorRef::from_array_view(([1usize, seq_len], &*encoding.input_ids))
            .map_err(|e| anyhow!("Failed to build input_ids tensor: {}", e))?;
        let mask_tensor =
            TensorRef::from_array_view(([1usize, seq_len], &*encoding.attention_mask))
                .map_err(|e| anyhow!("Failed to build attention_mask tensor: {}", e))?;
        let type_tensor =
            TensorRef::from_array_view(([1usize, seq_len], &*encoding.token_type_ids))
                .map_err(|e| anyhow!("Failed to build token_type_ids tensor: {}", e))?;
        let offsets_tensor =
            TensorRef::from_array_view(([1usize, num_offsets], &*encoding.input_offsets))
                .map_err(|e| anyhow!("Failed to build input_offsets tensor: {}", e))?;

        let outputs = self
            .session
            .run(ort::inputs![ids_tensor, mask_tensor, type_tensor, offsets_tensor])
            .map_err(|e| anyhow!("ONNX inference failed: {}", e))?;

        let (logits_shape, logits_data) = outputs["logits"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("Failed to read logits output: {}", e))?;

        let num_classes = self.labels.len();
        if logits_shape.len() != 3
            || logits_shape[1] as usize != num_offsets
            || logits_shape[2] as usize != num_classes
        {
            return Err(anyhow!(
                "Unexpected logits shape {:?} (expected [1, {}, {}])",
                logits_shape,
                num_offsets,
                num_classes
            ));
        }

        // Real words are offsets[1..num_offsets-1] — skip CLS (index 0) and SEP (last).
        let mut actions = Vec::with_capacity(words.len());
        for row in 1..(num_offsets - 1) {
            let row_start = row * num_classes;
            let row_logits = &logits_data[row_start..row_start + num_classes];
            let (best_idx, _) = row_logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .ok_or_else(|| anyhow!("Empty logits row"))?;
            actions.push(self.labels[best_idx]);
        }

        if actions.len() != words.len() {
            return Err(anyhow!(
                "Decoded {} actions for {} words — offset/word mismatch",
                actions.len(),
                words.len()
            ));
        }

        Ok(actions)
    }

    /// Runs the GECToR iterative correction loop (max `CAPU_MAX_ITERATIONS` passes,
    /// stopping early once a pass predicts `$KEEP` for every word).
    ///
    /// `boundary_index`, when set, is the index (into `words`, re-tracked every pass —
    /// see `boundary_index_after_apply`) of the last trailing-context word. That word
    /// was already emitted to the caller on a previous `restore_punctuation` call, so
    /// it must never merge forward into the new segment — the predicted action there is
    /// forced to `$KEEP` before every pass. A merge entirely *within* the context
    /// region (at an earlier index) is unaffected and still applied normally.
    fn restore_words(
        &mut self,
        mut words: Vec<String>,
        mut boundary_index: Option<usize>,
    ) -> Result<Vec<String>> {
        for _ in 0..CAPU_MAX_ITERATIONS {
            if words.is_empty() {
                break;
            }
            let mut actions = self.infer_once(&words)?;

            if let Some(idx) = boundary_index {
                if idx < actions.len() {
                    actions[idx] = Action::Keep;
                }
            }

            if actions.iter().all(|a| *a == Action::Keep) {
                break;
            }

            if let Some(idx) = boundary_index {
                if idx < actions.len() {
                    boundary_index = Some(boundary_index_after_apply(&actions, idx));
                }
            }

            words = apply_actions(&words, &actions);
        }
        Ok(words)
    }

    /// Restores punctuation/capitalization for `new_text`, using `trailing_context`
    /// (raw words from the tail of the previously processed segment) as left-context so
    /// the model has a chance to see across VAD segment boundaries. Returns the
    /// restored text for `new_text` only (context words are stripped back out) plus the
    /// trailing-context words to pass on the next call.
    ///
    /// # Known bug (unfixed): new-segment-internal `$MERGE_SPACE` duplicates a context word
    ///
    /// The boundary-masking logic (see `restore_words`'s `boundary_index` param) only
    /// prevents a `$MERGE_SPACE` from crossing *from* the context region *into* the new
    /// segment. It does nothing to protect the `take_from` calculation below from a
    /// `$MERGE_SPACE` that fires entirely *within* the new segment, unrelated to the
    /// boundary. `take_from = restored.len().saturating_sub(new_words.len())` implicitly
    /// assumes any shrinkage in total word count came from the context side; when it
    /// instead comes from a merge inside the new segment, `take_from` under-shoots and
    /// the returned text incorrectly re-includes one or more tail words from
    /// `trailing_context` — which were already emitted to the caller on the *previous*
    /// call, so this reads as a duplicated word in the transcript.
    ///
    /// Concrete repro: `trailing_context = ["xin", "chào"]`, `new_text = "hôm nay đẹp"`.
    /// If the model predicts `$MERGE_SPACE` on "hôm" (merging it with "nay", entirely
    /// inside the new segment — the boundary word "chào" is untouched), the returned
    /// `result_text` incorrectly includes "chào" as its first word.
    ///
    /// This is algebraically one-directional: it can only cause a context word to be
    /// **re-included** (duplication), never cause a new-segment word to be **dropped**.
    ///
    /// Not yet fixed, and not yet reachable in production — as of this writing nothing
    /// calls `restore_punctuation` (Task 9/10 will be the first callers). Fix this
    /// before or as part of wiring up those callers; do not ship it unfixed.
    pub fn restore_punctuation(
        &mut self,
        trailing_context: &[String],
        new_text: &str,
    ) -> Result<(String, Vec<String>)> {
        let new_words: Vec<String> = new_text.split_whitespace().map(str::to_string).collect();
        if new_words.is_empty() {
            return Ok((String::new(), trailing_context.to_vec()));
        }

        let combined: Vec<String> = trailing_context
            .iter()
            .cloned()
            .chain(new_words.iter().cloned())
            .collect();

        // Index of the last trailing-context word in `combined` — see `restore_words`'s
        // `boundary_index` doc comment for why it must never merge forward.
        let boundary_index = if trailing_context.is_empty() {
            None
        } else {
            Some(trailing_context.len() - 1)
        };

        let restored = self.restore_words(combined, boundary_index)?;

        // MERGE_SPACE can reduce word count, so recover the new-segment tail by count
        // from the end rather than assuming a fixed offset from the start.
        //
        // TODO(capu): known bug — this assumes any word-count shrinkage came from the
        // context region. A $MERGE_SPACE firing entirely within the new segment (not
        // touching the boundary) also shrinks `restored.len()`, which makes `take_from`
        // under-shoot and re-includes a tail word from `trailing_context` in
        // `result_text` (duplication, never loss — see doc comment above on
        // `restore_punctuation` for the full explanation and a concrete repro). Not yet
        // fixed; fix before/while wiring up the first caller (Task 9/10).
        let take_from = restored.len().saturating_sub(new_words.len());
        let result_text = restored[take_from..].join(" ");

        let context_start = new_words.len().saturating_sub(CAPU_TRAILING_CONTEXT_WORDS);
        let next_context = new_words[context_start..].to_vec();

        Ok((result_text, next_context))
    }
}

/// After one `apply_actions` pass, the boundary word (the last trailing-context word)
/// may have shifted position if a `$MERGE_SPACE` consumed one or more words *before* it
/// in the context region (that's allowed — only a merge starting *at* the boundary
/// itself is masked to `$KEEP` by the caller). Given the `actions` slice that was just
/// applied and the boundary's index into the pre-apply word list, this returns its
/// index into the post-apply word list, by walking `actions` with the exact same
/// pairing logic `apply_actions` uses (step by 2 on a consumed `$MERGE_SPACE`, else by
/// 1) until the word that contains `old_boundary_index` is found.
fn boundary_index_after_apply(actions: &[Action], old_boundary_index: usize) -> usize {
    let mut i = 0;
    let mut out_index = 0;
    loop {
        let consumes_pair = actions[i] == Action::MergeSpace && i + 1 < actions.len();
        let consumed_end = if consumes_pair { i + 1 } else { i };
        if old_boundary_index <= consumed_end {
            return out_index;
        }
        i = consumed_end + 1;
        out_index += 1;
    }
}
