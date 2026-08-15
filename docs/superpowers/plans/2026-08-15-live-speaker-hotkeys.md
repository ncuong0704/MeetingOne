# Live Speaker Hotkeys Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** MeetingOne live gán người nói bằng phím 1–9 giống test ASR, không nhét token vào text ASR.

**Architecture:** `LiveSpeakerTracker` dùng chung streaming + offline worker. Queue tên → force-finalize utterance cũ → speech sau stamp `speaker_name` trên `transcript-update`. Config JSON trong app data. UI dialog + preview dashed + nhãn đoạn.

**Tech Stack:** Rust (Tauri), React/TS, serde JSON.

---

### Task 1: Tracker thuần + unit test (không ONNX)

**Files:**
- Create: `frontend/src-tauri/src/audio/transcription/live_speaker.rs`
- Modify: `frontend/src-tauri/src/audio/transcription/mod.rs`

- [ ] `queue` bỏ tên rỗng; stamp là speaker hiện tại; `apply_pending` sau finalize; cùng tên vẫn queue; `reset` session; `should_force_endpoint` khi pending.
- [ ] Palette màu theo hash tên (8 màu).
- [ ] Tauri: `get_speaker_hotkeys`, `save_speaker_hotkeys`, `insert_live_speaker`.
- [ ] `cargo test live_speaker -- --nocapture`

### Task 2: Stamp worker + persist + CAPU

**Files:**
- Modify: `streaming_worker.rs`, `worker.rs`, `worker.rs` `TranscriptUpdate`
- Modify: `recording_commands.rs` (listener + reset lúc start/stop)
- Modify: `recording_saver.rs` `TranscriptSegment` + `replace_transcript_segments`
- Modify: `capu_engine/live_finalize.rs` flush khi speaker đổi
- Modify: `lib.rs` generate_handler

- [ ] Streaming: pending → force endpoint → emit cũ → apply → reset.
- [ ] Offline: stamp chunk hiện tại, apply trên final.
- [ ] CAPU: hai speaker khác nhau không chung 1 batch. Test.
- [ ] `cargo test live_finalize -- --nocapture`

### Task 3: Frontend

**Files:**
- Modify: `types/index.ts`, `TranscriptContext.tsx`, `TranscriptPanel.tsx`, `VirtualizedTranscriptView.tsx`, `LiveAsrPanel.tsx`
- Create: `SpeakerHotkeyDialog.tsx`, `useLiveSpeakerHotkeys.ts`, `lib/speakerHotkeys.ts`

- [ ] Dialog 9 ô; hotkey cửa sổ khi recording; preview + nhãn speaker.
- [ ] Map speaker vào upsert `sequence_id`.

### Task 4: Verify + restart

- [ ] Unit test tracker, CAPU, streaming_state (không regress).
- [ ] Restart `pnpm run tauri:dev` từ `frontend/`.
