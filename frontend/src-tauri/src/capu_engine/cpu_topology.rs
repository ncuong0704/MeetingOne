use sysinfo::System;

/// App-fixed CAPU ONNX intra-op threads. Clamped to physical cores at runtime.
pub const FIXED_CAPU_CPU_THREADS: usize = 4;
/// App-fixed punctuation restore level: UI label "Vừa".
pub const FIXED_CAPU_PUNCTUATION_LEVEL: u8 = 5;
/// App-fixed auto-capitalization level: UI label "Vừa".
pub const FIXED_CAPU_CASE_LEVEL: u8 = 5;

/// Returns `(threads, punctuation_level, case_level)` used by the CAPU engine.
/// These settings are no longer user-configurable. Thread count is still clamped
/// to this machine's physical cores so a 2-core laptop does not oversubscribe.
pub fn resolve_fixed_capu_runtime(physical_cores: usize) -> (usize, u8, u8) {
    let cores = physical_cores.max(1);
    (
        FIXED_CAPU_CPU_THREADS.clamp(1, cores),
        FIXED_CAPU_PUNCTUATION_LEVEL,
        FIXED_CAPU_CASE_LEVEL,
    )
}

/// Returns `(physical_cores, logical_threads)`. Falls back to `logical / 2` (min 1) if the
/// OS doesn't report a physical core count — mirrors the reference app's own fallback
/// (`core/config.py:_detect_cpu_topology`). Unlike that reference implementation, no manual
/// VM-detection heuristic is needed here: `sysinfo` queries real OS topology, and a vCPU's
/// physical/logical counts already come back equal at that layer.
pub fn detect_cpu_topology() -> (usize, usize) {
    let mut sys = System::new();
    sys.refresh_cpu_all();
    let logical = sys.cpus().len().max(1);
    let physical = sys.physical_core_count().unwrap_or((logical / 2).max(1));
    (physical, logical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_topology_is_internally_consistent() {
        let (physical, logical) = detect_cpu_topology();
        assert!(physical >= 1, "physical core count should be at least 1");
        assert!(logical >= 1, "logical thread count should be at least 1");
        assert!(
            physical <= logical,
            "physical cores can't exceed logical threads"
        );
    }

    #[test]
    fn fixed_capu_settings_are_four_threads_and_medium_levels() {
        assert_eq!(resolve_fixed_capu_runtime(8), (4, 5, 5));
        assert_eq!(resolve_fixed_capu_runtime(4), (4, 5, 5));
        assert_eq!(FIXED_CAPU_CPU_THREADS, 4);
        assert_eq!(FIXED_CAPU_PUNCTUATION_LEVEL, 5);
        assert_eq!(FIXED_CAPU_CASE_LEVEL, 5);
    }

    #[test]
    fn fixed_capu_threads_clamp_to_physical_cores() {
        assert_eq!(resolve_fixed_capu_runtime(2), (2, 5, 5));
        assert_eq!(resolve_fixed_capu_runtime(1), (1, 5, 5));
        assert_eq!(resolve_fixed_capu_runtime(0), (1, 5, 5));
    }
}
