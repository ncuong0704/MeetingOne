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

fn load_capu_session(model_path_str: &str, threads: usize) -> Result<Session> {
    Session::builder()
        .map_err(|e| anyhow!("Failed to create ONNX session builder: {}", e))?
        .with_intra_threads(threads.max(1))
        .map_err(|e| anyhow!("Failed to set CAPU intra-op threads: {}", e))?
        .commit_from_file(model_path_str)
        .map_err(|e| anyhow!("Failed to load CAPU model: {}", e))
}

/// Runs the CAPU model once and returns the raw `logits` output (shape, flattened data) as
/// owned data.
fn run_capu_inference(
    session: &mut Session,
    input_ids: &[i64],
    attention_mask: &[i64],
    token_type_ids: &[i64],
    input_offsets: &[i64],
    seq_len: usize,
    num_offsets: usize,
) -> Result<(Vec<i64>, Vec<f32>)> {
    let ids_tensor = TensorRef::from_array_view(([1usize, seq_len], input_ids))
        .map_err(|e| anyhow!("Failed to build input_ids tensor: {}", e))?;
    let mask_tensor = TensorRef::from_array_view(([1usize, seq_len], attention_mask))
        .map_err(|e| anyhow!("Failed to build attention_mask tensor: {}", e))?;
    let type_tensor = TensorRef::from_array_view(([1usize, seq_len], token_type_ids))
        .map_err(|e| anyhow!("Failed to build token_type_ids tensor: {}", e))?;
    let offsets_tensor = TensorRef::from_array_view(([1usize, num_offsets], input_offsets))
        .map_err(|e| anyhow!("Failed to build input_offsets tensor: {}", e))?;

    let outputs = session
        .run(ort::inputs![ids_tensor, mask_tensor, type_tensor, offsets_tensor])
        .map_err(|e| anyhow!("ONNX inference failed: {}", e))?;

    let (logits_shape, logits_data) = outputs["logits"]
        .try_extract_tensor::<f32>()
        .map_err(|e| anyhow!("Failed to read logits output: {}", e))?;

    Ok((logits_shape.to_vec(), logits_data.to_vec()))
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.into_iter().map(|x| x / sum).collect()
}

/// Picks the winning label index for one word's logits row, with optional pause-hint nudge
/// (ported from test ASR `gec_model._convert`).
fn decode_row(
    row_logits: &[f32],
    keep_index: usize,
    comma_index: usize,
    period_index: usize,
    case_label_indices: &[usize],
    punctuation_level: u8,
    case_level: u8,
    pause_gap: Option<f32>,
) -> usize {
    let mut probs = softmax(row_logits);
    probs[keep_index] += punctuation_confidence(punctuation_level);
    for &idx in case_label_indices {
        probs[idx] += case_confidence(case_level);
    }

    if let Some(gap) = pause_gap {
        let best_before = probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(idx, _)| idx)
            .unwrap_or(keep_index);
        let is_keep = best_before == keep_index;
        if gap >= 1.0 {
            if is_keep {
                probs[keep_index] -= 0.2;
                probs[period_index] += 0.2;
            }
        } else if gap >= 0.2 {
            if is_keep {
                probs[comma_index] += 0.2;
            }
        } else if gap < 0.1 {
            probs[comma_index] -= 0.3;
        }
    }

    probs
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(idx, _)| idx)
        .unwrap_or(keep_index)
}

pub struct CapuEngine {
    session: Session,
    tokenizer: CapuTokenizer,
    labels: Vec<Action>,
    /// Index of `Action::Keep` in `labels` — found once at `load()` time.
    keep_index: usize,
    comma_index: usize,
    period_index: usize,
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
        let session = load_capu_session(model_path_str, threads)?;

        let tokenizer = CapuTokenizer::from_vocab_file(vocab_path)?;
        let labels = load_action_labels(labels_path)?;
        let keep_index = labels
            .iter()
            .position(|a| *a == Action::Keep)
            .ok_or_else(|| anyhow!("Label file has no $KEEP action"))?;
        let comma_index = labels
            .iter()
            .position(|a| *a == Action::AppendComma)
            .ok_or_else(|| anyhow!("Label file has no $APPEND_, action"))?;
        let period_index = labels
            .iter()
            .position(|a| *a == Action::AppendPeriod)
            .ok_or_else(|| anyhow!("Label file has no $APPEND_. action"))?;
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
            comma_index,
            period_index,
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
    fn infer_once(
        &mut self,
        words: &[String],
        pause_hints: Option<&[f32]>,
    ) -> Result<Vec<Action>> {
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

        let (logits_shape, logits_data) = run_capu_inference(
            &mut self.session,
            &encoding.input_ids,
            &encoding.attention_mask,
            &encoding.token_type_ids,
            &encoding.input_offsets,
            seq_len,
            num_offsets,
        )?;

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

        // Real words are offsets[1..num_offsets] — skip $START (index 0). No SEP to skip
        // at the end; the reference model's input has none (see tokenizer.rs).
        let mut actions = Vec::with_capacity(words.len());
        for (word_idx, row) in (1..num_offsets).enumerate() {
            let row_start = row * num_classes;
            let row_logits = &logits_data[row_start..row_start + num_classes];
            let pause_gap = pause_hints.and_then(|hints| hints.get(word_idx).copied());
            let best_idx = decode_row(
                row_logits,
                self.keep_index,
                self.comma_index,
                self.period_index,
                &self.case_label_indices,
                self.punctuation_level,
                self.case_level,
                pause_gap,
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
    /// `boundary_index`, when set, is the index (into `words`, re-tracked every pass by
    /// `apply_pass`) of the last trailing-context word. That word was already emitted to
    /// the caller on a previous `restore_punctuation` call, so it must never merge
    /// forward into the new segment — the predicted action there is forced to `$KEEP`
    /// before every pass. A merge entirely *within* the context region (at an earlier
    /// index) is unaffected and still applied normally.
    ///
    /// Returns the final words alongside the final `boundary_index` — the caller
    /// (`restore_punctuation`) uses it to know exactly where the new segment starts in
    /// the output, however many merges happened on either side of it.
    fn restore_words(
        &mut self,
        mut words: Vec<String>,
        mut boundary_index: Option<usize>,
        pause_hints: Option<&[f32]>,
    ) -> Result<(Vec<String>, Option<usize>)> {
        for _ in 0..CAPU_MAX_ITERATIONS {
            if words.is_empty() {
                break;
            }
            let actions = self.infer_once(&words, pause_hints)?;
            let (new_words, new_boundary_index, done) = apply_pass(words, boundary_index, actions);
            words = new_words;
            boundary_index = new_boundary_index;
            if done {
                break;
            }
        }
        Ok((words, boundary_index))
    }

    /// Restores punctuation/capitalization for `new_text`, using `trailing_context`
    /// (raw words from the tail of the previously processed segment) as left-context so
    /// the model has a chance to see across VAD segment boundaries. Returns the
    /// restored text for `new_text` only (context words are stripped back out) plus the
    /// trailing-context words to pass on the next call. Called repeatedly across a whole
    /// file/session by `CapuBatcher::flush` (`capu_engine/batch.rs`), which threads
    /// `next_context` back in as `trailing_context` on the following call — so this must
    /// hold up across many consecutive calls, not just one.
    pub fn restore_punctuation(
        &mut self,
        trailing_context: &[String],
        new_text: &str,
    ) -> Result<(String, Vec<String>)> {
        self.restore_punctuation_with_hints(trailing_context, new_text, None)
    }

    /// Like `restore_punctuation`, with optional per-word pause gaps (seconds after each
    /// word in `new_text`) to nudge comma/period insertion — mirrors test ASR pause_hints.
    pub fn restore_punctuation_with_hints(
        &mut self,
        trailing_context: &[String],
        new_text: &str,
        pause_hints: Option<&[f32]>,
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

        let combined_hints: Option<Vec<f32>> = pause_hints.map(|hints| {
            let mut combined = vec![0.5; trailing_lower.len()];
            combined.extend_from_slice(hints);
            combined
        });

        let (restored, final_boundary_index) = self.restore_words(
            combined,
            boundary_index,
            combined_hints.as_deref(),
        )?;

        // `final_boundary_index` is the position of the last trailing-context word in
        // `restored`, tracked incrementally through every pass by `apply_pass` — so
        // everything after it is exactly the new-segment output, regardless of whether a
        // `$MERGE_SPACE` fired inside the context region, inside the new region, or both
        // (the boundary mask in `apply_pass` guarantees merges never cross the two, so a
        // word is never ambiguous about which side it belongs to). This intentionally
        // does *not* derive the split point from `restored.len() - new_words.len()`,
        // which breaks whenever a merge shrinks the new region's own word count instead
        // of the context region's.
        let take_from = final_boundary_index.map_or(0, |idx| idx + 1);
        let result_text = super::post_process::post_process(&restored[take_from..].join(" "));

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
/// One GECToR correction pass, factored out of `restore_words` so it can be exercised
/// directly with fixture `actions` — no ONNX session required. Masks the boundary word's
/// own action to `$KEEP` (a merge can never start *at* the boundary and reach into the
/// new segment), checks whether every action is now `$KEEP` (nothing left to change,
/// `words`/`boundary_index` returned unchanged), and otherwise advances `boundary_index`
/// past whatever this pass merges before applying the actions to `words`. See
/// `pass_tests` below — in particular
/// `merge_entirely_inside_new_segment_leaves_boundary_index_unchanged`, which is the
/// exact scenario `restore_punctuation`'s doc comment used to describe as an unfixed bug.
fn apply_pass(
    words: Vec<String>,
    mut boundary_index: Option<usize>,
    mut actions: Vec<Action>,
) -> (Vec<String>, Option<usize>, bool) {
    if let Some(idx) = boundary_index {
        if idx < actions.len() {
            actions[idx] = Action::Keep;
        }
    }

    if actions.iter().all(|a| *a == Action::Keep) {
        return (words, boundary_index, true);
    }

    if let Some(idx) = boundary_index {
        if idx < actions.len() {
            boundary_index = Some(boundary_index_after_apply(&actions, idx));
        }
    }

    let words = apply_actions(&words, &actions);
    (words, boundary_index, false)
}

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
        let idx = decode_row(&logits, 0, 1, 2, &[2], 1, 3, None);
        assert_eq!(idx, 0, "level=1 (+0.5 to KEEP) should flip this close call toward KEEP");
    }

    #[test]
    fn decode_row_max_punctuation_level_pulls_a_close_call_away_from_keep() {
        // Without bias, softmax([0.6, 0.5, 0.0]) narrowly favors KEEP (index 0).
        let logits = vec![0.6, 0.5, 0.0];
        let idx = decode_row(&logits, 0, 1, 2, &[2], 10, 3, None);
        assert_eq!(idx, 1, "level=10 (-0.8 to KEEP) should flip this close call away from KEEP");
    }

    #[test]
    fn decode_row_max_case_level_favors_a_trailing_case_action() {
        // KEEP (index 0) narrowly ahead of a case action (index 2); punctuation_level=4 gives
        // KEEP only a small +0.0667 boost, isolating the case-bias effect.
        let logits = vec![0.6, 0.0, 0.5];
        let idx = decode_row(&logits, 0, 1, 2, &[2], 4, 10, None);
        assert_eq!(idx, 2, "level=10 case bias (+0.5) should flip this close call toward the case action");
    }

    #[test]
    fn decode_row_min_case_level_suppresses_a_trailing_case_action() {
        let logits = vec![0.6, 0.0, 0.5];
        let idx = decode_row(&logits, 0, 1, 2, &[2], 4, 1, None);
        assert_eq!(idx, 0, "level=1 case bias (-1.5) should keep KEEP winning");
    }
}

#[cfg(test)]
mod pass_tests {
    use super::*;

    fn words(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn all_keep_actions_mark_the_pass_done_without_changing_words() {
        let (out_words, out_boundary, done) = apply_pass(
            words(&["xin", "chào"]),
            Some(1),
            vec![Action::Keep, Action::Keep],
        );
        assert!(done);
        assert_eq!(out_words, words(&["xin", "chào"]));
        assert_eq!(out_boundary, Some(1));
    }

    #[test]
    fn boundary_words_own_merge_action_is_masked_to_keep() {
        // The model wants to merge the boundary word ("chào", index 0) forward into the
        // new segment's first word — forbidden, since "chào" was already emitted to the
        // caller on a previous call. Masking it to $KEEP means every action is now
        // $KEEP, so the pass reports done with words unchanged.
        let (out_words, out_boundary, done) = apply_pass(
            words(&["chào", "hôm", "nay"]),
            Some(0),
            vec![Action::MergeSpace, Action::Keep, Action::Keep],
        );
        assert!(done, "masking the boundary's MergeSpace should leave all-$KEEP");
        assert_eq!(out_words, words(&["chào", "hôm", "nay"]));
        assert_eq!(out_boundary, Some(0));
    }

    #[test]
    fn merge_entirely_within_context_shifts_boundary_index_forward() {
        // Context = ["xin", "chào"] (boundary at index 1). A merge at index 0 ("xin" +
        // "chào") is entirely within the context region and allowed — the two collapse
        // into one output word, so the boundary's position shifts from 1 to 0.
        let (out_words, out_boundary, done) = apply_pass(
            words(&["xin", "chào", "hôm"]),
            Some(1),
            vec![Action::MergeSpace, Action::Keep, Action::Keep],
        );
        assert!(!done);
        assert_eq!(out_words, words(&["xinchào", "hôm"]));
        assert_eq!(out_boundary, Some(0));
    }

    #[test]
    fn merge_entirely_inside_new_segment_leaves_boundary_index_unchanged() {
        // This is the exact scenario `restore_punctuation`'s doc comment used to
        // describe as an unfixed bug: trailing_context = ["xin", "chào"] (boundary at
        // index 1), new_text = "hôm nay đẹp". The model merges "hôm" (index 2) with
        // "nay" (index 3) — entirely inside the new segment, not touching the boundary.
        let (out_words, out_boundary, done) = apply_pass(
            words(&["xin", "chào", "hôm", "nay", "đẹp"]),
            Some(1),
            vec![
                Action::Keep,
                Action::Keep,
                Action::MergeSpace,
                Action::Keep,
                Action::Keep,
            ],
        );
        assert!(!done);
        assert_eq!(out_words, words(&["xin", "chào", "hômnay", "đẹp"]));
        // Unchanged: the merge happened after the boundary, not at or before it.
        assert_eq!(out_boundary, Some(1));

        // Reproduce `restore_punctuation`'s take_from calculation directly: with the
        // boundary still at index 1, only "hômnay" and "đẹp" (indices 2..) belong to
        // the new segment — "chào" (the boundary word itself) must not reappear.
        let take_from = out_boundary.map(|idx| idx + 1).unwrap();
        assert_eq!(&out_words[take_from..], &words(&["hômnay", "đẹp"])[..]);

        // The bug this replaces: `restored.len() - new_words.len()` = 4 - 3 = 1 would
        // have taken from index 1 instead, incorrectly re-including "chào".
        let buggy_take_from = out_words.len().saturating_sub(3);
        assert_eq!(buggy_take_from, 1, "sanity-check the old formula would have picked index 1");
    }

    #[test]
    fn no_boundary_index_treats_every_word_as_new() {
        let (out_words, out_boundary, done) = apply_pass(
            words(&["hôm", "nay"]),
            None,
            vec![Action::Keep, Action::Keep],
        );
        assert!(done);
        assert_eq!(out_words, words(&["hôm", "nay"]));
        assert_eq!(out_boundary, None);
    }
}
