// src/audio/batch_transcribe.rs
//
// Shared batch-transcription pipeline for the file-based paths (`import.rs`,
// `retranscription.rs`): parallelizes ASR across 2 workers when there's enough work
// and enough CPU, then runs CAPU once per ~200-word batch via `CapuBatcher` instead of
// once per tiny VAD segment. See
// docs/superpowers/specs/2026-08-04-asr-pipeline-performance-design.md, section B.

use crate::api::TranscriptSegment;
use crate::asr_engine::engine::AsrEngine;
use crate::capu_engine::batch::{CapuBatcher, PendingSegment};
use crate::rover_engine::engine::RoverDecoder;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

/// The already-loaded ASR engine to transcribe with — resolved and validated by the
/// caller (`import.rs`/`retranscription.rs`) exactly as before this change.
pub enum PrimaryEngine {
    Single(Arc<AsrEngine>),
    Rover(Arc<TokioMutex<RoverDecoder>>),
}

/// True when there's enough work (>= 4 segments) and enough CPU (>= 4 physical cores)
/// to make 2-worker parallel ASR worthwhile. Below this, a second worker's model-load
/// cost isn't worth paying — falls back to the existing single-worker sequential path.
fn should_parallelize(segment_count: usize, physical_cores: usize) -> bool {
    segment_count >= 4 && physical_cores >= 4
}

/// Splits `items` into two index-tagged groups, alternating even/odd by original
/// position, so each group can be processed independently and merged back in order.
fn split_even_odd<T>(items: Vec<T>) -> (Vec<(usize, T)>, Vec<(usize, T)>) {
    let mut even = Vec::new();
    let mut odd = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        if i % 2 == 0 {
            even.push((i, item));
        } else {
            odd.push((i, item));
        }
    }
    (even, odd)
}

/// Merges two index-tagged result groups back into original order.
fn merge_indexed<T>(mut a: Vec<(usize, T)>, mut b: Vec<(usize, T)>) -> Vec<T> {
    a.append(&mut b);
    a.sort_by_key(|(i, _)| *i);
    a.into_iter().map(|(_, v)| v).collect()
}

/// Runs CAPU over the full list of raw ASR results, batching consecutive segments up to
/// `CAPU_BATCH_WORD_BUDGET` words per call (no debounce timer needed — unlike the live
/// path, this list is already complete). `raw_results` is `(text, start_ms, end_ms)`
/// tuples in original chronological order.
fn finalize_with_capu(raw_results: Vec<(String, f64, f64)>) -> Vec<TranscriptSegment> {
    let mut batcher = CapuBatcher::new();
    let mut finalized_segments = Vec::new();

    for (i, (text, start_ms, end_ms)) in raw_results.into_iter().enumerate() {
        let itn_text = crate::audio::post_asr::apply_itn(&text);
        batcher.push(PendingSegment {
            source_id: i as u64,
            raw_text: itn_text,
            audio_start_time: start_ms / 1000.0,
            audio_end_time: end_ms / 1000.0,
        });

        if batcher.should_flush(crate::config::CAPU_BATCH_WORD_BUDGET) {
            flush_into(&mut batcher, &mut finalized_segments);
        }
    }
    if !batcher.is_empty() {
        flush_into(&mut batcher, &mut finalized_segments);
    }

    finalized_segments
}

/// Flushes whatever `batcher` has pending into `out` as one `TranscriptSegment`, if
/// anything was pending. Uses `flush_with_fallback` so a batch is never silently lost
/// even if the CAPU model isn't loaded (e.g. not yet downloaded) — falls back to the
/// raw (ITN-only) text in that case.
fn flush_into(batcher: &mut CapuBatcher, out: &mut Vec<TranscriptSegment>) {
    let engine_arc = crate::capu_engine::commands::get_engine_arc();
    let finalized = match &engine_arc {
        Some(arc) => {
            let mut engine = arc.lock().unwrap();
            batcher.flush_with_fallback(Some(&mut engine))
        }
        None => batcher.flush_with_fallback(None),
    };

    if let Some(finalized) = finalized {
        out.push(TranscriptSegment {
            id: format!("transcript-{}", uuid::Uuid::new_v4()),
            text: finalized.text,
            timestamp: chrono::Utc::now().to_rfc3339(),
            audio_start_time: Some(finalized.audio_start_time),
            audio_end_time: Some(finalized.audio_end_time),
            duration: Some(finalized.audio_end_time - finalized.audio_start_time),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parallelize_requires_both_enough_segments_and_enough_cores() {
        assert!(!should_parallelize(3, 8), "too few segments");
        assert!(!should_parallelize(10, 2), "too few cores");
        assert!(should_parallelize(4, 4), "boundary: exactly enough of both");
        assert!(should_parallelize(100, 16));
    }

    #[test]
    fn split_even_odd_preserves_original_indices() {
        let items = vec!["a", "b", "c", "d", "e"];
        let (even, odd) = split_even_odd(items);
        assert_eq!(even, vec![(0, "a"), (2, "c"), (4, "e")]);
        assert_eq!(odd, vec![(1, "b"), (3, "d")]);
    }

    #[test]
    fn split_even_odd_handles_empty_input() {
        let items: Vec<&str> = vec![];
        let (even, odd) = split_even_odd(items);
        assert!(even.is_empty());
        assert!(odd.is_empty());
    }

    #[test]
    fn merge_indexed_restores_original_order_regardless_of_group_completion_order() {
        // Simulates worker B finishing first and being merged before worker A's results.
        let a = vec![(0, "a"), (2, "c"), (4, "e")];
        let b = vec![(1, "b"), (3, "d")];
        assert_eq!(merge_indexed(b, a), vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn merge_indexed_handles_one_side_empty() {
        let a: Vec<(usize, &str)> = vec![(0, "only")];
        let b: Vec<(usize, &str)> = vec![];
        assert_eq!(merge_indexed(a, b), vec!["only"]);
    }

    #[test]
    fn finalize_with_capu_falls_back_to_raw_text_without_a_loaded_capu_engine() {
        // No CAPU engine is loaded in this test process, so this exercises the
        // flush_with_fallback(None) path end-to-end through finalize_with_capu.
        let raw = vec![
            ("XIN CHAO".to_string(), 0.0, 1000.0),
            ("CAC BAN".to_string(), 1000.0, 2000.0),
        ];
        let segments = finalize_with_capu(raw);
        assert_eq!(segments.len(), 1, "small input stays under the word budget, one batch");
        assert_eq!(segments[0].text, "xin chao cac ban");
        assert_eq!(segments[0].audio_start_time, Some(0.0));
        assert_eq!(segments[0].audio_end_time, Some(2.0));
    }

    #[test]
    fn finalize_with_capu_returns_empty_for_empty_input() {
        assert!(finalize_with_capu(Vec::new()).is_empty());
    }

    #[test]
    fn finalize_with_capu_splits_into_multiple_batches_when_word_budget_exceeded() {
        // 50 segments x 7 words = 350 words, well over CAPU_BATCH_WORD_BUDGET (200) — must
        // produce more than one TranscriptSegment.
        let mut raw = Vec::new();
        for i in 0..50 {
            raw.push((
                format!("word1 word2 word3 word4 word5 word6 word{}", i),
                (i as f64) * 2000.0,
                (i as f64) * 2000.0 + 1000.0,
            ));
        }
        let segments = finalize_with_capu(raw);
        assert!(
            segments.len() > 1,
            "350 words over a 200-word budget must split into multiple batches, got {}",
            segments.len()
        );
        // Segments must be in chronological order with non-overlapping, increasing time spans.
        for pair in segments.windows(2) {
            assert!(
                pair[0].audio_end_time.unwrap() <= pair[1].audio_start_time.unwrap(),
                "batches must be in non-overlapping chronological order"
            );
        }
        // First batch starts at the first segment's start time, last batch ends at the
        // last segment's end time — no audio lost at the boundaries.
        assert_eq!(segments.first().unwrap().audio_start_time, Some(0.0));
        assert_eq!(segments.last().unwrap().audio_end_time, Some(49.0 * 2000.0 / 1000.0 + 1.0));
    }
}
