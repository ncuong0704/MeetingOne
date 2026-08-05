# Live vs File ASR Settings Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tách cấu hình model ASR theo luồng **ghi âm trực tiếp** vs **nhập file** (UI + DB + backend), đồng thời chuyển CAPU live sang **chỉ chạy khi kết thúc cuộc họp** (không CAPU nền trong lúc ghi).

**Architecture:** Một row `transcript_settings` với cột prefix `live*` / `file*`; resolver `AsrPath::Live | File` trong Rust; Settings UI có 2 sub-tab + block Chung (hotwords + CAPU). Live worker chỉ ASR+ITN trong lúc ghi; `stop_recording` gọi `finalize_live_with_capu` trên segments đã lưu trong `RecordingSaver`.

**Tech Stack:** Rust (Tauri 2, sqlx SQLite), TypeScript/React/Next.js 14, Radix Tabs.

**Reference spec:** `docs/superpowers/specs/2026-08-05-live-file-asr-settings-split-design.md`

---

## File map

| File | Change |
|---|---|
| `frontend/src-tauri/migrations/20260805100000_split_live_file_asr_config.sql` | New: live*/file* columns + backfill |
| `frontend/src-tauri/src/database/models.rs` | `TranscriptSetting` + 12 new fields |
| `frontend/src-tauri/src/database/repositories/setting.rs` | `get_path_asr_config`, `save_live_asr_config`, `save_file_asr_config`, `save_shared_transcript_config` |
| `frontend/src-tauri/src/asr_engine/config.rs` | New: `AsrPath`, `PathAsrConfig` |
| `frontend/src-tauri/src/asr_engine/mod.rs` | Export `config` |
| `frontend/src-tauri/src/audio/transcription/engine.rs` | Read live config |
| `frontend/src-tauri/src/audio/recording_commands.rs` | Live max_segment; CAPU finalize on stop |
| `frontend/src-tauri/src/audio/import.rs` | File config |
| `frontend/src-tauri/src/audio/retranscription.rs` | File config |
| `frontend/src-tauri/src/rover_engine/commands.rs` | File `rover_enabled` only |
| `frontend/src-tauri/src/asr_engine/commands.rs` | Validate/load from path when needed |
| `frontend/src-tauri/src/api/api.rs` | Nested get config; split save commands |
| `frontend/src-tauri/src/lib.rs` | Register new commands |
| `frontend/src-tauri/src/audio/transcription/worker.rs` | Remove CAPU background stage |
| `frontend/src-tauri/src/capu_engine/live_finalize.rs` | New: `finalize_live_with_capu` |
| `frontend/src/lib/asr.ts` | `liveDescription` on models |
| `frontend/src/components/AsrPathTabs.tsx` | New: sub-tab shell |
| `frontend/src/components/LiveAsrPanel.tsx` | New: live model UI |
| `frontend/src/components/FileAsrPanel.tsx` | New: file model + ROVER |
| `frontend/src/components/SharedTranscriptPanel.tsx` | New: hotwords + CAPU |
| `frontend/src/components/TranscriptSettings.tsx` | Compose panels |
| `frontend/src/components/AsrModelManager.tsx` | Delete after migration |
| `frontend/src/contexts/ConfigContext.tsx` | Nested transcript config |
| `frontend/src/hooks/useRecordingStart.ts` | Check live model |

---

### Task 1: SQLite migration + model struct

**Files:**
- Create: `frontend/src-tauri/migrations/20260805100000_split_live_file_asr_config.sql`
- Modify: `frontend/src-tauri/src/database/models.rs`

- [ ] **Step 1: Add migration SQL** (content from spec section 2 — live*/file* ALTER + UPDATE backfill).

- [ ] **Step 2: Extend `TranscriptSetting`**

Add to `database/models.rs` after existing fields:

```rust
#[sqlx(rename = "liveModel")]
#[serde(rename = "liveModel")]
pub live_model: Option<String>,
#[sqlx(rename = "liveAsrVariant")]
#[serde(rename = "liveAsrVariant")]
pub live_asr_variant: Option<String>,
#[sqlx(rename = "liveDecodingMethod")]
#[serde(rename = "liveDecodingMethod")]
pub live_decoding_method: Option<String>,
#[sqlx(rename = "liveNumActivePaths")]
#[serde(rename = "liveNumActivePaths")]
pub live_num_active_paths: Option<i32>,
#[sqlx(rename = "liveMaxSegmentSeconds")]
#[serde(rename = "liveMaxSegmentSeconds")]
pub live_max_segment_seconds: Option<i32>,
#[sqlx(rename = "fileModel")]
#[serde(rename = "fileModel")]
pub file_model: Option<String>,
#[sqlx(rename = "fileAsrVariant")]
#[serde(rename = "fileAsrVariant")]
pub file_asr_variant: Option<String>,
#[sqlx(rename = "fileDecodingMethod")]
#[serde(rename = "fileDecodingMethod")]
pub file_decoding_method: Option<String>,
#[sqlx(rename = "fileNumActivePaths")]
#[serde(rename = "fileNumActivePaths")]
pub file_num_active_paths: Option<i32>,
#[sqlx(rename = "fileMaxSegmentSeconds")]
#[serde(rename = "fileMaxSegmentSeconds")]
pub file_max_segment_seconds: Option<i32>,
#[sqlx(rename = "fileRoverEnabled")]
#[serde(rename = "fileRoverEnabled")]
pub file_rover_enabled: Option<bool>,
#[sqlx(rename = "fileRoverFamilyB")]
#[serde(rename = "fileRoverFamilyB")]
pub file_rover_family_b: Option<String>,
#[sqlx(rename = "fileRoverVariantB")]
#[serde(rename = "fileRoverVariantB")]
pub file_rover_variant_b: Option<String>,
```

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: PASS (no callers yet).

---

### Task 2: `PathAsrConfig` resolver

**Files:**
- Create: `frontend/src-tauri/src/asr_engine/config.rs`
- Modify: `frontend/src-tauri/src/asr_engine/mod.rs`
- Modify: `frontend/src-tauri/src/database/repositories/setting.rs`

- [ ] **Step 1: Write failing test** in `setting.rs` or `asr_engine/config.rs`:

```rust
#[test]
fn path_config_live_ignores_file_rover() {
    // Build TranscriptSetting with fileRoverEnabled true, call resolve for Live
    // assert rover_enabled == false
}
```

- [ ] **Step 2: Implement `asr_engine/config.rs`**

```rust
pub enum AsrPath { Live, File }

pub struct PathAsrConfig {
    pub family_id: String,
    pub variant: crate::asr_engine::model_family::ModelVariant,
    pub decoding_method: String,
    pub num_active_paths: i32,
    pub max_segment_seconds: u32,
    pub rover_enabled: bool,
    pub rover_family_b: Option<String>,
    pub rover_variant_b: Option<String>,
}

impl PathAsrConfig {
    pub fn from_transcript_setting(row: &TranscriptSetting, path: AsrPath) -> Self { ... }
    // Fallback: if live_model is None, use legacy model/asrVariant columns
}
```

- [ ] **Step 3: Add repository methods**

```rust
pub async fn get_path_asr_config(pool: &SqlitePool, path: AsrPath) -> PathAsrConfig { ... }

pub async fn save_live_asr_config(pool, family, variant, dm, paths, max_seg) -> Result<()> {
    // UPDATE only live* columns
}

pub async fn save_file_asr_config(pool, family, variant, dm, paths, max_seg, rover_on, rover_b_family, rover_b_variant) -> Result<()> { ... }

pub async fn save_shared_transcript_config(pool, hotwords, capu_threads, capu_punct, capu_case) -> Result<()> { ... }
```

- [ ] **Step 4: Run tests**

Run: `cd frontend/src-tauri && cargo test path_asr_config -- --nocapture`
Expected: PASS

---

### Task 3: Wire backend call sites to `AsrPath`

**Files:**
- Modify: `frontend/src-tauri/src/audio/transcription/engine.rs`
- Modify: `frontend/src-tauri/src/audio/recording_commands.rs`
- Modify: `frontend/src-tauri/src/audio/import.rs`
- Modify: `frontend/src-tauri/src/audio/retranscription.rs`
- Modify: `frontend/src-tauri/src/rover_engine/commands.rs`

- [ ] **Step 1: `transcription/engine.rs`**

Replace `get_transcript_config` + `is_rover_enabled` with:

```rust
let cfg = SettingsRepository::get_path_asr_config(pool, AsrPath::Live).await;
// rover never true for live — use Single ASR path only
```

- [ ] **Step 2: `recording_commands.rs`**

`start_recording` paths: `max_segment_seconds` from `get_path_asr_config(Live)`.

- [ ] **Step 3: `import.rs` + `retranscription.rs`**

Replace rover block:

```rust
let file_cfg = SettingsRepository::get_path_asr_config(pool, AsrPath::File).await;
if file_cfg.rover_enabled { ... rover ... } else { ... asr with file_cfg fields ... }
```

- [ ] **Step 4: `rover_engine/commands.rs`**

`rover_validate_model_ready`: read `file_cfg.rover_enabled` only; error if false.

- [ ] **Step 5: `cargo check`**

Run: `cd frontend/src-tauri && cargo check`
Expected: PASS

---

### Task 4: API — nested get + split save

**Files:**
- Modify: `frontend/src-tauri/src/api/api.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Add response structs**

```rust
pub struct LiveAsrConfigDto { model, asr_variant, decoding_method, num_active_paths, max_segment_seconds }
pub struct FileAsrConfigDto { ... + rover_enabled, rover_family_b, rover_variant_b }
pub struct SharedTranscriptConfigDto { hotwords, capu_cpu_threads, capu_punctuation_level, capu_case_level }
pub struct TranscriptConfigBundle { live, file, shared }
```

- [ ] **Step 2: Change `api_get_transcript_config`** to return `TranscriptConfigBundle`.

- [ ] **Step 3: Add commands**

```rust
#[tauri::command]
pub async fn api_save_live_asr_config(...) -> Result<(), String>

#[tauri::command]
pub async fn api_save_file_asr_config(...) -> Result<(), String>

#[tauri::command]
pub async fn api_save_shared_transcript_config(...) -> Result<(), String>
```

`api_save_live_asr_config`: validate family/variant, call `save_live_asr_config`, then `asr_validate_model_ready` with live params (not rover).

`api_save_file_asr_config`: save file columns; if rover → `rover_validate_model_ready`, else `asr_validate_model_ready` with file params.

`api_save_shared_transcript_config`: save hotwords + capu; call `capu_engine::commands::apply_settings_after_save`.

- [ ] **Step 4: Deprecate `api_save_transcript_config`**

Log warn; write **both** live and file columns with same values (backward compat 1 release).

- [ ] **Step 5: Register in `lib.rs`**

- [ ] **Step 6: `cargo check`**

---

### Task 5: CAPU live — chỉ khi kết thúc cuộc họp

**Files:**
- Create: `frontend/src-tauri/src/capu_engine/live_finalize.rs`
- Modify: `frontend/src-tauri/src/capu_engine/mod.rs`
- Modify: `frontend/src-tauri/src/audio/transcription/worker.rs`
- Modify: `frontend/src-tauri/src/audio/recording_commands.rs`

- [ ] **Step 1: Write test** `finalize_live_with_capu_returns_itn_when_no_capu_engine`

```rust
// segments with raw ITN text → finalize with None engine → same text grouped
```

- [ ] **Step 2: Implement `live_finalize.rs`**

```rust
pub fn finalize_live_with_capu(
    segments: &[TranscriptSegment], // from RecordingSaver
) -> Vec<FinalizedSegment> {
    let mut batcher = CapuBatcher::new();
    let engine_arc = crate::capu_engine::commands::get_engine_arc();
    for seg in segments {
        if seg.user_edited { continue; } // skip — same rule as replace_transcript_segments
        batcher.push(PendingSegment {
            source_id: seg.sequence_id,
            raw_text: seg.text.clone(), // already ITN from worker
            audio_start_time: seg.audio_start_time,
            audio_end_time: seg.audio_end_time,
        });
        if batcher.should_flush(crate::config::CAPU_BATCH_WORD_BUDGET) {
            flush_batch(&mut batcher, &engine_arc);
        }
    }
  flush remainder...
}
```

Reuse flush logic from `batch_transcribe::flush_into` pattern.

- [ ] **Step 3: Remove CAPU Stage 2 from `worker.rs`**

Delete:
- `capu_sender` / `capu_receiver` / `spawn_capu_background_stage`
- `capu_sender_clone.send(PendingSegment...)` in worker loop
- `drop(capu_sender)` + `capu_stage_handle.await`

Keep `transcript-update` emit with ITN text only.

- [ ] **Step 4: Integrate in `stop_recording`**

After transcription `task_handle` completes (line ~810), **before** unlisten `transcript-finalized`:

```rust
let _ = app.emit("recording-shutdown-progress", json!({
    "stage": "applying_punctuation",
    "message": "Đang thêm dấu câu...",
    "progress": 55
}));

if let Some(ref manager) = manager_for_cleanup.as_ref() {
    let raw_segments = manager.get_transcript_segments();
    let finalized = finalize_live_with_capu(&raw_segments);
    for f in finalized {
        manager.replace_transcript_segments(
            &f.source_ids,
            f.text,
            f.audio_start_time,
            f.audio_end_time,
        );
        // Optional: emit transcript-finalized for frontend refresh
        let _ = app.emit("transcript-finalized", TranscriptFinalized { ... });
    }
}
```

Ensure `manager_for_cleanup` is still available — may need to restructure: keep manager reference before take, or get segments before cleanup.

- [ ] **Step 5: Update SharedTranscriptPanel hint text** (Task 7) — CAPU applies at end of live meeting.

- [ ] **Step 6: Run tests**

Run: `cd frontend/src-tauri && cargo test live_finalize worker -- --nocapture`

---

### Task 6: Frontend — `asr.ts` live descriptions

**Files:**
- Modify: `frontend/src/lib/asr.ts`

- [ ] **Step 1: Extend interface + data**

```typescript
export interface AsrModelInfo {
  ...
  liveDescription?: string;
}

// zipformer-vi-30m:
liveDescription: 'Khuyến nghị cho ghi âm trực tiếp — nhanh, ít tốn CPU/RAM.',
// gipformer:
liveDescription: 'Chính xác hơn nhưng chậm hơn; cuộc họp dài có thể tụt transcript real-time.',
// sherpa:
liveDescription: 'Model lớn (~270 MB), chỉ bản full — không khuyến nghị khi ghi âm liên tục.',
```

- [ ] **Step 2: Add API helpers** (optional)

```typescript
export type LiveAsrConfig = { ... };
export type FileAsrConfig = { ... };
export type SharedTranscriptConfig = { ... };
```

---

### Task 7: Frontend — split Settings UI

**Files:**
- Create: `frontend/src/components/AsrPathTabs.tsx`
- Create: `frontend/src/components/LiveAsrPanel.tsx`
- Create: `frontend/src/components/FileAsrPanel.tsx`
- Create: `frontend/src/components/SharedTranscriptPanel.tsx`
- Modify: `frontend/src/components/TranscriptSettings.tsx`
- Delete: `frontend/src/components/AsrModelManager.tsx` (after panels work)

- [ ] **Step 1: Extract `SharedTranscriptPanel`**

Move hotwords + CAPU sliders + save → `api_save_shared_transcript_config` from tail of `AsrModelManager.tsx`.

- [ ] **Step 2: Create `LiveAsrPanel`**

- Load from `api_get_transcript_config().live`
- Full `ASR_MODELS` in `<select>`
- Show `selectedModelInfo.liveDescription` in amber info box below select
- Note: *"Dấu câu/viết hoa chỉ áp dụng sau khi kết thúc cuộc họp."*
- Fields: variant, decoding, paths, max segment — **no ROVER**
- Save → `api_save_live_asr_config`
- Copy download/validate logic from `AsrModelManager` for primary model only

- [ ] **Step 3: Create `FileAsrPanel`**

- Load from `.file`
- Same model list, use `description` field
- ROVER block from existing `AsrModelManager`
- Save → `api_save_file_asr_config`

- [ ] **Step 4: `AsrPathTabs`**

Radix `Tabs`: default `"live"`, tabs "Ghi âm trực tiếp" | "Nhập file".

- [ ] **Step 5: Update `TranscriptSettings.tsx`**

```tsx
<AsrPathTabs />
<SharedTranscriptPanel />
```

- [ ] **Step 6: Delete `AsrModelManager.tsx`** and fix imports.

- [ ] **Step 7: `pnpm run lint` in frontend**

---

### Task 8: ConfigContext + recording start

**Files:**
- Modify: `frontend/src/contexts/ConfigContext.tsx`
- Modify: `frontend/src/hooks/useRecordingStart.ts`
- Modify: `frontend/src/components/Sidebar/index.tsx` (if uses old save)

- [ ] **Step 1: Extend context**

```typescript
liveTranscriptConfig: LiveAsrConfig;
fileTranscriptConfig: FileAsrConfig;
// load from api_get_transcript_config bundle on init
```

- [ ] **Step 2: `useRecordingStart`**

Check live model: `asr_is_model_loaded` after ensuring live config validated (not file).

- [ ] **Step 3: Sidebar `handleSaveTranscriptConfig`**

Update to call split APIs or remove if obsolete.

---

### Task 9: Manual smoke test checklist

- [ ] Migration: existing user → live and file both equal old model after upgrade.
- [ ] Set live=30M, file=65M+ROVER → record 1 min → text thô during recording → dấu câu sau stop.
- [ ] Import file → uses file model (check Rust logs).
- [ ] Settings sub-tabs save independently.
- [ ] Live panel shows warning for sherpa model.
- [ ] Recording + import disabled states on panels.

---

### Task 10: Cleanup & docs

- [ ] Update footnote in `2026-08-04-capu-punctuation-settings-design.md` — live CAPU timing changed.
- [ ] Remove dead `spawn_capu_background_stage` if fully unused.
- [ ] Final: `cd frontend/src-tauri && cargo test` && `pnpm run lint`

---

## Execution order

```
Task 1 → 2 → 3 → 4 → 5 (backend complete)
Task 6 → 7 → 8 (frontend)
Task 9 → 10 (verify)
```

Tasks 5 can run in parallel with 4 after Task 3 if different files; sequential is safer.

---

## Risks during implementation

| Risk | Mitigation |
|---|---|
| `manager_for_cleanup` consumed before CAPU finalize | Call finalize while manager still in scope; read segments before `take()` |
| Frontend expects `transcript-finalized` during recording | Update UX copy; optional bulk refresh event after stop |
| `api_get_transcript_config` shape change breaks clients | Return nested bundle; keep flat fields deprecated 1 release if needed |
