// File-import ASR preparation: VAD merge → concat speech → preprocess → silence-aware chunking.
// Mirrors the test ASR file pipeline (VAD → preprocess on speech → concat → 30s chunks).

use crate::audio::audio_processing::{HighPassFilter, LoudnessNormalizer};
use crate::audio::vad::SpeechSegment;
use log::{info, warn};
use std::time::Instant;

const SAMPLE_RATE: usize = 16000;
/// test ASR `merge_gap_ms=250` — only merge gaps under 250ms.
const MAX_VAD_GAP_SAMPLES: usize = (250 * SAMPLE_RATE) / 1000;
/// test ASR `padding_ms=1000` — extend each VAD region before concat.
const VAD_EDGE_PADDING_SAMPLES: usize = SAMPLE_RATE;
/// test ASR auto-boost target peak for quiet inputs passed to VAD only.
const VAD_BOOST_TARGET_PEAK: f32 = 0.071;
/// Window size for VAD boosting: a peak is computed and applied per-window rather than
/// once for the whole file, so a single loud moment early in a long recording doesn't
/// suppress boosting for a quieter passage later on (which would otherwise never cross
/// Silero's speech threshold and get silently dropped from VAD output).
const VAD_BOOST_WINDOW_SAMPLES: usize = 10 * SAMPLE_RATE;
/// Target ASR chunk length on concat audio (test ASR uses 30s).
const DEFAULT_CHUNK_SAMPLES: usize = 30 * SAMPLE_RATE;
/// Overlap between consecutive chunks on concat audio (matches batch stitch window).
const CHUNK_OVERLAP_SAMPLES: usize = SAMPLE_RATE;

/// Maps a sample index in concat space back to the original timeline (samples).
#[derive(Debug, Clone)]
struct OffsetEntry {
    concat_start: usize,
    original_start: usize,
    length: usize,
}

/// Merge adjacent VAD ranges when the silence gap between them is small.
pub fn merge_vad_ranges(ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    if ranges.is_empty() {
        return ranges;
    }
    let mut merged = vec![ranges[0]];
    for (seg_s, seg_e) in ranges.into_iter().skip(1) {
        let (prev_s, prev_e) = *merged.last().unwrap();
        if seg_s.saturating_sub(prev_e) <= MAX_VAD_GAP_SAMPLES {
            merged.pop();
            merged.push((prev_s, seg_e));
        } else {
            merged.push((seg_s, seg_e));
        }
    }
    merged
}

/// Concatenate speech regions, dropping silence between VAD segments.
fn concat_vad_speech(audio: &[f32], ranges: &[(usize, usize)]) -> (Vec<f32>, Vec<OffsetEntry>) {
    let mut parts: Vec<f32> = Vec::new();
    let mut offset_map = Vec::new();
    let mut concat_pos = 0usize;

    for &(seg_start, seg_end) in ranges {
        let seg_start = seg_start.min(audio.len());
        let seg_end = seg_end.min(audio.len());
        if seg_end <= seg_start {
            continue;
        }
        let seg_len = seg_end - seg_start;
        offset_map.push(OffsetEntry {
            concat_start: concat_pos,
            original_start: seg_start,
            length: seg_len,
        });
        parts.extend_from_slice(&audio[seg_start..seg_end]);
        concat_pos += seg_len;
    }

    if parts.is_empty() {
        return (audio.to_vec(), vec![OffsetEntry {
            concat_start: 0,
            original_start: 0,
            length: audio.len(),
        }]);
    }

    (parts, offset_map)
}

/// Map a sample index in concat space back to the original timeline (matches test ASR).
fn map_concat_sample_to_original(concat_sample: usize, offset_map: &[OffsetEntry]) -> usize {
    for entry in offset_map {
        if concat_sample < entry.concat_start + entry.length {
            let offset_in_seg = concat_sample.saturating_sub(entry.concat_start);
            return entry.original_start + offset_in_seg;
        }
    }
    if let Some(first) = offset_map.first() {
        if concat_sample < first.concat_start {
            return first.original_start;
        }
    }
    if let Some(last) = offset_map.last() {
        return last.original_start + last.length;
    }
    concat_sample
}

/// Boost quiet audio for VAD only (matches test ASR `auto_boost` — does not touch ASR input).
/// Computes the peak and applies gain per `VAD_BOOST_WINDOW_SAMPLES` window rather than
/// once for the whole file, so a loud moment early in a long recording can't suppress
/// boosting for a quieter passage later on.
pub fn boost_audio_for_vad(samples: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(samples.len());
    for window in samples.chunks(VAD_BOOST_WINDOW_SAMPLES) {
        let peak = window.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        if peak > 1e-6 && peak < VAD_BOOST_TARGET_PEAK {
            let scale = VAD_BOOST_TARGET_PEAK / peak;
            info!(
                "[FileBatch] VAD boost: window peak {:.4} -> {:.3}",
                peak, VAD_BOOST_TARGET_PEAK
            );
            out.extend(window.iter().map(|s| s * scale));
        } else {
            out.extend_from_slice(window);
        }
    }
    out
}

fn pad_vad_ranges(ranges: Vec<(usize, usize)>, audio_len: usize) -> Vec<(usize, usize)> {
    ranges
        .into_iter()
        .map(|(start, end)| {
            let start = start.saturating_sub(VAD_EDGE_PADDING_SAMPLES);
            let end = (end + VAD_EDGE_PADDING_SAMPLES).min(audio_len);
            (start, end)
        })
        .filter(|(s, e)| e > s)
        .collect()
}

fn build_chunks_from_audio(
    audio: &[f32],
    offset_map: Vec<OffsetEntry>,
    max_chunk_seconds: u32,
    avg_confidence: f32,
) -> (Vec<SpeechSegment>, Vec<usize>, usize) {
    let concat_audio = preprocess_file_audio(audio);
    let chunk_samples = (max_chunk_seconds as usize * SAMPLE_RATE)
        .max(SAMPLE_RATE)
        .min(DEFAULT_CHUNK_SAMPLES.max(SAMPLE_RATE));
    let silent_regions = find_silent_regions(&concat_audio, 0.01, 0.3);
    let plan = build_chunk_plan(
        concat_audio.len(),
        &silent_regions,
        chunk_samples,
        CHUNK_OVERLAP_SAMPLES,
    );

    let mut out_segments = Vec::with_capacity(plan.len());
    let mut leading_context = Vec::with_capacity(plan.len());
    let chunk_count = plan.len();

    for (actual_start, actual_end, overlap_at_start) in plan {
        let actual_end = actual_end.min(concat_audio.len());
        let actual_start = actual_start.min(actual_end);
        if actual_end <= actual_start {
            continue;
        }
        let samples = concat_audio[actual_start..actual_end].to_vec();
        if samples.len() < 1600 {
            continue;
        }

        let logical_start = actual_start + overlap_at_start;
        let logical_end = actual_end;
        let orig_start = map_concat_sample_to_original(logical_start, &offset_map);
        let orig_end = map_concat_sample_to_original(logical_end.saturating_sub(1), &offset_map)
            .max(orig_start);

        out_segments.push(SpeechSegment {
            samples,
            start_timestamp_ms: sample_to_ms(orig_start),
            end_timestamp_ms: sample_to_ms(orig_end + 1),
            confidence: avg_confidence,
        });
        leading_context.push(overlap_at_start);
    }

    (out_segments, leading_context, chunk_count)
}

fn prepare_full_audio_fallback(
    audio: &[f32],
    max_chunk_seconds: u32,
) -> (Vec<SpeechSegment>, Vec<usize>, f64) {
    info!(
        "[FileBatch] Full-audio fallback: silence chunking on {:.1}s",
        audio.len() as f64 / SAMPLE_RATE as f64
    );
    let offset_map = vec![OffsetEntry {
        concat_start: 0,
        original_start: 0,
        length: audio.len(),
    }];
    let preprocess_start = Instant::now();
    let (segments, leading, _) =
        build_chunks_from_audio(audio, offset_map, max_chunk_seconds, 0.85);
    let preprocess_sec = preprocess_start.elapsed().as_secs_f64();
    (segments, leading, preprocess_sec)
}

/// High-pass + EBU R128 loudness normalization (speech-only, after VAD concat).
pub fn preprocess_file_audio(samples: &[f32]) -> Vec<f32> {
    let mut hpf = HighPassFilter::new(SAMPLE_RATE as u32, 80.0);
    let filtered = hpf.process(samples);
    let normalized = match LoudnessNormalizer::new(1, SAMPLE_RATE as u32) {
        Ok(mut normalizer) => normalizer.normalize_loudness(&filtered),
        Err(e) => {
            warn!(
                "Failed to create loudness normalizer for file import: {}, skipping normalization",
                e
            );
            filtered
        }
    };
    normalized
        .into_iter()
        .map(|s| if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 })
        .collect()
}

/// Find silent regions (start_sample, end_sample) using frame RMS energy.
fn find_silent_regions(audio: &[f32], threshold: f32, min_silence_sec: f32) -> Vec<(usize, usize)> {
    let frame_length = SAMPLE_RATE / 100; // 10ms
    if frame_length == 0 || audio.len() < frame_length {
        return Vec::new();
    }
    let num_frames = audio.len() / frame_length;
    let min_frames = (min_silence_sec * 100.0) as usize;

    let mut silent_regions = Vec::new();
    let mut in_silent = false;
    let mut silent_start_frame = 0usize;

    for frame_idx in 0..num_frames {
        let start = frame_idx * frame_length;
        let end = start + frame_length;
        let frame = &audio[start..end];
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
        let is_silent = rms < threshold;

        if is_silent && !in_silent {
            in_silent = true;
            silent_start_frame = frame_idx;
        } else if !is_silent && in_silent {
            let duration_frames = frame_idx - silent_start_frame;
            if duration_frames >= min_frames {
                silent_regions.push((
                    silent_start_frame * frame_length,
                    frame_idx * frame_length,
                ));
            }
            in_silent = false;
        }
    }

    if in_silent {
        let duration_frames = num_frames - silent_start_frame;
        if duration_frames >= min_frames {
            silent_regions.push((
                silent_start_frame * frame_length,
                audio.len(),
            ));
        }
    }

    silent_regions
}

fn find_best_split_point(
    target_sample: usize,
    total_samples: usize,
    silent_regions: &[(usize, usize)],
    search_window: usize,
) -> usize {
    let search_start = target_sample.saturating_sub(search_window);
    let search_end = (target_sample + search_window).min(total_samples);

    let mut best_point = target_sample;
    let mut best_distance = usize::MAX;

    for &(silent_start, silent_end) in silent_regions {
        if silent_end < search_start || silent_start > search_end {
            continue;
        }
        let mid = (silent_start + silent_end) / 2;
        let distance = mid.abs_diff(target_sample);
        if distance < best_distance {
            best_distance = distance;
            best_point = mid;
        }
    }

    best_point
}

/// Build chunk boundaries on concat audio (~`chunk_samples` per chunk, split at silence).
fn build_chunk_plan(
    concat_total: usize,
    silent_regions: &[(usize, usize)],
    chunk_samples: usize,
    overlap_samples: usize,
) -> Vec<(usize, usize, usize)> {
    if concat_total == 0 {
        return Vec::new();
    }

    let mut boundaries = vec![0usize];
    let mut current_pos = 0usize;
    while current_pos + chunk_samples < concat_total {
        let target = current_pos + chunk_samples;
        let best_split = find_best_split_point(target, concat_total, silent_regions, 2 * SAMPLE_RATE);
        let split = if best_split <= current_pos + 20 * SAMPLE_RATE {
            target
        } else {
            best_split
        };
        boundaries.push(split);
        current_pos = split;
    }
    boundaries.push(concat_total);

    let mut plan = Vec::new();
    for i in 0..boundaries.len() - 1 {
        let logical_start = boundaries[i];
        let logical_end = boundaries[i + 1];
        if i == 0 {
            plan.push((logical_start, logical_end, 0));
        } else {
            let actual_start = logical_start.saturating_sub(overlap_samples);
            plan.push((actual_start, logical_end, logical_start - actual_start));
        }
    }
    plan
}

fn sample_to_ms(sample: usize) -> f64 {
    sample as f64 / SAMPLE_RATE as f64 * 1000.0
}

/// Stats from `prepare_file_asr_segments` for benchmark logging.
#[derive(Debug, Clone)]
pub struct FilePrepareStats {
    pub vad_segments_in: usize,
    pub vad_ranges_merged: usize,
    pub asr_chunks_out: usize,
    pub preprocess_sec: f64,
    pub concat_speech_sec: f64,
    pub speech_coverage_pct: f64,
    pub used_full_audio_fallback: bool,
}
/// Convert VAD output into ASR-ready chunks: merge gaps → concat speech → preprocess → chunk.
///
/// Timestamps on each segment use **logical** (non-overlap) boundaries mapped to the
/// original timeline; `leading_context` carries overlap sample counts for stitch.
pub fn prepare_file_asr_segments(
    audio: &[f32],
    vad_segments: Vec<SpeechSegment>,
    max_chunk_seconds: u32,
) -> (Vec<SpeechSegment>, Vec<usize>, FilePrepareStats) {
    let vad_segments_in = vad_segments.len();
    if vad_segments.is_empty() {
        warn!("[FileBatch] No VAD segments — using full-audio fallback");
        let (segments, leading, preprocess_sec) =
            prepare_full_audio_fallback(audio, max_chunk_seconds);
        let chunk_count = segments.len();
        let audio_sec = audio.len() as f64 / SAMPLE_RATE as f64;
        return (
            segments,
            leading,
            FilePrepareStats {
                vad_segments_in: 0,
                vad_ranges_merged: 0,
                asr_chunks_out: chunk_count,
                preprocess_sec,
                concat_speech_sec: audio_sec,
                speech_coverage_pct: 100.0,
                used_full_audio_fallback: true,
            },
        );
    }

    let avg_confidence = vad_segments
        .iter()
        .map(|s| s.confidence)
        .sum::<f32>()
        / vad_segments.len().max(1) as f32;

    let mut ranges: Vec<(usize, usize)> = vad_segments
        .iter()
        .map(|seg| {
            let start = ((seg.start_timestamp_ms / 1000.0) * SAMPLE_RATE as f64) as usize;
            let end = ((seg.end_timestamp_ms / 1000.0) * SAMPLE_RATE as f64) as usize;
            (start.min(audio.len()), end.min(audio.len()))
        })
        .filter(|(s, e)| e > s)
        .collect();

    ranges = pad_vad_ranges(ranges, audio.len());

    let before_merge = ranges.len();
    ranges = merge_vad_ranges(ranges);
    if ranges.len() < before_merge {
        info!(
            "[FileBatch] Merged VAD ranges {} -> {} (gap < {}ms)",
            before_merge,
            ranges.len(),
            MAX_VAD_GAP_SAMPLES * 1000 / SAMPLE_RATE
        );
    }

    let vad_ranges_merged = ranges.len();
    let (concat_raw, offset_map) = concat_vad_speech(audio, &ranges);
    let concat_speech_sec = concat_raw.len() as f64 / SAMPLE_RATE as f64;
    let speech_coverage_pct = concat_raw.len() as f64 / audio.len().max(1) as f64 * 100.0;
    info!(
        "[FileBatch] Concat {} VAD ranges -> {:.1}s speech ({:.0}% coverage)",
        ranges.len(),
        concat_speech_sec,
        speech_coverage_pct
    );

    let preprocess_start = Instant::now();
    let (out_segments, leading_context, plan_len) =
        build_chunks_from_audio(&concat_raw, offset_map, max_chunk_seconds, avg_confidence);
    let preprocess_sec = preprocess_start.elapsed().as_secs_f64();
    info!(
        "[FileBatch] Built {} ASR chunks (target {}s, overlap {}s)",
        plan_len,
        max_chunk_seconds.min(30),
        CHUNK_OVERLAP_SAMPLES / SAMPLE_RATE
    );
    info!(
        "[FileBatch] Preprocess on {:.1}s speech took {:.3}s",
        concat_speech_sec, preprocess_sec
    );

    let stats = FilePrepareStats {
        vad_segments_in,
        vad_ranges_merged,
        asr_chunks_out: out_segments.len(),
        preprocess_sec,
        concat_speech_sec,
        speech_coverage_pct,
        used_full_audio_fallback: false,
    };

    (out_segments, leading_context, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_audio_for_vad_boosts_quiet_window_even_after_loud_window() {
        // A single global peak (the old behavior) would see the loud sample in the
        // first window and skip boosting for the whole file, leaving the quiet second
        // window (e.g. a softer-spoken trailing part of a meeting) below Silero's
        // speech threshold and silently dropped from VAD output.
        let window = VAD_BOOST_WINDOW_SAMPLES;
        let mut samples = vec![0.0f32; window * 2];
        samples[10] = 0.5; // loud: already above VAD_BOOST_TARGET_PEAK, no boost needed
        samples[window + 10] = 0.01; // quiet: well below VAD_BOOST_TARGET_PEAK

        let boosted = boost_audio_for_vad(&samples);

        assert_eq!(boosted[10], 0.5, "loud window must be left unchanged");
        assert!(
            boosted[window + 10] > 0.01,
            "quiet second window must be boosted independently of the loud first window, got {}",
            boosted[window + 10]
        );
    }

    #[test]
    fn merge_vad_ranges_joins_gaps_under_250ms() {
        let ranges = vec![(0, 1000), (1200, 5000), (200_000, 210_000)];
        let merged = merge_vad_ranges(ranges);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0], (0, 5000));
    }

    #[test]
    fn concat_and_chunk_produces_multiple_segments_for_long_audio() {
        let audio: Vec<f32> = (0..(60 * SAMPLE_RATE)).map(|i| (i as f32 * 0.001).sin() * 0.1).collect();
        let vad = vec![SpeechSegment {
            samples: audio.clone(),
            start_timestamp_ms: 0.0,
            end_timestamp_ms: 60_000.0,
            confidence: 0.9,
        }];
        let (segments, overlaps, stats) = prepare_file_asr_segments(&audio, vad, 30);
        assert!(segments.len() >= 2);
        assert_eq!(segments.len(), overlaps.len());
        assert_eq!(stats.asr_chunks_out, segments.len());
        for i in 1..segments.len() {
            assert!(
                segments[i].start_timestamp_ms >= segments[i - 1].start_timestamp_ms,
                "chunk timestamps must be monotonic (logical, no overlap)"
            );
        }
    }

    #[test]
    fn logical_timestamps_exclude_overlap_region() {
        let audio: Vec<f32> = (0..(45 * SAMPLE_RATE))
            .map(|i| (i as f32 * 0.001).sin() * 0.1)
            .collect();
        let vad = vec![SpeechSegment {
            samples: audio.clone(),
            start_timestamp_ms: 0.0,
            end_timestamp_ms: 45_000.0,
            confidence: 0.9,
        }];
        let (segments, overlaps, _) = prepare_file_asr_segments(&audio, vad, 30);
        assert!(segments.len() >= 2);
        assert!(overlaps[1] > 0);
        let gap_ms = segments[1].start_timestamp_ms - segments[0].end_timestamp_ms;
        assert!(
            gap_ms >= -1.0,
            "logical start should not precede previous end by more than 1ms rounding"
        );
    }
}
