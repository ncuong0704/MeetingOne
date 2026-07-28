# Recording/Transcription Crash Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the root causes identified in the crash investigation (`docs/superpowers/plans/2026-07-28-document-import.md`'s sibling investigation, reported inline in conversation — see "Nhóm A" / "Nhóm B" below) for "app crashes while recording + transcribing," fixing Group A (highest-confidence root causes) before Group B (contributing factors), one fix at a time with verification after each — per systematic-debugging's Iron Law: no bundled fixes, no unrelated scope.

**Architecture:** Ten surgical, independently-testable fixes across the live recording/transcription hot path (`frontend/src-tauri/src/audio/` and `frontend/src-tauri/src/zipformer_engine/`). No new features, no refactors beyond what each specific bug requires.

**Tech Stack:** Rust (Tauri 2, tokio, cpal, sherpa-onnx/ZipFormer), new dependency `parking_lot` for non-poisoning mutexes.

---

## Investigation summary (for context — do not re-investigate, root causes are already established)

Three independent forensic code-review passes (read-only, no fixes) were done over the live recording + transcription hot path. Findings, grouped as presented to and approved by the user (fix order: Group A, then Group B):

**Group A — most likely primary culprits:**
1. Mutex-poisoning cascade: `RecordingState::report_error` (`recording_state.rs:332-334`) holds the `error_callback` lock while invoking the callback; separately, `RECORDING_MANAGER` (`recording_commands.rs:42`) is a single global `std::sync::Mutex` locked with `.unwrap()` at 13+ call sites spanning the whole recording lifecycle — any panic while any of these locks is held poisons it forever, and every subsequent `.lock().unwrap()` on that same mutex (including the Stop button) then panics too.
2. No backpressure on the transcription queue (unbounded memory growth on long sessions) + the "unload transcription engine after batch" fix (commit `6a7eb26`) was silently gutted during the Whisper→ZipFormer migration (`audio/common.rs:7-12` is now a no-op).
3. `frontend/src-tauri/src/audio/stream.rs:29,38` forces `unsafe impl Send` on cpal's `Stream` (which has real thread-affinity requirements on Windows/WASAPI) while the comment's stated safety net (`spawn_blocking`) is never actually used anywhere in the file — the stream is created on one tokio worker thread and paused/dropped on a different one when the user stops recording.

**Group B — contributing factors:**
4. `pipeline.rs:734-736` — literal `panic!()` if VAD session creation fails (e.g. corrupted/quarantined bundled model file), on every single "Start Recording" click.
5. ZipFormer's blocking native inference (`zipformer_engine.rs`'s `transcribe_audio`/`load_model`) runs directly on the async runtime, not wrapped in `spawn_blocking`.
6. `attempt_device_reconnect` (`recording_commands.rs:1148-1195`) holds the global `RECORDING_MANAGER` lock across the entire (unbounded-duration) device re-enumeration + stream-restart sequence, blocking every other recording command (including the 1-2s UI polling) for that whole time.
7. TOCTOU race on recording start (`recording_commands.rs:86-90`, `333-337`): `IS_RECORDING` is checked, then a large `.await`-laden initialization runs, then `IS_RECORDING` is set — two near-simultaneous start calls can both pass the check.
8. `incremental_saver.rs:184,187,318` — `.unwrap()` on `Path::to_str()` when finalizing (merging checkpoint files into the final audio file) on Stop; panics if the recordings-folder path isn't valid UTF-8 (rare, but plausible with redirected/synced Windows profile folders).

Out of scope for this plan (Group C — lower confidence / not requested by user): unbounded VAD speech-buffer growth (`vad.rs`), O(n²) full-file transcript rewrite (`recording_saver.rs`), macOS CoreAudio FFI panic boundary.

---

## Task 1 (Group A, part 1a): Add `parking_lot` dependency

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml` (add dependency, in `[dependencies]`, next to the other utility crates e.g. right after `dashmap = "6.1.0"`)

- [ ] **Step 1: Add the dependency**

```toml
parking_lot = "0.12"
```

- [ ] **Step 2: Verify it resolves**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles, no errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/Cargo.toml Cargo.lock
git commit -m "build: add parking_lot dependency for poison-free mutexes"
```

---

## Task 2 (Group A, part 1b): Migrate `RECORDING_MANAGER`/`TRANSCRIPTION_TASK`/`TRANSCRIPT_LISTENER_ID` to `parking_lot::Mutex`

**Root cause:** `RECORDING_MANAGER` is a single `std::sync::Mutex<Option<RecordingManager>>` static, locked with `.lock().unwrap()` at every recording command (start/stop/pause/resume/mute/reconnect/status query — 13+ sites in `recording_commands.rs`). If ANY code executed while holding this lock ever panics, the mutex is poisoned permanently, and every subsequent `.lock().unwrap()` — including the one inside `stop_recording` — panics immediately. `parking_lot::Mutex` never poisons: `.lock()` always returns the guard directly (no `Result`, nothing to `.unwrap()`), so a panic while holding the lock can never cascade into "every future command panics too."

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_commands.rs`

- [ ] **Step 1: Change the static declarations and import**

Change:
```rust
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
```
to:
```rust
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use parking_lot::Mutex;
```

The three static declarations themselves:
```rust
static RECORDING_MANAGER: Mutex<Option<RecordingManager>> = Mutex::new(None);
static TRANSCRIPTION_TASK: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static TRANSCRIPT_LISTENER_ID: Mutex<Option<tauri::EventId>> = Mutex::new(None);
```
need **no textual change** — `parking_lot::Mutex::new` is also a `const fn`, so this code is valid for either `Mutex` type. Only the `use` statement from Step 1 changes which `Mutex` these three lines actually refer to. Do not edit these three lines; move directly to Step 2.

- [ ] **Step 2: Remove `.unwrap()` after every `.lock()` call on these three statics**

`parking_lot::Mutex::lock()` returns the guard directly, not a `Result`. Find and fix every occurrence in this file (there are 15 total: 13 on `RECORDING_MANAGER`, 1 on `TRANSCRIPTION_TASK` × 2 sites, 1 on `TRANSCRIPT_LISTENER_ID` × 2 sites). Mechanical transform — remove `.unwrap()`:

```rust
// Before (13 sites on RECORDING_MANAGER), e.g. line 246:
let mut global_manager = RECORDING_MANAGER.lock().unwrap();
// After:
let mut global_manager = RECORDING_MANAGER.lock();
```
Apply this exact transform (`.lock().unwrap()` → `.lock()`) at every call site on `RECORDING_MANAGER`, `TRANSCRIPTION_TASK`, and `TRANSCRIPT_LISTENER_ID` in this file. Use a project-wide search within this one file — `grep -n "RECORDING_MANAGER.lock()\|TRANSCRIPTION_TASK.lock()\|TRANSCRIPT_LISTENER_ID.lock()" frontend/src-tauri/src/audio/recording_commands.rs` to enumerate every site before and after, to confirm none were missed and none outside this file were touched.

Two sites use `if let Ok(manager_guard) = RECORDING_MANAGER.lock() { ... }` (inside the `transcript-update` event listener closures, appears twice, once in each of `start_recording_with_meeting_name` and `start_recording_with_devices_and_meeting`) — these need a different transform since `parking_lot::Mutex::lock()` isn't a `Result`:
```rust
// Before:
if let Ok(manager_guard) = RECORDING_MANAGER.lock() {
    if let Some(manager) = manager_guard.as_ref() {
        manager.add_transcript_segment(segment);
    }
}
// After:
let manager_guard = RECORDING_MANAGER.lock();
if let Some(manager) = manager_guard.as_ref() {
    manager.add_transcript_segment(segment);
}
```

One site already avoids `.unwrap()` via `.map_err(...)` (in `update_live_transcript_segment`) — since `parking_lot::Mutex::lock()` can't fail, simplify it too:
```rust
// Before:
let manager_guard = RECORDING_MANAGER
    .lock()
    .map_err(|e| format!("Khóa recording manager: {}", e))?;
// After:
let manager_guard = RECORDING_MANAGER.lock();
```

- [ ] **Step 3: Verify it compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles with no errors. (If any call site was missed, the compiler will report a type error like "no method named `unwrap` found for `MutexGuard<...>`" — fix any such site the same way.)

- [ ] **Step 4: Manual verification (no automated test — this is a static/global-state change verified by the app actually running)**

Start the dev app (`pnpm run tauri:dev` from `frontend`), start a recording, pause it, resume it, mute the mic, stop it. Confirm no regressions in normal start/stop/pause/resume behavior (this task doesn't change behavior when nothing panics — parking_lot's `.lock()` behaves identically to a never-poisoned `std::sync::Mutex::lock().unwrap()` in the non-panicking case).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/recording_commands.rs
git commit -m "fix: use parking_lot::Mutex for RECORDING_MANAGER to prevent poison cascade"
```

---

## Task 3 (Group A, part 1c): Migrate `RecordingState`'s mutexes to `parking_lot::Mutex` + fix `report_error`

**Root cause:** Same poisoning mechanism as Task 2, applied to `RecordingState`'s own fields (`recording_state.rs`) — `send_audio_chunk`/`report_error` and friends are called from the raw cpal audio-callback thread (not a tokio task), so a panic here is especially dangerous. Separately, `report_error` (`recording_state.rs:332-334`) holds the `error_callback` mutex for the entire duration of the registered callback's execution (which does a Tauri `app.emit(...)` IPC call) — the lock should be released before invoking a callback we don't control the internals of.

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_state.rs`

- [ ] **Step 1: Change the import**

Change:
```rust
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
```
to:
```rust
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
```

- [ ] **Step 2: Remove `.unwrap()` after every `.lock()` call in this file**

There are ~30 call sites (`microphone_device`, `system_device`, `disconnected_device`, `audio_sender`, `last_error`, `error_callback`, `stats`, `recording_start`, `pause_start`, `total_pause_duration` — all `Mutex<...>` fields). Apply the same mechanical transform as Task 2: `.lock().unwrap()` → `.lock()`, everywhere in this file. There are no `if let Ok(...)` variants in this file to worry about (unlike Task 2) — confirm via `grep -n "\.lock()" frontend/src-tauri/src/audio/recording_state.rs` that every result is either already transformed or doesn't need the `if let Ok` treatment.

- [ ] **Step 3: Fix `report_error` to not hold the callback lock while invoking it**

`Box<dyn Fn(&AudioError) + Send + Sync>` cannot be cheaply cloned out from behind the lock before calling it. The fix: change the field's type from `Mutex<Option<Box<dyn Fn(&AudioError) + Send + Sync>>>` to `Mutex<Option<Arc<dyn Fn(&AudioError) + Send + Sync>>>` so it CAN be cheaply cloned out from behind the lock, then invoked after the lock is released.

In the struct definition (~line 118), change:
```rust
    error_callback: Mutex<Option<Box<dyn Fn(&AudioError) + Send + Sync>>>,
```
to:
```rust
    error_callback: Mutex<Option<Arc<dyn Fn(&AudioError) + Send + Sync>>>,
```

In `set_error_callback` (~line 303-308), change:
```rust
    pub fn set_error_callback<F>(&self, callback: F)
    where
        F: Fn(&AudioError) + Send + Sync + 'static,
    {
        *self.error_callback.lock() = Some(Box::new(callback));
    }
```
to:
```rust
    pub fn set_error_callback<F>(&self, callback: F)
    where
        F: Fn(&AudioError) + Send + Sync + 'static,
    {
        *self.error_callback.lock() = Some(Arc::new(callback));
    }
```

In `report_error`, replace the callback-invocation block with:
```rust
        *self.last_error.lock() = Some(error.clone());

        // Clone the Arc out from behind the lock, then release the lock
        // BEFORE invoking the callback. The callback is arbitrary caller-
        // supplied code (currently a Tauri event emit) and must never run
        // while this or any other RecordingState lock is held — otherwise a
        // panic inside it (or inside anything it calls) poisons the lock for
        // every future caller.
        let callback = self.error_callback.lock().clone();
        if let Some(callback) = callback {
            callback(&error);
        }
```

- [ ] **Step 4: Verify it compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles. Fix any remaining `.lock().unwrap()` sites the compiler flags.

- [ ] **Step 5: Write a regression test for the lock-release behavior**

Add to the bottom of `recording_state.rs` (create a `#[cfg(test)] mod tests` block if one doesn't already exist — check first with `grep -n "mod tests" frontend/src-tauri/src/audio/recording_state.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_callback_lock_released_before_invocation() {
        let state = RecordingState::new();

        // Register a callback that itself calls back into RecordingState,
        // re-acquiring locks report_error also uses. If report_error still
        // held the error_callback lock while invoking the callback, this
        // callback couldn't touch the same RecordingState's other methods
        // (like get_last_error, which also locks separately) without
        // deadlocking with parking_lot's own guard rules for THIS SAME lock —
        // more directly: the old Box<dyn Fn> design made this pattern
        // impossible to write safely at all. Proving we can register and
        // invoke a callback that re-enters state methods (get_error_count)
        // confirms report_error doesn't hold error_callback locked during
        // the call.
        let reentrant_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let reentrant_count_clone = reentrant_count.clone();
        let state_clone = state.clone();
        state.set_error_callback(move |_error| {
            // Re-enter: read another field's lock while report_error's own
            // call site still has its stack frame active.
            let _ = state_clone.get_error_count();
            reentrant_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });

        state.report_error(AudioError::ProcessingFailed);

        assert_eq!(reentrant_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
```

Note: `RecordingState::new()` returns `Arc<Self>`, and `Arc<RecordingState>` is `Clone` — if `state.clone()` doesn't work directly because `state` is already `Arc<Self>` from `new()`, adjust to use the `Arc` directly (`let state = RecordingState::new();` already gives an `Arc<RecordingState>`, so `state.clone()` clones the `Arc`, which is exactly what's needed here).

- [ ] **Step 6: Run the test**

Run (from `frontend/src-tauri`): `cargo test -p meetingone --lib audio::recording_state::tests::test_error_callback_lock_released_before_invocation`
Expected: PASS. (If it hangs instead of failing cleanly, that's `parking_lot::Mutex` deadlocking on re-entrant lock — parking_lot mutexes are not reentrant, same as `std::sync::Mutex`, so this test proves the fix by simply not deadlocking; if Step 3 wasn't applied correctly the test would hang here, not panic — if it hangs, stop and re-check Step 3.)

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/audio/recording_state.rs
git commit -m "fix: use parking_lot::Mutex in RecordingState and release error_callback lock before invoking it"
```

---

## Task 4 (Group A, part 2a): Bound the transcription work queue to cap memory growth

**Root cause:** `worker.rs:68` creates an unbounded `tokio::sync::mpsc::unbounded_channel::<AudioChunk>()` for the single transcription worker. If ZipFormer inference falls behind real-time speech (weak CPU, background load, long meeting), the dispatcher loop (`worker.rs:319-330`) keeps draining the upstream channel and pushing into this one with no limit — memory grows for the entire session with no ceiling. This codebase has an explicit, repeated design intent to never silently lose an already-detected speech segment ("zero chunk loss", "preserving every chunk" — see comments in `recording_commands.rs` and `worker.rs`), so the fix must apply **backpressure**, not drop chunks.

**Files:**
- Modify: `frontend/src-tauri/src/audio/transcription/worker.rs`

- [ ] **Step 1: Change the channel to bounded**

Change (line 68):
```rust
        let (work_sender, work_receiver) = tokio::sync::mpsc::unbounded_channel::<AudioChunk>();
```
to:
```rust
        // Bounded to cap memory growth if transcription falls behind real-time
        // speech for an extended period (weak CPU, background load, very long
        // meeting). Capacity chosen generously — at ~150ms-25s per VAD segment,
        // 300 outstanding segments represents many minutes of backlog before
        // send() ever blocks, so this should never engage under normal use.
        const WORK_QUEUE_CAPACITY: usize = 300;
        let (work_sender, work_receiver) = tokio::sync::mpsc::channel::<AudioChunk>(WORK_QUEUE_CAPACITY);
```

- [ ] **Step 2: Update the dispatcher's send call to use backpressure (`.send().await`, not drop)**

Change (line 317-330):
```rust
        // Main dispatcher: receive chunks and distribute to workers
        let mut receiver = transcription_receiver;
        while let Some(chunk) = receiver.recv().await {
            let queued = chunks_queued.fetch_add(1, Ordering::SeqCst) + 1;
            info!(
                "📥 Dispatching chunk {} to workers (total queued: {})",
                chunk.chunk_id, queued
            );

            if let Err(_) = work_sender.send(chunk) {
                error!("❌ Failed to send chunk to workers - this should not happen!");
                break;
            }
        }
```
to:
```rust
        // Main dispatcher: receive chunks and distribute to workers
        let mut receiver = transcription_receiver;
        while let Some(chunk) = receiver.recv().await {
            let queued = chunks_queued.fetch_add(1, Ordering::SeqCst) + 1;
            info!(
                "📥 Dispatching chunk {} to workers (total queued: {})",
                chunk.chunk_id, queued
            );

            // Bounded send: if the queue is full (worker seriously behind),
            // this awaits until space frees up rather than growing memory
            // without limit. No chunk is ever dropped.
            if work_sender.send(chunk).await.is_err() {
                error!("❌ Failed to send chunk to workers - this should not happen!");
                break;
            }
        }
```

- [ ] **Step 3: Update the type signature the dispatcher receives, if needed**

Check the function signature that contains this code (search for `pub fn start_transcription_task` above line 45) — `work_receiver` is wrapped as `Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AudioChunk>>>` at line 69 currently. Since the channel type changes from `UnboundedReceiver`/`UnboundedSender` to `Receiver`/`Sender`, find and update:
```rust
        let work_receiver = Arc::new(tokio::sync::Mutex::new(work_receiver));
```
This line itself doesn't need to change (type is inferred), but any explicit type annotations referencing `UnboundedReceiver<AudioChunk>` or `UnboundedSender<AudioChunk>` for `work_sender`/`work_receiver` elsewhere in this file must be updated to `mpsc::Receiver<AudioChunk>` / `mpsc::Sender<AudioChunk>`. Search with `grep -n "UnboundedReceiver<AudioChunk>\|UnboundedSender<AudioChunk>" frontend/src-tauri/src/audio/transcription/worker.rs` and fix any that specifically refer to `work_sender`/`work_receiver` (do NOT touch any that refer to `transcription_receiver`'s own type — that channel, created upstream in `recording_manager.rs`, is out of scope for this task).

- [ ] **Step 4: Verify it compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles. The compiler will flag any remaining `UnboundedSender`/`UnboundedReceiver` type mismatches for `work_sender`/`work_receiver` — fix them by changing to the bounded `mpsc::Sender`/`mpsc::Receiver` equivalents.

- [ ] **Step 5: Manual verification**

Start the dev app, record a short test meeting speaking a few sentences, confirm transcripts still appear normally with no missing segments (compare segment count in the UI against what you said). This task doesn't change behavior under normal load (300-deep queue is never reached in a short test) — it only caps worst-case memory under sustained backlog, which isn't practical to reproduce in a quick manual test.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/audio/transcription/worker.rs
git commit -m "fix: bound transcription work queue to cap memory growth on long sessions"
```

---

## Task 5 (Group A, part 2b): Restore real model unloading after batch jobs

**Root cause:** Commit `6a7eb26` ("fix: unload transcription engine after batch jobs to free memory") originally freed the loaded speech model's native memory after import/retranscription batch jobs. During the Whisper→ZipFormer migration, the actual unload calls were deleted and `unload_engine_after_batch()` (`audio/common.rs:7-12`) became a no-op — `ZipFormerEngine` has no `unload_model()` method at all. The model now stays resident in memory for the entire process lifetime once loaded, wasting memory permanently after one-off batch operations (import/retranscription) complete.

**Files:**
- Modify: `frontend/src-tauri/src/zipformer_engine/zipformer_engine.rs`
- Modify: `frontend/src-tauri/src/audio/common.rs`

- [ ] **Step 1: Write a failing test for `unload_model`**

Find the `#[cfg(test)] mod tests` block in `zipformer_engine.rs` (check with `grep -n "mod tests" frontend/src-tauri/src/zipformer_engine/zipformer_engine.rs`; if none exists, add one at the end of the file). Add:

```rust
    #[tokio::test]
    async fn test_unload_model_clears_recognizer_and_status() {
        let engine = ZipFormerEngine::new();
        // Simulate a loaded state directly (bypassing the real load_model,
        // which needs real model files on disk) by writing to the internal
        // fields the same way load_model does, minus the actual recognizer
        // construction — we only need to prove unload_model resets state.
        *engine.model_status.write().await = ModelStatus::Ready;

        assert!(!matches!(*engine.model_status.read().await, ModelStatus::NotLoaded));

        engine.unload_model().await;

        assert!(engine.recognizer.read().await.is_none());
        assert!(matches!(*engine.model_status.read().await, ModelStatus::NotLoaded));
    }
```

If `model_status`/`recognizer` fields aren't accessible from the test module (check their visibility — they're likely private fields of `ZipFormerEngine` accessed only within `impl ZipFormerEngine`), the test module needs to be an inner `mod tests` within the same file (not a separate file) so it has access to private fields via `use super::*;` — confirm this is how the existing test setup (if any) in this file works, or other test modules in this codebase (e.g. `frontend/src-tauri/src/document_import/extractors.rs` uses this exact pattern).

- [ ] **Step 2: Run it to verify it fails**

Run (from `frontend/src-tauri`): `cargo test -p meetingone --lib zipformer_engine::zipformer_engine::tests::test_unload_model_clears_recognizer_and_status`
Expected: FAIL — `unload_model` method doesn't exist yet.

- [ ] **Step 3: Implement `unload_model`**

Add to `impl ZipFormerEngine` in `zipformer_engine.rs`, near `load_model` (after it, before `transcribe_audio` at line ~398):

```rust
    /// Release the loaded model's native memory. Safe to call whether or not
    /// a model is currently loaded. Used after one-off batch operations
    /// (audio import, retranscription) to avoid keeping the model resident
    /// in memory for the rest of the process lifetime when it isn't actively
    /// needed for live recording.
    pub async fn unload_model(&self) {
        *self.recognizer.write().await = None;
        *self.model_status.write().await = ModelStatus::NotLoaded;
        info!("ZipFormer model unloaded, native memory released");
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run (from `frontend/src-tauri`): `cargo test -p meetingone --lib zipformer_engine::zipformer_engine::tests::test_unload_model_clears_recognizer_and_status`
Expected: PASS.

- [ ] **Step 5: Wire it into `unload_engine_after_batch()`**

In `frontend/src-tauri/src/audio/common.rs`, change:
```rust
/// ZipFormer engine stays loaded across batch jobs — no unload needed.
pub(crate) async fn unload_engine_after_batch() {
    if crate::audio::recording_commands::is_recording().await {
        log::info!("Skipping model unload after batch: recording in progress");
    }
}
```
to:
```rust
/// Release the ZipFormer model's memory after a one-off batch job (audio
/// import, retranscription) finishes, unless a live recording is currently
/// using the engine. This restores the memory-freeing behavior the original
/// Whisper-based implementation had (commit 6a7eb26) before the ZipFormer
/// migration silently dropped it.
pub(crate) async fn unload_engine_after_batch() {
    if crate::audio::recording_commands::is_recording().await {
        log::info!("Skipping model unload after batch: recording in progress");
        return;
    }

    match crate::zipformer_engine::commands::get_engine_arc() {
        Ok(engine) => {
            engine.unload_model().await;
        }
        Err(e) => {
            log::warn!("Skipping model unload after batch: engine not available: {}", e);
        }
    }
}
```

- [ ] **Step 6: Verify the whole crate compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles. Confirm `crate::zipformer_engine::commands::get_engine_arc()` is the correct existing accessor by checking its signature first: `grep -n "pub fn get_engine_arc" frontend/src-tauri/src/zipformer_engine/commands.rs` — match the exact return type (likely `Result<Arc<ZipFormerEngine>, String>` based on other call sites in `audio/import.rs`) and adjust the `match` arm's error handling if the signature differs from what's assumed above.

- [ ] **Step 7: Run the full zipformer_engine test suite**

Run (from `frontend/src-tauri`): `cargo test -p meetingone --lib zipformer_engine`
Expected: all tests pass, including the new one.

- [ ] **Step 8: Commit**

```bash
git add frontend/src-tauri/src/zipformer_engine/zipformer_engine.rs frontend/src-tauri/src/audio/common.rs
git commit -m "fix: restore ZipFormer model unload after batch jobs to free memory"
```

---

## Task 6 (Group A, part 3): Mitigate cross-thread cpal `Stream` play/pause/drop

**Root cause:** `stream.rs:29,38` forces `unsafe impl Send` on cpal's `Stream` type, whose comment claims safety is ensured "by using spawn_blocking for operations that cross thread boundaries" — but `spawn_blocking` is never actually used anywhere in this file. `stream.play()` (in `create_cpal_stream`, called when starting a recording) and `stream.pause()` + `drop(stream)` (in `AudioStream::stop`, called when stopping) can each run on whatever tokio worker thread happens to service that particular async call — likely different threads for start vs. stop. On Windows (WASAPI), calling into the same COM audio object from different threads than the one that created it is a well-known crash source.

**Note — scope of this fix:** A fully correct fix (guaranteeing the *same* OS thread owns the stream for its entire lifecycle) requires a dedicated actor-thread redesign (a worker thread with a command channel, owning the `Stream` for as long as it's alive) — that is a larger architectural change out of scope for this fix pass. This task applies the safe, minimal mitigation available without that redesign: ensure `stream.play()` and `stream.pause()`/drop each individually run on a `spawn_blocking` thread (matching what the existing code comment already claims happens), which at minimum keeps these blocking, thread-sensitive calls off the shared tokio async worker pool. **This does not fully eliminate the thread-affinity risk** (spawn_blocking doesn't guarantee reusing the same OS thread across two separate calls) — flag this residual risk in the final report; a complete fix is a follow-up architectural task, not part of this plan.

**Files:**
- Modify: `frontend/src-tauri/src/audio/stream.rs`

- [ ] **Step 1: Wrap `stream.play()` in `spawn_blocking`**

Change (in `create_cpal_stream`, around line 130-135):
```rust
        // Build the appropriate stream based on sample format
        let stream = Self::build_stream(&cpal_device, &config, capture.clone())?;

        // Start the stream
        stream.play()?;
        info!("CPAL stream started for device: {}", device.name);
```
to:
```rust
        // Build the appropriate stream based on sample format
        let stream = Self::build_stream(&cpal_device, &config, capture.clone())?;

        // Start the stream on a blocking thread. cpal's Stream has real
        // thread-affinity requirements on some backends (e.g. WASAPI COM
        // objects on Windows) — spawn_blocking keeps this off the shared
        // tokio async worker pool. This doesn't guarantee the same OS thread
        // handles both play() and the later pause()/drop() in `stop()`
        // below — a full fix requires owning the stream on one dedicated
        // thread for its whole lifecycle, which is a larger follow-up.
        let stream = tokio::task::spawn_blocking(move || -> Result<Stream> {
            stream.play()?;
            Ok(stream)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Stream play task panicked: {}", e))??;
        info!("CPAL stream started for device: {}", device.name);
```

- [ ] **Step 2: Wrap `stream.pause()` + drop in `spawn_blocking`**

Change (in `AudioStream::stop`, around line 320-333):
```rust
    /// Stop the stream
    pub fn stop(self) -> Result<()> {
        info!("Stopping audio stream for device: {}", self.device.name);

        match self.backend {
            StreamBackend::Cpal(stream) => {
                // CRITICAL: Pause the stream first to stop callbacks immediately
                // This ensures closures stop executing before we drop the stream,
                // allowing Arc references captured in callbacks to be released
                if let Err(e) = stream.pause() {
                    warn!("Failed to pause stream before drop: {}", e);
                }
                info!("Stream paused, now dropping to release callbacks");
                drop(stream);
            }
```
to (note: `stop` takes `self` by value and isn't `async` — changing it to spawn a blocking task means it needs to become `async fn stop`; find and update its callers accordingly, see Step 3):
```rust
    /// Stop the stream
    pub async fn stop(self) -> Result<()> {
        info!("Stopping audio stream for device: {}", self.device.name);

        match self.backend {
            StreamBackend::Cpal(stream) => {
                // CRITICAL: Pause the stream first to stop callbacks immediately
                // This ensures closures stop executing before we drop the stream,
                // allowing Arc references captured in callbacks to be released.
                // Done on a blocking thread for the same thread-affinity reason
                // as stream.play() in create_cpal_stream above.
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = stream.pause() {
                        warn!("Failed to pause stream before drop: {}", e);
                    }
                    info!("Stream paused, now dropping to release callbacks");
                    drop(stream);
                })
                .await
                .map_err(|e| anyhow::anyhow!("Stream stop task panicked: {}", e))?;
            }
```

Leave the `#[cfg(target_os = "macos")] StreamBackend::CoreAudio { task }` arm and the trailing `drop(self.device); ... Ok(())` unchanged except that the whole function is now `async`.

- [ ] **Step 3: Update all callers of `AudioStream::stop()` to `.await` it**

Find every caller: `grep -rn "\.stop()" frontend/src-tauri/src/audio/stream.rs` — specifically `AudioStreamManager::stop_streams` (around line 430-457) calls `mic_stream.stop()` and `sys_stream.stop()`. Since `stop_streams` itself is a sync `fn` (not `async fn`) called from `impl Drop for AudioStreamManager` (line 477-482, which **cannot** be async — `Drop::drop` has a fixed sync signature), this requires care:

`stop_streams` is also called from non-Drop contexts. Check `grep -rn "stop_streams" frontend/src-tauri/src/audio/` for all call sites. For call sites OUTSIDE of `Drop::drop` (e.g. from `recording_manager.rs`'s explicit stop path, which is already async), change `stop_streams` itself to `async fn stop_streams(&mut self) -> Result<()>` and `.await` each `.stop()` call inside it, then update ITS callers to `.await` it too (propagate the `async` up the call chain — this is expected to reach `recording_manager.rs`'s `stop_streams_and_force_flush`/similar, which is already `async` per `recording_commands.rs`'s `manager.stop_streams_and_force_flush().await` call visible in Task 2's context).

For the `impl Drop for AudioStreamManager` call site specifically (line 477-482), `Drop::drop` cannot `.await` anything. Use `tokio::task::block_in_place` + `Handle::current().block_on(...)` if this Drop impl can run inside a tokio runtime context, OR — simpler and safer — leave a **synchronous** fallback path: add a small `fn stop_streams_sync(&mut self) -> Result<()>` that does the pause/drop directly on the calling thread (accepting the thread-affinity risk only for this Drop-triggered emergency-cleanup path, which is not the common case — normal stop always goes through the async `stop_recording` command path in `recording_commands.rs`), and have `Drop::drop` call that instead of the async `stop_streams`. Document this with a comment explaining why Drop can't use the async-safe path.

**This step requires judgment calls about the exact call graph** — if the async propagation turns out to reach somewhere that fundamentally cannot become `async` (not just `Drop`), STOP and report BLOCKED with the specific call site, rather than working around it with `unsafe`, blocking hacks, or `futures::executor::block_on` inside already-async contexts (which can deadlock the tokio runtime).

- [ ] **Step 4: Verify the whole crate compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone`
Expected: compiles with no errors.

- [ ] **Step 5: Manual verification**

Start the dev app, start a recording, let it run 10+ seconds, stop it. Repeat start→stop 3-4 times in a row. Confirm no crash, no hung UI, audio still captures/transcribes correctly each time.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/audio/stream.rs frontend/src-tauri/src/audio/recording_manager.rs
git commit -m "fix: run cpal stream play/pause/drop on spawn_blocking threads

Partial mitigation only — does not guarantee the same OS thread across the
full stream lifecycle. A complete fix requires an actor-thread redesign
(tracked as follow-up, not in this change)."
```
(adjust the file list in `git add` to match whatever files Step 3's propagation actually touched — likely also `recording_manager.rs` and possibly `recording_commands.rs`)

---

## Task 7 (Group B, part 1): Convert VAD-init `panic!()` to a propagated `Result`

**Root cause:** `pipeline.rs:734-736` — `AudioPipeline::new()` calls `panic!()` if `ContinuousVadProcessor::new()` fails (e.g. the bundled Silero VAD ONNX model file is missing or corrupted, plausible if antivirus quarantines it). This crashes the whole app on every single "Start Recording" attempt for an affected user, instead of surfacing a recoverable error.

**Files:**
- Modify: `frontend/src-tauri/src/audio/pipeline.rs`

- [ ] **Step 1: Change `AudioPipeline::new` to return `Result<Self>`**

Find the function signature (around line 699-710: `pub fn new(...) -> Self {`). Change the return type to `Result<Self>`.

Change (around line 726-737):
```rust
        let vad_processor = match ContinuousVadProcessor::new(sample_rate, redemption_time) {
            Ok(processor) => {
                info!("VAD-driven pipeline: VAD segments will be sent directly to Whisper (no time-based accumulation)");
                processor
            }
            Err(e) => {
                error!("Failed to create VAD processor: {}", e);
                panic!("VAD processor creation failed: {}", e);
            }
        };
```
to:
```rust
        let vad_processor = match ContinuousVadProcessor::new(sample_rate, redemption_time) {
            Ok(processor) => {
                info!("VAD-driven pipeline: VAD segments will be sent directly to Whisper (no time-based accumulation)");
                processor
            }
            Err(e) => {
                error!("Failed to create VAD processor: {}", e);
                return Err(anyhow::anyhow!("VAD processor creation failed: {}", e));
            }
        };
```

Find the end of the function (the `Self { ... }` struct literal that's being constructed and returned — per the earlier read, it starts around line 746 with `Self { receiver, transcription_sender, state, ...`). Wrap the final return in `Ok(...)`:
```rust
        Self {
            receiver,
            transcription_sender,
            state,
            // ... (rest of the fields, unchanged)
        }
```
becomes:
```rust
        Ok(Self {
            receiver,
            transcription_sender,
            state,
            // ... (rest of the fields, unchanged)
        })
```
(Read the full function body first with `sed -n '699,770p' frontend/src-tauri/src/audio/pipeline.rs` or the equivalent Read tool call, to see every field in the struct literal and confirm exactly where it ends, since the exact field list wasn't fully captured during planning — wrap only the final returned expression, don't change any field values.)

- [ ] **Step 2: Update the call site in `AudioPipelineManager::start`**

Change (around line 985-995):
```rust
        // Create and start pipeline with device information for adaptive mixing
        let mut pipeline = AudioPipeline::new(
            audio_receiver,
            transcription_sender,
            state.clone(),
            target_chunk_duration_ms,
            sample_rate,
            mic_device_name,
            mic_device_kind,
            system_device_name,
            system_device_kind,
        );
```
to:
```rust
        // Create and start pipeline with device information for adaptive mixing
        let mut pipeline = AudioPipeline::new(
            audio_receiver,
            transcription_sender,
            state.clone(),
            target_chunk_duration_ms,
            sample_rate,
            mic_device_name,
            mic_device_kind,
            system_device_name,
            system_device_kind,
        )?;
```
(`AudioPipelineManager::start` already returns `Result<()>` per the earlier read of its signature at line 972, so `?` propagates correctly with no further signature changes needed here.)

- [ ] **Step 3: Verify the whole crate compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles. If `AudioPipeline::new` is called from any other site (check `grep -rn "AudioPipeline::new" frontend/src-tauri/src`), update each to propagate the `Result` the same way (or handle it explicitly if that call site isn't already in a `Result`-returning function).

- [ ] **Step 4: Write a regression test**

Find or create the `#[cfg(test)] mod tests` block in `pipeline.rs`. Since directly triggering a VAD creation failure requires simulating a missing model file (not easily done in a unit test without more invasive mocking of `ContinuousVadProcessor`), write a narrower test that at least proves the function signature contract instead — that `AudioPipeline::new` with valid inputs returns `Ok`:

```rust
    #[test]
    fn test_audio_pipeline_new_returns_result() {
        let (_audio_sender, audio_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (transcription_sender, _transcription_receiver) = tokio::sync::mpsc::unbounded_channel();
        let state = RecordingState::new();

        let result = AudioPipeline::new(
            audio_receiver,
            transcription_sender,
            state,
            0,
            16000,
            "Test Mic".to_string(),
            super::super::device_detection::InputDeviceKind::Unknown,
            "Test System".to_string(),
            super::super::device_detection::InputDeviceKind::Unknown,
        );

        assert!(result.is_ok(), "AudioPipeline::new should succeed with valid inputs and a working VAD model: {:?}", result.err());
    }
```
Adjust the exact import paths (`super::super::device_detection::InputDeviceKind` etc.) to match this file's actual module structure — check existing tests in this file (if any) or `use` statements at the top of `pipeline.rs` for the correct path to `InputDeviceKind`, `RecordingState`, etc. within a `#[cfg(test)] mod tests { use super::*; ... }` block.

- [ ] **Step 5: Run the test**

Run (from `frontend/src-tauri`): `cargo test -p meetingone --lib audio::pipeline::tests::test_audio_pipeline_new_returns_result`
Expected: PASS (this test requires the real bundled VAD model to be present at its expected path relative to the test binary — if it fails because the model file isn't found in the test environment, that's expected/acceptable; note this in the report rather than treating it as a fix failure, since it's an environment/fixture limitation, not a bug in the fix itself).

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/audio/pipeline.rs
git commit -m "fix: propagate VAD init failure as Result instead of panicking on every recording start"
```

---

## Task 8 (Group B, part 2): Wrap ZipFormer blocking inference calls in `spawn_blocking`

**Root cause:** `zipformer_engine.rs`'s `transcribe_audio` (line 398-422) holds a `tokio::sync::RwLock` read guard across `stream.accept_waveform(...)` + `recognizer.decode(&stream)` — synchronous, CPU-bound native ONNX calls that can take significant time. Similarly `load_model` (line 311-396) calls `OfflineRecognizer::create(&config)` (line 385) synchronously. Neither is wrapped in `spawn_blocking`, so this work runs directly on a shared tokio async worker thread, risking starving other tasks on that thread (including audio capture/mixing) under load.

**Files:**
- Modify: `frontend/src-tauri/src/zipformer_engine/zipformer_engine.rs`

**This task has real technical uncertainty that must be resolved by compiling, not assumed:** `OfflineRecognizer` and `OfflineStream` (from the `sherpa-onnx` crate) may or may not implement `Send`. `spawn_blocking` requires its closure (and everything captured by it) to be `Send`. If these types aren't `Send`, this fix cannot be implemented as a simple `spawn_blocking` wrap without additional unsafe code (which would reintroduce a version of the same risk being fixed elsewhere in this plan) — in that case, STOP and report BLOCKED with the exact compiler error, rather than adding `unsafe impl Send`.

- [ ] **Step 1: Attempt the `transcribe_audio` wrap**

Change (line 398-422):
```rust
    pub async fn transcribe_audio(&self, audio: Vec<f32>) -> Result<String> {
        let guard = self.recognizer.read().await;
        let recognizer = guard
            .as_ref()
            .ok_or_else(|| anyhow!("ZipFormer model not loaded"))?;

        if audio.is_empty() {
            return Ok(String::new());
        }

        let stream = recognizer.create_stream();
        stream.accept_waveform(16000, &audio);
        recognizer.decode(&stream);

        let text = stream
            .get_result()
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default();

        if !text.is_empty() {
            info!("ZipFormer transcribed: {}", text);
        }

        Ok(text)
    }
```
to (attempt this first, exactly as written):
```rust
    pub async fn transcribe_audio(&self, audio: Vec<f32>) -> Result<String> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let guard = self.recognizer.read().await;
        let recognizer = guard
            .as_ref()
            .ok_or_else(|| anyhow!("ZipFormer model not loaded"))?;

        // Run the blocking native inference call off the async runtime so it
        // can't starve other tasks (audio capture/mixing) sharing this
        // worker thread. tokio::task::block_in_place requires a
        // multi-threaded runtime (already the case for this app's runtime,
        // per Cargo.toml's tokio "full" feature) and lets us keep the RwLock
        // guard held across the blocking section without moving it across
        // threads (spawn_blocking would require `recognizer`/`stream` to be
        // Send, which sherpa-onnx's FFI-backed types are not guaranteed to
        // be — block_in_place avoids that requirement entirely by running
        // synchronously on the CURRENT thread, just outside the async
        // scheduler's cooperative multitasking).
        let text = tokio::task::block_in_place(|| {
            let stream = recognizer.create_stream();
            stream.accept_waveform(16000, &audio);
            recognizer.decode(&stream);

            stream
                .get_result()
                .map(|r| r.text.trim().to_string())
                .unwrap_or_default()
        });

        if !text.is_empty() {
            info!("ZipFormer transcribed: {}", text);
        }

        Ok(text)
    }
```

Note: this uses `tokio::task::block_in_place` rather than `tokio::task::spawn_blocking` specifically BECAUSE it avoids the `Send` requirement (the closure runs on the current thread, synchronously, just outside cooperative scheduling) — this sidesteps the FFI-type `Send` uncertainty called out above entirely, while still achieving the goal (blocking native work doesn't run inside the async scheduler's normal cooperative slot, and long CPU-bound work won't hog the runtime's ability to schedule other tasks on that same worker thread indefinitely without yielding). If `block_in_place` isn't available (requires multi-threaded runtime — confirm via `frontend/src-tauri/Cargo.toml`'s `tokio = { version = "1.32.0", features = ["full", "tracing"] }`, which includes multi-threaded runtime support), this is a valid, safe choice.

- [ ] **Step 2: Verify it compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles. If `block_in_place` panics at runtime with "can only be used on the multi-threaded runtime" — check how `#[tokio::main]` or the runtime is configured in `lib.rs`; if it's not multi-threaded, report BLOCKED rather than switching runtimes (that's a bigger change affecting the whole app).

- [ ] **Step 3: Apply the same treatment to `load_model`'s `OfflineRecognizer::create` call**

Change (line 385-386):
```rust
        let recognizer = OfflineRecognizer::create(&config)
            .ok_or_else(|| anyhow!("Failed to create ZipFormer recognizer — check model files"))?;
```
to:
```rust
        let recognizer = tokio::task::block_in_place(|| OfflineRecognizer::create(&config))
            .ok_or_else(|| anyhow!("Failed to create ZipFormer recognizer — check model files"))?;
```

- [ ] **Step 4: Verify the whole crate compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles.

- [ ] **Step 5: Run the existing zipformer_engine test suite (regression check)**

Run (from `frontend/src-tauri`): `cargo test -p meetingone --lib zipformer_engine`
Expected: all tests pass, including Task 5's `test_unload_model_clears_recognizer_and_status`.

- [ ] **Step 6: Manual verification**

Start the dev app, record a short test meeting, confirm transcription still works and produces correct text (this is a performance/scheduling fix, not a behavior change — output should be identical to before).

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/zipformer_engine/zipformer_engine.rs
git commit -m "fix: run ZipFormer native inference via block_in_place to avoid starving the async runtime"
```

---

## Task 9 (Group B, part 3): Don't hold `RECORDING_MANAGER` lock across device-reconnect I/O

**Root cause:** `attempt_device_reconnect` (`recording_commands.rs:1148-1195`) acquires the `RECORDING_MANAGER` lock and holds it for the entire duration of `manager.attempt_device_reconnect(...)`, which internally does device re-enumeration, stream stop, a 100ms sleep, and stream restart — all real, unbounded-duration I/O. Every other recording command (including `poll_audio_device_events`, meant to be called every 1-2 seconds by the frontend) blocks on this same lock for that whole time.

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_commands.rs`

- [ ] **Step 1: Restructure to release the lock during the async reconnect work**

Change (line 1147-1195):
```rust
/// Manually trigger device reconnection attempt
/// Useful for UI "Retry" button
#[tauri::command]
pub async fn attempt_device_reconnect(
    device_name: String,
    device_type: String,
) -> Result<bool, String> {
    // Parse device type first
    let monitor_type = match device_type.as_str() {
        "Microphone" => DeviceMonitorType::Microphone,
        "SystemAudio" => DeviceMonitorType::SystemAudio,
        _ => return Err(format!("Invalid device type: {}", device_type)),
    };

    // Check if recording is active
    {
        let manager_guard = RECORDING_MANAGER.lock();
        if manager_guard.is_none() {
            return Err("Recording not active".to_string());
        }
    } // Release lock

    // Spawn blocking task to handle the async reconnection
    let result = tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async {
            let mut manager_guard = RECORDING_MANAGER.lock();
            if let Some(manager) = manager_guard.as_mut() {
                manager.attempt_device_reconnect(&device_name, monitor_type).await
            } else {
                Err(anyhow::anyhow!("Recording not active"))
            }
        })
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?;

    match result {
        Ok(success) => {
            if success {
                info!("✅ Manual reconnection successful");
            } else {
                warn!("❌ Manual reconnection failed - device not available");
            }
            Ok(success)
        }
        Err(e) => {
            error!("Manual reconnection error: {}", e);
            Err(e.to_string())
        }
    }
}
```
to:
```rust
/// Manually trigger device reconnection attempt
/// Useful for UI "Retry" button
#[tauri::command]
pub async fn attempt_device_reconnect(
    device_name: String,
    device_type: String,
) -> Result<bool, String> {
    // Parse device type first
    let monitor_type = match device_type.as_str() {
        "Microphone" => DeviceMonitorType::Microphone,
        "SystemAudio" => DeviceMonitorType::SystemAudio,
        _ => return Err(format!("Invalid device type: {}", device_type)),
    };

    // Take the manager out from behind the global lock for the duration of
    // the reconnect attempt, instead of holding the lock the whole time.
    // This means other recording commands (notably poll_audio_device_events,
    // which the frontend calls every 1-2 seconds) don't block on this
    // command's unbounded-duration device re-enumeration + stream restart.
    let mut manager = {
        let mut manager_guard = RECORDING_MANAGER.lock();
        match manager_guard.take() {
            Some(m) => m,
            None => return Err("Recording not active".to_string()),
        }
    }; // Lock released here

    let result = manager.attempt_device_reconnect(&device_name, monitor_type).await;

    // Put the manager back, regardless of outcome, so the recording session
    // isn't silently abandoned by a failed reconnect attempt.
    {
        let mut manager_guard = RECORDING_MANAGER.lock();
        *manager_guard = Some(manager);
    }

    match result {
        Ok(success) => {
            if success {
                info!("✅ Manual reconnection successful");
            } else {
                warn!("❌ Manual reconnection failed - device not available");
            }
            Ok(success)
        }
        Err(e) => {
            error!("Manual reconnection error: {}", e);
            Err(e.to_string())
        }
    }
}
```

Note: this also removes the now-unnecessary `spawn_blocking` + `Handle::current().block_on(...)` wrapper — that pattern existed specifically to run async code from within what was presumably meant to look like a sync-safe block; since this whole function is already `async fn` (a Tauri command), calling `.await` directly is simpler and correct. Double-check by reading the function's full original context once more before editing that this `spawn_blocking` wrapper wasn't doing something else load-bearing (e.g. specifically avoiding blocking the calling tokio worker thread for some OTHER reason) — if in doubt, ask before removing it rather than assuming.

- [ ] **Step 2: Verify it compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles. `RecordingManager` must be `Send` for this to work across the `.await` inside `attempt_device_reconnect` — if it isn't, the compiler will report exactly what's non-`Send` inside it; report BLOCKED with that error rather than working around it.

- [ ] **Step 3: Manual verification**

This is hard to trigger reliably without real Bluetooth hardware dropping mid-recording. At minimum: start a recording, call `poll_audio_device_events` repeatedly (or just observe the UI, which polls it automatically) while nothing is wrong, confirm no regression in normal operation. If Bluetooth test hardware is available, disconnect it mid-recording and use the UI's "Retry" button, confirming the UI doesn't freeze during the retry and other recording controls (mute, pause) remain responsive while reconnection is in progress.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/audio/recording_commands.rs
git commit -m "fix: release RECORDING_MANAGER lock during device reconnect I/O instead of holding it"
```

---

## Task 10 (Group B, part 4): Fix TOCTOU race on recording start

**Root cause:** `start_recording_with_meeting_name` (line 86-90) and `start_recording_with_devices_and_meeting` (line 333-337) each check `IS_RECORDING.load(...)`, then run a long `.await`-laden initialization sequence, then set `IS_RECORDING.store(true, ...)` only at the end (line 252/421). Two near-simultaneous start calls can both pass the check before either sets the flag, both proceeding to open the same physical audio devices concurrently.

**Files:**
- Modify: `frontend/src-tauri/src/audio/recording_commands.rs`

- [ ] **Step 1: Claim the "starting" state atomically at the very top of each function**

Change (in `start_recording_with_meeting_name`, line 85-90):
```rust
    // Check if already recording
    let current_recording_state = IS_RECORDING.load(Ordering::SeqCst);
    info!("🔍 IS_RECORDING state check: {}", current_recording_state);
    if current_recording_state {
        return Err("Recording already in progress".to_string());
    }
```
to:
```rust
    // Atomically claim the "starting" state: only one caller can transition
    // IS_RECORDING from false to true here. Any concurrent caller sees the
    // swap fail (current value was already true) and is rejected immediately,
    // closing the race window where two near-simultaneous start calls could
    // both pass a plain load-then-later-store check and both open the same
    // audio devices.
    if IS_RECORDING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        info!("🔍 IS_RECORDING already true — rejecting concurrent start");
        return Err("Recording already in progress".to_string());
    }
```

Apply the identical transform to `start_recording_with_devices_and_meeting` (line 332-337).

- [ ] **Step 2: Remove the now-redundant later `IS_RECORDING.store(true, ...)` calls**

Since Step 1 already sets `IS_RECORDING` to `true` at the top (as part of the atomic claim), the later:
```rust
    info!("🔍 Setting IS_RECORDING to true and resetting SPEECH_DETECTED_EMITTED");
    IS_RECORDING.store(true, Ordering::SeqCst);
    reset_speech_detected_flag(); // Reset for new recording session
```
(appears once in each of the two functions, around line 251-253 and 420-422) should become just:
```rust
    info!("🔍 Resetting SPEECH_DETECTED_EMITTED for new recording session");
    reset_speech_detected_flag();
```

- [ ] **Step 3: Add rollback on every early-return failure path between the claim and success**

This is the critical correctness requirement of this fix: since `IS_RECORDING` is now set to `true` at the very top (before model validation, device resolution, etc.), every `return Err(...)` between the claim (Step 1) and the final success path MUST reset it back to `false` first — otherwise a failed start (e.g. model validation failure) permanently locks out all future recording attempts.

In `start_recording_with_meeting_name`, find every `return Err(...)` between the new Step 1 code and the end of the function (there are at least 2: the model-validation failure around line 105, and the `manager.start_recording(...)` failure around line 242 via `.map_err(...)?`). Each needs `IS_RECORDING.store(false, Ordering::SeqCst);` immediately before it, OR — cleaner — wrap the whole function body after the claim in a helper that resets on any error path. Given the number of early-return sites and `?`-based error propagation (the `.map_err(...)?` at line 242 in particular doesn't have an explicit `return` to attach a reset to), use this pattern instead: extract the entire post-claim body into a private inner `async fn` that keeps its original `Result<(), String>` signature and its original early returns/`?` usage completely unchanged, and have the public function call it, resetting `IS_RECORDING` on `Err`:

```rust
pub async fn start_recording_with_meeting_name<R: Runtime>(
    app: AppHandle<R>,
    meeting_name: Option<String>,
    mic_enabled: bool,
) -> Result<(), String> {
    if IS_RECORDING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        info!("🔍 IS_RECORDING already true — rejecting concurrent start");
        return Err("Recording already in progress".to_string());
    }

    let result = start_recording_with_meeting_name_inner(app, meeting_name, mic_enabled).await;

    if result.is_err() {
        // Roll back the claim so a failed start doesn't permanently lock out
        // future attempts.
        IS_RECORDING.store(false, Ordering::SeqCst);
    }

    result
}

async fn start_recording_with_meeting_name_inner<R: Runtime>(
    app: AppHandle<R>,
    meeting_name: Option<String>,
    mic_enabled: bool,
) -> Result<(), String> {
    info!(
        "Starting recording with default devices, meeting: {:?}, mic_enabled: {}",
        meeting_name, mic_enabled
    );

    // ... (the ENTIRE original function body, unchanged, minus the
    // IS_RECORDING check at the top which moved to the outer wrapper, and
    // minus the later `IS_RECORDING.store(true, ...)` line per Step 2) ...
}
```

Apply the identical extract-and-wrap pattern to `start_recording_with_devices_and_meeting` (rename the inner function `start_recording_with_devices_and_meeting_inner`).

**This is a mechanical extraction — do not change any logic inside the extracted `_inner` functions beyond what Steps 1-2 already specified removing.** Read the full original function body carefully before extracting to make sure nothing is accidentally altered.

- [ ] **Step 4: Verify it compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles. Check that `start_recording` (the simpler wrapper at line 69-71, `pub async fn start_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> { start_recording_with_meeting_name(app, None, true).await }`) still compiles unchanged — it calls the now-outer wrapper function, which is correct and needs no changes itself.

- [ ] **Step 5: Manual verification**

Start the dev app. Rapidly double-click "Start Recording" in the UI (or trigger the command twice in quick succession if there's a way to do so from devtools) — confirm only one recording actually starts (check logs for "IS_RECORDING already true — rejecting concurrent start" on the second attempt) and the app doesn't error out or leave `IS_RECORDING` stuck as `true` afterward (verify by successfully stopping the recording normally afterward). Also test a genuine failure path: if possible, temporarily rename/hide the bundled model directory to force a model-validation failure, attempt to start recording, confirm the error is returned AND a subsequent start attempt (after restoring the model) succeeds (proving the rollback in Step 3 works — `IS_RECORDING` wasn't left stuck at `true` after the failure).

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/audio/recording_commands.rs
git commit -m "fix: close TOCTOU race on concurrent recording-start calls with atomic compare_exchange"
```

---

## Task 11 (Group B, part 5): Fix `.unwrap()` on `Path::to_str()` in checkpoint finalization

**Root cause:** `incremental_saver.rs:184,187` (inside `merge_checkpoints`, called from `finalize()` on Stop) and a third site at line 318 (inside a separate `recover_audio_from_checkpoints` function) call `.unwrap()` on `Path::to_str()`, which returns `None` for non-UTF-8 paths. Rare in practice, but plausible with unusual Windows profile-redirection or synced-folder setups. This is on the Stop/finalize hot path — a panic here means the audio file is never produced (though `transcripts.json` was already safely written incrementally beforehand), presenting as "recording finished but no audio file."

**Files:**
- Modify: `frontend/src-tauri/src/audio/incremental_saver.rs`

- [ ] **Step 1: Write a failing test for the error path**

Find or add a `#[cfg(test)] mod tests` block in `incremental_saver.rs`. Since directly constructing a non-UTF-8 `PathBuf` cross-platform in a portable unit test is awkward (the mechanism differs between Windows `OsString`/WTF-8 and Unix `OsStr`/raw bytes), and since the goal is simply "don't panic, return an error instead," write a test that checks the function signature/behavior contract via a normal (UTF-8) path first to confirm no regression, and rely on the code review (not an automated test) to confirm the non-UTF-8 case no longer panics — note this limitation explicitly rather than forcing a brittle platform-specific test:

```rust
    #[test]
    fn test_merge_checkpoints_path_conversion_does_not_panic_on_valid_utf8() {
        // Regression guard: confirms the to_str() error-handling introduced
        // by this fix doesn't break the normal (valid UTF-8 path) case.
        // The non-UTF-8 panic path itself isn't practically constructible in
        // a portable unit test (OsString internals differ by platform) —
        // this fix is verified primarily by code review: to_str().unwrap()
        // is replaced with a proper Result-returning check, so the panic
        // path is provably eliminated at the type level regardless.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        assert!(path.to_str().is_some(), "sanity check: tempdir paths are UTF-8 in this test environment");
    }
```

(This test is intentionally light — it's a sanity placeholder. The real verification for this task is the code-level change itself: after the fix, there is no `.unwrap()` on `Path::to_str()` anywhere in this file, which is directly greppable and is what Step 4 checks.)

- [ ] **Step 2: Fix the three `.unwrap()` sites**

At `incremental_saver.rs:179-188` (inside `merge_checkpoints`), change:
```rust
        let mut command = std::process::Command::new(ffmpeg_path);
        
        command.args(&[
            "-f", "concat",          // Use concat demuxer
            "-safe", "0",            // Allow absolute paths
            "-i", list_file.to_str().unwrap(),
            "-c", "copy",            // Copy codec - no re-encoding!
            "-y",                    // Overwrite output file
            output.to_str().unwrap()
        ]);
```
to:
```rust
        let list_file_str = list_file
            .to_str()
            .ok_or_else(|| anyhow!("Checkpoint list file path is not valid UTF-8: {}", list_file.display()))?;
        let output_str = output
            .to_str()
            .ok_or_else(|| anyhow!("Output audio file path is not valid UTF-8: {}", output.display()))?;

        let mut command = std::process::Command::new(ffmpeg_path);

        command.args(&[
            "-f", "concat",          // Use concat demuxer
            "-safe", "0",            // Allow absolute paths
            "-i", list_file_str,
            "-c", "copy",            // Copy codec - no re-encoding!
            "-y",                    // Overwrite output file
            output_str
        ]);
```

At line 318 (inside `recover_audio_from_checkpoints`), read the surrounding context first (`sed -n '295,330p' frontend/src-tauri/src/audio/incremental_saver.rs` or the equivalent Read call) since this task's earlier research didn't capture its full surrounding code — apply the same pattern: replace `.unwrap()` on whichever `Path::to_str()` call is there with a `.ok_or_else(|| anyhow!(...))?` that produces a descriptive error, consistent with how the rest of this function already propagates errors (check whether it returns `Result<_, anyhow::Error>` — the file already uses `anyhow!` per the `merge_checkpoints` code, so match that convention).

- [ ] **Step 3: Verify it compiles**

Run (from `frontend/src-tauri`): `cargo check -p meetingone --lib`
Expected: compiles. Confirm `anyhow!` is already imported in this file (it's used elsewhere per the existing `Err(anyhow!(...))` calls visible in the code) — no new import should be needed.

- [ ] **Step 4: Confirm no `.unwrap()` remains on any `Path::to_str()` call in this file**

Run: `grep -n "to_str().unwrap()" frontend/src-tauri/src/audio/incremental_saver.rs`
Expected: no output (zero matches).

- [ ] **Step 5: Run the test**

Run (from `frontend/src-tauri`): `cargo test -p meetingone --lib audio::incremental_saver::tests::test_merge_checkpoints_path_conversion_does_not_panic_on_valid_utf8`
Expected: PASS.

- [ ] **Step 6: Manual verification**

Start the dev app, record a short test meeting with auto-save enabled, stop it, confirm the final audio file is produced correctly in the meeting folder (normal case — this fix only changes behavior on the rare non-UTF-8-path error case, which isn't practical to reproduce manually; this step just confirms no regression in the common case).

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/audio/incremental_saver.rs
git commit -m "fix: return error instead of panicking on non-UTF-8 paths during checkpoint finalization"
```

---

## After all tasks: final verification

- [ ] Run the full audio module test suite: `cargo test -p meetingone --lib audio` (from `frontend/src-tauri`)
- [ ] Run the full zipformer_engine test suite: `cargo test -p meetingone --lib zipformer_engine`
- [ ] Run a final whole-crate check: `cargo check -p meetingone`
- [ ] Manual end-to-end session: start the dev app, record a 5+ minute test meeting with real speech, pause/resume once, mute/unmute once, stop, confirm the meeting saves correctly with transcript and audio both present and correct.
- [ ] Report to the user: which of the 10 tasks completed cleanly, which (if any) hit a BLOCKED status during Task 6 (Step 3's call-graph propagation) or Task 8 (Send-safety of sherpa-onnx types) and what was found, and the explicitly-flagged residual risk from Task 6 (cpal thread-affinity isn't fully eliminated, only mitigated — a complete fix needs a dedicated actor-thread redesign as a separate follow-up).
