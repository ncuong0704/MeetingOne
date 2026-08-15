//! Align diarization turns onto transcript segment time ranges.

#[derive(Debug, Clone, PartialEq)]
pub struct SpeakerTurn {
    pub start_sec: f64,
    pub end_sec: f64,
    pub cluster_index: usize,
}

/// One transcript piece after splitting a CAPU segment on speaker-turn boundaries.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedPiece {
    pub text: String,
    pub start_sec: f64,
    pub end_sec: f64,
    pub cluster_index: Option<usize>,
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

/// Split long transcript segments at diarization turn boundaries and assign speakers.
///
/// CAPU often merges speech across speaker changes into one long segment; plain
/// max-overlap then collapses everything onto the dominant (longest) speaker.
/// Word-proportional text split keeps both speakers visible without word timings.
pub fn resegment_by_speaker_turns(
    segments: &[(String, f64, f64)],
    turns: &[SpeakerTurn],
) -> Vec<AlignedPiece> {
    let mut out = Vec::new();
    for (text, seg_start, seg_end) in segments {
        let mut slices = turn_slices_in_range(*seg_start, *seg_end, turns);
        if slices.is_empty() {
            out.push(AlignedPiece {
                text: text.clone(),
                start_sec: *seg_start,
                end_sec: *seg_end,
                cluster_index: best_cluster_for_range(*seg_start, *seg_end, turns),
            });
            continue;
        }

        // Cover edges of the segment that fall outside any turn.
        if slices[0].0 > *seg_start + 1e-3 {
            let ci = slices[0].2;
            slices.insert(0, (*seg_start, slices[0].0, ci));
        }
        let last_end = slices.last().map(|s| s.1).unwrap_or(*seg_end);
        if last_end + 1e-3 < *seg_end {
            let ci = slices.last().map(|s| s.2).unwrap_or(0);
            slices.push((last_end, *seg_end, ci));
        }
        slices = merge_adjacent_slices(slices);

        if slices.len() == 1 {
            out.push(AlignedPiece {
                text: text.clone(),
                start_sec: slices[0].0,
                end_sec: slices[0].1,
                cluster_index: Some(slices[0].2),
            });
            continue;
        }

        let weights: Vec<f64> = slices.iter().map(|(a, b, _)| (b - a).max(0.0)).collect();
        let texts = split_text_by_weights(text, &weights);
        for (i, (a, b, ci)) in slices.into_iter().enumerate() {
            let piece_text = texts.get(i).cloned().unwrap_or_default();
            if piece_text.trim().is_empty() && (b - a) < 0.5 {
                continue;
            }
            out.push(AlignedPiece {
                text: if piece_text.trim().is_empty() {
                    // Keep a non-empty label so short minority turns stay visible.
                    text.clone()
                } else {
                    piece_text
                },
                start_sec: a,
                end_sec: b,
                cluster_index: Some(ci),
            });
        }
    }
    out
}

fn turn_slices_in_range(
    seg_start: f64,
    seg_end: f64,
    turns: &[SpeakerTurn],
) -> Vec<(f64, f64, usize)> {
    let mut slices: Vec<(f64, f64, usize)> = Vec::new();
    for t in turns {
        let a = t.start_sec.max(seg_start);
        let b = t.end_sec.min(seg_end);
        if b > a + 1e-3 {
            slices.push((a, b, t.cluster_index));
        }
    }
    slices.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
    merge_adjacent_slices(slices)
}

fn merge_adjacent_slices(slices: Vec<(f64, f64, usize)>) -> Vec<(f64, f64, usize)> {
    let mut merged: Vec<(f64, f64, usize)> = Vec::new();
    for (a, b, ci) in slices {
        if let Some(last) = merged.last_mut() {
            if last.2 == ci && a <= last.1 + 0.05 {
                last.1 = last.1.max(b);
                continue;
            }
        }
        merged.push((a, b, ci));
    }
    merged
}

fn split_text_by_weights(text: &str, weights: &[f64]) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if weights.is_empty() {
        return Vec::new();
    }
    if words.is_empty() {
        return weights.iter().map(|_| String::new()).collect();
    }
    let total_w: f64 = weights.iter().sum::<f64>().max(1e-9);
    let mut counts = vec![0usize; weights.len()];
    let mut assigned = 0usize;
    for i in 0..weights.len().saturating_sub(1) {
        let n = ((weights[i] / total_w) * words.len() as f64).floor() as usize;
        counts[i] = n;
        assigned += n;
    }
    counts[weights.len() - 1] = words.len().saturating_sub(assigned);

    let mut cursor = 0usize;
    let mut out = Vec::with_capacity(weights.len());
    for (i, &n) in counts.iter().enumerate() {
        let end = if i + 1 == counts.len() {
            words.len()
        } else {
            (cursor + n).min(words.len())
        };
        out.push(words[cursor..end].join(" "));
        cursor = end;
    }
    out
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

    #[test]
    fn resegment_long_capu_keeps_both_speakers() {
        // Mirrors the giao-ban failure: one long CAPU span vs two clusters.
        let turns = vec![
            SpeakerTurn {
                start_sec: 0.0,
                end_sec: 225.0,
                cluster_index: 0,
            },
            SpeakerTurn {
                start_sec: 225.0,
                end_sec: 255.0,
                cluster_index: 1,
            },
            SpeakerTurn {
                start_sec: 255.0,
                end_sec: 293.0,
                cluster_index: 0,
            },
        ];
        let segs = vec![(
            "mot hai ba bon nam sau bay tam chin muoi".to_string(),
            0.0,
            293.0,
        )];
        let pieces = resegment_by_speaker_turns(&segs, &turns);
        let mut clusters: Vec<usize> = pieces.iter().filter_map(|p| p.cluster_index).collect();
        clusters.sort_unstable();
        clusters.dedup();
        assert_eq!(clusters, vec![0, 1], "pieces={pieces:?}");
        assert!(pieces.len() >= 2);
        assert!(pieces.iter().all(|p| !p.text.trim().is_empty()));
    }
}
