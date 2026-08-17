# Meeting Speaker Dialog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After file-import diarization, a «Người nói» button opens a dialog to rename speakers, preview ~15s of first speech, and merge split clusters.

**Architecture:** List/merge live in `SpeakersRepository` (new `list_for_meeting` + `merge_into`). Frontend pure helpers own preview window + merge-target rules. Dialog sits on meeting details and drives the existing `AudioPlayer` (seek + play + auto-pause). Keep per-block «Gộp với trước».

**Tech Stack:** Rust sqlx, Tauri commands, Next.js/React, existing Dialog/Input, `node:test` for TS helpers.

---

## Files

| File | Role |
|---|---|
| `frontend/src/lib/speakerPreview.ts` | Preview 15s, list uniqueness, merge targets |
| `frontend/src/lib/speakerPreview.test.ts` | TS unit tests |
| `frontend/src-tauri/src/database/models.rs` | `MeetingSpeakerWithPreview` |
| `frontend/src-tauri/src/database/repositories/speaker.rs` | `list_for_meeting`, `merge_into` + sqlite tests |
| `frontend/src-tauri/src/api/api.rs` | `list_meeting_speakers`, `merge_meeting_speakers` |
| `frontend/src-tauri/src/lib.rs` | Register commands |
| `frontend/src/lib/asr.ts` | `DiarizationAPI.listSpeakers`, `mergeSpeakers` |
| `frontend/src/components/MeetingDetails/AudioPlayer.tsx` | `controlsRef` + `onReady` |
| `frontend/src/components/MeetingDetails/SpeakerListDialog.tsx` | Dialog UI |
| `frontend/src/components/MeetingDetails/TranscriptPanel.tsx` | Button + preview playback |

Do **not** change `merge_with_previous`, live hotkeys, or speaker directory.

---

### Task 1: TS helpers (TDD)

**Files:**
- Create: `frontend/src/lib/speakerPreview.test.ts`
- Create: `frontend/src/lib/speakerPreview.ts`

- [ ] **Step 1: Write failing tests**

Cases:

- `listDetectedSpeakers` skips null `speakerId`, unique by id, `previewStart` = min timestamp, name/color from first segment of that id.
- `previewStopTime(12, 100)` = 17; `previewStopTime(98, 100)` = 100; missing duration = start + 5.
- `mergeTargets` excludes source; empty when only one speaker.
- `shouldShowSpeakerButton` false on `[]`.

- [ ] **Step 2: RED**

`node --test --experimental-strip-types src/lib/speakerPreview.test.ts` from `frontend/`. Expect module not found.

- [ ] **Step 3: Implement helpers — GREEN**

---

### Task 2: SpeakersRepository list + merge (TDD)

**Files:**
- Modify: `frontend/src-tauri/src/database/models.rs`
- Modify: `frontend/src-tauri/src/database/repositories/speaker.rs`

- [ ] **Step 1: Sqlite in-memory tests (max 1 connection)**

Seed 1 meeting, speakers A/B, transcripts. Assert:

- `list_for_meeting` returns both, `preview_start` = min start of that speaker.
- `merge_into(A, B)`: all A transcripts now B; A row gone; B name kept.
- `merge_into` same id / missing / other meeting → false or protocol error.
- `merge_with_previous` still reassigns **one** transcript only.

- [ ] **Step 2: RED then implement `list_for_meeting` + `merge_into`**

---

### Task 3: Tauri + frontend API

**Files:**
- Modify: `frontend/src-tauri/src/api/api.rs`
- Modify: `frontend/src-tauri/src/lib.rs`
- Modify: `frontend/src/lib/asr.ts`

- [ ] `list_meeting_speakers(meetingId)` → `Vec<MeetingSpeakerWithPreview>`
- [ ] `merge_meeting_speakers(sourceSpeakerId, targetSpeakerId)`
- [ ] `DiarizationAPI.listSpeakers` / `mergeSpeakers`

---

### Task 4: Play 15s via existing player

**Files:**
- Modify: `frontend/src/components/MeetingDetails/AudioPlayer.tsx`
- Modify: `frontend/src/components/MeetingDetails/TranscriptPanel.tsx`

- [ ] Add `controlsRef: { seek, play, pause }` and `onReady` when `duration > 0`. Keep `seekRef`.
- [ ] Preview: show player → wait ready → seek start → play → `timeupdate` pause at `previewStopTime`.

---

### Task 5: Dialog + header button

**Files:**
- Create: `frontend/src/components/MeetingDetails/SpeakerListDialog.tsx`
- Modify: `frontend/src/components/MeetingDetails/TranscriptPanel.tsx`

- [ ] Icon button (Users), title «Người nói», chỉ khi `meetingId` và list speakers không rỗng (load khi mount / sau refetch).
- [ ] Row: color, Input, Play, Gộp + `<select>` đích.
- [ ] Rename blur/Enter; merge then `onRefetchTranscripts`; Play calls panel preview.

Style: match `MeetingDocumentsDialog` (p-0, header mono label).

---

### Task 6: Verify + restart

- [ ] `node --test --experimental-strip-types src/lib/speakerPreview.test.ts src/lib/transcriptDisplay.test.ts src/lib/transcriptAudioSync.test.ts src/lib/speakerHotkeys.test.ts src/lib/speakerDirectory.test.ts`
- [ ] `cargo test --manifest-path frontend/src-tauri/Cargo.toml --lib database::repositories::speaker -- --nocapture`
- [ ] Restart: stop current `tauri:dev`, then `pnpm run tauri:dev` from `frontend/` (do **not** use `clean_run_windows.bat`).
