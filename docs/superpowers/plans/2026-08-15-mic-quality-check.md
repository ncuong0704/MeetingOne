# Mic Quality Check Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** MeetingOne đánh giá mic trước live bằng DNSMOS + VAD + ASR-Proxy giống test ASR.

**Architecture:** Rust `audio/mic_quality.rs` ghi 10s mic → VAD file-batch → DNSMOS ONNX → ASR-Proxy nếu model đã load. UI dialog ghi/kết quả. Commands Tauri; unit test thuần không ONNX.

**Tech Stack:** Rust (ort, silero_rs, cpal, sherpa-onnx), React/TS, Dialog UI sẵn có.

---

### Task 1: Analyzer thuần + unit test (không ONNX)

**Files:**
- Create: `frontend/src-tauri/src/audio/mic_quality.rs`
- Modify: `config.rs`, `audio/mod.rs`, `Cargo.toml` (`sha2`)

- [ ] Polynomial DNSMOS + clip 1–5; pad/truncate 144160; sliding 50%.
- [ ] Gợi ý và `is_ready` giống test ASR; thiếu ASR score → chỉ OVRL.
- [ ] `cargo test mic_quality -- --nocapture`

### Task 2: Ghi mic + DNSMOS + commands

**Files:**
- Modify: `mic_quality.rs`, `lib.rs` generate_handler

- [ ] Download DNSMOS SHA-256; record 10s; VAD; infer; progress events.
- [ ] Cấm khi đang ghi họp. ASR-Proxy streaming nếu loaded, else offline.
- [ ] Commands: `mic_quality_is_model_ready`, `mic_quality_download_model`, `mic_quality_analyze`, `mic_quality_cancel`.

### Task 3: Frontend

**Files:**
- Create: `MicQualityDialog.tsx`, `lib/micQuality.ts`
- Modify: `DeviceSelection.tsx`, `RecordingControls.tsx`

- [ ] Nút Đánh giá; dialog hướng dẫn/progress/kết quả; hỏi tải model.

### Task 4: Verify + restart

- [ ] `cargo test mic_quality` + `live_speaker` + `live_finalize` (không regress).
- [ ] Restart `pnpm run tauri:dev` từ `frontend/`.
