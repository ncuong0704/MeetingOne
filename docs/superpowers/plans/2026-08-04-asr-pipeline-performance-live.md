# Tối ưu pipeline âm thanh → transcript: Nền tảng + Luồng Live — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tách CAPU (dấu câu/viết hoa) khỏi đường nghẽn chính của luồng ghi âm trực tiếp — chạy nền song song thay vì chặn đồng bộ mỗi đoạn — và cân bằng lại ngân sách thread ASR/ROVER theo CPU thật của máy, để giảm độ trễ audio → transcript trong lúc ghi âm.

**Architecture:** Thêm 1 hàm thuần `asr_thread_budget` dùng chung cho mọi nơi cấu hình thread ONNX. Thêm `CapuBatcher` (module thuần, không phụ thuộc Tauri/tokio) gom nhiều đoạn ASR thô thành lô lớn hơn trước khi gọi CAPU 1 lần. `transcription/worker.rs` tách thành 2 stage: Stage 1 (ASR, giữ 1 worker, chỉ làm ITN rồi emit ngay) đẩy đoạn thô vào 1 channel; Stage 2 (task nền mới) tiêu thụ channel đó, gom lô qua `CapuBatcher`, rồi phát sự kiện Tauri mới `transcript-finalized` để `recording_commands.rs` gộp lại đúng đoạn đã lưu.

**Tech Stack:** Rust, Tauri 2.x, tokio (mpsc channel, `select!`, `spawn`), `ort` (ONNX Runtime) crate cho CAPU, `sherpa-onnx`/`ort` cho ASR/ROVER.

**Spec liên quan:** [docs/superpowers/specs/2026-08-04-asr-pipeline-performance-design.md](../specs/2026-08-04-asr-pipeline-performance-design.md) — plan này triển khai phần "Nguyên tắc chung" + "A. Luồng Live". Phần "B. Luồng File" là 1 plan riêng, làm sau, phụ thuộc `CapuBatcher`/`asr_thread_budget` đã có ở đây.

**Lệnh build/test dùng xuyên suốt plan** (chạy từ thư mục gốc repo):
```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml
cargo test --manifest-path frontend/src-tauri/Cargo.toml <tên_test> -- --nocapture
```

---

### Task 1: `asr_thread_budget` — ngân sách thread thuần theo số đường giải mã đồng thời

**Files:**
- Create: `frontend/src-tauri/src/asr_engine/thread_budget.rs`
- Modify: `frontend/src-tauri/src/asr_engine/mod.rs`

- [ ] **Step 1: Viết file mới với test trước (sẽ fail vì hàm chưa tồn tại)**

```rust
// frontend/src-tauri/src/asr_engine/thread_budget.rs
//
// ONNX intra-op thread budgets for ASR/ROVER decode paths, sized by how many
// decode contexts run concurrently on the same machine at once. See
// docs/superpowers/specs/2026-08-04-asr-pipeline-performance-design.md for the
// reasoning behind each number.

/// How many ONNX decode contexts run truly concurrently for a given call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeConcurrency {
    /// Live recording, single ASR model (no ROVER).
    SingleLive,
    /// Live recording, ROVER — 2 decoders run in parallel via `std::thread::scope`.
    RoverLive,
    /// File batch mode, single ASR model — 1 of 2 parallel file-workers.
    SingleFileWorker,
    /// File batch mode with ROVER — 1 of 2 parallel file-workers, each itself
    /// running ROVER's 2 decoders (4 decode contexts total across both workers).
    RoverFileWorker,
}

/// Computes the ONNX intra-op thread count for one decode context, given the
/// machine's physical core count and how many such contexts run at once.
/// Always returns at least 1.
pub fn asr_thread_budget(physical_cores: usize, concurrency: DecodeConcurrency) -> usize {
    use DecodeConcurrency::*;
    match concurrency {
        SingleLive => physical_cores.clamp(2, 4),
        RoverLive => (physical_cores.clamp(2, 4) / 2).max(1),
        SingleFileWorker => (physical_cores / 2).max(1),
        RoverFileWorker => (physical_cores / 4).max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_live_clamps_between_2_and_4() {
        assert_eq!(asr_thread_budget(1, DecodeConcurrency::SingleLive), 2);
        assert_eq!(asr_thread_budget(2, DecodeConcurrency::SingleLive), 2);
        assert_eq!(asr_thread_budget(4, DecodeConcurrency::SingleLive), 4);
        assert_eq!(asr_thread_budget(8, DecodeConcurrency::SingleLive), 4);
        assert_eq!(asr_thread_budget(16, DecodeConcurrency::SingleLive), 4);
    }

    #[test]
    fn rover_live_is_at_most_single_live_and_never_zero() {
        for cores in [1usize, 2, 4, 8, 16] {
            let single = asr_thread_budget(cores, DecodeConcurrency::SingleLive);
            let rover = asr_thread_budget(cores, DecodeConcurrency::RoverLive);
            assert!(rover >= 1);
            assert!(rover <= single);
        }
    }

    #[test]
    fn single_file_worker_is_half_physical_cores_minimum_1() {
        assert_eq!(asr_thread_budget(1, DecodeConcurrency::SingleFileWorker), 1);
        assert_eq!(asr_thread_budget(2, DecodeConcurrency::SingleFileWorker), 1);
        assert_eq!(asr_thread_budget(4, DecodeConcurrency::SingleFileWorker), 2);
        assert_eq!(asr_thread_budget(8, DecodeConcurrency::SingleFileWorker), 4);
        assert_eq!(asr_thread_budget(16, DecodeConcurrency::SingleFileWorker), 8);
    }

    #[test]
    fn rover_file_worker_is_quarter_physical_cores_minimum_1() {
        assert_eq!(asr_thread_budget(1, DecodeConcurrency::RoverFileWorker), 1);
        assert_eq!(asr_thread_budget(4, DecodeConcurrency::RoverFileWorker), 1);
        assert_eq!(asr_thread_budget(8, DecodeConcurrency::RoverFileWorker), 2);
        assert_eq!(asr_thread_budget(16, DecodeConcurrency::RoverFileWorker), 4);
    }

    #[test]
    fn rover_file_worker_never_exceeds_single_file_worker() {
        for cores in [1usize, 2, 4, 8, 16, 32] {
            let single = asr_thread_budget(cores, DecodeConcurrency::SingleFileWorker);
            let rover = asr_thread_budget(cores, DecodeConcurrency::RoverFileWorker);
            assert!(rover <= single);
        }
    }
}
```

- [ ] **Step 2: Đăng ký module mới**

Modify `frontend/src-tauri/src/asr_engine/mod.rs` — nội dung hiện tại:

```rust
pub mod commands;
pub mod engine;
pub mod hotwords;
pub mod model_family;
```

Thay bằng:

```rust
pub mod commands;
pub mod engine;
pub mod hotwords;
pub mod model_family;
pub mod thread_budget;
```

- [ ] **Step 3: Chạy test, xác nhận PASS**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml asr_engine::thread_budget -- --nocapture
```
Expected: 5 test PASS (`single_live_clamps_between_2_and_4`, `rover_live_is_at_most_single_live_and_never_zero`, `single_file_worker_is_half_physical_cores_minimum_1`, `rover_file_worker_is_quarter_physical_cores_minimum_1`, `rover_file_worker_never_exceeds_single_file_worker`).

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/asr_engine/thread_budget.rs frontend/src-tauri/src/asr_engine/mod.rs
git commit -m "feat(perf): add asr_thread_budget for CPU-topology-aware ONNX thread counts"
```

---

### Task 2: Áp `asr_thread_budget` vào `AsrEngine::load_model`

**Files:**
- Modify: `frontend/src-tauri/src/asr_engine/engine.rs:268-274,352,450-487`
- Modify: `frontend/src-tauri/src/asr_engine/commands.rs`

- [ ] **Step 1: Cập nhật test hiện có để gọi với tham số mới (sẽ fail biên dịch)**

Trong `frontend/src-tauri/src/asr_engine/engine.rs`, tìm test `test_load_model_rejects_unsupported_variant` (cuối file), thay:

```rust
    #[tokio::test]
    async fn test_load_model_rejects_unsupported_variant() {
        let engine = AsrEngine::new();
        let result = engine
            .load_model(
                ModelFamily::SherpaZipformerVi2025,
                ModelVariant::Int8,
                "modified_beam_search".to_string(),
                15,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not support variant"));
    }
```

bằng:

```rust
    #[tokio::test]
    async fn test_load_model_rejects_unsupported_variant() {
        let engine = AsrEngine::new();
        let result = engine
            .load_model(
                ModelFamily::SherpaZipformerVi2025,
                ModelVariant::Int8,
                "modified_beam_search".to_string(),
                15,
                2,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not support variant"));
    }
```

- [ ] **Step 2: Chạy test, xác nhận FAIL (lỗi biên dịch)**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml test_load_model_rejects_unsupported_variant
```
Expected: FAIL — `this function takes 4 arguments but 5 arguments were supplied`.

- [ ] **Step 3: Sửa `load_model` để nhận `num_threads`**

Trong `frontend/src-tauri/src/asr_engine/engine.rs`, thay chữ ký hàm (dòng 268-274):

```rust
    pub async fn load_model(
        &self,
        family: ModelFamily,
        variant: ModelVariant,
        decoding_method: String,
        num_active_paths: i32,
    ) -> Result<()> {
```

bằng:

```rust
    pub async fn load_model(
        &self,
        family: ModelFamily,
        variant: ModelVariant,
        decoding_method: String,
        num_active_paths: i32,
        num_threads: usize,
    ) -> Result<()> {
```

Và thay dòng 352:

```rust
        config.model_config.num_threads = 2;
```

bằng:

```rust
        config.model_config.num_threads = num_threads.max(1) as i32;
```

- [ ] **Step 4: Chạy lại test, xác nhận PASS**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml test_load_model_rejects_unsupported_variant
```
Expected: PASS.

- [ ] **Step 5: Cập nhật 3 nơi gọi `load_model` trong `commands.rs`**

Trong `frontend/src-tauri/src/asr_engine/commands.rs`, thêm hàm helper ngay sau import (trước `pub(crate) static ASR_ENGINE`):

```rust
fn live_asr_thread_count() -> usize {
    let (physical_cores, _) = crate::capu_engine::cpu_topology::detect_cpu_topology();
    super::thread_budget::asr_thread_budget(physical_cores, super::thread_budget::DecodeConcurrency::SingleLive)
}
```

Trong `asr_download_model`, thay:

```rust
                if let Err(e) = engine_clone
                    .load_model(f_for_load, v_for_load, decoding, paths)
                    .await
                {
```

bằng:

```rust
                if let Err(e) = engine_clone
                    .load_model(f_for_load, v_for_load, decoding, paths, live_asr_thread_count())
                    .await
                {
```

Trong `asr_load_model` (Tauri command), thay:

```rust
    engine
        .load_model(f, v, decoding_method, num_active_paths)
        .await
        .map_err(|e| e.to_string())
```

bằng:

```rust
    engine
        .load_model(f, v, decoding_method, num_active_paths, live_asr_thread_count())
        .await
        .map_err(|e| e.to_string())
```

Trong `asr_validate_model_ready`, thay:

```rust
        engine
            .load_model(f, v, dm, paths)
            .await
            .map_err(|e| e.to_string())?;
```

bằng:

```rust
        engine
            .load_model(f, v, dm, paths, live_asr_thread_count())
            .await
            .map_err(|e| e.to_string())?;
```

- [ ] **Step 6: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml
```
Expected: biên dịch thành công, không lỗi.

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/asr_engine/engine.rs frontend/src-tauri/src/asr_engine/commands.rs
git commit -m "feat(perf): size ASR ONNX threads from CPU topology instead of hardcoded 2"
```

---

### Task 3: Áp `asr_thread_budget` vào ROVER (`rnnt_decoder` + `rover_engine`)

**Files:**
- Modify: `frontend/src-tauri/src/rnnt_decoder/sessions.rs`
- Modify: `frontend/src-tauri/src/rnnt_decoder/engine.rs:37-51,139-146`
- Modify: `frontend/src-tauri/src/rover_engine/engine.rs:20-30,92-97`
- Modify: `frontend/src-tauri/src/rover_engine/commands.rs:85-92`

- [ ] **Step 1: Cập nhật call site thật + 2 manual test call site để dùng tham số thread mới (sẽ fail biên dịch)**

Trong `frontend/src-tauri/src/rover_engine/commands.rs`, thay khối gọi `RoverDecoder::load` (trong `rover_load_model`):

```rust
    let decoder = tokio::task::block_in_place(|| {
        RoverDecoder::load(
            (&enc_a, &dec_a, &joi_a, &tok_a),
            (&enc_b, &dec_b, &joi_b, &tok_b),
            4,
        )
    })
    .map_err(|e| format!("Failed to load ROVER models: {}", e))?;
```

bằng:

```rust
    let threads_per_decoder = {
        let (physical_cores, _) = crate::capu_engine::cpu_topology::detect_cpu_topology();
        crate::asr_engine::thread_budget::asr_thread_budget(
            physical_cores,
            crate::asr_engine::thread_budget::DecodeConcurrency::RoverLive,
        )
    };

    let decoder = tokio::task::block_in_place(|| {
        RoverDecoder::load(
            (&enc_a, &dec_a, &joi_a, &tok_a),
            (&enc_b, &dec_b, &joi_b, &tok_b),
            4,
            threads_per_decoder,
        )
    })
    .map_err(|e| format!("Failed to load ROVER models: {}", e))?;
```

Trong `frontend/src-tauri/src/rover_engine/engine.rs`, trong test `rover_decode_on_real_audio`, thay:

```rust
        let mut rover = RoverDecoder::load(
            (&enc_a, &dec_a, &joi_a, &tok_a),
            (&enc_b, &dec_b, &joi_b, &tok_b),
            4,
        )
        .expect("load RoverDecoder");
```

bằng:

```rust
        let mut rover = RoverDecoder::load(
            (&enc_a, &dec_a, &joi_a, &tok_a),
            (&enc_b, &dec_b, &joi_b, &tok_b),
            4,
            2,
        )
        .expect("load RoverDecoder");
```

Trong `frontend/src-tauri/src/rnnt_decoder/engine.rs`, trong test `rnnt_decoder_decode_on_real_audio`, thay:

```rust
        let mut decoder = RnntDecoder::load(
            &model_dir.join(&encoder_file),
            &model_dir.join(&decoder_file),
            &model_dir.join(&joiner_file),
            &resolve_tokens_path(&model_dir),
            4,
        )
        .expect("load decoder");
```

bằng:

```rust
        let mut decoder = RnntDecoder::load(
            &model_dir.join(&encoder_file),
            &model_dir.join(&decoder_file),
            &model_dir.join(&joiner_file),
            &resolve_tokens_path(&model_dir),
            4,
            2,
        )
        .expect("load decoder");
```

- [ ] **Step 2: `cargo check`, xác nhận FAIL (lỗi biên dịch)**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml
```
Expected: FAIL — `this function takes 3/4/5 arguments but 4/5/6 arguments were supplied` ở `RoverDecoder::load`/`RnntDecoder::load`.

- [ ] **Step 3: Sửa `RnntSessions::load`/`load_session` nhận `threads`**

Trong `frontend/src-tauri/src/rnnt_decoder/sessions.rs`, thay:

```rust
fn load_session(path: &Path, label: &str) -> Result<Session> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("Non-UTF8 {} path: {:?}", label, path))?;
    Session::builder()
        .map_err(|e| anyhow!("Failed to create {} session builder: {}", label, e))?
        .commit_from_file(path_str)
        .map_err(|e| anyhow!("Failed to load {} {:?}: {}", label, path, e))
}

impl RnntSessions {
    pub fn load(encoder_path: &Path, decoder_path: &Path, joiner_path: &Path) -> Result<Self> {
        Ok(Self {
            encoder: load_session(encoder_path, "encoder")?,
            decoder: load_session(decoder_path, "decoder")?,
            joiner: load_session(joiner_path, "joiner")?,
        })
    }
```

bằng:

```rust
fn load_session(path: &Path, label: &str, threads: usize) -> Result<Session> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("Non-UTF8 {} path: {:?}", label, path))?;
    Session::builder()
        .map_err(|e| anyhow!("Failed to create {} session builder: {}", label, e))?
        .with_intra_threads(threads.max(1))
        .map_err(|e| anyhow!("Failed to set {} intra-op threads: {}", label, e))?
        .commit_from_file(path_str)
        .map_err(|e| anyhow!("Failed to load {} {:?}: {}", label, path, e))
}

impl RnntSessions {
    pub fn load(encoder_path: &Path, decoder_path: &Path, joiner_path: &Path, threads: usize) -> Result<Self> {
        Ok(Self {
            encoder: load_session(encoder_path, "encoder", threads)?,
            decoder: load_session(decoder_path, "decoder", threads)?,
            joiner: load_session(joiner_path, "joiner", threads)?,
        })
    }
```

- [ ] **Step 4: Sửa `RnntDecoder::load` nhận và truyền `threads`**

Trong `frontend/src-tauri/src/rnnt_decoder/engine.rs`, thay:

```rust
    pub fn load(
        encoder_path: &Path,
        decoder_path: &Path,
        joiner_path: &Path,
        tokens_path: &Path,
        beam_size: usize,
    ) -> Result<Self> {
        let sessions = RnntSessions::load(encoder_path, decoder_path, joiner_path)?;
        let vocab = Vocab::from_tokens_file(tokens_path)?;
        Ok(Self {
            sessions,
            vocab,
            frame_shift_ms: 10.0,
            beam_size,
        })
    }
```

bằng:

```rust
    pub fn load(
        encoder_path: &Path,
        decoder_path: &Path,
        joiner_path: &Path,
        tokens_path: &Path,
        beam_size: usize,
        threads: usize,
    ) -> Result<Self> {
        let sessions = RnntSessions::load(encoder_path, decoder_path, joiner_path, threads)?;
        let vocab = Vocab::from_tokens_file(tokens_path)?;
        Ok(Self {
            sessions,
            vocab,
            frame_shift_ms: 10.0,
            beam_size,
        })
    }
```

- [ ] **Step 5: Sửa `RoverDecoder::load` nhận `threads_per_decoder` và truyền cho cả 2 decoder**

Trong `frontend/src-tauri/src/rover_engine/engine.rs`, thay:

```rust
    pub fn load(
        family_a: (&Path, &Path, &Path, &Path),
        family_b: (&Path, &Path, &Path, &Path),
        beam_size: usize,
    ) -> Result<Self> {
        let decoder_a =
            RnntDecoder::load(family_a.0, family_a.1, family_a.2, family_a.3, beam_size)?;
        let decoder_b =
            RnntDecoder::load(family_b.0, family_b.1, family_b.2, family_b.3, beam_size)?;
        Ok(Self { decoder_a, decoder_b })
    }
```

bằng:

```rust
    pub fn load(
        family_a: (&Path, &Path, &Path, &Path),
        family_b: (&Path, &Path, &Path, &Path),
        beam_size: usize,
        threads_per_decoder: usize,
    ) -> Result<Self> {
        let decoder_a = RnntDecoder::load(
            family_a.0, family_a.1, family_a.2, family_a.3, beam_size, threads_per_decoder,
        )?;
        let decoder_b = RnntDecoder::load(
            family_b.0, family_b.1, family_b.2, family_b.3, beam_size, threads_per_decoder,
        )?;
        Ok(Self { decoder_a, decoder_b })
    }
```

- [ ] **Step 6: `cargo check`, xác nhận PASS**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml
```
Expected: biên dịch thành công.

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/rnnt_decoder/sessions.rs frontend/src-tauri/src/rnnt_decoder/engine.rs frontend/src-tauri/src/rover_engine/engine.rs frontend/src-tauri/src/rover_engine/commands.rs
git commit -m "feat(perf): set explicit ONNX intra-op threads for ROVER's 2 concurrent decoders"
```

---

### Task 4: `CapuBatcher` — gom nhiều đoạn ASR thành lô trước khi gọi CAPU

**Files:**
- Modify: `frontend/src-tauri/src/config.rs:88-90`
- Create: `frontend/src-tauri/src/capu_engine/batch.rs`
- Modify: `frontend/src-tauri/src/capu_engine/mod.rs`

- [ ] **Step 1: Thêm hằng số cấu hình lô**

Trong `frontend/src-tauri/src/config.rs`, ngay sau dòng `pub const CAPU_TRAILING_CONTEXT_WORDS: usize = 15;` (dòng 90), thêm:

```rust
pub const CAPU_BATCH_WORD_BUDGET: usize = 200;
pub const CAPU_BATCH_DEBOUNCE_SECS: u64 = 5;
```

- [ ] **Step 2: Viết `capu_engine/batch.rs` — struct + logic thuần trước (test trước code, code trước ở dạng để test biên dịch được)**

```rust
// frontend/src-tauri/src/capu_engine/batch.rs
//
// Groups small raw ASR segments into larger batches before running CAPU, so punctuation
// restoration pays its fixed per-call overhead far fewer times per meeting/file. Used by
// both the live background stage (transcription/worker.rs, with a debounce timer) and the
// file batch path (added in a later plan; no timer there — the segment list is already
// complete). See docs/superpowers/specs/2026-08-04-asr-pipeline-performance-design.md.

use super::CapuEngine;

/// One raw ASR segment (post-ITN, pre-CAPU) waiting to be batched.
#[derive(Debug, Clone)]
pub struct PendingSegment {
    pub source_id: u64,
    pub raw_text: String,
    pub audio_start_time: f64,
    pub audio_end_time: f64,
}

/// The result of running CAPU over one accumulated batch of `PendingSegment`s.
#[derive(Debug, Clone)]
pub struct FinalizedSegment {
    pub text: String,
    pub audio_start_time: f64,
    pub audio_end_time: f64,
    /// `source_id` of every `PendingSegment` this batch replaces, in original order.
    pub source_ids: Vec<u64>,
}

pub struct CapuBatcher {
    pending: Vec<PendingSegment>,
    pending_word_count: usize,
    trailing_context: Vec<String>,
}

impl CapuBatcher {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            pending_word_count: 0,
            trailing_context: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// True once the accumulated pending word count has reached `word_budget`. Callers
    /// (live: also a debounce timer; file: only this) decide when to act on it.
    pub fn should_flush(&self, word_budget: usize) -> bool {
        self.pending_word_count >= word_budget
    }

    /// Buffers one raw segment. Does not run CAPU — call `should_flush`/`flush` separately.
    pub fn push(&mut self, seg: PendingSegment) {
        self.pending_word_count += seg.raw_text.split_whitespace().count();
        self.pending.push(seg);
    }

    /// The exact string CAPU would receive if flushed right now — exposed separately so
    /// the join logic is unit-testable without a loaded `CapuEngine`.
    pub fn joined_pending_text(&self) -> String {
        self.pending
            .iter()
            .map(|s| s.raw_text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Discards whatever is pending without running CAPU (used when no CAPU engine is
    /// available at all — e.g. model not downloaded). The raw text was already emitted
    /// live by Stage 1; only the punctuated-merge for this batch is lost.
    pub fn discard_pending(&mut self) {
        self.pending.clear();
        self.pending_word_count = 0;
    }

    /// Runs CAPU over everything pending and clears it. `None` if nothing is pending. On
    /// CAPU failure, falls back to the joined raw text (same fallback
    /// `post_asr::process_asr_text` already uses) rather than losing the batch.
    pub fn flush(&mut self, engine: &mut CapuEngine) -> Option<FinalizedSegment> {
        if self.pending.is_empty() {
            return None;
        }

        let joined = self.joined_pending_text();
        let source_ids: Vec<u64> = self.pending.iter().map(|s| s.source_id).collect();
        let audio_start_time = self.pending.first().unwrap().audio_start_time;
        let audio_end_time = self.pending.last().unwrap().audio_end_time;

        let text = match engine.restore_punctuation(&self.trailing_context, &joined) {
            Ok((restored, next_context)) => {
                self.trailing_context = next_context;
                restored
            }
            Err(e) => {
                log::warn!("CapuBatcher: CAPU failed on batch, falling back to raw text: {}", e);
                joined
            }
        };

        self.pending.clear();
        self.pending_word_count = 0;

        Some(FinalizedSegment {
            text,
            audio_start_time,
            audio_end_time,
            source_ids,
        })
    }
}

impl Default for CapuBatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(id: u64, text: &str, start: f64, end: f64) -> PendingSegment {
        PendingSegment {
            source_id: id,
            raw_text: text.to_string(),
            audio_start_time: start,
            audio_end_time: end,
        }
    }

    #[test]
    fn new_batcher_is_empty_and_does_not_flush() {
        let batcher = CapuBatcher::new();
        assert!(batcher.is_empty());
        assert!(!batcher.should_flush(1));
    }

    #[test]
    fn should_flush_triggers_once_word_budget_reached() {
        let mut batcher = CapuBatcher::new();
        batcher.push(pending(0, "one two three", 0.0, 1.0));
        assert!(!batcher.should_flush(5));
        batcher.push(pending(1, "four five", 1.0, 2.0));
        assert!(batcher.should_flush(5));
    }

    #[test]
    fn joined_pending_text_joins_with_single_spaces_in_order() {
        let mut batcher = CapuBatcher::new();
        batcher.push(pending(0, "xin chao", 0.0, 1.0));
        batcher.push(pending(1, "cac ban", 1.0, 2.0));
        assert_eq!(batcher.joined_pending_text(), "xin chao cac ban");
    }

    #[test]
    fn push_updates_word_count_by_whitespace_split() {
        let mut batcher = CapuBatcher::new();
        batcher.push(pending(0, "  xin   chao  ", 0.0, 1.0));
        assert!(batcher.should_flush(2));
        assert!(!batcher.should_flush(3));
    }

    #[test]
    fn discard_pending_clears_state() {
        let mut batcher = CapuBatcher::new();
        batcher.push(pending(0, "xin chao", 0.0, 1.0));
        batcher.discard_pending();
        assert!(batcher.is_empty());
        assert!(!batcher.should_flush(1));
    }

    /// Requires a downloaded CAPU model on disk — same gate as
    /// `capu_engine::capu_engine::integration_tests::restore_punctuation_on_real_model`.
    /// Run with: cargo test --manifest-path frontend/src-tauri/Cargo.toml -- --ignored capu_engine::batch
    #[test]
    #[ignore = "requires downloaded CAPU model on disk"]
    fn flush_runs_capu_and_returns_finalized_segment_spanning_the_batch() {
        let dir = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap())
            .join("AppData/Roaming/com.meetingone.app/models/capu-vi");
        let mut engine = CapuEngine::load(
            &dir.join("vibert-capu.int8.onnx"),
            &dir.join("vocab.txt"),
            &dir.join("vocabulary/labels.txt"),
            4,
            7,
            3,
        )
        .expect("load model");

        let mut batcher = CapuBatcher::new();
        batcher.push(pending(10, "xin chao cac ban", 0.0, 2.0));
        batcher.push(pending(11, "hom nay chung ta hop", 2.0, 4.5));

        let finalized = batcher.flush(&mut engine).expect("batch was non-empty");
        assert_eq!(finalized.source_ids, vec![10, 11]);
        assert_eq!(finalized.audio_start_time, 0.0);
        assert_eq!(finalized.audio_end_time, 4.5);
        assert!(batcher.is_empty());
    }
}
```

- [ ] **Step 3: Đăng ký module**

Modify `frontend/src-tauri/src/capu_engine/mod.rs` — nội dung hiện tại:

```rust
pub mod vocabulary;
pub mod edits;
pub mod tokenizer;
pub mod cpu_topology;
pub mod capu_engine;
pub mod commands;

pub use capu_engine::CapuEngine;
```

Thay bằng:

```rust
pub mod vocabulary;
pub mod edits;
pub mod tokenizer;
pub mod cpu_topology;
pub mod capu_engine;
pub mod commands;
pub mod batch;

pub use capu_engine::CapuEngine;
```

- [ ] **Step 4: Chạy các test không cần model thật, xác nhận PASS**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml capu_engine::batch::tests -- --nocapture
```
Expected: 5 test PASS (`new_batcher_is_empty_and_does_not_flush`, `should_flush_triggers_once_word_budget_reached`, `joined_pending_text_joins_with_single_spaces_in_order`, `push_updates_word_count_by_whitespace_split`, `discard_pending_clears_state`). Test `flush_runs_capu_and_returns_finalized_segment_spanning_the_batch` sẽ hiện "ignored" — đúng như mong đợi (cần model CAPU đã tải).

- [ ] **Step 5: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml
```
Expected: biên dịch thành công.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/config.rs frontend/src-tauri/src/capu_engine/batch.rs frontend/src-tauri/src/capu_engine/mod.rs
git commit -m "feat(perf): add CapuBatcher to group segments before running CAPU"
```

---

### Task 5: Tách `apply_itn` khỏi `process_asr_text` trong `post_asr.rs`

**Files:**
- Modify: `frontend/src-tauri/src/audio/post_asr.rs`

- [ ] **Step 1: Viết test trước cho `apply_itn` (fail vì hàm chưa tồn tại)**

Thêm vào cuối `frontend/src-tauri/src/audio/post_asr.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_itn_lowercases_input() {
        let result = apply_itn("XIN CHAO");
        assert_eq!(result, result.to_lowercase());
    }

    #[test]
    fn apply_itn_does_not_panic_on_empty_input() {
        assert_eq!(apply_itn(""), "");
    }
}
```

- [ ] **Step 2: Chạy test, xác nhận FAIL (biên dịch lỗi — không tìm thấy hàm `apply_itn`)**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml audio::post_asr
```
Expected: FAIL — `cannot find function \`apply_itn\` in this scope`.

- [ ] **Step 3: Tách hàm `apply_itn`, `process_asr_text` gọi lại nó**

Thay toàn bộ nội dung hàm hiện có (giữ nguyên docblock đầu file) — nội dung hiện tại:

```rust
/// Apply ITN then CAPU to raw ASR text. Falls back gracefully on any failure. When the
/// punctuation level is at its minimum (1), CAPU is skipped entirely — matching the
/// reference app's `bypass_restorer` behavior — rather than running inference with an
/// extreme bias.
pub fn process_asr_text(raw: &str, capu_trailing: &mut Vec<String>) -> String {
    let lowered = raw.to_lowercase();
    let after_itn = crate::itn_engine::engine::inverse_normalize_or_pass(&lowered);

    match crate::capu_engine::commands::get_engine_arc() {
        Some(engine_arc) => {
            let mut engine = engine_arc.lock().unwrap();
            if engine.punctuation_level() <= 1 {
                return after_itn;
            }
            match engine.restore_punctuation(capu_trailing, &after_itn) {
                Ok((restored, next_context)) => {
                    *capu_trailing = next_context;
                    restored
                }
                Err(e) => {
                    log::warn!("CAPU failed after ITN: {}", e);
                    after_itn
                }
            }
        }
        None => after_itn,
    }
}
```

thay bằng:

```rust
/// Lowercase + inverse text normalization (numbers, units, etc.) — the cheap part of
/// post-ASR processing, safe to run inline on the live transcription hot path. CAPU
/// (punctuation/capitalization) is intentionally NOT applied here — see `CapuBatcher`
/// (capu_engine::batch), which runs it off the hot path in batches.
pub fn apply_itn(raw: &str) -> String {
    let lowered = raw.to_lowercase();
    crate::itn_engine::engine::inverse_normalize_or_pass(&lowered)
}

/// Apply ITN then CAPU to raw ASR text. Falls back gracefully on any failure. When the
/// punctuation level is at its minimum (1), CAPU is skipped entirely — matching the
/// reference app's `bypass_restorer` behavior — rather than running inference with an
/// extreme bias. Used by the file/batch paths (`import.rs`, `retranscription.rs`), which
/// still call CAPU per-segment today. The live path uses `apply_itn` + `CapuBatcher` instead.
pub fn process_asr_text(raw: &str, capu_trailing: &mut Vec<String>) -> String {
    let after_itn = apply_itn(raw);

    match crate::capu_engine::commands::get_engine_arc() {
        Some(engine_arc) => {
            let mut engine = engine_arc.lock().unwrap();
            if engine.punctuation_level() <= 1 {
                return after_itn;
            }
            match engine.restore_punctuation(capu_trailing, &after_itn) {
                Ok((restored, next_context)) => {
                    *capu_trailing = next_context;
                    restored
                }
                Err(e) => {
                    log::warn!("CAPU failed after ITN: {}", e);
                    after_itn
                }
            }
        }
        None => after_itn,
    }
}
```

- [ ] **Step 4: Chạy test, xác nhận PASS**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml audio::post_asr
```
Expected: 2 test PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/post_asr.rs
git commit -m "refactor(perf): extract apply_itn from process_asr_text for the live hot path"
```

---

### Task 6: `replace_transcript_segments` trong `RecordingSaver`/`RecordingManager`

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_saver.rs`
- Modify: `frontend/src-tauri/src/audio/recording_manager.rs:453-456`

- [ ] **Step 1: Viết test trước (fail vì hàm chưa tồn tại)**

Thêm vào cuối `frontend/src-tauri/src/audio/recording_saver.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn seg(sequence_id: u64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            id: format!("seg_{}", sequence_id),
            text: text.to_string(),
            audio_start_time: sequence_id as f64,
            audio_end_time: sequence_id as f64 + 1.0,
            duration: 1.0,
            display_time: "[00:00]".to_string(),
            confidence: 0.9,
            sequence_id,
            user_edited: false,
        }
    }

    #[test]
    fn replace_merges_matched_segments_into_one_in_order() {
        let saver = RecordingSaver::new();
        saver.add_transcript_segment(seg(0, "xin"));
        saver.add_transcript_segment(seg(1, "chao"));
        saver.add_transcript_segment(seg(2, "ban"));

        saver.replace_transcript_segments(&[0, 1], "Xin chào.".to_string(), 0.0, 2.0);

        let segments = saver.get_transcript_segments();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Xin chào.");
        assert_eq!(segments[0].sequence_id, 0);
        assert_eq!(segments[0].audio_start_time, 0.0);
        assert_eq!(segments[0].audio_end_time, 2.0);
        assert_eq!(segments[1].text, "ban");
    }

    #[test]
    fn replace_is_noop_when_no_source_ids_match() {
        let saver = RecordingSaver::new();
        saver.add_transcript_segment(seg(0, "xin"));

        saver.replace_transcript_segments(&[99], "ignored".to_string(), 0.0, 1.0);

        let segments = saver.get_transcript_segments();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "xin");
    }

    #[test]
    fn replace_skips_batch_containing_a_user_edited_segment() {
        let saver = RecordingSaver::new();
        saver.add_transcript_segment(seg(0, "xin"));
        saver.add_transcript_segment(seg(1, "chao"));
        saver
            .update_live_transcript_text(1, "Chào (đã sửa)".to_string())
            .unwrap();

        saver.replace_transcript_segments(&[0, 1], "Xin chào.".to_string(), 0.0, 2.0);

        let segments = saver.get_transcript_segments();
        assert_eq!(segments.len(), 2, "user-edited segment must not be clobbered");
        assert_eq!(segments[1].text, "Chào (đã sửa)");
    }
}
```

- [ ] **Step 2: Chạy test, xác nhận FAIL (biên dịch lỗi)**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml audio::recording_saver
```
Expected: FAIL — `no method named \`replace_transcript_segments\` found`.

- [ ] **Step 3: Thêm `replace_transcript_segments` vào `RecordingSaver`**

Trong `frontend/src-tauri/src/audio/recording_saver.rs`, ngay sau hàm `add_transcript_segment` (kết thúc ở dòng 146, trước `/// Legacy method...`), thêm:

```rust
    /// Replaces one or more stored segments (matched by `source_ids`) with a single
    /// finalized segment — used when the CAPU background stage finishes punctuating a
    /// batch of raw ASR segments. The replacement is inserted at the position of the
    /// first matched segment, preserving chronological order (by `sequence_id`).
    ///
    /// No-op (with a warning log) if none of `source_ids` are found. Also a no-op if any
    /// matched segment has `user_edited = true` — mirrors `add_transcript_segment`'s own
    /// rule that a user's manual correction is never silently overwritten.
    pub fn replace_transcript_segments(
        &self,
        source_ids: &[u64],
        finalized_text: String,
        audio_start_time: f64,
        audio_end_time: f64,
    ) {
        let mut segments = match self.transcript_segments.lock() {
            Ok(s) => s,
            Err(_) => {
                error!("Failed to lock transcript segments for replace");
                return;
            }
        };

        let matched: Vec<usize> = segments
            .iter()
            .enumerate()
            .filter(|(_, s)| source_ids.contains(&s.sequence_id))
            .map(|(i, _)| i)
            .collect();

        if matched.is_empty() {
            warn!(
                "replace_transcript_segments: none of {:?} found in stored segments",
                source_ids
            );
            return;
        }

        if matched.iter().any(|&i| segments[i].user_edited) {
            info!(
                "replace_transcript_segments: skipping batch {:?} — contains a user-edited segment",
                source_ids
            );
            return;
        }

        let sequence_id = segments[matched[0]].sequence_id;
        let display_time = segments[matched[0]].display_time.clone();
        let confidence = segments[matched[0]].confidence;

        let replacement = TranscriptSegment {
            id: format!("seg_{}_finalized", sequence_id),
            text: finalized_text,
            audio_start_time,
            audio_end_time,
            duration: audio_end_time - audio_start_time,
            display_time,
            confidence,
            sequence_id,
            user_edited: false,
        };

        segments.retain(|s| !source_ids.contains(&s.sequence_id));
        let insert_at = segments.partition_point(|s| s.sequence_id < sequence_id);
        segments.insert(insert_at, replacement);

        let replaced_count = matched.len();
        drop(segments);

        info!(
            "Replaced {} raw segment(s) with 1 finalized segment (sequence_id={})",
            replaced_count, sequence_id
        );

        if let Some(folder) = &self.meeting_folder {
            if let Err(e) = self.write_transcripts_json(folder) {
                warn!("Failed to write transcripts.json after replace: {}", e);
            }
        }
    }

```

- [ ] **Step 4: Chạy test, xác nhận PASS**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml audio::recording_saver
```
Expected: 3 test PASS.

- [ ] **Step 5: Thêm wrapper trong `RecordingManager`**

Trong `frontend/src-tauri/src/audio/recording_manager.rs`, ngay sau `add_transcript_segment` (dòng 453-456):

```rust
    /// Add a structured transcript segment to be saved later
    pub fn add_transcript_segment(&self, segment: super::recording_saver::TranscriptSegment) {
        self.recording_saver.add_transcript_segment(segment);
    }
```

thêm ngay bên dưới:

```rust
    /// Replace raw ASR segments with 1 CAPU-finalized segment — called by the CAPU
    /// background stage (Stage 2) once a batch finishes. See
    /// `RecordingSaver::replace_transcript_segments`.
    pub fn replace_transcript_segments(
        &self,
        source_ids: &[u64],
        finalized_text: String,
        audio_start_time: f64,
        audio_end_time: f64,
    ) {
        self.recording_saver.replace_transcript_segments(
            source_ids,
            finalized_text,
            audio_start_time,
            audio_end_time,
        );
    }
```

- [ ] **Step 6: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml
```
Expected: biên dịch thành công.

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/audio/recording_saver.rs frontend/src-tauri/src/audio/recording_manager.rs
git commit -m "feat(perf): add replace_transcript_segments to merge CAPU-finalized batches"
```

---

### Task 7: `transcription/worker.rs` — Stage 1 bỏ CAPU, thêm Stage 2 chạy nền

**Files:**
- Modify: `frontend/src-tauri/src/audio/transcription/worker.rs`
- Modify: `frontend/src-tauri/src/audio/transcription/mod.rs`

- [ ] **Step 1: Xoá `CAPU_TRAILING_CONTEXT`/`reset_capu_context`, thêm import**

Trong `frontend/src-tauri/src/audio/transcription/worker.rs`, xoá đoạn (dòng 20-27):

```rust
// Trailing-context words carried across VAD segments within one recording session,
// so CAPU punctuation restoration has left-context across segment boundaries.
static CAPU_TRAILING_CONTEXT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Reset the CAPU trailing-context buffer for a new recording session.
pub fn reset_capu_context() {
    CAPU_TRAILING_CONTEXT.lock().unwrap().clear();
}

```

Thêm import ở đầu file, cùng khối `use` hiện có:

```rust
use crate::capu_engine::batch::{CapuBatcher, PendingSegment};
```

Thêm struct sự kiện mới ngay sau `TranscriptUpdate` (giữ cùng style derive):

```rust
/// Emitted by the CAPU background stage (Stage 2) once a batch of raw segments has been
/// punctuated. `recording_commands.rs` listens for this to merge the finalized text into
/// storage, replacing the raw segments it covers.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptFinalized {
    pub source_sequence_ids: Vec<u64>,
    pub text: String,
    pub audio_start_time: f64,
    pub audio_end_time: f64,
}
```

- [ ] **Step 2: Tạo channel CAPU + spawn Stage 2 bên trong `start_transcription_task`**

Trong `start_transcription_task`, ngay trước dòng `// Track completion: AtomicU64 for chunks queued...` (trước `let chunks_queued = ...`), thêm:

```rust
        // Stage 2: CAPU background task. Its receiver is consumed independently of Stage
        // 1's work queue above — Stage 1 only pushes into it, never blocks on it.
        let (capu_sender, capu_receiver) = tokio::sync::mpsc::unbounded_channel::<PendingSegment>();
        let capu_stage_handle = spawn_capu_background_stage(app.clone(), capu_receiver);

```

Trong khối `for worker_id in 0..NUM_WORKERS { ... }`, cùng chỗ các biến khác được `.clone()` trước `tokio::spawn` (`let app_clone = app.clone();` v.v.), thêm:

```rust
            let capu_sender_clone = capu_sender.clone();
```

- [ ] **Step 3: Thay khối CAPU đồng bộ bằng ITN + gửi vào CAPU_QUEUE**

Trong nhánh `Ok((transcript, confidence_opt, is_partial)) => { ... if !transcript.trim().is_empty() && meets_threshold { ... } }`, thay:

```rust
                                        // ITN + CAPU post-processing before emitting.
                                        let mut trailing = CAPU_TRAILING_CONTEXT.lock().unwrap().clone();
                                        let punctuated_text =
                                            crate::audio::post_asr::process_asr_text(&transcript, &mut trailing);
                                        *CAPU_TRAILING_CONTEXT.lock().unwrap() = trailing;

                                        // Emit transcript update with NEW recording-relative timestamps

                                        let update = TranscriptUpdate {
                                            text: punctuated_text,
                                            timestamp: format_current_timestamp(), // Wall-clock for reference
                                            source: "Audio".to_string(),
                                            sequence_id,
                                            chunk_start_time: chunk_timestamp, // Legacy compatibility
                                            is_partial,
                                            confidence: confidence_opt.unwrap_or(0.85), // Default for providers without confidence
                                            // NEW: Recording-relative timestamps for sync
                                            audio_start_time,
                                            audio_end_time,
                                            duration: chunk_duration,
                                        };

                                        if let Err(e) = app_clone.emit("transcript-update", &update)
                                        {
                                            error!(
                                                "Worker {}: Failed to emit transcript update: {}",
                                                worker_id, e
                                            );
                                        }
                                        // PERFORMANCE: Removed verbose logging of every emission
```

bằng:

```rust
                                        // ITN only — CAPU now runs off the hot path (Stage 2 below).
                                        let itn_text = crate::audio::post_asr::apply_itn(&transcript);

                                        // Emit transcript update with NEW recording-relative timestamps

                                        let update = TranscriptUpdate {
                                            text: itn_text.clone(),
                                            timestamp: format_current_timestamp(), // Wall-clock for reference
                                            source: "Audio".to_string(),
                                            sequence_id,
                                            chunk_start_time: chunk_timestamp, // Legacy compatibility
                                            is_partial,
                                            confidence: confidence_opt.unwrap_or(0.85), // Default for providers without confidence
                                            // NEW: Recording-relative timestamps for sync
                                            audio_start_time,
                                            audio_end_time,
                                            duration: chunk_duration,
                                        };

                                        if let Err(e) = app_clone.emit("transcript-update", &update)
                                        {
                                            error!(
                                                "Worker {}: Failed to emit transcript update: {}",
                                                worker_id, e
                                            );
                                        }
                                        // PERFORMANCE: Removed verbose logging of every emission

                                        if let Err(e) = capu_sender_clone.send(PendingSegment {
                                            source_id: sequence_id,
                                            raw_text: itn_text,
                                            audio_start_time,
                                            audio_end_time,
                                        }) {
                                            warn!(
                                                "Worker {}: failed to enqueue segment {} for CAPU: {}",
                                                worker_id, sequence_id, e
                                            );
                                        }
```

- [ ] **Step 4: Đóng CAPU queue + đợi Stage 2 sau khi Stage 1 xong**

Tìm khối cuối cùng của `start_transcription_task` (ngay trước dòng `info!("✅ Parallel transcription task completed - all workers finished, ready for model unload");`), thêm ngay trước nó:

```rust
        // Stage 1 fully done — every worker's capu_sender clone already dropped when that
        // worker task returned above. Dropping this original closes the channel, letting
        // Stage 2 flush its last partial batch and exit on its own.
        drop(capu_sender);
        if let Err(e) = capu_stage_handle.await {
            error!("CAPU background stage panicked: {:?}", e);
        }

```

- [ ] **Step 5: Thêm hàm `spawn_capu_background_stage` + `flush_batch`**

Thêm 2 hàm mới ở cuối file (trước hàm `format_current_timestamp`, hoặc ngay sau `transcribe_chunk_with_provider` — chèn trước `/// Format current timestamp (wall-clock time)`):

```rust
/// Stage 2: runs CAPU off the live ASR hot path. Consumes raw (post-ITN) segments from
/// `receiver`, batches them via `CapuBatcher` up to `CAPU_BATCH_WORD_BUDGET` words, or
/// flushes early if no new segment arrives within `CAPU_BATCH_DEBOUNCE_SECS` (standard
/// debounce: the timer restarts on every new segment, so a batch flushes once speech goes
/// quiet for that long, even if the word budget was never reached). Exits once `receiver`
/// closes, after flushing whatever is still pending.
fn spawn_capu_background_stage<R: Runtime>(
    app: AppHandle<R>,
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<PendingSegment>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut batcher = CapuBatcher::new();
        let word_budget = crate::config::CAPU_BATCH_WORD_BUDGET;
        let debounce = tokio::time::Duration::from_secs(crate::config::CAPU_BATCH_DEBOUNCE_SECS);

        loop {
            let flush_now = tokio::select! {
                maybe_seg = receiver.recv() => {
                    match maybe_seg {
                        Some(seg) => {
                            batcher.push(seg);
                            batcher.should_flush(word_budget)
                        }
                        None => {
                            flush_batch(&app, &mut batcher);
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(debounce), if !batcher.is_empty() => true,
            };

            if flush_now {
                flush_batch(&app, &mut batcher);
            }
        }

        info!("CAPU background stage finished");
    })
}

/// Runs CAPU over whatever `batcher` has pending (if any) and emits `transcript-finalized`
/// with the result. If no CAPU engine is loaded at all, discards the pending batch instead
/// of growing memory forever — the raw text was already emitted live by Stage 1.
fn flush_batch<R: Runtime>(app: &AppHandle<R>, batcher: &mut CapuBatcher) {
    let Some(engine_arc) = crate::capu_engine::commands::get_engine_arc() else {
        batcher.discard_pending();
        return;
    };

    let finalized = {
        let mut engine = engine_arc.lock().unwrap();
        batcher.flush(&mut engine)
    };

    let Some(finalized) = finalized else {
        return;
    };

    let payload = TranscriptFinalized {
        source_sequence_ids: finalized.source_ids,
        text: finalized.text,
        audio_start_time: finalized.audio_start_time,
        audio_end_time: finalized.audio_end_time,
    };

    if let Err(e) = app.emit("transcript-finalized", &payload) {
        error!("Failed to emit transcript-finalized event: {}", e);
    }
}
```

- [ ] **Step 6: Re-export `TranscriptFinalized`**

Trong `frontend/src-tauri/src/audio/transcription/mod.rs`, thay:

```rust
pub use worker::{reset_speech_detected_flag, start_transcription_task, TranscriptUpdate};
```

bằng:

```rust
pub use worker::{reset_speech_detected_flag, start_transcription_task, TranscriptFinalized, TranscriptUpdate};
```

- [ ] **Step 7: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml
```
Expected: biên dịch thành công. Lỗi còn lại (nếu có) sẽ ở `recording_commands.rs` do vẫn gọi `reset_capu_context()` — sửa ở Task 8.

- [ ] **Step 8: Commit**

```bash
git add frontend/src-tauri/src/audio/transcription/worker.rs frontend/src-tauri/src/audio/transcription/mod.rs
git commit -m "feat(perf): split live worker into ASR Stage 1 + CAPU background Stage 2"
```

---

### Task 8: `recording_commands.rs` — lắng nghe `transcript-finalized`, dọn `reset_capu_context`

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_commands.rs`

- [ ] **Step 1: Thêm static cho listener mới**

Thay dòng 66:

```rust
static TRANSCRIPT_LISTENER_ID: Mutex<Option<tauri::EventId>> = Mutex::new(None);
```

bằng:

```rust
static TRANSCRIPT_LISTENER_ID: Mutex<Option<tauri::EventId>> = Mutex::new(None);
static TRANSCRIPT_FINALIZED_LISTENER_ID: Mutex<Option<tauri::EventId>> = Mutex::new(None);
```

- [ ] **Step 2: Xoá 2 lời gọi `reset_capu_context()`**

Tìm và xoá dòng sau tại **cả 2** nơi nó xuất hiện (gần `reset_speech_detected_flag();`, 1 lần trong hàm start bằng thiết bị mặc định, 1 lần trong hàm start bằng thiết bị tuỳ chỉnh):

```rust
    crate::audio::transcription::worker::reset_capu_context();
```

(chỉ xoá đúng dòng này, giữ nguyên `reset_speech_detected_flag();` ngay phía trên nó ở cả 2 chỗ)

- [ ] **Step 3: Đăng ký listener `transcript-finalized` ở cả 2 hàm start**

Ngay sau khối đăng ký listener `"transcript-update"` hiện có (kết thúc bằng `info!("✅ Transcript-update event listener registered for history persistence");`), ở **cả 2** hàm start (mặc định và tuỳ chỉnh thiết bị), thêm:

```rust
    // Listen for transcript-finalized events (CAPU background stage) and merge the
    // finalized segment into the recording manager, replacing the raw segments it covers.
    {
        use tauri::Listener;
        let listener_id = app.listen("transcript-finalized", move |event: tauri::Event| {
            if let Ok(update) =
                serde_json::from_str::<crate::audio::transcription::TranscriptFinalized>(event.payload())
            {
                let manager_guard = RECORDING_MANAGER.lock();
                if let Some(manager) = manager_guard.as_ref() {
                    manager.replace_transcript_segments(
                        &update.source_sequence_ids,
                        update.text,
                        update.audio_start_time,
                        update.audio_end_time,
                    );
                }
            }
        });
        let mut global_listener = TRANSCRIPT_FINALIZED_LISTENER_ID.lock();
        *global_listener = Some(listener_id);
        info!("✅ Transcript-finalized event listener registered for CAPU background stage");
    }
```

- [ ] **Step 4: Dọn listener khi dừng ghi**

Trong `stop_recording`, ngay sau khối:

```rust
    {
        use tauri::Listener;
        if let Some(listener_id) = TRANSCRIPT_LISTENER_ID.lock().take() {
            app.unlisten(listener_id);
            info!("✅ Transcript-update listener removed");
        }
    }
```

thêm:

```rust
    {
        use tauri::Listener;
        if let Some(listener_id) = TRANSCRIPT_FINALIZED_LISTENER_ID.lock().take() {
            app.unlisten(listener_id);
            info!("✅ Transcript-finalized listener removed");
        }
    }
```

- [ ] **Step 5: `cargo check` toàn bộ crate**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml
```
Expected: biên dịch thành công, không còn lỗi nào.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/audio/recording_commands.rs
git commit -m "feat(perf): wire transcript-finalized listener into recording start/stop"
```

---

### Task 9: Kiểm chứng toàn bộ

**Files:** (không sửa code — chỉ chạy lệnh và kiểm tra thủ công)

- [ ] **Step 1: Build toàn bộ crate ở chế độ release-check**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml --all-targets
```
Expected: biên dịch thành công cho cả code lẫn test (bao gồm các test `#[ignore]`).

- [ ] **Step 2: Chạy toàn bộ test tự động (không cần model thật)**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml
```
Expected: tất cả test PASS; các test `#[ignore]` hiện "ignored" (bình thường, cần model thật trên máy).

- [ ] **Step 3: Build app đầy đủ và chạy thử**

```bash
cd frontend
pnpm run tauri:dev
```
Theo dõi log terminal khi build xong và app mở lên.

- [ ] **Step 4: Kiểm thử thủ công — ghi âm ngắn**

Ghi âm ~1 phút nói tiếng Việt. Xác nhận:
- Transcript hiện ngay trong lúc nói (chưa có dấu câu/viết hoa — đúng thiết kế).
- Sau khi bấm dừng, transcript có dấu câu/viết hoa đúng, số đoạn transcript đã gộp lại (ít đoạn hơn, dài hơn) so với trước khi sửa.
- Log terminal có dòng `CAPU background stage finished` và không có `CAPU background stage panicked`.

- [ ] **Step 5: Kiểm thử thủ công — ghi âm dài + đo thời gian**

Ghi âm ~15-20 phút nói liên tục. Đo thời gian từ lúc bấm dừng tới lúc transcript "final" sẵn sàng (log `Parallel transcription task completed`). So với thời gian tương tự trước khi áp dụng plan này (nếu có số liệu cũ) — kỳ vọng giảm rõ rệt vì phần lớn CAPU đã chạy nền trong lúc ghi.

- [ ] **Step 6: Kiểm thử thủ công — ROVER**

Bật ROVER trong Settings (nếu đã cấu hình family B), lặp lại Step 4-5. Xác nhận không treo máy, không lỗi oversubscribe thread, thời gian vẫn cải thiện so với trước.

- [ ] **Step 7: Xác nhận không phá vỡ import/retranscription hiện có**

Chạy thử "Import audio file" và "Retranscribe" 1 cuộc họp cũ (các luồng này KHÔNG đổi trong plan này, vẫn dùng `process_asr_text` per-segment cũ) — xác nhận vẫn hoạt động bình thường như trước, không bị ảnh hưởng bởi các thay đổi ở Task 1-8.
