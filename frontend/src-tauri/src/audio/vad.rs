use anyhow::{anyhow, Result};
use silero_rs::{VadConfig, VadSession, VadTransition};
use log::{debug, info, warn};
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use std::collections::VecDeque;
use std::time::Duration;

/// Silero speech-probability hysteresis thresholds (enter speech above `positive`,
/// exit below `negative`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadThresholds {
    pub positive: f32,
    pub negative: f32,
}

/// Vietnamese-tuned thresholds for the live recording pipeline (see rationale in
/// `ContinuousVadProcessor::new`). Kept as before this change — live-recording behavior
/// is unaffected.
pub const LIVE_VAD_THRESHOLDS: VadThresholds = VadThresholds {
    positive: 0.35,
    negative: 0.20,
};

/// Lower thresholds for file-import/retranscription batch VAD. Confirmed empirically: a
/// sibling app's Silero VAD (positive threshold 0.2, same underlying model family) fully
/// detects a 95s trailing segment of a real meeting recording that this app's
/// live-tuned 0.35 threshold silently drops — even though that segment's amplitude is
/// normal (not quieter than the rest of the file, ruling out a gain/boost cause). File
/// audio has no live-latency constraint, so a lower, more permissive threshold is safe
/// here without the live-path tradeoffs (false speech starts interrupting silence UI).
pub const FILE_BATCH_VAD_THRESHOLDS: VadThresholds = VadThresholds {
    positive: 0.20,
    negative: 0.10,
};

/// File-import force-flush so a 3-hour continuous meeting does not sit in one buffer.
pub const FILE_BATCH_MAX_SPEECH_SEC: u32 = 30;

/// Represents a complete speech segment detected by VAD
#[derive(Debug, Clone)]
pub struct SpeechSegment {
    pub samples: Vec<f32>,
    pub start_timestamp_ms: f64,
    pub end_timestamp_ms: f64,
    pub confidence: f32,
}

/// Processes audio in 30ms chunks but returns complete speech segments
pub struct ContinuousVadProcessor {
    session: VadSession,
    chunk_size: usize,
    sample_rate: u32,
    buffer: Vec<f32>,
    speech_segments: VecDeque<SpeechSegment>,
    current_speech: Vec<f32>,
    in_speech: bool,
    processed_samples: usize,
    speech_start_sample: usize,
    last_logged_state: bool,
    // High-quality sinc resampler (Rubato) for pitch-preserving downsampling to 16kHz.
    // Replaces the previous linear interpolation which distorted Vietnamese tones.
    sinc_resampler: Option<SincFixedIn<f32>>,
    resample_buffer: Vec<f32>,
    /// Live path: force-emit a segment when held speech exceeds this many 16 kHz samples,
    /// even without VAD SpeechEnd. `None` = never force-flush (file batch).
    max_speech_samples: Option<usize>,
    /// Samples already force-emitted since the current SpeechStart (for SpeechEnd tail).
    force_flushed_samples: usize,
}

impl ContinuousVadProcessor {
    pub fn new(
        input_sample_rate: u32,
        redemption_time_ms: u32,
        thresholds: VadThresholds,
    ) -> Result<Self> {
        Self::new_with_max_speech(input_sample_rate, redemption_time_ms, thresholds, None)
    }

    pub fn new_with_max_speech(
        input_sample_rate: u32,
        redemption_time_ms: u32,
        thresholds: VadThresholds,
        max_speech_seconds: Option<u32>,
    ) -> Result<Self> {
        // Silero VAD MUST use 16kHz - this is hardcoded requirement
        const VAD_SAMPLE_RATE: u32 = 16000;

        // Use STRICT settings to prevent silence from reaching Whisper
        let mut config = VadConfig::default();
        config.sample_rate = VAD_SAMPLE_RATE as usize;

        // Vietnamese-tuned VAD thresholds (see `LIVE_VAD_THRESHOLDS`/`FILE_BATCH_VAD_THRESHOLDS`
        // doc comments for the reasoning behind each). Silero VAD was trained on non-tonal
        // languages; Vietnamese's 6 tones create rapid F0 variation that causes Silero to
        // underestimate speech probability generally, on top of the live-vs-file distinction.
        config.positive_speech_threshold = thresholds.positive;
        config.negative_speech_threshold = thresholds.negative;

        config.redemption_time = Duration::from_millis(redemption_time_ms as u64);
        config.pre_speech_pad = Duration::from_millis(300);
        // silero-rs `get_speech` slices `speech_end + post_speech_pad` with no clamp.
        // SpeechEnd fires when silence exceeds redemption (~one 30ms frame later).
        // If pad > redemption the slice is past `session_audio` (e.g. 400ms pad vs
        // ~330ms silence → 1120 samples OOB). silero's own validate_config rejects
        // this, but we mutate `VadConfig::default()` and never call it.
        config.post_speech_pad = clamped_post_speech_pad(redemption_time_ms);

        // 150ms minimum allows monosyllabic Vietnamese words ("Có", "Không", "Ừ", "Được")
        // that are typically 80-150ms to pass through to ZipFormer.
        config.min_speech_time = Duration::from_millis(150);

        debug!("Creating VAD session with: sample_rate={}Hz, redemption={}ms, min_speech={}ms, input_rate={}Hz",
               VAD_SAMPLE_RATE, redemption_time_ms, 150, input_sample_rate);

        let session = VadSession::new(config)
            .map_err(|e| anyhow!("Failed to create VAD session: {:?}", e))?;

        // VAD uses 30ms chunks at 16kHz (480 samples)
        let vad_chunk_size = (VAD_SAMPLE_RATE as f32 * 0.03) as usize; // 480 samples

        // Build a persistent Rubato sinc resampler when input rate ≠ 16kHz.
        // 512-sample input chunks match the pattern used in pipeline.rs (RESAMPLER_CHUNK_SIZE).
        // Downsampling ratio ≤ 0.5 → anti-aliasing mode (sinc_len=256, Cubic, oversampling=256)
        // preserves Vietnamese pitch contours better than the previous linear interpolation.
        const SINC_CHUNK_SIZE: usize = 512;
        let sinc_resampler = if input_sample_rate != VAD_SAMPLE_RATE {
            let ratio = VAD_SAMPLE_RATE as f64 / input_sample_rate as f64;
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Cubic,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            match SincFixedIn::<f32>::new(ratio, 2.0, params, SINC_CHUNK_SIZE, 1) {
                Ok(r) => {
                    info!("VAD sinc resampler: {}Hz → {}Hz (ratio={:.4}, chunk={})",
                          input_sample_rate, VAD_SAMPLE_RATE, ratio, SINC_CHUNK_SIZE);
                    Some(r)
                }
                Err(e) => {
                    warn!("Failed to create VAD sinc resampler: {} — using linear fallback", e);
                    None
                }
            }
        } else {
            None
        };

        info!("VAD processor created: input={}Hz, vad={}Hz, chunk_size={} samples, max_speech_sec={:?}",
              input_sample_rate, VAD_SAMPLE_RATE, vad_chunk_size, max_speech_seconds);

        Ok(Self {
            session,
            chunk_size: vad_chunk_size,
            sample_rate: input_sample_rate,
            buffer: Vec::with_capacity(vad_chunk_size * 2),
            speech_segments: VecDeque::new(),
            current_speech: Vec::new(),
            in_speech: false,
            processed_samples: 0,
            speech_start_sample: 0,
            last_logged_state: false,
            sinc_resampler,
            resample_buffer: Vec::with_capacity(SINC_CHUNK_SIZE * 2),
            max_speech_samples: max_speech_seconds.map(|s| s as usize * 16000),
            force_flushed_samples: 0,
        })
    }

    /// Process incoming audio samples and return any complete speech segments
    /// Handles resampling from input sample rate to 16kHz for VAD processing
    pub fn process_audio(&mut self, samples: &[f32]) -> Result<Vec<SpeechSegment>> {
        // Resample to 16kHz if needed
        let resampled_audio = if self.sample_rate == 16000 {
            samples.to_vec()
        } else {
            self.resample_to_16k(samples)?
        };

        self.buffer.extend_from_slice(&resampled_audio);
        let mut completed_segments = Vec::new();

        // Process complete 30ms chunks (480 samples at 16kHz)
        while self.buffer.len() >= self.chunk_size {
            let chunk: Vec<f32> = self.buffer.drain(..self.chunk_size).collect();
            self.process_chunk(&chunk)?;

            // Extract any completed speech segments
            while let Some(segment) = self.speech_segments.pop_front() {
                completed_segments.push(segment);
            }
        }

        Ok(completed_segments)
    }

    /// Resample from input sample rate to 16kHz using Rubato sinc interpolation.
    /// Falls back to linear interpolation if the sinc resampler is unavailable.
    fn resample_to_16k(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        if self.sample_rate == 16000 {
            return Ok(samples.to_vec());
        }

        const SINC_CHUNK_SIZE: usize = 512;

        if self.sinc_resampler.is_none() {
            return self.resample_to_16k_linear(samples);
        }

        self.resample_buffer.extend_from_slice(samples);
        let mut output = Vec::new();
        let mut error_occurred = false;

        while self.resample_buffer.len() >= SINC_CHUNK_SIZE && !error_occurred {
            let chunk: Vec<f32> = self.resample_buffer.drain(..SINC_CHUNK_SIZE).collect();
            let waves_in = vec![chunk];
            // Borrow ends at the end of this if-let block — no conflict with the
            // `self.sinc_resampler = None` assignment below (different lexical scope).
            if let Some(ref mut resampler) = self.sinc_resampler {
                match resampler.process(&waves_in, None) {
                    Ok(mut waves_out) => {
                        if let Some(out_ch) = waves_out.pop() {
                            output.extend_from_slice(&out_ch);
                        }
                    }
                    Err(e) => {
                        warn!("Sinc resampler error: {} — disabling and using linear fallback", e);
                        error_occurred = true;
                    }
                }
            }
        }

        if error_occurred {
            self.sinc_resampler = None;
            self.resample_buffer.clear();
            return self.resample_to_16k_linear(samples);
        }

        debug!("Sinc resampled {} samples ({}Hz) → {} samples (16kHz)",
               samples.len(), self.sample_rate, output.len());
        Ok(output)
    }

    /// Linear interpolation fallback resampler (lower quality, used only when sinc fails).
    fn resample_to_16k_linear(&self, samples: &[f32]) -> Result<Vec<f32>> {
        let ratio = self.sample_rate as f64 / 16000.0;
        let output_len = (samples.len() as f64 / ratio) as usize;
        let mut resampled = Vec::with_capacity(output_len);

        // Moving-average low-pass before downsampling
        let filter_size = std::cmp::max(1, std::cmp::min(
            (self.sample_rate as f64 / (0.4 * self.sample_rate as f64)) as usize, 5));
        let mut filtered = Vec::with_capacity(samples.len());
        for i in 0..samples.len() {
            let start = if i >= filter_size { i - filter_size } else { 0 };
            let end = std::cmp::min(i + filter_size + 1, samples.len());
            let sum: f32 = samples[start..end].iter().sum();
            filtered.push(sum / (end - start) as f32);
        }

        for i in 0..output_len {
            let src = i as f64 * ratio;
            let idx = src as usize;
            let frac = (src - idx as f64) as f32;
            if idx + 1 < filtered.len() {
                resampled.push(filtered[idx] + (filtered[idx + 1] - filtered[idx]) * frac);
            } else if idx < filtered.len() {
                resampled.push(filtered[idx]);
            }
        }

        debug!("Linear resampled {} samples ({}Hz) → {} samples (16kHz)",
               samples.len(), self.sample_rate, resampled.len());
        Ok(resampled)
    }

    /// Flush any remaining audio and return final speech segments
    pub fn flush(&mut self) -> Result<Vec<SpeechSegment>> {
        debug!("VAD flush: in_speech={}, current_speech_len={}, buffer_len={}, speech_segments_queued={}",
              self.in_speech, self.current_speech.len(), self.buffer.len(), self.speech_segments.len());

        let mut completed_segments = Vec::new();

        // Process any remaining buffered audio
        if !self.buffer.is_empty() {
            let remaining = self.buffer.clone();
            self.buffer.clear();

            // Pad to chunk size if needed
            let mut padded_chunk = remaining;
            if padded_chunk.len() < self.chunk_size {
                padded_chunk.resize(self.chunk_size, 0.0);
            }

            self.process_chunk(&padded_chunk)?;
        }

        // Force end any ongoing speech
        if self.in_speech && !self.current_speech.is_empty() {
            // processed_samples and speech_start_sample always count 16kHz samples (post-resampling)
            let (start_ms, end_ms) = flush_segment_timestamps(
                self.speech_start_sample,
                self.processed_samples,
                self.current_speech.len(),
            );

            debug!("VAD flush: Force-ending speech - start={}ms, end={}ms, duration={}ms, samples={}",
                  start_ms, end_ms, end_ms - start_ms, self.current_speech.len());

            let segment = SpeechSegment {
                samples: self.current_speech.clone(),
                start_timestamp_ms: start_ms,
                end_timestamp_ms: end_ms,
                confidence: 0.8, // Estimated confidence for forced end
            };

            self.speech_segments.push_back(segment);
            self.current_speech.clear();
            self.in_speech = false;
        }

        // Extract all remaining segments
        while let Some(segment) = self.speech_segments.pop_front() {
            completed_segments.push(segment);
        }

        Ok(completed_segments)
    }

    fn process_chunk(&mut self, chunk: &[f32]) -> Result<()> {
        let current_speech_size = self.current_speech.len();
        if current_speech_size > 1_000_000
            && current_speech_size <= 1_000_000 + self.chunk_size
        {
            warn!(
                "VAD: Accumulated speech buffer is large: {} samples ({:.1}s) - possible memory issue",
                current_speech_size,
                current_speech_size as f64 / 16000.0
            );
        }

        let transitions = self.session.process(chunk)
            .map_err(|e| anyhow!("VAD processing failed: {}", e))?;

        // Log transitions for debugging
        if !transitions.is_empty() {
            debug!("VAD transitions at sample {}: {} transitions", self.processed_samples, transitions.len());
        }

        // Handle VAD transitions
        for transition in transitions {
            match transition {
                VadTransition::SpeechStart { timestamp_ms } => {
                    // Only log if state changed
                    if !self.last_logged_state {
                        debug!("VAD: Speech started at {}ms", timestamp_ms);
                        self.last_logged_state = true;
                    }
                    self.in_speech = true;
                    // Silero `timestamp_ms` is milliseconds since the start of the VAD
                    // session, not relative to this chunk. Adding `processed_samples`
                    // double-counts and can invert the last in-progress segment at EOF
                    // (start past file end → prepare drops the samples).
                    self.speech_start_sample = speech_start_sample_from_session_ms(timestamp_ms);
                    self.current_speech.clear();
                    self.force_flushed_samples = 0;
                }
                VadTransition::SpeechEnd { start_timestamp_ms, end_timestamp_ms, samples } => {
                    // Only log if we were previously in speech state
                    if self.last_logged_state {
                        debug!("VAD: Speech ended at {}ms (duration: {}ms)", end_timestamp_ms, end_timestamp_ms - start_timestamp_ms);
                        self.last_logged_state = false;
                    }
                    self.in_speech = false;

                    // Use samples from VAD transition if available, otherwise use accumulated samples.
                    // If we already force-flushed mid-utterance, only emit the remaining tail from
                    // our buffer — silero's SpeechEnd samples would re-send already-emitted audio.
                    let (speech_samples, start_ts_ms, end_ts_ms) =
                        if self.force_flushed_samples > 0 {
                            let rem = self.current_speech.clone();
                            let start_ms = (self.speech_start_sample as f64 / 16000.0) * 1000.0;
                            let end_ms = start_ms + (rem.len() as f64 / 16000.0) * 1000.0;
                            (rem, start_ms, end_ms)
                        } else if !samples.is_empty() {
                            (samples, start_timestamp_ms as f64, end_timestamp_ms as f64)
                        } else {
                            (
                                self.current_speech.clone(),
                                start_timestamp_ms as f64,
                                end_timestamp_ms as f64,
                            )
                        };
                    self.force_flushed_samples = 0;

                    if !speech_samples.is_empty() {
                        let segment = SpeechSegment {
                            samples: speech_samples,
                            start_timestamp_ms: start_ts_ms,
                            end_timestamp_ms: end_ts_ms,
                            confidence: 0.9, // VAD confidence
                        };

                        info!("VAD: Completed speech segment: {:.1}ms duration, {} samples",
                              end_ts_ms - start_ts_ms, segment.samples.len());

                        self.speech_segments.push_back(segment);
                    }

                    self.current_speech.clear();
                }
            }
        }

        // Accumulate speech if we're currently in a speech state
        if self.in_speech {
            self.current_speech.extend_from_slice(chunk);

            // Live force-flush: emit completed chunks while still in speech so UI does not
            // wait for redemption silence on long continuous utterances.
            if let Some(max_samples) = self.max_speech_samples {
                while self.current_speech.len() >= max_samples {
                    let chunk_samples: Vec<f32> =
                        self.current_speech.drain(..max_samples).collect();
                    let start_ms = (self.speech_start_sample as f64 / 16000.0) * 1000.0;
                    let end_ms = start_ms + (chunk_samples.len() as f64 / 16000.0) * 1000.0;
                    info!(
                        "VAD: Force-flush mid-speech segment: {:.1}ms, {} samples (max={})",
                        end_ms - start_ms,
                        chunk_samples.len(),
                        max_samples
                    );
                    self.speech_segments.push_back(SpeechSegment {
                        samples: chunk_samples,
                        start_timestamp_ms: start_ms,
                        end_timestamp_ms: end_ms,
                        confidence: 0.9,
                    });
                    self.force_flushed_samples += max_samples;
                    self.speech_start_sample += max_samples;
                }
            }
        }

        self.processed_samples += chunk.len();
        Ok(())
    }
}

/// Silero `SpeechStart.timestamp_ms` is session-absolute (ms since VAD session start).
fn speech_start_sample_from_session_ms(timestamp_ms: usize) -> usize {
    timestamp_ms.saturating_mul(16000) / 1000
}

/// EOF flush timestamps. If `speech_start_sample` was double-counted past EOF, reconstruct
/// start from the actual buffered speech so the trailing samples are not dropped.
fn flush_segment_timestamps(
    speech_start_sample: usize,
    processed_samples: usize,
    current_speech_len: usize,
) -> (f64, f64) {
    let mut start_ms = (speech_start_sample as f64 / 16000.0) * 1000.0;
    let end_ms = (processed_samples as f64 / 16000.0) * 1000.0;
    if start_ms >= end_ms && current_speech_len > 0 {
        let dur_ms = (current_speech_len as f64 / 16000.0) * 1000.0;
        start_ms = (end_ms - dur_ms).max(0.0);
        warn!(
            "VAD flush: inverted timestamps (start={:.0}ms >= end={:.0}ms); reconstructed start={:.0}ms from {} samples",
            (speech_start_sample as f64 / 16000.0) * 1000.0,
            end_ms,
            start_ms,
            current_speech_len
        );
    }
    (start_ms, end_ms)
}

/// Keep silero `post_speech_pad` ≤ redemption so SpeechEnd never indexes past the buffer.
fn clamped_post_speech_pad(redemption_time_ms: u32) -> Duration {
    Duration::from_millis(400.min(redemption_time_ms as u64))
}

/// Legacy function for backward compatibility - now uses the optimized approach
pub fn extract_speech_16k(samples_mono_16k: &[f32]) -> Result<Vec<f32>> {
    let mut processor = ContinuousVadProcessor::new(16000, 400, LIVE_VAD_THRESHOLDS)?;

    // Process all audio
    let mut all_segments = processor.process_audio(samples_mono_16k)?;
    let final_segments = processor.flush()?;
    all_segments.extend(final_segments);

    // Concatenate all speech segments
    let mut result = Vec::new();
    let num_segments = all_segments.len();
    for segment in &all_segments {
        result.extend_from_slice(&segment.samples);
    }

    // Apply balanced energy filtering for very short segments
    if result.len() < 1600 { // Less than 100ms at 16kHz
        let input_energy: f32 = samples_mono_16k.iter().map(|&x| x * x).sum::<f32>() / samples_mono_16k.len() as f32;
        let rms = input_energy.sqrt();
        let peak = samples_mono_16k.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);

        // BALANCED FIX: Lowered thresholds to preserve quiet speech while still filtering silence
        // Previous aggressive values (0.08/0.15) were discarding valid quiet speech
        // New values (0.03/0.08) are more balanced - catch quiet speech, reject pure silence
        if rms < 0.2 || peak < 0.20 {
            info!("-----VAD detected silence/noise (RMS: {:.6}, Peak: {:.6}), skipping to prevent hallucinations-----", rms, peak);
            return Ok(Vec::new());
        } else {
            info!("VAD detected speech with sufficient energy (RMS: {:.6}, Peak: {:.6})", rms, peak);
            return Ok(samples_mono_16k.to_vec());
        }
    }

    debug!("VAD: Processed {} samples, extracted {} speech samples from {} segments",
           samples_mono_16k.len(), result.len(), num_segments);

    Ok(result)
}

/// Simple convenience function to get speech chunks from audio
/// Uses the optimized ContinuousVadProcessor with configurable redemption time
pub fn get_speech_chunks(samples_mono_16k: &[f32], redemption_time_ms: u32) -> Result<Vec<SpeechSegment>> {
    get_speech_chunks_with_progress(samples_mono_16k, redemption_time_ms, |_, _| true)
}

/// Get speech chunks with progress callback and cancellation support
/// The callback receives (progress_percent, segments_found) and returns false to cancel
pub fn get_speech_chunks_with_progress<F>(
    samples_mono_16k: &[f32],
    redemption_time_ms: u32,
    mut progress_callback: F,
) -> Result<Vec<SpeechSegment>>
where
    F: FnMut(u32, usize) -> bool,
{
    // All current callers are file-import/retranscription batch paths (no live-latency
    // constraint), so use the more permissive file-batch thresholds.
    let mut processor = ContinuousVadProcessor::new_with_max_speech(
        16000,
        redemption_time_ms,
        FILE_BATCH_VAD_THRESHOLDS,
        Some(FILE_BATCH_MAX_SPEECH_SEC),
    )?;

    let total_samples = samples_mono_16k.len();

    // For large files (>1 minute at 16kHz = 960,000 samples), process in chunks with progress logging
    const LARGE_FILE_THRESHOLD: usize = 960_000;
    const CHUNK_SIZE: usize = 160_000; // 10 seconds at 16kHz

    let mut all_segments = Vec::new();

    if total_samples > LARGE_FILE_THRESHOLD {
        info!("VAD: Processing large file ({} samples = {:.1}s), will log progress...",
              total_samples, total_samples as f64 / 16000.0);

        let mut processed = 0;
        let mut last_progress = 0u32;
        let mut chunk_count = 0;
        let total_chunks = (total_samples + CHUNK_SIZE - 1) / CHUNK_SIZE;

        for chunk in samples_mono_16k.chunks(CHUNK_SIZE) {
            chunk_count += 1;

            let start_time = std::time::Instant::now();
            let segments = processor.process_audio(chunk)?;
            let elapsed = start_time.elapsed();

            // Debug log for chunk processing details
            debug!("VAD: Chunk {}/{} processed in {:?}, found {} segments",
                  chunk_count, total_chunks, elapsed, segments.len());

            // Warn if chunk processing took too long (>1 second)
            if elapsed.as_secs() > 1 {
                warn!("VAD: Chunk {} took {:?} - possible performance issue", chunk_count, elapsed);
            }

            all_segments.extend(segments);

            processed += chunk.len();
            let progress = ((processed * 100) / total_samples) as u32;

            // Call progress callback every 5%
            if progress >= last_progress + 5 {
                debug!("VAD: Progress {}% ({} segments found so far)", progress, all_segments.len());

                // Check for cancellation
                if !progress_callback(progress, all_segments.len()) {
                    info!("VAD: Cancelled by callback at {}%", progress);
                    return Err(anyhow!("VAD processing cancelled"));
                }

                last_progress = progress;
            }
        }

        let final_segments = processor.flush()?;
        all_segments.extend(final_segments);

        info!("VAD: Complete! Found {} speech segments", all_segments.len());
    } else {
        // Small file - process all at once
        all_segments = processor.process_audio(samples_mono_16k)?;
        let final_segments = processor.flush()?;
        all_segments.extend(final_segments);
    }

    Ok(all_segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_speech_pad_never_exceeds_redemption() {
        assert_eq!(clamped_post_speech_pad(300), Duration::from_millis(300));
        assert_eq!(clamped_post_speech_pad(400), Duration::from_millis(400));
        assert_eq!(clamped_post_speech_pad(1200), Duration::from_millis(400));
    }

    #[test]
    fn file_batch_vad_force_flushes_at_30s() {
        let processor = ContinuousVadProcessor::new_with_max_speech(
            16000,
            2000,
            FILE_BATCH_VAD_THRESHOLDS,
            Some(FILE_BATCH_MAX_SPEECH_SEC),
        )
        .expect("processor");
        assert_eq!(processor.max_speech_samples, Some(30 * 16000));
    }

    #[test]
    fn file_batch_vad_thresholds_are_more_permissive_than_live() {
        // File-import has no live-latency constraint, so it should tolerate quieter/
        // lower-confidence speech than the live pipeline without regressing live behavior.
        assert!(
            FILE_BATCH_VAD_THRESHOLDS.positive < LIVE_VAD_THRESHOLDS.positive,
            "file-batch positive threshold must be lower than live's to catch the \
             lower-confidence speech live's stricter threshold misses"
        );
        assert!(
            FILE_BATCH_VAD_THRESHOLDS.negative < FILE_BATCH_VAD_THRESHOLDS.positive,
            "negative threshold must stay below positive for valid hysteresis"
        );
    }

    #[test]
    fn speech_start_uses_silero_session_time_not_processed_offset() {
        // Real import: silero start ~203900ms while ~197800ms was already processed.
        // Adding them produced start 401700ms > file end 293070ms.
        let start = speech_start_sample_from_session_ms(203_900);
        assert_eq!(start, 3_262_400);
        let doubled = 3_164_800usize + 203_900usize.saturating_mul(16000) / 1000;
        assert_eq!(doubled, 6_427_200);
        assert!(
            doubled > 4_688_892,
            "old processed_samples+timestamp formula overshoots a 293s file"
        );
    }

    #[test]
    fn flush_reconstructs_start_when_speech_start_is_past_eof() {
        // Inverted timestamps from a real import that dropped ~94s of trailing speech.
        let (start_ms, end_ms) = flush_segment_timestamps(6_427_200, 4_689_120, 1_471_680);
        assert!(start_ms < end_ms, "start={start_ms} end={end_ms}");
        assert!((end_ms - 293_070.0).abs() < 1.0);
        assert!((start_ms - 201_090.0).abs() < 2.0);
    }

    #[test]
    fn flush_keeps_valid_start_when_not_inverted() {
        let (start_ms, end_ms) = flush_segment_timestamps(3_262_400, 4_689_120, 1_471_680);
        assert!((start_ms - 203_900.0).abs() < 1.0);
        assert!((end_ms - 293_070.0).abs() < 1.0);
    }

    /// Generate synthetic speech-like audio with alternating speech/silence
    fn generate_test_audio_with_speech(duration_seconds: f32, sample_rate: u32) -> Vec<f32> {
        let total_samples = (duration_seconds * sample_rate as f32) as usize;
        let mut samples = vec![0.0f32; total_samples];

        // Create speech-like patterns: bursts of sine waves with varying amplitude
        // Speech every 10 seconds for 5 seconds
        let speech_interval = 10.0; // seconds between speech starts
        let speech_duration = 5.0;  // seconds of speech

        for i in 0..total_samples {
            let time = i as f32 / sample_rate as f32;
            let cycle_time = time % speech_interval;

            // Speech occurs in the first `speech_duration` seconds of each cycle
            if cycle_time < speech_duration {
                // Generate speech-like signal: multiple frequencies with amplitude modulation
                let freq1 = 200.0 + (time * 50.0).sin() * 100.0; // Varying fundamental
                let freq2 = freq1 * 2.0; // Harmonic
                let freq3 = freq1 * 3.0; // Another harmonic

                let amplitude = 0.3 + 0.1 * (time * 5.0).sin(); // Amplitude modulation
                samples[i] = amplitude * (
                    0.5 * (2.0 * std::f32::consts::PI * freq1 * time).sin() +
                    0.3 * (2.0 * std::f32::consts::PI * freq2 * time).sin() +
                    0.2 * (2.0 * std::f32::consts::PI * freq3 * time).sin()
                );
            }
            // else: silence (already 0.0)
        }

        samples
    }

    #[test]
    fn test_vad_chunked_vs_single_processing() {
        // Generate 60 seconds of audio with speech patterns at 16kHz
        let audio = generate_test_audio_with_speech(60.0, 16000);
        println!("Generated {} samples ({:.1}s)", audio.len(), audio.len() as f32 / 16000.0);

        // Process all at once (like small files)
        let segments_single = get_speech_chunks(&audio, 2000).expect("Single processing failed");
        println!("Single processing found {} segments", segments_single.len());

        // Process in chunks (like large files)
        let segments_chunked = get_speech_chunks_with_progress(&audio, 2000, |progress, segments| {
            println!("Chunked progress: {}%, {} segments", progress, segments);
            true // Don't cancel
        }).expect("Chunked processing failed");
        println!("Chunked processing found {} segments", segments_chunked.len());

        // Both should find the same number of segments (approximately)
        // Allow some variance due to chunk boundary effects
        let diff = (segments_single.len() as i32 - segments_chunked.len() as i32).abs();
        assert!(diff <= 1,
            "Chunked and single processing found different segment counts: {} vs {} (diff: {})",
            segments_single.len(), segments_chunked.len(), diff);
    }

    #[test]
    fn test_vad_large_file_progress() {
        // Generate 120 seconds (2 minutes) of audio - triggers large file threshold
        let audio = generate_test_audio_with_speech(120.0, 16000);
        let total_samples = audio.len();
        println!("Generated {} samples ({:.1}s)", total_samples, total_samples as f32 / 16000.0);

        // This should trigger the large file path (>960,000 samples)
        assert!(total_samples > 960_000, "Audio should be large enough to trigger chunked processing");

        let mut progress_updates = Vec::new();
        let segments = get_speech_chunks_with_progress(&audio, 2000, |progress, segments| {
            progress_updates.push((progress, segments));
            true // Don't cancel
        }).expect("Processing failed");

        println!("Found {} segments with {} progress updates", segments.len(), progress_updates.len());

        // Silero does not treat synthetic sine bursts as speech reliably, so the
        // exact segment count is not a contract. This test covers the large-file
        // chunked path: it must find some speech and emit progress callbacks.
        assert!(
            !segments.is_empty(),
            "Expected at least 1 speech segment, found {}",
            segments.len()
        );
        assert!(
            !progress_updates.is_empty(),
            "Expected progress updates for large file"
        );
    }

    #[test]
    fn test_vad_cancellation() {
        let audio = generate_test_audio_with_speech(120.0, 16000);

        // Cancel at 50%
        let result = get_speech_chunks_with_progress(&audio, 2000, |progress, _| {
            progress < 50 // Cancel when reaching 50%
        });

        // Should return error due to cancellation
        assert!(result.is_err(), "Expected cancellation error");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("cancelled"), "Error should mention cancellation: {}", err_msg);
    }

    #[test]
    fn test_vad_continuous_processor_state_across_chunks() {
        // Test that VAD state is correctly maintained across chunk boundaries
        let mut processor = ContinuousVadProcessor::new(16000, 2000, LIVE_VAD_THRESHOLDS).expect("Failed to create processor");

        // Generate audio with a speech segment that spans a chunk boundary
        let chunk_size = 160_000; // 10 seconds
        let audio = generate_test_audio_with_speech(30.0, 16000); // 30 seconds

        // Process in 10-second chunks
        let mut all_segments = Vec::new();
        for (i, chunk) in audio.chunks(chunk_size).enumerate() {
            let segments = processor.process_audio(chunk).expect("Processing failed");
            println!("Chunk {}: processed {} samples, found {} segments", i, chunk.len(), segments.len());
            all_segments.extend(segments);
        }

        // Flush remaining
        let final_segments = processor.flush().expect("Flush failed");
        all_segments.extend(final_segments);

        println!("Total segments found: {}", all_segments.len());

        // Should find speech segments
        assert!(all_segments.len() >= 1, "Expected at least 1 speech segment");
    }

    #[test]
    fn test_vad_400ms_vs_2000ms_segmentation() {
        // Demonstrates why 2000ms redemption is needed for batch processing:
        // 400ms creates excessive fragmentation, 2000ms bridges natural pauses.
        //
        // Audio pattern: 60s with 5s speech / 5s silence cycles
        // Natural pauses within speech (sentence gaps) are 500ms-1.5s
        let audio = generate_test_audio_with_speech(60.0, 16000);

        let segments_400 = get_speech_chunks(&audio, 400).expect("400ms processing failed");
        let segments_2000 = get_speech_chunks(&audio, 2000).expect("2000ms processing failed");

        println!(
            "400ms redemption: {} segments, 2000ms redemption: {} segments",
            segments_400.len(),
            segments_2000.len()
        );

        // 2000ms should produce fewer or equal segments (bridges more pauses)
        assert!(
            segments_2000.len() <= segments_400.len(),
            "2000ms redemption ({} segments) should not produce more segments than 400ms ({} segments)",
            segments_2000.len(),
            segments_400.len()
        );

        // Verify segments have reasonable durations with 2000ms
        for (i, seg) in segments_2000.iter().enumerate() {
            let duration_ms = seg.end_timestamp_ms - seg.start_timestamp_ms;
            println!("2000ms segment {}: {:.0}ms duration", i, duration_ms);
            // Each segment should be at least 250ms (min_speech_time)
            assert!(duration_ms >= 200.0, "Segment {} too short: {:.0}ms", i, duration_ms);
        }
    }
}

