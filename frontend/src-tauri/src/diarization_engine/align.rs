//! Align diarization turns onto transcript segment time ranges.

#[derive(Debug, Clone, PartialEq)]
pub struct SpeakerTurn {
    pub start_sec: f64,
    pub end_sec: f64,
    pub cluster_index: usize,
}

const SPEAKER_COLORS: [&str; 8] = [
    "#2563EB", // blue
    "#DC2626", // red
    "#059669", // emerald
    "#D97706", // amber
    "#7C3AED", // violet
    "#DB2777", // pink
    "#0891B2", // cyan
    "#65A30D", // lime
];

/// Returns per-segment cluster_index (None if no overlapping turn).
pub fn align_speakers_to_segments(
    segment_ranges: &[(f64, f64)],
    turns: &[SpeakerTurn],
) -> Vec<Option<usize>> {
    segment_ranges
        .iter()
        .map(|&(seg_start, seg_end)| best_cluster_for_range(seg_start, seg_end, turns))
        .collect()
}

fn overlap_sec(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

fn best_cluster_for_range(seg_start: f64, seg_end: f64, turns: &[SpeakerTurn]) -> Option<usize> {
    let mut best: Option<(f64, usize)> = None; // (overlap, cluster_index)
    for turn in turns {
        let ov = overlap_sec(seg_start, seg_end, turn.start_sec, turn.end_sec);
        if ov <= 0.0 {
            continue;
        }
        match best {
            None => best = Some((ov, turn.cluster_index)),
            Some((best_ov, best_ci)) => {
                if ov > best_ov || (ov == best_ov && turn.cluster_index < best_ci) {
                    best = Some((ov, turn.cluster_index));
                }
            }
        }
    }
    best.map(|(_, ci)| ci)
}

pub fn speaker_color_for_index(cluster_index: usize) -> &'static str {
    SPEAKER_COLORS[cluster_index % SPEAKER_COLORS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_overlap_picks_dominant_turn() {
        let turns = vec![
            SpeakerTurn {
                start_sec: 0.0,
                end_sec: 5.0,
                cluster_index: 0,
            },
            SpeakerTurn {
                start_sec: 5.0,
                end_sec: 10.0,
                cluster_index: 1,
            },
        ];
        let segs = vec![(0.0, 4.0), (4.5, 7.0), (7.0, 9.0)];
        assert_eq!(
            align_speakers_to_segments(&segs, &turns),
            vec![Some(0), Some(1), Some(1)]
        );
    }

    #[test]
    fn no_turns_yields_none() {
        assert_eq!(
            align_speakers_to_segments(&[(0.0, 1.0)], &[]),
            vec![None]
        );
    }

    #[test]
    fn color_cycles_palette() {
        let c0 = speaker_color_for_index(0);
        let c8 = speaker_color_for_index(8);
        assert_eq!(c0, speaker_color_for_index(0));
        assert_ne!(c0, speaker_color_for_index(1));
        assert_eq!(c0, c8); // palette len 8
    }
}
