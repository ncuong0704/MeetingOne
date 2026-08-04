// src/audio/batch_transcribe.rs
//
// Shared batch-transcription pipeline for the file-based paths (`import.rs`,
// `retranscription.rs`): parallelizes ASR across 2 workers when there's enough work
// and enough CPU, then runs CAPU once per ~200-word batch via `CapuBatcher` instead of
// once per tiny VAD segment. See
// docs/superpowers/specs/2026-08-04-asr-pipeline-performance-design.md, section B.

use crate::asr_engine::engine::AsrEngine;
use crate::rover_engine::engine::RoverDecoder;
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
}
