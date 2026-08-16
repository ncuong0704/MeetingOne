# Live Playback Highlight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sau ghi trực tiếp, highlight câu trên trang chi tiết khớp `audio.mp4` giống luồng nhập file.

**Architecture:** Giữ UI sync. Sửa hai nguồn lệch: (1) merge checkpoint AAC re-encode thay vì `-c copy`; (2) sau CAPU live, tách câu và map thời gian từ utterance PCM như `sentence_segment` của import.

**Tech Stack:** Rust (Tauri), ffmpeg concat re-encode, `split_sentences` / `align_sentences_to_words` hiện có.

---

### Task 1: Utterance → TimedWord + tách câu (giây)

**Files:**
- Modify: `frontend/src-tauri/src/audio/sentence_segment.rs`

- [ ] Test: nội suy đều từ trong `[start, end]` theo số từ
- [ ] Test: văn có dấu câu → 2 span giây, không trùng lộn xộn
- [ ] `utterances_to_timed_words` + `split_punctuated_onto_utterances` (align trả ms → chia 1000)

### Task 2: Live CAPU emit từng câu

**Files:**
- Modify: `frontend/src-tauri/src/capu_engine/batch.rs` (`pending_segments()`)
- Modify: `frontend/src-tauri/src/capu_engine/live_finalize.rs`

- [ ] Sau flush, tách câu; batch không dấu câu vẫn 1 segment (test hiện tại)
- [ ] Test: text giả lập hai câu → `out.len() == 2` và thời gian tăng dần

### Task 3: Rebuild transcript lúc stop

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_saver.rs`
- Modify: `frontend/src-tauri/src/audio/recording_manager.rs`
- Modify: `frontend/src-tauri/src/audio/recording_commands.rs`

- [ ] Test: N câu thay M raw; `user_edited` giữ nguyên
- [ ] `apply_live_capu_results`; unlisten `transcript-finalized` **trước** apply

### Task 4: FFmpeg concat re-encode

**Files:**
- Modify: `frontend/src-tauri/src/audio/incremental_saver.rs`

- [ ] Test: args có `-c:a aac`, không có `-c copy`
- [ ] Dùng helper cho `merge_checkpoints` và recovery

### Task 5: Verify + restart

- [ ] `cargo test` sentence_segment / live_finalize / recording_saver / incremental_saver
- [ ] Restart `pnpm run tauri:dev`
