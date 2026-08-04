use super::edits::apply_actions;
use super::tokenizer::CapuTokenizer;
use super::vocabulary::{load_action_labels, Action};
use crate::config::{CAPU_MAX_ITERATIONS, CAPU_MAX_SEQ_LEN, CAPU_TRAILING_CONTEXT_WORDS};
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

/// Converts a punctuation-level UI slider value (1..10, default 7) into the probability
/// bias added to the `$KEEP` (no-op) label before argmax. Formula ported verbatim from the
/// reference app's `tab_file.py:get_config()`. Higher level -> more negative bias -> $KEEP
/// suppressed -> the model adds punctuation more aggressively. Level 1 is handled by the
/// caller as a full bypass (see `post_asr.rs`), not by this bias alone.
fn punctuation_confidence(level: u8) -> f32 {
    let level = level.clamp(1, 10) as f32;
    0.5 - (level - 1.0) * (1.3 / 9.0)
}

/// Same idea as `punctuation_confidence`, but for every `$TRANSFORM_CASE_*` label
/// (case-level slider, 1..10, default 3). Higher level -> more positive bias -> case
/// transforms fire more readily.
fn case_confidence(level: u8) -> f32 {
    let level = level.clamp(1, 10) as f32;
    -1.5 + (level - 1.0) * (2.0 / 9.0)
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.into_iter().map(|x| x / sum).collect()
}

/// Picks the winning label index for one word's logits row: softmax -> add
/// `punctuation_confidence(punctuation_level)` to `keep_index`'s probability -> add
/// `case_confidence(case_level)` to every index in `case_label_indices` -> argmax. A pure
/// function (no ONNX involved), so it's unit-testable with fixture logits — see `bias_tests`.
fn decode_row(
    row_logits: &[f32],
    keep_index: usize,
    case_label_indices: &[usize],
    punctuation_level: u8,
    case_level: u8,
) -> usize {
    let mut probs = softmax(row_logits);
    probs[keep_index] += punctuation_confidence(punctuation_level);
    for &idx in case_label_indices {
        probs[idx] += case_confidence(case_level);
    }
    probs
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(idx, _)| idx)
        .unwrap()
}

pub struct CapuEngine {
    session: Session,
    tokenizer: CapuTokenizer,
    labels: Vec<Action>,
    /// Index of `Action::Keep` in `labels` — found once at `load()` time.
    keep_index: usize,
    /// Indices of every `Action::TransformCase*` variant in `labels` — found once at
    /// `load()` time.
    case_label_indices: Vec<usize>,
    /// Number of ONNX intra-op threads the current `session` was built with. Compared
    /// against the caller's requested thread count to decide whether a rebuild is needed
    /// (see `capu_engine::commands::apply_settings_after_save`).
    threads: usize,
    punctuation_level: u8,
    case_level: u8,
}

impl CapuEngine {
    pub fn load(
        model_path: &Path,
        vocab_path: &Path,
        labels_path: &Path,
        threads: usize,
        punctuation_level: u8,
        case_level: u8,
    ) -> Result<Self> {
        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| anyhow!("Non-UTF8 model path: {:?}", model_path))?;
        let session = Session::builder()
            .map_err(|e| anyhow!("Failed to create ONNX session builder: {}", e))?
            .with_intra_threads(threads.max(1))
            .map_err(|e| anyhow!("Failed to set CAPU intra-op threads: {}", e))?
            .commit_from_file(model_path_str)
            .map_err(|e| anyhow!("Failed to load CAPU model {:?}: {}", model_path, e))?;

        let tokenizer = CapuTokenizer::from_vocab_file(vocab_path)?;
        let labels = load_action_labels(labels_path)?;
        let keep_index = labels
            .iter()
            .position(|a| *a == Action::Keep)
            .ok_or_else(|| anyhow!("Label file has no $KEEP action"))?;
        let case_label_indices = labels
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                matches!(
                    a,
                    Action::TransformCaseCapital
                        | Action::TransformCaseUpper
                        | Action::TransformCaseLower
                        | Action::TransformCaseCapital1
                        | Action::TransformCaseUpperMinus1
                )
            })
            .map(|(i, _)| i)
            .collect();

        Ok(Self {
            session,
            tokenizer,
            labels,
            keep_index,
            case_label_indices,
            threads: threads.max(1),
            punctuation_level: punctuation_level.clamp(1, 10),
            case_level: case_level.clamp(1, 10),
        })
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    pub fn punctuation_level(&self) -> u8 {
        self.punctuation_level
    }

    pub fn set_punctuation_level(&mut self, level: u8) {
        self.punctuation_level = level.clamp(1, 10);
    }

    pub fn set_case_level(&mut self, level: u8) {
        self.case_level = level.clamp(1, 10);
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
            let best_idx = decode_row(
                row_logits,
                self.keep_index,
                &self.case_label_indices,
                self.punctuation_level,
                self.case_level,
            );
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
        // ZipFormer ASR outputs all-uppercase raw text; CAPU was trained on normally-cased
        // Vietnamese and predicts mostly $KEEP on all-caps input. Lowercase before inference.
        let new_words: Vec<String> = new_text
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();
        if new_words.is_empty() {
            return Ok((String::new(), trailing_context.to_vec()));
        }

        let trailing_lower: Vec<String> = trailing_context
            .iter()
            .map(|w| w.to_lowercase())
            .collect();

        let combined: Vec<String> = trailing_lower
            .iter()
            .cloned()
            .chain(new_words.iter().cloned())
            .collect();

        // Index of the last trailing-context word in `combined` — see `restore_words`'s
        // `boundary_index` doc comment for why it must never merge forward.
        let boundary_index = if trailing_lower.is_empty() {
            None
        } else {
            Some(trailing_lower.len() - 1)
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

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::path::PathBuf;

    fn model_dir() -> PathBuf {
        PathBuf::from(std::env::var("USERPROFILE").unwrap())
            .join("AppData/Roaming/com.meetingone.app/models/capu-vi")
    }

    #[test]
    #[ignore = "requires downloaded CAPU model on disk"]
    fn restore_punctuation_on_real_model() {
        let dir = model_dir();
        let mut engine = CapuEngine::load(
            &dir.join("vibert-capu.int8.onnx"),
            &dir.join("vocab.txt"),
            &dir.join("vocabulary/labels.txt"),
            4,
            7,
            3,
        )
        .expect("load model");

        let lowercase = "xin chào các bạn hôm nay chúng ta họp về dự án mới";
        let (restored_lower, _) = engine
            .restore_punctuation(&[], lowercase)
            .expect("infer lowercase");
        eprintln!("LOWER RAW:      {}", lowercase);
        eprintln!("LOWER RESTORED: {}", restored_lower);

        let uppercase = "XIN CHÀO CÁC BẠN HÔM NAY CHÚNG TA HỌP VỀ DỰ ÁN MỚI";
        let (restored_upper, _) = engine
            .restore_punctuation(&[], uppercase)
            .expect("infer uppercase");
        eprintln!("UPPER RAW:      {}", uppercase);
        eprintln!("UPPER RESTORED: {}", restored_upper);

        assert_ne!(
            lowercase, restored_lower,
            "expected punctuation change on lowercase input"
        );
        assert_ne!(
            uppercase, restored_upper,
            "expected punctuation change on uppercase ASR-style input"
        );
    }
}

#[cfg(test)]
mod bias_tests {
    use super::*;

    #[test]
    fn punctuation_confidence_matches_reference_formula_at_key_levels() {
        // Ported verbatim from the reference app's tab_file.py get_config():
        // confidence = 0.5 - (slider_val - 1) * (1.3 / 9)
        assert!((punctuation_confidence(1) - 0.5).abs() < 1e-6);
        assert!((punctuation_confidence(7) - (-0.366667)).abs() < 1e-4);
        assert!((punctuation_confidence(10) - (-0.8)).abs() < 1e-6);
    }

    #[test]
    fn case_confidence_matches_reference_formula_at_key_levels() {
        // case_confidence = -1.5 + (case_val - 1) * (2.0 / 9)
        assert!((case_confidence(1) - (-1.5)).abs() < 1e-6);
        assert!((case_confidence(3) - (-1.055556)).abs() < 1e-4);
        assert!((case_confidence(10) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn decode_row_min_punctuation_level_pulls_a_close_call_toward_keep() {
        // Without bias, softmax([0.5, 0.6, 0.0]) narrowly favors index 1 over KEEP (index 0).
        let logits = vec![0.5, 0.6, 0.0];
        let idx = decode_row(&logits, 0, &[2], 1, 3);
        assert_eq!(idx, 0, "level=1 (+0.5 to KEEP) should flip this close call toward KEEP");
    }

    #[test]
    fn decode_row_max_punctuation_level_pulls_a_close_call_away_from_keep() {
        // Without bias, softmax([0.6, 0.5, 0.0]) narrowly favors KEEP (index 0).
        let logits = vec![0.6, 0.5, 0.0];
        let idx = decode_row(&logits, 0, &[2], 10, 3);
        assert_eq!(idx, 1, "level=10 (-0.8 to KEEP) should flip this close call away from KEEP");
    }

    #[test]
    fn decode_row_max_case_level_favors_a_trailing_case_action() {
        // KEEP (index 0) narrowly ahead of a case action (index 2); punctuation_level=4 gives
        // KEEP only a small +0.0667 boost, isolating the case-bias effect.
        let logits = vec![0.6, 0.0, 0.5];
        let idx = decode_row(&logits, 0, &[2], 4, 10);
        assert_eq!(idx, 2, "level=10 case bias (+0.5) should flip this close call toward the case action");
    }

    #[test]
    fn decode_row_min_case_level_suppresses_a_trailing_case_action() {
        let logits = vec![0.6, 0.0, 0.5];
        let idx = decode_row(&logits, 0, &[2], 4, 1);
        assert_eq!(idx, 0, "level=1 case bias (-1.5) should keep KEEP winning");
    }
}
