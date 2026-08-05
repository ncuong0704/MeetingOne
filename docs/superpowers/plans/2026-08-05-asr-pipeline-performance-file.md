# Tối ưu pipeline âm thanh → transcript: Luồng File — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Song song hoá ASR (2 worker) và gộp batch CAPU cho luồng xử lý file (import audio có sẵn + transcribe lại 1 cuộc họp cũ), dùng chung 1 module `batch_transcribe` cho cả `import.rs` và `retranscription.rs` thay vì lặp lại logic tuần tự hiện có ở cả 2 nơi.

**Architecture:** Thêm module mới `audio/batch_transcribe.rs` với 1 hàm công khai `batch_transcribe(app, segments, primary_engine, on_progress) -> Vec<TranscriptSegment>`. Khi đủ điều kiện (≥4 đoạn, ≥4 core vật lý), tự dựng thêm 1 bộ engine/ROVER **độc lập, tạm thời** (không đụng tới singleton toàn cục đang dùng cho luồng live) để chạy song song 2 worker theo chỉ số chẵn/lẻ, gộp kết quả lại đúng thứ tự gốc. Sau đó dùng lại `CapuBatcher` (đã có từ plan Live) để batch CAPU cho toàn bộ transcript, không cần debounce timer vì danh sách đã hoàn chỉnh.

**Tech Stack:** Rust, Tokio (`tokio::spawn` cho 2 worker song song), sherpa-onnx (`AsrEngine`), `rnnt_decoder`/`RoverDecoder`.

**Phụ thuộc plan trước:** Yêu cầu `docs/superpowers/plans/2026-08-04-asr-pipeline-performance-live.md` đã hoàn tất (dùng `asr_engine::thread_budget::{asr_thread_budget, DecodeConcurrency}` và `capu_engine::batch::{CapuBatcher, PendingSegment, FinalizedSegment}` đã có).

**Spec liên quan:** [docs/superpowers/specs/2026-08-04-asr-pipeline-performance-design.md](../specs/2026-08-04-asr-pipeline-performance-design.md), mục "B. Luồng File".

**Quyết định phạm vi đã chốt với người dùng:** ROVER vẫn được song song hoá ở luồng file giống app tham khảo (không rơi về tuần tự khi ROVER bật) — mỗi trong 2 worker tự dựng 1 `RoverDecoder` độc lập.

**Lệnh build/test dùng xuyên suốt plan** (chạy từ thư mục gốc repo):
```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml --all-targets
cargo test --manifest-path frontend/src-tauri/Cargo.toml <tên_test> -- --nocapture
```

---

### Task 1: `batch_transcribe.rs` — khung module + hàm thuần (should_parallelize, split/merge theo chỉ số)

**Files:**
- Create: `frontend/src-tauri/src/audio/batch_transcribe.rs`
- Modify: `frontend/src-tauri/src/audio/mod.rs`

- [ ] **Step 1: Viết file mới với logic thuần + test trước**

```rust
// frontend/src-tauri/src/audio/batch_transcribe.rs
//
// Shared batch-transcription pipeline for the file-based paths (`import.rs`,
// `retranscription.rs`): parallelizes ASR across 2 workers when there's enough work
// and enough CPU, then runs CAPU once per ~200-word batch via `CapuBatcher` instead of
// once per tiny VAD segment. See
// docs/superpowers/specs/2026-08-04-asr-pipeline-performance-design.md, section B.

use crate::asr_engine::engine::AsrEngine;
use crate::rover_engine::engine::RoverDecoder;
use std::sync::Arc;
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
}
```

- [ ] **Step 2: Đăng ký module**

Modify `frontend/src-tauri/src/audio/mod.rs` — tìm dòng:

```rust
// Shared utilities for import and retranscription
pub(crate) mod common;
```

thay bằng:

```rust
// Shared utilities for import and retranscription
pub(crate) mod common;
pub(crate) mod batch_transcribe;
```

- [ ] **Step 3: Chạy test, xác nhận PASS**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml audio::batch_transcribe::tests -- --nocapture
```
Expected: 5 test PASS.

- [ ] **Step 4: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml --all-targets
```
Expected: biên dịch thành công (sẽ có warning "unused" cho `PrimaryEngine`/`split_even_odd`/`merge_indexed`/`should_parallelize` vì chưa được dùng ở đâu khác — bình thường, các task sau sẽ dùng).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/batch_transcribe.rs frontend/src-tauri/src/audio/mod.rs
git commit -m "feat(perf): add batch_transcribe module skeleton with pure split/merge helpers"
```

---

### Task 2: `CapuBatcher::flush_with_fallback` + gộp CAPU cho toàn bộ transcript file

**Files:**
- Modify: `frontend/src-tauri/src/capu_engine/batch.rs`
- Modify: `frontend/src-tauri/src/audio/batch_transcribe.rs`

- [ ] **Step 1: Viết test trước cho `flush_with_fallback` (fail vì hàm chưa tồn tại)**

Thêm vào `#[cfg(test)] mod tests` trong `frontend/src-tauri/src/capu_engine/batch.rs` (cùng khối test đã có từ plan Live):

```rust
    #[test]
    fn flush_with_fallback_returns_raw_joined_text_when_no_engine() {
        let mut batcher = CapuBatcher::new();
        batcher.push(pending(0, "xin chao", 0.0, 1.0));
        batcher.push(pending(1, "cac ban", 1.0, 2.0));

        let finalized = batcher
            .flush_with_fallback(None)
            .expect("batch was non-empty");
        assert_eq!(finalized.text, "xin chao cac ban");
        assert_eq!(finalized.source_ids, vec![0, 1]);
        assert_eq!(finalized.audio_start_time, 0.0);
        assert_eq!(finalized.audio_end_time, 2.0);
        assert!(batcher.is_empty());
    }

    #[test]
    fn flush_with_fallback_returns_none_when_nothing_pending() {
        let mut batcher = CapuBatcher::new();
        assert!(batcher.flush_with_fallback(None).is_none());
    }
```

- [ ] **Step 2: Chạy test, xác nhận FAIL (biên dịch lỗi)**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml capu_engine::batch::tests::flush_with_fallback
```
Expected: FAIL — `no method named \`flush_with_fallback\` found`.

- [ ] **Step 3: Thêm `flush_with_fallback` vào `CapuBatcher`**

Trong `frontend/src-tauri/src/capu_engine/batch.rs`, ngay sau hàm `flush` (kết thúc trước `}` đóng `impl CapuBatcher`), thêm:

```rust
    /// Like `flush`, but when no CAPU engine is available at all, returns the batch with
    /// its raw (un-punctuated) joined text instead of discarding it. Used by the
    /// file/batch path (`batch_transcribe.rs`), where this is the ONLY place raw text
    /// ever reaches storage — unlike the live path (`transcription/worker.rs`), which
    /// already emitted the raw text live before Stage 2 ever sees "no engine", so it
    /// safely discards there via `discard_pending` instead.
    pub fn flush_with_fallback(&mut self, engine: Option<&mut CapuEngine>) -> Option<FinalizedSegment> {
        match engine {
            Some(engine) => self.flush(engine),
            None => {
                if self.pending.is_empty() {
                    return None;
                }
                let joined = self.joined_pending_text();
                let source_ids: Vec<u64> = self.pending.iter().map(|s| s.source_id).collect();
                let audio_start_time = self.pending.first().unwrap().audio_start_time;
                let audio_end_time = self.pending.last().unwrap().audio_end_time;

                self.pending.clear();
                self.pending_word_count = 0;

                Some(FinalizedSegment {
                    text: joined,
                    audio_start_time,
                    audio_end_time,
                    source_ids,
                })
            }
        }
    }
```

- [ ] **Step 4: Chạy test, xác nhận PASS**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml capu_engine::batch::tests -- --nocapture
```
Expected: tất cả test trong `capu_engine::batch::tests` PASS (2 test mới + các test cũ từ plan Live vẫn còn nguyên).

- [ ] **Step 5: Thêm `finalize_with_capu` vào `batch_transcribe.rs`**

Trong `frontend/src-tauri/src/audio/batch_transcribe.rs`, thêm import ở đầu file (cùng khối `use` hiện có):

```rust
use crate::api::TranscriptSegment;
use crate::capu_engine::batch::{CapuBatcher, PendingSegment};
use anyhow::Result;
```

Thêm 2 hàm mới ở cuối file (trước `#[cfg(test)] mod tests`):

```rust
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
```

Thêm test cho `finalize_with_capu` trong `#[cfg(test)] mod tests` (dùng dữ liệu giả, không cần model CAPU thật — khi không có engine nào được load trong test process, `get_engine_arc()` trả về `None`, đúng nhánh fallback):

```rust
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
```

- [ ] **Step 6: Chạy test, xác nhận PASS**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml audio::batch_transcribe::tests -- --nocapture
```
Expected: tất cả test PASS (5 test cũ từ Task 1 + 2 test mới).

- [ ] **Step 7: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml --all-targets
```
Expected: biên dịch thành công.

- [ ] **Step 8: Commit**

```bash
git add frontend/src-tauri/src/capu_engine/batch.rs frontend/src-tauri/src/audio/batch_transcribe.rs
git commit -m "feat(perf): batch CAPU over the whole file transcript via CapuBatcher"
```

---

### Task 3: Song song hoá ASR đơn model (2 worker) + đường tuần tự dự phòng

**Files:**
- Modify: `frontend/src-tauri/src/audio/batch_transcribe.rs`

- [ ] **Step 1: Thêm import cần thiết**

Thêm vào đầu file, cùng khối `use` hiện có:

```rust
use crate::asr_engine::model_family::{ModelFamily, ModelVariant};
use crate::asr_engine::thread_budget::{asr_thread_budget, DecodeConcurrency};
use crate::audio::vad::SpeechSegment;
use crate::capu_engine::cpu_topology::detect_cpu_topology;
use anyhow::anyhow;
use tauri::{AppHandle, Runtime};
```

(Gộp với các `use` đã có ở Task 1/2 — không trùng lặp `anyhow::Result` đã có, chỉ thêm `anyhow` nếu chưa có sẵn để dùng `anyhow!` macro.)

- [ ] **Step 2: Thêm `Worker` enum, hàm chạy 1 worker, hàm dựng cặp worker (chỉ nhánh Single), hàm tuần tự dự phòng, và hàm công khai `batch_transcribe`**

Thêm vào cuối file (trước `finalize_with_capu`/`flush_into` đã thêm ở Task 2, và trước `#[cfg(test)] mod tests`):

```rust
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
) -> Result<Vec<(String, f64, f64)>> {
    let total = segments.len();
    let mut results = Vec::with_capacity(total);
    for (i, segment) in segments.into_iter().enumerate() {
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
) -> Result<Vec<(usize, (String, f64, f64))>> {
    let mut results = Vec::with_capacity(indexed_segments.len());
    for (i, segment) in indexed_segments {
        if segment.samples.len() < 1600 {
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

/// Builds 2 fresh, independent workers for parallel file transcription — NEVER reuses
/// or mutates the shared global singleton (`asr_engine::commands::ASR_ENGINE` /
/// `rover_engine::commands::ROVER_ENGINE`), so a concurrent live recording (or another
/// batch job) using that singleton is completely unaffected. This is the Single-model
/// branch only; the Rover branch is added in Task 4.
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
            let _ = app; // used by the Rover branch, added in Task 4
            Err(anyhow!("ROVER parallel file transcription not yet implemented (Task 4)"))
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
) -> Result<Vec<(String, f64, f64)>> {
    let total = segments.len();
    let (even, odd) = split_even_odd(segments);

    let (worker_a, worker_b) = build_worker_pair(app, primary, physical_cores).await?;

    let handle_a = tokio::spawn(run_worker(worker_a, even));
    let handle_b = tokio::spawn(run_worker(worker_b, odd));

    let results_a = handle_a
        .await
        .map_err(|e| anyhow!("ASR worker A task panicked: {}", e))??;
    on_progress(total / 2, total);
    let results_b = handle_b
        .await
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
) -> Result<Vec<TranscriptSegment>> {
    let (physical_cores, _) = detect_cpu_topology();
    let total = segments.len();

    let raw_results = if should_parallelize(total, physical_cores) {
        transcribe_parallel(app, segments, &primary, physical_cores, &mut on_progress).await?
    } else {
        transcribe_sequential(segments, &primary, &mut on_progress).await?
    };

    Ok(finalize_with_capu(raw_results))
}
```

- [ ] **Step 3: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml --all-targets
```
Expected: biên dịch thành công. `ModelFamily`/`ModelVariant` import ở Step 1 có thể chưa dùng tới (chỉ cần ở Task 4) — nếu `cargo check` báo warning "unused import", đó là bình thường, không phải lỗi; nếu báo **lỗi** biên dịch thật sự, dừng lại và báo cáo.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/audio/batch_transcribe.rs
git commit -m "feat(perf): parallelize single-model file transcription across 2 workers"
```

---

### Task 4: Song song hoá ROVER (2 worker, mỗi worker 1 cặp RoverDecoder riêng)

**Files:**
- Modify: `frontend/src-tauri/src/rover_engine/commands.rs`
- Modify: `frontend/src-tauri/src/audio/batch_transcribe.rs`

- [ ] **Step 1: Mở `family_paths` thành `pub(crate)`**

Trong `frontend/src-tauri/src/rover_engine/commands.rs`, thay:

```rust
fn family_paths(
    base: &PathBuf,
    family: ModelFamily,
    variant: ModelVariant,
) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
```

bằng:

```rust
pub(crate) fn family_paths(
    base: &PathBuf,
    family: ModelFamily,
    variant: ModelVariant,
) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
```

- [ ] **Step 2: Cài đặt nhánh ROVER trong `build_worker_pair`**

Trong `frontend/src-tauri/src/audio/batch_transcribe.rs`, thay khối:

```rust
        PrimaryEngine::Rover(_) => {
            let _ = app; // used by the Rover branch, added in Task 4
            Err(anyhow!("ROVER parallel file transcription not yet implemented (Task 4)"))
        }
```

bằng:

```rust
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

            let build_one = || {
                tokio::task::block_in_place(|| {
                    RoverDecoder::load(
                        (&enc_a, &dec_a, &joi_a, &tok_a),
                        (&enc_b, &dec_b, &joi_b, &tok_b),
                        4,
                        threads_per_decoder,
                    )
                })
                .map_err(|e| anyhow!("Failed to load parallel ROVER worker: {}", e))
            };

            let worker_a = build_one()?;
            let worker_b = build_one()?;
            Ok((Worker::Rover(worker_a), Worker::Rover(worker_b)))
        }
```

- [ ] **Step 3: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml --all-targets
```
Expected: biên dịch thành công, không còn cảnh báo "unused import" cho `ModelFamily`/`ModelVariant` (giờ đã dùng qua kiểu trả về của `ROVER_CONFIG`).

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/rover_engine/commands.rs frontend/src-tauri/src/audio/batch_transcribe.rs
git commit -m "feat(perf): parallelize ROVER file transcription across 2 workers"
```

---

### Task 5: Nối `import.rs` dùng `batch_transcribe`

**Files:**
- Modify: `frontend/src-tauri/src/audio/import.rs`

- [ ] **Step 1: Thay toàn bộ đoạn từ "Best-effort CAPU init" tới hết vòng `for` transcribe**

Trong `frontend/src-tauri/src/audio/import.rs`, thay:

```rust
    // Best-effort CAPU init before import transcription
    if crate::capu_engine::commands::capu_is_model_downloaded(app.clone())
        .await
        .unwrap_or(false)
    {
        let _ = crate::capu_engine::commands::capu_init(app.clone()).await;
    }

    let mut capu_trailing_context: Vec<String> = Vec::new();

    // Process each speech segment
    let mut all_transcripts: Vec<(String, f64, f64)> = Vec::new();

    for (i, segment) in processable_segments.iter().enumerate() {
        if IMPORT_CANCELLED.load(Ordering::SeqCst) {
            let _ = std::fs::remove_dir_all(&meeting_folder);
            return Err(anyhow!("Import cancelled"));
        }

        let progress = 30 + ((i as f32 / processable_count.max(1) as f32) * 50.0) as u32;
        let segment_duration_sec = (segment.end_timestamp_ms - segment.start_timestamp_ms) / 1000.0;
        emit_progress(
            &app,
            "transcribing",
            progress,
            &format!(
                "Transcribing segment {} of {} ({:.1}s)...",
                i + 1,
                processable_count,
                segment_duration_sec
            ),
        );

        // Skip very short segments
        if segment.samples.len() < 1600 {
            debug!(
                "Skipping short segment {} with {} samples",
                i,
                segment.samples.len()
            );
            continue;
        }

        // Transcribe with ASR or ROVER, per the branch resolved above
        let text = if let Some(rover) = &rover {
            let rover = rover.clone();
            let samples = segment.samples.clone();
            tokio::task::block_in_place(move || {
                let mut guard = rover.blocking_lock();
                guard.decode(&samples, 16000.0)
            })
            .map(|r| r.text)
            .map_err(|e| anyhow!("ROVER transcription failed on segment {}: {}", i, e))?
        } else {
            asr.as_ref()
                .expect("asr must be Some when rover is None")
                .transcribe_audio(segment.samples.clone())
                .await
                .map_err(|e| anyhow!("ASR transcription failed on segment {}: {}", i, e))?
        };

        let trimmed = text.trim();
        if !trimmed.is_empty() {
            debug!("Segment {}/{}: {:.1}s — '{}'", i + 1, processable_count, segment_duration_sec, trimmed);

            let punctuated =
                crate::audio::post_asr::process_asr_text(&text, &mut capu_trailing_context);

            all_transcripts.push((punctuated, segment.start_timestamp_ms, segment.end_timestamp_ms));
        } else {
            debug!("Segment {}/{}: {:.1}s — empty", i + 1, processable_count, segment_duration_sec);
        }
    }

    info!("Transcription complete: {} segments", all_transcripts.len());

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&meeting_folder);
        return Err(anyhow!("Import cancelled"));
    }

    emit_progress(&app, "saving", 85, "Creating meeting...");

    // Create transcript segments
    let segments = create_transcript_segments(&all_transcripts);
```

bằng:

```rust
    // Best-effort CAPU init before import transcription
    if crate::capu_engine::commands::capu_is_model_downloaded(app.clone())
        .await
        .unwrap_or(false)
    {
        let _ = crate::capu_engine::commands::capu_init(app.clone()).await;
    }

    let primary = if let Some(rover) = rover {
        crate::audio::batch_transcribe::PrimaryEngine::Rover(rover)
    } else {
        crate::audio::batch_transcribe::PrimaryEngine::Single(
            asr.expect("asr must be Some when rover is None"),
        )
    };

    let app_for_progress = app.clone();
    let segments = crate::audio::batch_transcribe::batch_transcribe(
        &app,
        processable_segments,
        primary,
        move |done, total| {
            let progress = 30 + ((done as f32 / total.max(1) as f32) * 50.0) as u32;
            emit_progress(
                &app_for_progress,
                "transcribing",
                progress,
                &format!("Transcribing segment {} of {}...", done, total),
            );
        },
    )
    .await?;

    info!("Transcription complete: {} segments", segments.len());

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&meeting_folder);
        return Err(anyhow!("Import cancelled"));
    }

    emit_progress(&app, "saving", 85, "Creating meeting...");
```

Lưu ý: biến `processable_segments` ở trên đã là `Vec<SpeechSegment>` từ `expand_segments_at_silence` (không đổi); dòng `let processable_count = processable_segments.len();` phía trước đoạn thay thế này vẫn giữ nguyên — không xoá.

- [ ] **Step 2: Xoá import không còn dùng nếu `cargo check` báo unused**

Vòng `for` cũ dùng `create_transcript_segments` (từ `super::common`) — hàm này KHÔNG còn được gọi trong `import.rs` sau thay đổi trên (giờ `batch_transcribe` tự tạo `TranscriptSegment`). Nếu `cargo check` báo `unused import: create_transcript_segments`, sửa dòng import ở đầu file:

```rust
use super::common::{create_transcript_segments, expand_segments_at_silence, write_transcripts_json};
```

thành:

```rust
use super::common::{expand_segments_at_silence, write_transcripts_json};
```

(Chỉ sửa nếu `cargo check` thực sự báo warning/error về việc này — `create_transcript_segments` vẫn được dùng trong `#[cfg(test)] mod tests` của chính file này, nên **không** xoá khỏi `super::common` — chỉ xoá khỏi import của `import.rs` nếu nó thật sự không còn dùng ở phần code chính; nếu test module vẫn cần nó, giữ nguyên import và bỏ qua bước này.)

- [ ] **Step 3: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml --all-targets
```
Expected: biên dịch thành công.

- [ ] **Step 4: `cargo test` cho các test hiện có của `import.rs`**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml audio::import::tests -- --nocapture
```
Expected: tất cả test hiện có (không đổi bởi task này) vẫn PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/import.rs
git commit -m "feat(perf): use batch_transcribe in import.rs"
```

---

### Task 6: Nối `retranscription.rs` dùng `batch_transcribe`

**Files:**
- Modify: `frontend/src-tauri/src/audio/retranscription.rs`

- [ ] **Step 1: Thay toàn bộ đoạn từ "Best-effort CAPU init" tới hết vòng `for` transcribe**

Trong `frontend/src-tauri/src/audio/retranscription.rs`, thay:

```rust
    // Best-effort CAPU init before retranscription
    if crate::capu_engine::commands::capu_is_model_downloaded(app.clone())
        .await
        .unwrap_or(false)
    {
        let _ = crate::capu_engine::commands::capu_init(app.clone()).await;
    }

    let mut capu_trailing_context: Vec<String> = Vec::new();

    let mut all_transcripts: Vec<(String, f64, f64)> = Vec::new();

    for (i, segment) in processable_segments.iter().enumerate() {
        if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
            return Err(anyhow!("Retranscription cancelled"));
        }

        let progress = 25 + ((i as f32 / processable_count as f32) * 55.0) as u32;
        let segment_duration_sec = (segment.end_timestamp_ms - segment.start_timestamp_ms) / 1000.0;
        emit_progress(
            &app,
            &meeting_id,
            "transcribing",
            progress,
            &format!("Transcribing segment {} of {} ({:.1}s)...", i + 1, processable_count, segment_duration_sec),
        );

        if segment.samples.len() < 1600 {
            debug!("Skipping short segment {}", i);
            continue;
        }

        let text = if let Some(rover) = &rover {
            let rover = rover.clone();
            let samples = segment.samples.clone();
            tokio::task::block_in_place(move || {
                let mut guard = rover.blocking_lock();
                guard.decode(&samples, 16000.0)
            })
            .map(|r| r.text)
            .map_err(|e| anyhow!("ROVER transcription failed on segment {}: {}", i, e))?
        } else {
            engine
                .as_ref()
                .expect("engine must be Some when rover is None")
                .transcribe_audio(segment.samples.clone())
                .await
                .map_err(|e| anyhow!("ASR transcription failed on segment {}: {}", i, e))?
        };

        let trimmed = text.trim();
        if !trimmed.is_empty() {
            debug!("Segment {}/{}: {:.1}s — '{}'", i + 1, processable_count, segment_duration_sec, trimmed);

            let punctuated =
                crate::audio::post_asr::process_asr_text(&text, &mut capu_trailing_context);

            all_transcripts.push((punctuated, segment.start_timestamp_ms, segment.end_timestamp_ms));
        }
    }

    info!("Transcription complete: {} segments", all_transcripts.len());

    if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Retranscription cancelled"));
    }

    emit_progress(&app, &meeting_id, "saving", 80, "Saving transcripts...");

    let segments = create_transcript_segments(&all_transcripts);
```

bằng:

```rust
    // Best-effort CAPU init before retranscription
    if crate::capu_engine::commands::capu_is_model_downloaded(app.clone())
        .await
        .unwrap_or(false)
    {
        let _ = crate::capu_engine::commands::capu_init(app.clone()).await;
    }

    let primary = if let Some(rover) = rover {
        crate::audio::batch_transcribe::PrimaryEngine::Rover(rover)
    } else {
        crate::audio::batch_transcribe::PrimaryEngine::Single(
            engine.expect("engine must be Some when rover is None"),
        )
    };

    let app_for_progress = app.clone();
    let meeting_id_for_progress = meeting_id.clone();
    let segments = crate::audio::batch_transcribe::batch_transcribe(
        &app,
        processable_segments,
        primary,
        move |done, total| {
            let progress = 25 + ((done as f32 / total.max(1) as f32) * 55.0) as u32;
            emit_progress(
                &app_for_progress,
                &meeting_id_for_progress,
                "transcribing",
                progress,
                &format!("Transcribing segment {} of {}...", done, total),
            );
        },
        || RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst),
    )
    .await?;

    info!("Transcription complete: {} segments", segments.len());

    if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Retranscription cancelled"));
    }

    emit_progress(&app, &meeting_id, "saving", 80, "Saving transcripts...");
```

Lưu ý: biến `processable_segments`/`processable_count` phía trước đoạn thay thế này giữ nguyên như hiện có, không xoá.

- [ ] **Step 2: Xoá import không còn dùng nếu `cargo check` báo unused**

Cùng lưu ý như Task 5/Step 2: chỉ sửa dòng import `create_transcript_segments` ở đầu `retranscription.rs` nếu `cargo check` thực sự báo không còn dùng ở phần code chính (giữ lại nếu `#[cfg(test)] mod tests` của file này vẫn gọi nó).

- [ ] **Step 3: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml --all-targets
```
Expected: biên dịch thành công.

- [ ] **Step 4: `cargo test` cho các test hiện có của `retranscription.rs`**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml audio::retranscription::tests -- --nocapture
```
Expected: tất cả test hiện có vẫn PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/retranscription.rs
git commit -m "feat(perf): use batch_transcribe in retranscription.rs"
```

---

### Task 7: Kiểm chứng toàn bộ

**Files:** (không sửa code — chỉ chạy lệnh và kiểm tra thủ công)

- [ ] **Step 1: Build + test toàn bộ**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml --all-targets
cargo test --manifest-path frontend/src-tauri/Cargo.toml
```
Expected: biên dịch sạch; test suite pass (2 lỗi có sẵn từ trước — `test_calculate_buffer_timeout_bluetooth`, `test_vad_large_file_progress` — không liên quan, đã biết từ plan Live).

- [ ] **Step 2: Build app đầy đủ**

```bash
cargo build --manifest-path frontend/src-tauri/Cargo.toml
```
Expected: biên dịch + link thành công.

- [ ] **Step 3: Kiểm thử thủ công — Import file ngắn (< 4 đoạn VAD hoặc máy < 4 core)**

Import 1 file audio ngắn (~30s-1 phút). Xác nhận: import chạy thành công, transcript đúng, log KHÔNG hiện dấu hiệu chạy song song (vì không đủ điều kiện) — đây là đường tuần tự dự phòng.

- [ ] **Step 4: Kiểm thử thủ công — Import file dài trên máy ≥4 core**

Import 1 file audio dài (≥10 phút, đủ để VAD tạo ≥4 đoạn). Xác nhận: 2 worker chạy song song, transcript đúng thứ tự thời gian, nội dung hợp lý, so thời gian xử lý trước/sau khi có plan này.

- [ ] **Step 5: Kiểm thử thủ công — Retranscription**

Chọn 1 cuộc họp cũ, chạy "Retranscribe". Xác nhận hành vi giống Step 3/4 (dùng chung `batch_transcribe`).

- [ ] **Step 6: Kiểm thử thủ công — ROVER**

Bật ROVER trong Settings, lặp lại Step 3/4. Xác nhận: file ngắn → tuần tự qua 1 `RoverDecoder`; file dài trên máy ≥4 core → 2 worker song song, mỗi worker tự có `RoverDecoder` riêng (xem log để xác nhận không lỗi load model 2 lần), không treo máy, không lỗi oversubscribe thread.

- [ ] **Step 7: Xác nhận không phá vỡ luồng Live**

Ghi âm trực tiếp thử 1 đoạn ngắn — xác nhận luồng Live (từ plan trước) vẫn hoạt động bình thường, không bị ảnh hưởng bởi các thay đổi ở luồng File.
