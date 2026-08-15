# Live Streaming ASR Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MeetingOne live transcribes incrementally like test ASR (`OnlineRecognizer` + Zipformer chunk-64), with partial transcript updates as the user speaks.

**Architecture:** Add a live-only streaming model family. Mixed 48 kHz audio is downsampled to 16 kHz and fed to `sherpa-onnx::OnlineRecognizer` with no VAD. Partials/finals emit `transcript-update` (stable `sequence_id` per utterance). File/offline live path stays on `OfflineRecognizer` + VAD.

**Tech Stack:** Rust (Tauri), sherpa-onnx 1.13.0 `OnlineRecognizer`, React TranscriptContext upsert.

---

### Task 1: Model family + tokens resource

**Files:**
- Modify: `frontend/src-tauri/src/config.rs`
- Modify: `frontend/src-tauri/src/asr_engine/model_family.rs`
- Create: `frontend/src-tauri/resources/zipformer-streaming-tokens.txt`
- Modify: `frontend/src-tauri/tauri.conf.json` (bundle resource)

- [ ] Add `ModelFamily::ZipFormer30MStreaming` and tests for files/subdir/from_id.
- [ ] Bundle tokens copied from test ASR `models/zipformer-30m-rnnt-streaming-6000h/tokens.txt`.

### Task 2: Streaming session state (unit-tested, no ONNX)

**Files:**
- Create: `frontend/src-tauri/src/asr_engine/streaming_state.rs`

- [ ] `StreamingSession::on_hypothesis` emits partial on text change, final+reset on endpoint or max 12s.
- [ ] Tests: partial then final same seq; max duration reset; empty endpoint no emit.

### Task 3: Online engine + worker + pipeline + UI upsert

**Files:**
- Create: `frontend/src-tauri/src/asr_engine/streaming.rs`
- Create: `frontend/src-tauri/src/audio/transcription/streaming_worker.rs`
- Modify: pipeline, recording_manager, recording_commands, asr commands/load/download
- Modify: `frontend/src/contexts/TranscriptContext.tsx`
- Modify: `frontend/src/lib/asr.ts`, `LiveAsrPanel` / File panel filter, `asrSettingsConstants.ts`

- [ ] Live family streaming → downsample mix → OnlineRecognizer; else existing VAD path.
- [ ] UI upsert by `sequence_id`.

### Task 4: Tests + run app

- [ ] `cargo test --lib asr_engine::`
- [ ] Restart `pnpm run tauri:dev`
