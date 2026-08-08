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
use crate::audio::chunk_word_stitch::{
    offset_rover_words, stitch_word_chunks, TimedWord,
};
use crate::audio::sentence_segment::finalize_rover_word_timeline;
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

/// Merges two index-tagged result groups back into original order, keeping each item's
/// original index instead of discarding it. `stitch_overlapping_raw_results` needs the
/// index to look up `leading_context_samples[index]` after reordering —
/// `transcribe_sequential`/`transcribe_parallel` can silently skip segments (empty ASR
/// text, too-short audio), so position-in-the-final-list is not a reliable stand-in for
/// original segment index.
fn merge_indexed_keep_index<T>(mut a: Vec<(usize, T)>, mut b: Vec<(usize, T)>) -> Vec<(usize, T)> {
    a.append(&mut b);
    a.sort_by_key(|(i, _)| *i);
    a
}

/// Vietnamese speech runs roughly 2-4 words/second; this generously over-estimates how
/// many words could fall within the 1-second overlap window `expand_segments_with_overlap`
/// (`audio/common.rs`) uses, so the search below never misses a genuine match while
/// staying cheap (at most `MAX_OVERLAP_WORDS_TO_CHECK^2` word comparisons per boundary).
const MAX_OVERLAP_WORDS_TO_CHECK: usize = 12;

/// Finds the longest `k` (up to `max_words`) such that the last `k` (normalized) words of
/// `prev_text` exactly equal the first `k` (normalized) words of `next_text`, and returns
/// `next_text` with those `k` words dropped from the front.
///
/// This de-duplicates the shared audio region a no-silence chunk split intentionally
/// decodes twice — once as trailing context for one chunk, once as leading context for
/// the next — so a mid-word split doesn't truncate a word in the final transcript. If no
/// matching `k > 0` exists (the two chunks decoded the shared audio differently),
/// `next_text` is returned unchanged: the worst case is the same duplicated-phrase
/// behavior this replaces, never a worse outcome.
fn trim_overlap_prefix(prev_text: &str, next_text: &str, max_words: usize) -> String {
    let prev_words: Vec<&str> = prev_text.split_whitespace().collect();
    let next_words: Vec<&str> = next_text.split_whitespace().collect();
    let max_k = max_words.min(prev_words.len()).min(next_words.len());

    for k in (1..=max_k).rev() {
        let prev_tail = &prev_words[prev_words.len() - k..];
        let next_head = &next_words[..k];
        let all_match = prev_tail.iter().zip(next_head.iter()).all(|(a, b)| {
            crate::rover_engine::normalize::normalize_word(a)
                == crate::rover_engine::normalize::normalize_word(b)
        });
        if all_match {
            return next_words[k..].join(" ");
        }
    }
    next_text.to_string()
}

/// Walks `raw_results` and trims duplicate overlap prefix from each segment that
/// was decoded with leading context. Each chunk stays a separate segment (no merge).
fn stitch_overlapping_raw_results(
    raw_results: Vec<(usize, (String, f64, f64))>,
    leading_context_samples: &[usize],
) -> Vec<(String, f64, f64)> {
    let mut out: Vec<(String, f64, f64)> = Vec::with_capacity(raw_results.len());
    for (index, (text, start_ms, end_ms)) in raw_results {
        let has_leading_context = leading_context_samples.get(index).copied().unwrap_or(0) > 0;
        let text = match (has_leading_context, out.last()) {
            (true, Some((prev_text, _, _))) => {
                trim_overlap_prefix(prev_text, &text, MAX_OVERLAP_WORDS_TO_CHECK)
            }
            _ => text,
        };
        if !text.trim().is_empty() {
            out.push((text, start_ms, end_ms));
        }
    }
    out
}

fn raw_timed_results_to_segments(raw_results: Vec<(String, f64, f64)>) -> Vec<TranscriptSegment> {
    raw_results
        .into_iter()
        .filter(|(text, _, _)| !text.trim().is_empty())
        .map(|(text, start_ms, end_ms)| {
            let start_sec = start_ms / 1000.0;
            let end_sec = end_ms / 1000.0;
            TranscriptSegment {
                id: format!("transcript-{}", uuid::Uuid::new_v4()),
                text,
                timestamp: chrono::Utc::now().to_rfc3339(),
                audio_start_time: Some(start_sec),
                audio_end_time: Some(end_sec),
                duration: Some(end_sec - start_sec),
            }
        })
        .collect()
}

/// Runs CAPU over the full list of raw ASR results, batching consecutive segments up to
/// `CAPU_BATCH_WORD_BUDGET` words per call (no debounce timer needed — unlike the live
/// path, this list is already complete). `raw_results` is `(text, start_ms, end_ms)`
/// tuples in original chronological order.
fn finalize_with_capu(raw_results: Vec<(String, f64, f64)>) -> Vec<TranscriptSegment> {
    let mut batcher = CapuBatcher::new();
    let mut finalized_segments = Vec::new();
    let mut normalize_sec = 0.0f64;
    let mut capu_sec = 0.0f64;

    for (i, (text, start_ms, end_ms)) in raw_results.into_iter().enumerate() {
        let normalize_start = std::time::Instant::now();
        let normalized_text = crate::audio::post_asr::normalize_asr_text(&text);
        normalize_sec += normalize_start.elapsed().as_secs_f64();
        batcher.push(PendingSegment {
            source_id: i as u64,
            raw_text: normalized_text,
            audio_start_time: start_ms / 1000.0,
            audio_end_time: end_ms / 1000.0,
        });

        if batcher.should_flush(crate::config::CAPU_BATCH_WORD_BUDGET) {
            let capu_start = std::time::Instant::now();
            flush_into(&mut batcher, &mut finalized_segments);
            capu_sec += capu_start.elapsed().as_secs_f64();
        }
    }
    if !batcher.is_empty() {
        let capu_start = std::time::Instant::now();
        flush_into(&mut batcher, &mut finalized_segments);
        capu_sec += capu_start.elapsed().as_secs_f64();
    }

    log::info!("[BENCHMARK] stage=lowercase_only duration_sec={:.3}", normalize_sec);
    log::info!("[BENCHMARK] stage=capu_only duration_sec={:.3}", capu_sec);

    finalized_segments
}

/// Flushes whatever `batcher` has pending into `out` as one `TranscriptSegment`, if
/// anything was pending. Uses `flush_with_fallback` so a batch is never silently lost
/// even if the CAPU model isn't loaded (e.g. not yet downloaded) — falls back to the
/// raw (lowercased-only) text in that case.
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

fn rover_word_base_sec(
    segment: &SpeechSegment,
    index: usize,
    leading_context_samples: &[usize],
) -> f64 {
    let overlap_sec = leading_context_samples.get(index).copied().unwrap_or(0) as f64 / 16000.0;
    segment.start_timestamp_ms / 1000.0 - overlap_sec
}

async fn decode_rover_words_for_segment(
    rover: &Arc<TokioMutex<RoverDecoder>>,
    segment: &SpeechSegment,
    index: usize,
    leading_context_samples: &[usize],
) -> Result<Vec<TimedWord>> {
    let samples = segment.samples.clone();
    let base_sec = rover_word_base_sec(segment, index, leading_context_samples);
    tokio::task::block_in_place(|| {
        let mut guard = rover.blocking_lock();
        guard.decode(&samples, 16000.0)
    })
    .map(|r| offset_rover_words(&r.words, base_sec))
    .map_err(|e| anyhow!("ROVER word decode failed on segment {}: {}", index, e))
}

async fn transcribe_sequential_rover_words(
    segments: Vec<SpeechSegment>,
    rover: &Arc<TokioMutex<RoverDecoder>>,
    on_progress: &mut impl FnMut(usize, usize),
    is_cancelled: &impl Fn() -> bool,
    leading_context_samples: &[usize],
) -> Result<Vec<(usize, Vec<TimedWord>)>> {
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
        let words = decode_rover_words_for_segment(rover, &segment, i, leading_context_samples).await?;
        if !words.is_empty() {
            results.push((i, words));
        }
    }
    on_progress(total, total);
    Ok(results)
}

async fn run_worker_rover_words(
    mut worker: RoverDecoder,
    indexed_segments: Vec<(usize, SpeechSegment)>,
    leading_context_samples: Arc<Vec<usize>>,
    is_cancelled: impl Fn() -> bool,
    done_counter: Option<Arc<AtomicUsize>>,
) -> Result<Vec<(usize, Vec<TimedWord>)>> {
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
        let base_sec = rover_word_base_sec(&segment, i, &leading_context_samples);
        let samples = segment.samples.clone();
        let words = tokio::task::block_in_place(|| worker.decode(&samples, 16000.0))
            .map(|r| offset_rover_words(&r.words, base_sec))
            .map_err(|e| anyhow!("ROVER transcription failed on segment {}: {}", i, e))?;
        if !words.is_empty() {
            results.push((i, words));
        }
        if let Some(c) = &done_counter {
            c.fetch_add(1, Ordering::Relaxed);
        }
    }
    Ok(results)
}

async fn transcribe_parallel_rover_words<R: Runtime>(
    app: &AppHandle<R>,
    segments: Vec<SpeechSegment>,
    _rover: &Arc<TokioMutex<RoverDecoder>>,
    physical_cores: usize,
    on_progress: &mut impl FnMut(usize, usize),
    is_cancelled: impl Fn() -> bool + Send + Sync + Clone + 'static,
    leading_context_samples: &[usize],
) -> Result<Vec<(usize, Vec<TimedWord>)>> {
    let total = segments.len();
    let (even, odd) = split_even_odd(segments);
    on_progress(0, total);

    let (worker_a, worker_b) =
        build_worker_pair(app, &PrimaryEngine::Rover(_rover.clone()), physical_cores).await?;
    let (Worker::Rover(worker_a), Worker::Rover(worker_b)) = (worker_a, worker_b) else {
        return Err(anyhow!("Expected ROVER workers"));
    };

    let ctx = Arc::new(leading_context_samples.to_vec());
    let done = Arc::new(AtomicUsize::new(0));
    let done_a = done.clone();
    let done_b = done.clone();
    let ctx_a = ctx.clone();
    let ctx_b = ctx.clone();

    let handle_a = tokio::spawn(run_worker_rover_words(
        worker_a,
        even,
        ctx_a,
        is_cancelled.clone(),
        Some(done_a),
    ));
    let handle_b = tokio::spawn(run_worker_rover_words(
        worker_b,
        odd,
        ctx_b,
        is_cancelled,
        Some(done_b),
    ));

    let mut progress_interval = tokio::time::interval(tokio::time::Duration::from_millis(300));
    loop {
        progress_interval.tick().await;
        let d = done.load(Ordering::Relaxed);
        on_progress(d.min(total), total);
        if handle_a.is_finished() && handle_b.is_finished() {
            break;
        }
    }

    let results_a = handle_a
        .await
        .map_err(|e| anyhow!("ROVER worker A task panicked: {}", e))??;
    let results_b = handle_b
        .await
        .map_err(|e| anyhow!("ROVER worker B task panicked: {}", e))??;
    on_progress(total, total);
    Ok(merge_indexed_keep_index(results_a, results_b))
}

/// Sequential fallback: reuses the already-loaded shared `primary` engine directly (no
/// extra model load), processing segments one at a time — identical behavior to the
/// pre-existing `import.rs`/`retranscription.rs` for-loops this replaces. Each result
/// keeps its original segment index (see `merge_indexed_keep_index`'s doc comment for
/// why: segments can be silently skipped here, so position alone isn't a stable index).
async fn transcribe_sequential(
    segments: Vec<SpeechSegment>,
    primary: &PrimaryEngine,
    on_progress: &mut impl FnMut(usize, usize),
    is_cancelled: &impl Fn() -> bool,
) -> Result<Vec<(usize, (String, f64, f64))>> {
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
            results.push((i, (text, segment.start_timestamp_ms, segment.end_timestamp_ms)));
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
/// merges results back into original chronological order (indices kept — see
/// `merge_indexed_keep_index`).
async fn transcribe_parallel<R: Runtime>(
    app: &AppHandle<R>,
    segments: Vec<SpeechSegment>,
    primary: &PrimaryEngine,
    physical_cores: usize,
    on_progress: &mut impl FnMut(usize, usize),
    is_cancelled: impl Fn() -> bool + Send + Sync + Clone + 'static,
) -> Result<Vec<(usize, (String, f64, f64))>> {
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

    Ok(merge_indexed_keep_index(results_a, results_b))
}

/// Transcribes `segments` (already VAD-detected and silence-split by the caller),
/// parallelizing across 2 workers when there's enough work and CPU (see
/// `should_parallelize`), then stitches any overlap-split boundaries
/// (`leading_context_samples`, from `audio::common::expand_segments_with_overlap`) and
/// batches the result through CAPU once per ~200-word group instead of once per tiny
/// segment. Returns finished, punctuated `TranscriptSegment`s ready to save to the
/// database.
///
/// `leading_context_samples` must be index-aligned with `segments` (same length); pass
/// an all-zero `Vec` (or reuse `audio::common::expand_segments_at_silence`'s plain
/// output with a zero-filled vec) if the caller didn't split with overlap.
pub async fn batch_transcribe<R: Runtime>(
    app: &AppHandle<R>,
    segments: Vec<SpeechSegment>,
    leading_context_samples: Vec<usize>,
    primary: PrimaryEngine,
    mut on_progress: impl FnMut(usize, usize),
    is_cancelled: impl Fn() -> bool + Send + Sync + Clone + 'static,
) -> Result<Vec<TranscriptSegment>> {
    let (physical_cores, _) = detect_cpu_topology();
    let total = segments.len();

    if matches!(&primary, PrimaryEngine::Rover(_)) {
        let rover = match &primary {
            PrimaryEngine::Rover(r) => r.clone(),
            _ => unreachable!(),
        };
        let asr_start = std::time::Instant::now();
        let indexed_word_chunks = if should_parallelize(total, physical_cores) {
            transcribe_parallel_rover_words(
                app,
                segments,
                &rover,
                physical_cores,
                &mut on_progress,
                is_cancelled.clone(),
                &leading_context_samples,
            )
            .await?
        } else {
            transcribe_sequential_rover_words(
                segments,
                &rover,
                &mut on_progress,
                &is_cancelled,
                &leading_context_samples,
            )
            .await?
        };
        log::info!(
            "[BENCHMARK] stage=asr_inference_rover duration_sec={:.3}",
            asr_start.elapsed().as_secs_f64()
        );
        let merged_words = stitch_word_chunks(indexed_word_chunks, &leading_context_samples);
        let finalize_start = std::time::Instant::now();
        let engine_arc = crate::capu_engine::commands::get_engine_arc();
        let raw_results = if let Some(arc) = &engine_arc {
            let mut engine = arc.lock().unwrap();
            finalize_rover_word_timeline(&merged_words, Some(&mut engine))
        } else {
            finalize_rover_word_timeline(&merged_words, None)
        };
        log::info!(
            "[BENCHMARK] stage=finalize_rover_capu duration_sec={:.3}",
            finalize_start.elapsed().as_secs_f64()
        );
        return Ok(raw_timed_results_to_segments(raw_results));
    }

    let asr_start = std::time::Instant::now();
    let indexed_raw_results = if should_parallelize(total, physical_cores) {
        transcribe_parallel(app, segments, &primary, physical_cores, &mut on_progress, is_cancelled).await?
    } else {
        transcribe_sequential(segments, &primary, &mut on_progress, &is_cancelled).await?
    };
    log::info!(
        "[BENCHMARK] stage=asr_inference duration_sec={:.3}",
        asr_start.elapsed().as_secs_f64()
    );

    let raw_results = stitch_overlapping_raw_results(indexed_raw_results, &leading_context_samples);

    let finalize_start = std::time::Instant::now();
    let result = finalize_with_capu(raw_results);
    log::info!(
        "[BENCHMARK] stage=finalize_capu_total duration_sec={:.3}",
        finalize_start.elapsed().as_secs_f64()
    );
    Ok(result)
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
    fn merge_indexed_keep_index_restores_original_order_and_indices() {
        let a = vec![(0, "a"), (2, "c"), (4, "e")];
        let b = vec![(1, "b"), (3, "d")];
        assert_eq!(
            merge_indexed_keep_index(b, a),
            vec![(0, "a"), (1, "b"), (2, "c"), (3, "d"), (4, "e")]
        );
    }

    #[test]
    fn trim_overlap_prefix_drops_the_longest_matching_run() {
        let prev = "hôm nay chúng ta họp về dự án mới";
        let next = "họp về dự án mới rất là quan trọng";
        assert_eq!(
            trim_overlap_prefix(prev, next, MAX_OVERLAP_WORDS_TO_CHECK),
            "rất là quan trọng"
        );
    }

    #[test]
    fn trim_overlap_prefix_is_case_and_diacritic_form_insensitive() {
        // Raw ASR output is all-uppercase; must still match a lowercase prev_text.
        let prev = "xin chào các bạn";
        let next = "CÁC BẠN HÔM NAY KHỎE KHÔNG";
        assert_eq!(trim_overlap_prefix(prev, next, 10), "HÔM NAY KHỎE KHÔNG");
    }

    #[test]
    fn trim_overlap_prefix_returns_next_unchanged_when_no_overlap_matches() {
        let prev = "một hai ba";
        let next = "hoàn toàn khác nhau";
        assert_eq!(trim_overlap_prefix(prev, next, MAX_OVERLAP_WORDS_TO_CHECK), next);
    }

    #[test]
    fn trim_overlap_prefix_does_not_detect_a_match_longer_than_max_words() {
        // The true overlap is 3 words ("a b c"): prev's last 3 words equal next's first
        // 3. With enough budget, it's found and trimmed.
        let prev = "x y a b c";
        let next = "a b c d";
        assert_eq!(trim_overlap_prefix(prev, next, 3), "d");

        // Capping the search below the true overlap length (2 < 3) means neither that
        // 3-word run nor any smaller k happens to align at prev's absolute tail / next's
        // absolute head (a k=2 check compares prev's actual last 2 words, "b c", against
        // next's actual first 2, "a b" — a different alignment, not a subset of the
        // k=3 match) — so nothing matches, and the safe fallback (unchanged) applies.
        assert_eq!(trim_overlap_prefix(prev, next, 2), next);
    }

    #[test]
    fn stitch_overlapping_raw_results_trims_prefix_but_keeps_segments_separate() {
        let indexed = vec![
            (0, ("xin chào các bạn".to_string(), 0.0, 1000.0)),
            (1, ("các bạn hôm nay khỏe không".to_string(), 900.0, 2000.0)),
            (2, ("một câu hoàn toàn mới".to_string(), 2000.0, 3000.0)),
        ];
        let leading_context_samples = vec![0, 16000, 0];

        let stitched = stitch_overlapping_raw_results(indexed, &leading_context_samples);

        assert_eq!(stitched.len(), 3);
        assert_eq!(stitched[0].0, "xin chào các bạn");
        assert_eq!(stitched[1].0, "hôm nay khỏe không");
        assert_eq!(stitched[2].0, "một câu hoàn toàn mới");
    }

    #[test]
    fn stitch_overlapping_raw_results_leaves_text_unchanged_when_no_segment_has_overlap() {
        let indexed = vec![
            (0, ("một".to_string(), 0.0, 500.0)),
            (1, ("hai".to_string(), 500.0, 1000.0)),
        ];
        let stitched = stitch_overlapping_raw_results(indexed, &[0, 0]);
        assert_eq!(stitched[0].0, "một");
        assert_eq!(stitched[1].0, "hai");
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
    fn merge_indexed_keep_index_handles_one_side_empty() {
        let a: Vec<(usize, &str)> = vec![(0, "only")];
        let b: Vec<(usize, &str)> = vec![];
        assert_eq!(merge_indexed_keep_index(a, b), vec![(0, "only")]);
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
