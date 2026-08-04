// ONNX intra-op thread budgets for ASR/ROVER decode paths, sized by how many
// decode contexts run concurrently on the same machine at once. See
// docs/superpowers/specs/2026-08-04-asr-pipeline-performance-design.md for the
// reasoning behind each number.

/// How many ONNX decode contexts run truly concurrently for a given call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeConcurrency {
    /// Live recording, single ASR model (no ROVER).
    SingleLive,
    /// Live recording, ROVER — 2 decoders run in parallel via `std::thread::scope`.
    RoverLive,
    /// File batch mode, single ASR model — 1 of 2 parallel file-workers.
    SingleFileWorker,
    /// File batch mode with ROVER — 1 of 2 parallel file-workers, each itself
    /// running ROVER's 2 decoders (4 decode contexts total across both workers).
    RoverFileWorker,
}

/// Computes the ONNX intra-op thread count for one decode context, given the
/// machine's physical core count and how many such contexts run at once.
/// Always returns at least 1.
pub fn asr_thread_budget(physical_cores: usize, concurrency: DecodeConcurrency) -> usize {
    use DecodeConcurrency::*;
    match concurrency {
        SingleLive => physical_cores.clamp(2, 4),
        RoverLive => (physical_cores.clamp(2, 4) / 2).max(1),
        SingleFileWorker => (physical_cores / 2).max(1),
        RoverFileWorker => (physical_cores / 4).max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_live_clamps_between_2_and_4() {
        assert_eq!(asr_thread_budget(1, DecodeConcurrency::SingleLive), 2);
        assert_eq!(asr_thread_budget(2, DecodeConcurrency::SingleLive), 2);
        assert_eq!(asr_thread_budget(4, DecodeConcurrency::SingleLive), 4);
        assert_eq!(asr_thread_budget(8, DecodeConcurrency::SingleLive), 4);
        assert_eq!(asr_thread_budget(16, DecodeConcurrency::SingleLive), 4);
    }

    #[test]
    fn rover_live_is_at_most_single_live_and_never_zero() {
        for cores in [1usize, 2, 4, 8, 16] {
            let single = asr_thread_budget(cores, DecodeConcurrency::SingleLive);
            let rover = asr_thread_budget(cores, DecodeConcurrency::RoverLive);
            assert!(rover >= 1);
            assert!(rover <= single);
        }
    }

    #[test]
    fn single_file_worker_is_half_physical_cores_minimum_1() {
        assert_eq!(asr_thread_budget(1, DecodeConcurrency::SingleFileWorker), 1);
        assert_eq!(asr_thread_budget(2, DecodeConcurrency::SingleFileWorker), 1);
        assert_eq!(asr_thread_budget(4, DecodeConcurrency::SingleFileWorker), 2);
        assert_eq!(asr_thread_budget(8, DecodeConcurrency::SingleFileWorker), 4);
        assert_eq!(asr_thread_budget(16, DecodeConcurrency::SingleFileWorker), 8);
    }

    #[test]
    fn rover_file_worker_is_quarter_physical_cores_minimum_1() {
        assert_eq!(asr_thread_budget(1, DecodeConcurrency::RoverFileWorker), 1);
        assert_eq!(asr_thread_budget(4, DecodeConcurrency::RoverFileWorker), 1);
        assert_eq!(asr_thread_budget(8, DecodeConcurrency::RoverFileWorker), 2);
        assert_eq!(asr_thread_budget(16, DecodeConcurrency::RoverFileWorker), 4);
    }

    #[test]
    fn rover_file_worker_never_exceeds_single_file_worker() {
        for cores in [1usize, 2, 4, 8, 16, 32] {
            let single = asr_thread_budget(cores, DecodeConcurrency::SingleFileWorker);
            let rover = asr_thread_budget(cores, DecodeConcurrency::RoverFileWorker);
            assert!(rover <= single);
        }
    }
}
