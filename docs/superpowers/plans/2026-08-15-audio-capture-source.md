# Audio Capture Source Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** User picks Microphone, System audio, or both via a select; live mic toggle is removed.

**Architecture:** Persist `audio_source` on recording preferences. One resolver maps source + optional device names to `Option<mic>` / `Option<system>`. UI select on the record bar and Settings Chung share Config state.

**Tech Stack:** Rust (Tauri commands, serde), React/TypeScript, existing shadcn Select.

---

### Task 1: Enum + unit tests (Rust)

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_preferences.rs`

- [ ] Add `AudioCaptureSource` (`microphone` / `system` / `both`), `wants_microphone` / `wants_system`, `from_legacy_mic_enabled`
- [ ] Field `audio_source` on `RecordingPreferences` with `#[serde(default)]`
- [ ] Tests for flags, default both, legacy mapping

### Task 2: Unify start resolver

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_commands.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] Resolve devices from `audio_source` + names; `None` name = OS default **only if** source wants that channel
- [ ] `start_recording` (tray) reads persisted `audio_source`
- [ ] Command accepts `audioSource`; fallback `mic_enabled`

### Task 3: Frontend state + start

**Files:**
- Create: `frontend/src/lib/audioCaptureSource.ts`
- Modify: ConfigContext, recordingService, useRecordingStart, configService types, page.tsx

- [ ] Shared labels + flags helper
- [ ] Replace `micEnabled` with `audioCaptureSource`; load/save with preferences
- [ ] Start passes `audioSource`; block microphone-only without permission
- [ ] Remove session live-mute / auto-disable-mic effect

### Task 4: UI

**Files:**
- Modify: `RecordingControls.tsx`, `RecordingSettings.tsx`, `DeviceSelection.tsx` (disable unused dropdowns)

- [ ] Select 3 options on record bar (short labels); disable while recording
- [ ] Remove both mic toggle buttons
- [ ] Full labels on Chung; disable unused device dropdowns
- [ ] Đánh giá only when source includes mic

### Task 5: Verify + restart

- [ ] `cargo test` audio_source / recording_preferences / mic_quality
- [ ] Restart `pnpm run tauri:dev`
