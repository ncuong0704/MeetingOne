use crate::api::TranscriptSegment;
use anyhow::Result;
use log::{debug, info};
use std::path::Path;
use uuid::Uuid;

pub const DEFAULT_MAX_SEGMENT_SECONDS: u32 = 25;
pub const MIN_MAX_SEGMENT_SECONDS: u32 = 5;
pub const MAX_MAX_SEGMENT_SECONDS: u32 = 30;

/// Clamp user-configured max segment length to the supported range (5–30 seconds).
pub fn clamp_max_segment_seconds(seconds: i32) -> u32 {
    let seconds = if seconds <= 0 {
        DEFAULT_MAX_SEGMENT_SECONDS as i32
    } else {
        seconds
    };
    (seconds as u32).clamp(MIN_MAX_SEGMENT_SECONDS, MAX_MAX_SEGMENT_SECONDS)
}

/// Split long VAD segments at silence boundaries before sending to ASR.
pub fn expand_segments_at_silence(
    segments: impl IntoIterator<Item = crate::audio::vad::SpeechSegment>,
    max_seconds: u32,
) -> Vec<crate::audio::vad::SpeechSegment> {
    let max_samples = max_seconds as usize * 16000;
    let mut result = Vec::new();
    for segment in segments {
        if segment.samples.len() > max_samples {
            debug!(
                "Splitting large segment ({:.0}ms, {} samples) at silence boundaries (max {}s)",
                segment.end_timestamp_ms - segment.start_timestamp_ms,
                segment.samples.len(),
                max_seconds
            );
            result.extend(split_segment_at_silence(&segment, max_samples));
        } else {
            result.push(segment);
        }
    }
    result
}

/// File-path-only counterpart to [`expand_segments_at_silence`] that additionally reports
/// which of the returned segments were extended with extra *leading* audio context
/// because no silence boundary could be found nearby. The live pipeline
/// (`audio/pipeline.rs`) keeps using the plain `expand_segments_at_silence` above
/// unchanged — this one is for `batch_transcribe`'s overlap-stitch step
/// (`audio/batch_transcribe.rs`), which only makes sense for the file/retranscription
/// path where every segment's ASR result is available at once before finalizing.
///
/// Returns `(segments, leading_context_samples)`, index-aligned: `leading_context_samples[i]`
/// is the number of extra samples at the *start* of `segments[i].samples` that were
/// already decoded as the *tail* of `segments[i-1]` — 0 for a normal or cleanly-split
/// segment.
pub fn expand_segments_with_overlap(
    segments: impl IntoIterator<Item = crate::audio::vad::SpeechSegment>,
    max_seconds: u32,
) -> (Vec<crate::audio::vad::SpeechSegment>, Vec<usize>) {
    let max_samples = max_seconds as usize * 16000;
    let mut result_segments = Vec::new();
    let mut result_overlap = Vec::new();
    for segment in segments {
        if segment.samples.len() > max_samples {
            debug!(
                "Splitting large segment ({:.0}ms, {} samples) at silence boundaries with overlap-stitch (max {}s)",
                segment.end_timestamp_ms - segment.start_timestamp_ms,
                segment.samples.len(),
                max_seconds
            );
            for (chunk, leading_context) in split_segment_with_overlap(&segment, max_samples) {
                result_segments.push(chunk);
                result_overlap.push(leading_context);
            }
        } else {
            result_segments.push(segment);
            result_overlap.push(0);
        }
    }
    (result_segments, result_overlap)
}

/// Release the ZipFormer model's memory after a one-off batch job (audio
/// import, retranscription) finishes, unless a live recording is currently
/// using the engine. This restores the memory-freeing behavior the original
/// Whisper-based implementation had (commit 6a7eb26) before the ZipFormer
/// migration silently dropped it.
pub(crate) async fn unload_engine_after_batch() {
    if crate::audio::recording_commands::is_recording().await {
        log::info!("Skipping model unload after batch: recording in progress");
        return;
    }

    match crate::asr_engine::commands::get_engine_arc() {
        Ok(engine) => {
            engine.unload_model().await;
        }
        Err(e) => {
            log::warn!("Skipping model unload after batch: engine not available: {}", e);
        }
    }
}

/// Create transcript segments from transcription results.
/// Each tuple is (text, start_ms, end_ms) from VAD timestamps.
pub(crate) fn create_transcript_segments(transcripts: &[(String, f64, f64)]) -> Vec<TranscriptSegment> {
    transcripts
        .iter()
        .map(|(text, start_ms, end_ms)| {
            let start_seconds = start_ms / 1000.0;
            let end_seconds = end_ms / 1000.0;
            let duration = end_seconds - start_seconds;

            TranscriptSegment {
                id: format!("transcript-{}", Uuid::new_v4()),
                text: text.trim().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                audio_start_time: Some(start_seconds),
                audio_end_time: Some(end_seconds),
                duration: Some(duration),
            }
        })
        .collect()
}

/// Write transcripts.json to a meeting folder (atomic write with temp file)
pub(crate) fn write_transcripts_json(folder: &Path, segments: &[TranscriptSegment]) -> Result<()> {
    let transcript_path = folder.join("transcripts.json");
    let temp_path = folder.join(".transcripts.json.tmp");

    let json = serde_json::json!({
        "version": "1.0",
        "last_updated": chrono::Utc::now().to_rfc3339(),
        "total_segments": segments.len(),
        "segments": segments.iter().enumerate().map(|(i, s)| {
            serde_json::json!({
                "id": s.id,
                "text": s.text,
                "timestamp": s.timestamp,
                "audio_start_time": s.audio_start_time,
                "audio_end_time": s.audio_end_time,
                "duration": s.duration,
                "sequence_id": i
            })
        }).collect::<Vec<_>>()
    });

    let json_string = serde_json::to_string_pretty(&json)?;
    std::fs::write(&temp_path, &json_string)?;
    std::fs::rename(&temp_path, &transcript_path)?;

    info!(
        "Wrote transcripts.json with {} segments to {}",
        segments.len(),
        transcript_path.display()
    );
    Ok(())
}

/// Split a long speech segment at the lowest-energy (silence) point near the target size.
///
/// Scans for 100ms windows with minimal RMS energy within +/-3 seconds of each target
/// split point. If no clear silence is found, falls back to a 1-second overlap split
/// to avoid cutting words at boundaries.
pub(crate) fn split_segment_at_silence(
    segment: &crate::audio::vad::SpeechSegment,
    max_samples: usize,
) -> Vec<crate::audio::vad::SpeechSegment> {
    const SAMPLE_RATE: usize = 16000;
    // 100ms window for energy measurement (1600 samples at 16kHz)
    const ENERGY_WINDOW: usize = SAMPLE_RATE / 10;
    // Search +/-3 seconds around the target split point
    const SEARCH_RADIUS: usize = SAMPLE_RATE * 3;
    // RMS threshold below which we consider a window "silent"
    const SILENCE_RMS_THRESHOLD: f32 = 0.02;
    // Overlap to use when no silence boundary is found (1 second)
    const FALLBACK_OVERLAP: usize = SAMPLE_RATE;

    let total = segment.samples.len();
    if total <= max_samples {
        return vec![segment.clone()];
    }

    let ms_per_sample = (segment.end_timestamp_ms - segment.start_timestamp_ms)
        / segment.samples.len() as f64;
    let mut result = Vec::new();
    let mut pos = 0usize;

    while pos < total {
        let remaining = total - pos;
        if remaining <= max_samples {
            // Last chunk - take everything remaining
            let chunk_samples = segment.samples[pos..].to_vec();
            let chunk_start_ms = segment.start_timestamp_ms + (pos as f64 * ms_per_sample);
            let chunk_end_ms = segment.end_timestamp_ms;
            result.push(crate::audio::vad::SpeechSegment {
                samples: chunk_samples,
                start_timestamp_ms: chunk_start_ms,
                end_timestamp_ms: chunk_end_ms,
                confidence: segment.confidence,
            });
            break;
        }

        // Target split point
        let target = pos + max_samples;

        // Search window: [target - SEARCH_RADIUS, target + SEARCH_RADIUS]
        let search_start = target.saturating_sub(SEARCH_RADIUS).max(pos + SAMPLE_RATE);
        let search_end = (target + SEARCH_RADIUS).min(total.saturating_sub(ENERGY_WINDOW));

        // Find the lowest-energy 100ms window in the search range
        let mut best_split = target.min(total); // fallback: exact target
        let mut best_rms = f32::MAX;

        if search_start + ENERGY_WINDOW <= search_end {
            let mut idx = search_start;
            while idx + ENERGY_WINDOW <= search_end {
                let window = &segment.samples[idx..idx + ENERGY_WINDOW];
                let rms = (window.iter().map(|s| s * s).sum::<f32>() / ENERGY_WINDOW as f32).sqrt();
                if rms < best_rms {
                    best_rms = rms;
                    best_split = idx + ENERGY_WINDOW / 2; // split at center of quiet window
                }
                // Step by 10ms (160 samples) for efficiency
                idx += SAMPLE_RATE / 100;
            }
        }

        let split_at = best_split;
        if best_rms <= SILENCE_RMS_THRESHOLD {
            debug!(
                "Splitting at silence boundary: sample {} (RMS={:.4})",
                split_at, best_rms
            );
        } else {
            debug!(
                "No silence found near target (best RMS={:.4}), splitting with overlap at sample {}",
                best_rms, split_at
            );
        }

        // Determine the actual end of this chunk (with overlap if no silence)
        let chunk_end = if best_rms > SILENCE_RMS_THRESHOLD {
            (split_at + FALLBACK_OVERLAP).min(total)
        } else {
            split_at
        };

        let chunk_samples = segment.samples[pos..chunk_end].to_vec();
        let chunk_start_ms = segment.start_timestamp_ms + (pos as f64 * ms_per_sample);
        let chunk_end_ms = segment.start_timestamp_ms + (chunk_end as f64 * ms_per_sample);

        result.push(crate::audio::vad::SpeechSegment {
            samples: chunk_samples,
            start_timestamp_ms: chunk_start_ms,
            end_timestamp_ms: chunk_end_ms,
            confidence: segment.confidence,
        });

        // Advance position to where the current chunk actually ends
        // to avoid transcribing the overlap region twice
        pos = chunk_end;
    }

    result
}

/// File-path-only counterpart to [`split_segment_at_silence`]: identical silence search,
/// but when no silence is found near the target split point, both sides of the cut
/// *share* the extended region instead of just extending the earlier chunk — the next
/// chunk's audio starts back at `split_at` (not at `chunk_end`), so it re-decodes the
/// same ~1s tail the previous chunk already decoded. `batch_transcribe`'s overlap-stitch
/// step then trims whichever of the two decodes duplicates the other, rather than
/// hoping a blind 1-second extension happened to land between words.
///
/// Returns `(segment, leading_context_samples)` pairs — `leading_context_samples > 0`
/// marks a segment whose `samples` include that many extra leading samples already
/// decoded as the tail of the *previous* returned segment.
pub(crate) fn split_segment_with_overlap(
    segment: &crate::audio::vad::SpeechSegment,
    max_samples: usize,
) -> Vec<(crate::audio::vad::SpeechSegment, usize)> {
    const SAMPLE_RATE: usize = 16000;
    const ENERGY_WINDOW: usize = SAMPLE_RATE / 10;
    const SEARCH_RADIUS: usize = SAMPLE_RATE * 3;
    const SILENCE_RMS_THRESHOLD: f32 = 0.02;
    const OVERLAP_SAMPLES: usize = SAMPLE_RATE;

    let total = segment.samples.len();
    if total <= max_samples {
        return vec![(segment.clone(), 0)];
    }

    let ms_per_sample = (segment.end_timestamp_ms - segment.start_timestamp_ms)
        / segment.samples.len() as f64;
    let mut result: Vec<(crate::audio::vad::SpeechSegment, usize)> = Vec::new();
    let mut pos = 0usize;
    // Leading-context overlap to attach to the *next* chunk pushed. Only known once the
    // previous (no-silence) iteration decides where that next chunk must start reading
    // from, so it's carried across loop iterations rather than computed inline.
    let mut pending_leading_overlap = 0usize;

    while pos < total {
        let remaining = total - pos;
        if remaining <= max_samples {
            let chunk_samples = segment.samples[pos..].to_vec();
            let chunk_start_ms = segment.start_timestamp_ms + (pos as f64 * ms_per_sample);
            result.push((
                crate::audio::vad::SpeechSegment {
                    samples: chunk_samples,
                    start_timestamp_ms: chunk_start_ms,
                    end_timestamp_ms: segment.end_timestamp_ms,
                    confidence: segment.confidence,
                },
                pending_leading_overlap,
            ));
            break;
        }

        let target = pos + max_samples;
        let search_start = target.saturating_sub(SEARCH_RADIUS).max(pos + SAMPLE_RATE);
        let search_end = (target + SEARCH_RADIUS).min(total.saturating_sub(ENERGY_WINDOW));

        let mut best_split = target.min(total);
        let mut best_rms = f32::MAX;

        if search_start + ENERGY_WINDOW <= search_end {
            let mut idx = search_start;
            while idx + ENERGY_WINDOW <= search_end {
                let window = &segment.samples[idx..idx + ENERGY_WINDOW];
                let rms = (window.iter().map(|s| s * s).sum::<f32>() / ENERGY_WINDOW as f32).sqrt();
                if rms < best_rms {
                    best_rms = rms;
                    best_split = idx + ENERGY_WINDOW / 2;
                }
                idx += SAMPLE_RATE / 100;
            }
        }

        let split_at = best_split;

        if best_rms <= SILENCE_RMS_THRESHOLD {
            // Clean cut at genuine silence — no overlap needed, matches
            // split_segment_at_silence exactly.
            debug!(
                "Splitting at silence boundary: sample {} (RMS={:.4})",
                split_at, best_rms
            );
            let chunk_samples = segment.samples[pos..split_at].to_vec();
            let chunk_start_ms = segment.start_timestamp_ms + (pos as f64 * ms_per_sample);
            let chunk_end_ms = segment.start_timestamp_ms + (split_at as f64 * ms_per_sample);
            result.push((
                crate::audio::vad::SpeechSegment {
                    samples: chunk_samples,
                    start_timestamp_ms: chunk_start_ms,
                    end_timestamp_ms: chunk_end_ms,
                    confidence: segment.confidence,
                },
                pending_leading_overlap,
            ));
            pending_leading_overlap = 0;
            pos = split_at;
        } else {
            // No silence nearby: extend this chunk past split_at by OVERLAP_SAMPLES so
            // its ASR decode has full trailing context, AND start the next chunk back at
            // split_at so its decode has full leading context — both see the shared
            // [split_at, chunk_end) region.
            debug!(
                "No silence found near target (best RMS={:.4}), overlap-splitting at sample {}",
                best_rms, split_at
            );
            let chunk_end = (split_at + OVERLAP_SAMPLES).min(total);
            let chunk_samples = segment.samples[pos..chunk_end].to_vec();
            let chunk_start_ms = segment.start_timestamp_ms + (pos as f64 * ms_per_sample);
            let chunk_end_ms = segment.start_timestamp_ms + (chunk_end as f64 * ms_per_sample);
            result.push((
                crate::audio::vad::SpeechSegment {
                    samples: chunk_samples,
                    start_timestamp_ms: chunk_start_ms,
                    end_timestamp_ms: chunk_end_ms,
                    confidence: segment.confidence,
                },
                pending_leading_overlap,
            ));
            // The chunk we push *next* will start reading from `split_at`, which is
            // inside the chunk just pushed — that's its leading-context overlap.
            pending_leading_overlap = chunk_end - split_at;
            pos = split_at;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::vad::SpeechSegment;

    fn segment(samples: Vec<f32>, start_ms: f64, end_ms: f64) -> SpeechSegment {
        SpeechSegment {
            samples,
            start_timestamp_ms: start_ms,
            end_timestamp_ms: end_ms,
            confidence: 0.9,
        }
    }

    #[test]
    fn split_segment_with_overlap_short_segment_is_returned_unsplit_with_no_overlap() {
        let seg = segment(vec![0.1; 16000], 0.0, 1000.0);
        let result = split_segment_with_overlap(&seg, 25 * 16000);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.samples.len(), 16000);
        assert_eq!(result[0].1, 0);
    }

    #[test]
    fn split_segment_with_overlap_clean_silence_produces_zero_overlap_everywhere() {
        // 60s of low-level noise with a real silent gap at 25s — mirrors
        // split_segment_at_silence's own equivalent fixture.
        let mut samples = vec![0.01f32; 60 * 16000];
        for i in (25 * 16000)..(25 * 16000 + 3200) {
            samples[i] = 0.0;
        }
        let seg = segment(samples, 0.0, 60_000.0);

        let result = split_segment_with_overlap(&seg, 25 * 16000);
        assert!(result.len() >= 2, "expected a split, got {} chunk(s)", result.len());
        for (i, (chunk, overlap)) in result.iter().enumerate() {
            assert_eq!(*overlap, 0, "chunk {} should have no overlap at a clean silence split", i);
            assert!(!chunk.samples.is_empty());
        }
    }

    #[test]
    fn split_segment_with_overlap_no_silence_marks_leading_context_on_the_next_chunk() {
        // Continuous constant-energy "speech" with no silence anywhere — every split
        // point falls back to the overlap strategy.
        let seg = segment(vec![0.5f32; 60 * 16000], 0.0, 60_000.0);

        let result = split_segment_with_overlap(&seg, 25 * 16000);
        assert!(result.len() >= 2, "expected a split, got {} chunk(s)", result.len());

        // First chunk never has leading context (nothing precedes it).
        assert_eq!(result[0].1, 0);
        // Every subsequent chunk in a no-silence-anywhere segment must have leading
        // context recorded, and it must not exceed the 1-second overlap budget.
        for (i, (_, overlap)) in result.iter().enumerate().skip(1) {
            assert!(*overlap > 0, "chunk {} should have leading-context overlap", i);
            assert!(*overlap <= 16000, "chunk {} overlap {} exceeds the 1s budget", i, overlap);
        }

        // No audio lost: total decoded samples (including intentional double-counting of
        // the overlapped region) must be >= the original segment length.
        let total_samples: usize = result.iter().map(|(s, _)| s.samples.len()).sum();
        assert!(total_samples >= 60 * 16000, "overlap must not lose samples");

        // Each overlapping chunk's reported start time is genuinely inside the previous
        // chunk's reported time range — that's what "shares audio" means here.
        for pair in result.windows(2) {
            let (prev, _) = &pair[0];
            let (next, next_overlap) = &pair[1];
            if *next_overlap > 0 {
                assert!(
                    next.start_timestamp_ms < prev.end_timestamp_ms,
                    "overlapping chunk should start before the previous chunk ends"
                );
            }
        }
    }

    #[test]
    fn expand_segments_with_overlap_passes_through_short_segments_unsplit() {
        let segments = vec![segment(vec![0.1; 8000], 0.0, 500.0)];
        let (out_segments, overlaps) = expand_segments_with_overlap(segments, 25);
        assert_eq!(out_segments.len(), 1);
        assert_eq!(overlaps, vec![0]);
    }

    #[test]
    fn expand_segments_with_overlap_keeps_segments_and_overlaps_index_aligned() {
        let short = segment(vec![0.1; 8000], 0.0, 500.0);
        let long_no_silence = segment(vec![0.5f32; 60 * 16000], 1000.0, 61_000.0);
        let (out_segments, overlaps) =
            expand_segments_with_overlap(vec![short, long_no_silence], 25);

        assert_eq!(out_segments.len(), overlaps.len());
        // The short segment passes through first, with no overlap.
        assert_eq!(overlaps[0], 0);
        // At least one of the long segment's split pieces must carry overlap.
        assert!(overlaps[1..].iter().any(|&o| o > 0));
    }
}
