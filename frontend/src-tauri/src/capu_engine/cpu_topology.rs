use sysinfo::System;

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
}
