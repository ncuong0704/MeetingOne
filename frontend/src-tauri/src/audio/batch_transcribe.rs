// src/audio/batch_transcribe.rs
//
// Shared batch-transcription pipeline for the file-based paths (`import.rs`,
// `retranscription.rs`): parallelizes ASR across 2 workers when there's enough work
// and enough CPU, then runs CAPU once per ~200-word batch via `CapuBatcher` instead of
// once per tiny VAD segment. See
// docs/superpowers/specs/2026-08-04-asr-pipeline-performance-design.md, section B.

use crate::api::TranscriptSegment;
use crate::asr_engine::engine::AsrEngine;
use crate::asr_engine::model_family::{ModelFamily, ModelVariant};
use crate::asr_engine::thread_budget::{asr_thread_budget, DecodeConcurrency};
use crate::audio::vad::SpeechSegment;
use crate::capu_engine::batch::{CapuBatcher, PendingSegment};
use crate::capu_engine::cpu_topology::detect_cpu_topology;
use crate::rover_engine::engine::RoverDecoder;
use anyhow::anyhow;
use anyhow::Result;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Runtime};
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

enum Worker {
    Single(AsrEngine),
    Rover(RoverDecoder),
}

/// Transcribes one segment with whichever engine `primary` wraps.
async fn transcribe_one(primary: &PrimaryEngine, samples: &[f32]) -> Result<String> {
    match primary {
        PrimaryEngine::Single(engine) => engine
            .transcribe_audio(samples.to_vec())
            .await
            .map_err(|e| anyhow!("ASR transcription failed: {}", e)),
        PrimaryEngine::Rover(rover) => {
            let rover = rover.clone();
            let samples = samples.to_vec();
            tokio::task::block_in_place(move || {
                let mut guard = rover.blocking_lock();
                guard.decode(&samples, 16000.0)
            })
            .map(|r| r.text)
            .map_err(|e| anyhow!("ROVER transcription failed: {}", e))
        }
    }
}

/// Sequential fallback: reuses the already-loaded shared `primary` engine directly (no
/// extra model load), processing segments one at a time — identical behavior to the
/// pre-existing `import.rs`/`retranscription.rs` for-loops this replaces.
async fn transcribe_sequential(
    segments: Vec<SpeechSegment>,
    primary: &PrimaryEngine,
    on_progress: &mut impl FnMut(usize, usize),
    is_cancelled: &impl Fn() -> bool,
) -> Result<Vec<(String, f64, f64)>> {
    let total = segments.len();
    let mut results = Vec::with_capacity(total);
    for (i, segment) in segments.into_iter().enumerate() {
        if is_cancelled() {
            return Err(anyhow!("Cancelled"));
        }
        on_progress(i, total);
        if segment.samples.len() < 1600 {
            continue;
        }
        let text = transcribe_one(primary, &segment.samples).await?;
        if !text.trim().is_empty() {
            results.push((text, segment.start_timestamp_ms, segment.end_timestamp_ms));
        }
    }
    on_progress(total, total);
    Ok(results)
}

/// Runs one worker's assigned (index-tagged) segments through to completion,
/// preserving each result's original index for later reordering.
async fn run_worker(
    mut worker: Worker,
    indexed_segments: Vec<(usize, SpeechSegment)>,
    is_cancelled: impl Fn() -> bool,
    done_counter: Option<Arc<AtomicUsize>>,
) -> Result<Vec<(usize, (String, f64, f64))>> {
    let mut results = Vec::with_capacity(indexed_segments.len());
    for (i, segment) in indexed_segments {
        if is_cancelled() {
            return Err(anyhow!("Cancelled"));
        }
        if segment.samples.len() < 1600 {
            if let Some(c) = &done_counter {
                c.fetch_add(1, Ordering::Relaxed);
            }
            continue;
        }
        let text = match &mut worker {
            Worker::Single(engine) => engine
                .transcribe_audio(segment.samples.clone())
                .await
                .map_err(|e| anyhow!("ASR transcription failed on segment {}: {}", i, e))?,
            Worker::Rover(rover) => {
                let samples = segment.samples.clone();
                tokio::task::block_in_place(|| rover.decode(&samples, 16000.0))
                    .map(|r| r.text)
                    .map_err(|e| anyhow!("ROVER transcription failed on segment {}: {}", i, e))?
            }
        };
        if !text.trim().is_empty() {
            results.push((i, (text, segment.start_timestamp_ms, segment.end_timestamp_ms)));
        }
        if let Some(c) = &done_counter {
            c.fetch_add(1, Ordering::Relaxed);
        }
    }
    Ok(results)
}

/// Loads one standalone `AsrEngine` instance for parallel file transcription. A plain
/// free function (not a closure) so each of the 2 call sites owns its arguments
/// outright — no shared captures, no lifetime ambiguity between the two calls.
async fn build_single_worker(
    family: ModelFamily,
    variant: ModelVariant,
    decoding_method: String,
    num_active_paths: i32,
    models_dir: std::path::PathBuf,
    threads: usize,
) -> Result<AsrEngine> {
    let fresh = AsrEngine::new();
    fresh.set_models_directory(models_dir).await;
    fresh
        .load_model(family, variant, decoding_method, num_active_paths, threads)
        .await
        .map_err(|e| anyhow!("Failed to load parallel ASR worker: {}", e))?;
    Ok(fresh)
}

/// Loads one standalone `RoverDecoder` instance (its own internal pair of decoders) for
/// parallel file transcription. A plain free function (not a closure) so each of the 2
/// call sites owns its arguments outright — no shared captures, no lifetime ambiguity
/// between the two calls, matching `build_single_worker`'s pattern above.
async fn build_rover_worker(
    enc_a: std::path::PathBuf,
    dec_a: std::path::PathBuf,
    joi_a: std::path::PathBuf,
    tok_a: std::path::PathBuf,
    enc_b: std::path::PathBuf,
    dec_b: std::path::PathBuf,
    joi_b: std::path::PathBuf,
    tok_b: std::path::PathBuf,
    threads_per_decoder: usize,
) -> Result<RoverDecoder> {
    tokio::task::block_in_place(|| {
        RoverDecoder::load(
            (&enc_a, &dec_a, &joi_a, &tok_a),
            (&enc_b, &dec_b, &joi_b, &tok_b),
            4,
            threads_per_decoder,
        )
    })
    .map_err(|e| anyhow!("Failed to load parallel ROVER worker: {}", e))
}

/// Builds 2 fresh, independent workers for parallel file transcription — NEVER reuses
/// or mutates the shared global singleton (`asr_engine::commands::ASR_ENGINE` /
/// `rover_engine::commands::ROVER_ENGINE`), so a concurrent live recording (or another
/// batch job) using that singleton is completely unaffected. The Rover branch reads the
/// already-validated family/variant config from `rover_engine::commands::ROVER_CONFIG`
/// (set by `rover_validate_model_ready`) and builds a fresh `RoverDecoder` pair per
/// worker from that same config.
async fn build_worker_pair<R: Runtime>(
    app: &AppHandle<R>,
    primary: &PrimaryEngine,
    physical_cores: usize,
) -> Result<(Worker, Worker)> {
    match primary {
        PrimaryEngine::Single(engine) => {
            let family = engine.get_current_family().await;
            let variant = engine.get_current_variant().await;
            let decoding_method = engine.get_decoding_method().await;
            let num_active_paths = engine.get_num_active_paths().await;
            let models_dir = engine.get_models_directory().await;
            let threads = asr_thread_budget(physical_cores, DecodeConcurrency::SingleFileWorker);

            let worker_a = build_single_worker(
                family,
                variant,
                decoding_method.clone(),
                num_active_paths,
                models_dir.clone(),
                threads,
            )
            .await?;
            let worker_b = build_single_worker(
                family,
                variant,
                decoding_method,
                num_active_paths,
                models_dir,
                threads,
            )
            .await?;
            Ok((Worker::Single(worker_a), Worker::Single(worker_b)))
        }
        PrimaryEngine::Rover(_) => {
            let rover_config: Option<(ModelFamily, ModelVariant, ModelFamily, ModelVariant)> =
                *crate::rover_engine::commands::ROVER_CONFIG.lock().unwrap();
            let (fa, va, fb, vb) = rover_config.ok_or_else(|| {
                anyhow!("ROVER config not set — rover_validate_model_ready must run before batch_transcribe")
            })?;
            let base = crate::asr_engine::commands::resolve_models_base_dir(app)
                .ok_or_else(|| anyhow!("Cannot resolve models directory"))?;
            let (enc_a, dec_a, joi_a, tok_a) =
                crate::rover_engine::commands::family_paths(&base, fa, va);
            let (enc_b, dec_b, joi_b, tok_b) =
                crate::rover_engine::commands::family_paths(&base, fb, vb);
            let threads_per_decoder =
                asr_thread_budget(physical_cores, DecodeConcurrency::RoverFileWorker);

            let worker_a = build_rover_worker(
                enc_a.clone(),
                dec_a.clone(),
                joi_a.clone(),
                tok_a.clone(),
                enc_b.clone(),
                dec_b.clone(),
                joi_b.clone(),
                tok_b.clone(),
                threads_per_decoder,
            )
            .await?;
            let worker_b = build_rover_worker(
                enc_a, dec_a, joi_a, tok_a, enc_b, dec_b, joi_b, tok_b, threads_per_decoder,
            )
            .await?;
            Ok((Worker::Rover(worker_a), Worker::Rover(worker_b)))
        }
    }
}

/// Parallel path: splits `segments` even/odd, builds 2 fresh workers, runs both
/// concurrently via `tokio::spawn` (so they land on separate OS threads under the
/// multi-threaded Tokio runtime — real parallelism, not just async concurrency), then
/// merges results back into original chronological order.
async fn transcribe_parallel<R: Runtime>(
    app: &AppHandle<R>,
    segments: Vec<SpeechSegment>,
    primary: &PrimaryEngine,
    physical_cores: usize,
    on_progress: &mut impl FnMut(usize, usize),
    is_cancelled: impl Fn() -> bool + Send + Sync + Clone + 'static,
) -> Result<Vec<(String, f64, f64)>> {
    let total = segments.len();
    let (even, odd) = split_even_odd(segments);

    on_progress(0, total);

    let (worker_a, worker_b) = build_worker_pair(app, primary, physical_cores).await?;

    let done = Arc::new(AtomicUsize::new(0));
    let done_a = done.clone();
    let done_b = done.clone();

    let handle_a = tokio::spawn(run_worker(
        worker_a,
        even,
        is_cancelled.clone(),
        Some(done_a),
    ));
    let handle_b = tokio::spawn(run_worker(worker_b, odd, is_cancelled, Some(done_b)));

    let mut progress_interval = tokio::time::interval(tokio::time::Duration::from_millis(300));
    loop {
        progress_interval.tick().await;
        let d = done.load(Ordering::Relaxed);
        on_progress(d.min(total), total);
        if handle_a.is_finished() && handle_b.is_finished() {
            break;
        }
    }

    let (result_a, result_b) = tokio::join!(handle_a, handle_b);

    let results_a = result_a
        .map_err(|e| anyhow!("ASR worker A task panicked: {}", e))??;
    let results_b = result_b
        .map_err(|e| anyhow!("ASR worker B task panicked: {}", e))??;
    on_progress(total, total);

    Ok(merge_indexed(results_a, results_b))
}

/// Transcribes `segments` (already VAD-detected and silence-split by the caller),
/// parallelizing across 2 workers when there's enough work and CPU (see
/// `should_parallelize`), then batches the result through CAPU once per ~200-word
/// group instead of once per tiny segment. Returns finished, punctuated
/// `TranscriptSegment`s ready to save to the database.
pub async fn batch_transcribe<R: Runtime>(
    app: &AppHandle<R>,
    segments: Vec<SpeechSegment>,
    primary: PrimaryEngine,
    mut on_progress: impl FnMut(usize, usize),
    is_cancelled: impl Fn() -> bool + Send + Sync + Clone + 'static,
) -> Result<Vec<TranscriptSegment>> {
    let (physical_cores, _) = detect_cpu_topology();
    let total = segments.len();

    let raw_results = if should_parallelize(total, physical_cores) {
        transcribe_parallel(app, segments, &primary, physical_cores, &mut on_progress, is_cancelled).await?
    } else {
        transcribe_sequential(segments, &primary, &mut on_progress, &is_cancelled).await?
    };

    Ok(finalize_with_capu(raw_results))
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
