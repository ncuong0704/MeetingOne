// frontend/src-tauri/src/capu_engine/batch.rs
//
// Groups small raw ASR segments into larger batches before running CAPU, so punctuation
// restoration pays its fixed per-call overhead far fewer times per meeting/file. Used by
// both the live background stage (transcription/worker.rs, with a debounce timer) and the
// file batch path (added in a later plan; no timer there — the segment list is already
// complete). See docs/superpowers/specs/2026-08-04-asr-pipeline-performance-design.md.

use super::CapuEngine;

/// One raw ASR segment (post-ITN, pre-CAPU) waiting to be batched.
#[derive(Debug, Clone)]
pub struct PendingSegment {
    pub source_id: u64,
    pub raw_text: String,
    pub audio_start_time: f64,
    pub audio_end_time: f64,
}

/// The result of running CAPU over one accumulated batch of `PendingSegment`s.
#[derive(Debug, Clone)]
pub struct FinalizedSegment {
    pub text: String,
    pub audio_start_time: f64,
    pub audio_end_time: f64,
    /// `source_id` of every `PendingSegment` this batch replaces, in original order.
    pub source_ids: Vec<u64>,
}

pub struct CapuBatcher {
    pending: Vec<PendingSegment>,
    pending_word_count: usize,
    trailing_context: Vec<String>,
}

impl CapuBatcher {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            pending_word_count: 0,
            trailing_context: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// True once the accumulated pending word count has reached `word_budget`. Callers
    /// (live: also a debounce timer; file: only this) decide when to act on it.
    pub fn should_flush(&self, word_budget: usize) -> bool {
        self.pending_word_count >= word_budget
    }

    /// Buffers one raw segment. Does not run CAPU — call `should_flush`/`flush` separately.
    pub fn push(&mut self, seg: PendingSegment) {
        self.pending_word_count += seg.raw_text.split_whitespace().count();
        self.pending.push(seg);
    }

    /// The exact string CAPU would receive if flushed right now — exposed separately so
    /// the join logic is unit-testable without a loaded `CapuEngine`.
    pub fn joined_pending_text(&self) -> String {
        self.pending
            .iter()
            .map(|s| s.raw_text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Discards whatever is pending without running CAPU (used when no CAPU engine is
    /// available at all — e.g. model not downloaded). The raw text was already emitted
    /// live by Stage 1; only the punctuated-merge for this batch is lost.
    pub fn discard_pending(&mut self) {
        self.pending.clear();
        self.pending_word_count = 0;
    }

    /// Runs CAPU over everything pending and clears it. `None` if nothing is pending. On
    /// CAPU failure, falls back to the joined raw text (same fallback
    /// `post_asr::process_asr_text` already uses) rather than losing the batch.
    pub fn flush(&mut self, engine: &mut CapuEngine) -> Option<FinalizedSegment> {
        if self.pending.is_empty() {
            return None;
        }

        let joined = self.joined_pending_text();
        let source_ids: Vec<u64> = self.pending.iter().map(|s| s.source_id).collect();
        let audio_start_time = self.pending.first().unwrap().audio_start_time;
        let audio_end_time = self.pending.last().unwrap().audio_end_time;

        // At the minimum punctuation level, skip CAPU entirely rather than running
        // inference with an extreme bias — matches `post_asr::process_asr_text`'s
        // identical bypass, so live and file/batch transcription behave consistently.
        let text = if engine.punctuation_level() <= 1 {
            joined.clone()
        } else {
            match engine.restore_punctuation(&self.trailing_context, &joined) {
                Ok((restored, next_context)) => {
                    self.trailing_context = next_context;
                    restored
                }
                Err(e) => {
                    log::warn!("CapuBatcher: CAPU failed on batch, falling back to raw text: {}", e);
                    joined
                }
            }
        };

        self.pending.clear();
        self.pending_word_count = 0;

        Some(FinalizedSegment {
            text,
            audio_start_time,
            audio_end_time,
            source_ids,
        })
    }
}

impl Default for CapuBatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(id: u64, text: &str, start: f64, end: f64) -> PendingSegment {
        PendingSegment {
            source_id: id,
            raw_text: text.to_string(),
            audio_start_time: start,
            audio_end_time: end,
        }
    }

    #[test]
    fn new_batcher_is_empty_and_does_not_flush() {
        let batcher = CapuBatcher::new();
        assert!(batcher.is_empty());
        assert!(!batcher.should_flush(1));
    }

    #[test]
    fn should_flush_triggers_once_word_budget_reached() {
        let mut batcher = CapuBatcher::new();
        batcher.push(pending(0, "one two three", 0.0, 1.0));
        assert!(!batcher.should_flush(5));
        batcher.push(pending(1, "four five", 1.0, 2.0));
        assert!(batcher.should_flush(5));
    }

    #[test]
    fn joined_pending_text_joins_with_single_spaces_in_order() {
        let mut batcher = CapuBatcher::new();
        batcher.push(pending(0, "xin chao", 0.0, 1.0));
        batcher.push(pending(1, "cac ban", 1.0, 2.0));
        assert_eq!(batcher.joined_pending_text(), "xin chao cac ban");
    }

    #[test]
    fn push_updates_word_count_by_whitespace_split() {
        let mut batcher = CapuBatcher::new();
        batcher.push(pending(0, "  xin   chao  ", 0.0, 1.0));
        assert!(batcher.should_flush(2));
        assert!(!batcher.should_flush(3));
    }

    #[test]
    fn discard_pending_clears_state() {
        let mut batcher = CapuBatcher::new();
        batcher.push(pending(0, "xin chao", 0.0, 1.0));
        batcher.discard_pending();
        assert!(batcher.is_empty());
        assert!(!batcher.should_flush(1));
    }

    /// Requires a downloaded CAPU model on disk — same gate as
    /// `capu_engine::capu_engine::integration_tests::restore_punctuation_on_real_model`.
    /// Run with: cargo test --manifest-path frontend/src-tauri/Cargo.toml -- --ignored capu_engine::batch
    #[test]
    #[ignore = "requires downloaded CAPU model on disk"]
    fn flush_runs_capu_and_returns_finalized_segment_spanning_the_batch() {
        let dir = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap())
            .join("AppData/Roaming/com.meetingone.app/models/capu-vi");
        let mut engine = CapuEngine::load(
            &dir.join("vibert-capu.int8.onnx"),
            &dir.join("vocab.txt"),
            &dir.join("vocabulary/labels.txt"),
            4,
            7,
            3,
        )
        .expect("load model");

        let mut batcher = CapuBatcher::new();
        batcher.push(pending(10, "xin chao cac ban", 0.0, 2.0));
        batcher.push(pending(11, "hom nay chung ta hop", 2.0, 4.5));

        let finalized = batcher.flush(&mut engine).expect("batch was non-empty");
        assert_eq!(finalized.source_ids, vec![10, 11]);
        assert_eq!(finalized.audio_start_time, 0.0);
        assert_eq!(finalized.audio_end_time, 4.5);
        assert!(batcher.is_empty());
    }
}
