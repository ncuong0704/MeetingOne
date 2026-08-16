# Speaker directory — Implementation Plan

> **For agentic workers:** Execute inline. Spec: [2026-08-16-speaker-directory-design.md](../specs/2026-08-16-speaker-directory-design.md)

**Goal:** Tab Cài đặt «Danh sách» CRUD người nói; ô tên trong dialog 1–9 gợi ý từ danh bạ.

**Architecture:** JSON local + helper lọc thuần TS; combobox tái sử dụng trên 9 ô hotkey.

**Tech Stack:** Tauri command, TypeScript, `node:test`.

---

### Task 1: Model + gợi ý (test trước)

- Create: `frontend/src/lib/speakerDirectory.ts`
- Create: `frontend/src/lib/speakerDirectory.test.ts`

### Task 2: Persist Rust

- Create: `frontend/src-tauri/src/audio/transcription/speaker_directory.rs`
- Modify: `transcription/mod.rs`, `lib.rs` (đăng ký `get_speaker_directory` / `save_speaker_directory`)

### Task 3: UI Cài đặt + combobox

- Create: `SpeakerDirectorySettings.tsx`, `SpeakerNameCombobox.tsx`
- Modify: `settings/page.tsx`, `SpeakerHotkeyDialog.tsx`

### Task 4: Test + restart

- `npx tsx --test src/lib/speakerDirectory.test.ts` cùng các test quanh transcript/guide
- Restart `tauri:dev`
