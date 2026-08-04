# ROVER Wiring Implementation Plan (Phase C)

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking. Execute tasks in order; each task ends with a commit. Do not skip "run and verify" steps.

**Goal:** Wire the already-verified `RoverDecoder` (Phase A/B, confirmed correct on real audio) into Settings, the database, and the three ASR call sites (live recording, file import, retranscription) — behind an opt-in `roverEnabled` flag that defaults to off.

**Architecture:** Extend `transcript_settings` with 3 columns (reusing the existing `model`/`asrVariant` columns as "family A"). Add `rover_engine::commands` (mirrors `asr_engine::commands`'s shape, but does not duplicate download logic — that stays generic in `asr_engine`) and `rover_engine::provider::RoverProvider` (bridges `RoverDecoder`'s `&mut self` decode into the `&self` `TranscriptionProvider` trait via `tokio::sync::Mutex` + `block_in_place`, same blocking pattern `AsrEngine::transcribe_audio` already uses). Three call sites branch on `roverEnabled`: `audio/transcription/engine.rs` branches internally (so `worker.rs`, which already goes through the trait, needs zero changes), while `audio/import.rs` and `audio/retranscription.rs` (which already bypass the trait and call the engine directly) each get a small added branch.

**Tech Stack:** Rust (sqlx/SQLite migration, Tauri commands, `tokio::sync::Mutex`), TypeScript/React.

**Reference spec:** `docs/superpowers/specs/2026-08-04-rover-wiring-design.md`

**Verified before writing this plan:** `RoverDecoder` was run end-to-end on real audio with real ZipFormer 30M + Gipformer 65M models (Phase B verification) — correct merge, 0 disagreements on a clip where all three families independently agree, confidence values preserved correctly. All Rust code below was checked against the actual current contents of every file it modifies (`database/models.rs`, `database/repositories/setting.rs`, `api/api.rs`, `audio/import.rs`, `audio/retranscription.rs`, `audio/transcription/engine.rs`, `audio/transcription/provider.rs`), not written from memory of the earlier Gipformer-era plan.

---

## File map

| File | Change |
|---|---|
| `frontend/src-tauri/migrations/20260804000000_add_rover_config.sql` | New: 3 columns on `transcript_settings` |
| `frontend/src-tauri/src/database/models.rs` | Add 3 fields to `TranscriptSetting` |
| `frontend/src-tauri/src/database/repositories/setting.rs` | `save_transcript_config` gets 3 new params |
| `frontend/src-tauri/src/api/api.rs` | `TranscriptConfig` gets 3 fields; save/get handlers pass them through |
| `frontend/src-tauri/src/asr_engine/commands.rs` | `resolve_models_base_dir` → `pub(crate)` |
| `frontend/src-tauri/src/rover_engine/commands.rs` | New: `rover_init`, `rover_load_model`, `rover_is_model_loaded`, `rover_get_current_config`, `rover_validate_model_ready`, `get_engine_arc` |
| `frontend/src-tauri/src/rover_engine/provider.rs` | New: `RoverProvider` |
| `frontend/src-tauri/src/rover_engine/mod.rs` | Register new submodules |
| `frontend/src-tauri/src/lib.rs` | Register `rover_*` Tauri commands |
| `frontend/src-tauri/src/audio/transcription/engine.rs` | Branch on `roverEnabled` internally |
| `frontend/src-tauri/src/audio/import.rs` | Add rover branch around the existing ASR init + per-segment transcribe |
| `frontend/src-tauri/src/audio/retranscription.rs` | Same as `import.rs` |
| `frontend/src/lib/asr.ts` | Add ROVER config fields + save/load |
| `frontend/src/components/AsrModelManager.tsx` | Add ROVER toggle + second family/variant picker |

---

### Task 1: Database — migration, model, repository

**Files:**
- Create: `frontend/src-tauri/migrations/20260804000000_add_rover_config.sql`
- Modify: `frontend/src-tauri/src/database/models.rs`
- Modify: `frontend/src-tauri/src/database/repositories/setting.rs`

- [ ] **Step 1: Create the migration**

```sql
ALTER TABLE transcript_settings ADD COLUMN roverEnabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE transcript_settings ADD COLUMN roverFamilyB TEXT;
ALTER TABLE transcript_settings ADD COLUMN roverVariantB TEXT;
```

- [ ] **Step 2: Extend `TranscriptSetting`**

In `frontend/src-tauri/src/database/models.rs`, find:

```rust
    #[sqlx(rename = "maxSegmentSeconds")]
    #[serde(rename = "maxSegmentSeconds")]
    pub max_segment_seconds: i32,
}
```

Replace with:

```rust
    #[sqlx(rename = "maxSegmentSeconds")]
    #[serde(rename = "maxSegmentSeconds")]
    pub max_segment_seconds: i32,
    #[sqlx(rename = "roverEnabled")]
    #[serde(rename = "roverEnabled")]
    pub rover_enabled: bool,
    #[sqlx(rename = "roverFamilyB")]
    #[serde(rename = "roverFamilyB")]
    pub rover_family_b: Option<String>,
    #[sqlx(rename = "roverVariantB")]
    #[serde(rename = "roverVariantB")]
    pub rover_variant_b: Option<String>,
}
```

- [ ] **Step 3: Extend `save_transcript_config`**

In `frontend/src-tauri/src/database/repositories/setting.rs`, find:

```rust
    pub async fn save_transcript_config(
        pool: &SqlitePool,
        provider: &str,
        model: &str,
        asr_variant: &str,
        decoding_method: &str,
        num_active_paths: i32,
        max_segment_seconds: i32,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO transcript_settings
                (id, provider, model, asrVariant, decodingMethod, numActivePaths, maxSegmentSeconds)
            VALUES ('1', $1, $2, $3, $4, $5, $6)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                asrVariant = excluded.asrVariant,
                decodingMethod = excluded.decodingMethod,
                numActivePaths = excluded.numActivePaths,
                maxSegmentSeconds = excluded.maxSegmentSeconds
            "#,
        )
        .bind(provider)
        .bind(model)
        .bind(asr_variant)
        .bind(decoding_method)
        .bind(num_active_paths)
        .bind(max_segment_seconds)
        .execute(pool)
        .await?;

        Ok(())
    }
```

Replace with:

```rust
    pub async fn save_transcript_config(
        pool: &SqlitePool,
        provider: &str,
        model: &str,
        asr_variant: &str,
        decoding_method: &str,
        num_active_paths: i32,
        max_segment_seconds: i32,
        rover_enabled: bool,
        rover_family_b: Option<&str>,
        rover_variant_b: Option<&str>,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO transcript_settings
                (id, provider, model, asrVariant, decodingMethod, numActivePaths, maxSegmentSeconds, roverEnabled, roverFamilyB, roverVariantB)
            VALUES ('1', $1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                asrVariant = excluded.asrVariant,
                decodingMethod = excluded.decodingMethod,
                numActivePaths = excluded.numActivePaths,
                maxSegmentSeconds = excluded.maxSegmentSeconds,
                roverEnabled = excluded.roverEnabled,
                roverFamilyB = excluded.roverFamilyB,
                roverVariantB = excluded.roverVariantB
            "#,
        )
        .bind(provider)
        .bind(model)
        .bind(asr_variant)
        .bind(decoding_method)
        .bind(num_active_paths)
        .bind(max_segment_seconds)
        .bind(rover_enabled)
        .bind(rover_family_b)
        .bind(rover_variant_b)
        .execute(pool)
        .await?;

        Ok(())
    }
```

- [ ] **Step 4: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: errors at the one call site in `api.rs` (`save_transcript_config` now needs 3 more args) — this is expected; Task 4 fixes it. Confirm the error is *only* about that call site's argument count, not about the migration/model changes themselves.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/migrations/20260804000000_add_rover_config.sql frontend/src-tauri/src/database/models.rs frontend/src-tauri/src/database/repositories/setting.rs
git commit -m "feat(rover): add roverEnabled/roverFamilyB/roverVariantB to transcript_settings"
```

---

### Task 2: Expose `resolve_models_base_dir` to `rover_engine`

**Files:**
- Modify: `frontend/src-tauri/src/asr_engine/commands.rs`

- [ ] **Step 1: Widen visibility**

Find:

```rust
fn resolve_models_base_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
```

Replace with:

```rust
pub(crate) fn resolve_models_base_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
```

**Why:** `rover_engine::commands` needs the exact same models directory `asr_engine` already resolves — duplicating this computation risks the two drifting apart over time (e.g. if the app data dir logic ever changes).

- [ ] **Step 2: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: same pre-existing errors as Task 1 (not new ones from this change — widening visibility never breaks a build).

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/asr_engine/commands.rs
git commit -m "refactor(asr): expose resolve_models_base_dir to rover_engine"
```

---

### Task 3: `rover_engine::commands`

**Files:**
- Create: `frontend/src-tauri/src/rover_engine/commands.rs`
- Modify: `frontend/src-tauri/src/rover_engine/mod.rs`

Does not reimplement downloading — `asr_download_model`/`asr_get_variant_status` in `asr_engine::commands` already work for any family+variant, called twice (once per side) by the frontend. This module only adds what ROVER needs beyond that: holding two decoders loaded simultaneously and reading the ROVER-specific saved config.

- [ ] **Step 1: Implement**

Create `frontend/src-tauri/src/rover_engine/commands.rs`:

```rust
use crate::asr_engine::model_family::{ModelFamily, ModelVariant};
use crate::rover_engine::engine::RoverDecoder;
use log::info;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::Mutex as TokioMutex;

pub(crate) static ROVER_ENGINE: Mutex<Option<Arc<TokioMutex<RoverDecoder>>>> = Mutex::new(None);
type RoverConfigTuple = (ModelFamily, ModelVariant, ModelFamily, ModelVariant);
pub(crate) static ROVER_CONFIG: Mutex<Option<RoverConfigTuple>> = Mutex::new(None);

fn family_variant_dir(base: &PathBuf, family: ModelFamily, variant: ModelVariant) -> PathBuf {
    base.join(family.variant_subdir(variant))
}

fn family_paths(base: &PathBuf, family: ModelFamily, variant: ModelVariant) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let dir = family_variant_dir(base, family, variant);
    (
        dir.join(family.encoder_file(variant)),
        dir.join(family.decoder_file(variant)),
        dir.join(family.joiner_file(variant)),
        dir.join(family.token_file()),
    )
}

fn files_present(base: &PathBuf, family: ModelFamily, variant: ModelVariant) -> bool {
    let dir = family_variant_dir(base, family, variant);
    family.model_files(variant).iter().all(|f| dir.join(f).exists())
}

#[tauri::command]
pub async fn rover_init() -> Result<(), String> {
    // No pre-allocated state to set up — unlike AsrEngine there is no useful "empty"
    // RoverDecoder; it's only ever constructed once both models' paths are known, in
    // rover_load_model. This command exists so call sites can mirror asr_init()'s shape.
    Ok(())
}

#[tauri::command]
pub async fn rover_load_model<R: Runtime>(
    app: AppHandle<R>,
    family_a: String,
    variant_a: String,
    family_b: String,
    variant_b: String,
) -> Result<(), String> {
    let base = crate::asr_engine::commands::resolve_models_base_dir(&app)
        .ok_or_else(|| "Cannot resolve models directory".to_string())?;

    let fa = ModelFamily::from_id(&family_a);
    let va = ModelVariant::from_str(&variant_a);
    let fb = ModelFamily::from_id(&family_b);
    let vb = ModelVariant::from_str(&variant_b);

    if !files_present(&base, fa, va) {
        return Err(format!("Missing model files for {} ({})", fa.id(), va.as_str()));
    }
    if !files_present(&base, fb, vb) {
        return Err(format!("Missing model files for {} ({})", fb.id(), vb.as_str()));
    }

    let (enc_a, dec_a, joi_a, tok_a) = family_paths(&base, fa, va);
    let (enc_b, dec_b, joi_b, tok_b) = family_paths(&base, fb, vb);

    let decoder = tokio::task::block_in_place(|| {
        RoverDecoder::load(
            (&enc_a, &dec_a, &joi_a, &tok_a),
            (&enc_b, &dec_b, &joi_b, &tok_b),
            4,
        )
    })
    .map_err(|e| format!("Failed to load ROVER models: {}", e))?;

    *ROVER_ENGINE.lock().unwrap() = Some(Arc::new(TokioMutex::new(decoder)));
    *ROVER_CONFIG.lock().unwrap() = Some((fa, va, fb, vb));
    info!("ROVER models loaded: {} ({}) + {} ({})", fa.id(), va.as_str(), fb.id(), vb.as_str());
    Ok(())
}

#[tauri::command]
pub async fn rover_is_model_loaded() -> Result<bool, String> {
    Ok(ROVER_ENGINE.lock().unwrap().is_some())
}

#[tauri::command]
pub async fn rover_get_current_config() -> Result<serde_json::Value, String> {
    let cfg = *ROVER_CONFIG.lock().unwrap();
    match cfg {
        Some((fa, va, fb, vb)) => Ok(serde_json::json!({
            "familyA": fa.id(),
            "variantA": va.as_str(),
            "familyB": fb.id(),
            "variantB": vb.as_str(),
            "isLoaded": true,
        })),
        None => Ok(serde_json::json!({ "isLoaded": false })),
    }
}

pub fn get_engine_arc() -> Result<Arc<TokioMutex<RoverDecoder>>, String> {
    ROVER_ENGINE
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .ok_or_else(|| "ROVER engine not loaded. Call rover_load_model first.".to_string())
}

/// Reads the saved config, ensures both models are loaded (loading them if this is the
/// first call or the saved family/variant pair changed), and returns `"familyA+familyB"`.
#[tauri::command]
pub async fn rover_validate_model_ready<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let app_state = app
        .try_state::<crate::state::AppState>()
        .ok_or_else(|| "App state not available".to_string())?;
    let config = crate::database::repositories::setting::SettingsRepository::get_transcript_config(
        app_state.db_manager.pool(),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "No transcript config found".to_string())?;

    if !config.rover_enabled {
        return Err("ROVER is not enabled in settings".to_string());
    }
    let family_b = config
        .rover_family_b
        .ok_or_else(|| "ROVER family B not configured".to_string())?;
    let variant_b = config.rover_variant_b.unwrap_or_else(|| "int8".to_string());

    let fa = ModelFamily::from_id(&config.model);
    let va = ModelVariant::from_str(&config.asr_variant);
    let fb = ModelFamily::from_id(&family_b);
    let vb = ModelVariant::from_str(&variant_b);

    let needs_reload = {
        let loaded = ROVER_ENGINE.lock().unwrap().is_some();
        let current = *ROVER_CONFIG.lock().unwrap();
        !loaded || current != Some((fa, va, fb, vb))
    };

    if needs_reload {
        rover_load_model(
            app.clone(),
            fa.id().to_string(),
            va.as_str().to_string(),
            fb.id().to_string(),
            vb.as_str().to_string(),
        )
        .await?;
    }

    Ok(format!("{}+{}", fa.id(), fb.id()))
}
```

- [ ] **Step 2: Add a unit test for `files_present`**

Append to `frontend/src-tauri/src/rover_engine/commands.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_present_is_false_when_any_required_file_is_missing() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let base = dir.path().to_path_buf();
        let family = ModelFamily::ZipFormer30M;
        let variant = ModelVariant::Int8;
        let subdir = base.join(family.variant_subdir(variant));
        std::fs::create_dir_all(&subdir).expect("create subdir");

        let files = family.model_files(variant);
        // Create all but the last required file.
        for f in &files[..files.len() - 1] {
            std::fs::write(subdir.join(f), b"stub").expect("write stub file");
        }

        assert!(!files_present(&base, family, variant));
    }

    #[test]
    fn files_present_is_true_when_all_required_files_exist() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let base = dir.path().to_path_buf();
        let family = ModelFamily::ZipFormer30M;
        let variant = ModelVariant::Int8;
        let subdir = base.join(family.variant_subdir(variant));
        std::fs::create_dir_all(&subdir).expect("create subdir");

        for f in family.model_files(variant) {
            std::fs::write(subdir.join(f), b"stub").expect("write stub file");
        }

        assert!(files_present(&base, family, variant));
    }
}
```

`tempfile` is already a dev-dependency (added in the RNN-T decoder core plan, Task 3).

Run: `cd frontend/src-tauri && cargo test rover_engine::commands -- --nocapture`
Expected: 2 tests PASS.

- [ ] **Step 3: Register the module**

Create `frontend/src-tauri/src/rover_engine/mod.rs` if it doesn't already list these (it currently lists `engine`, `merge`, `normalize` from Phase B) — find:

```rust
pub mod engine;
pub mod merge;
pub mod normalize;
```

Replace with:

```rust
pub mod commands;
pub mod engine;
pub mod merge;
pub mod normalize;
```

- [ ] **Step 4: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: same pre-existing `save_transcript_config` call-site error as before (fixed in Task 8), no new errors from this file.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/rover_engine/commands.rs frontend/src-tauri/src/rover_engine/mod.rs
git commit -m "feat(rover): add rover_engine::commands for loading and validating both models"
```

---

### Task 4: `RoverProvider`

**Files:**
- Create: `frontend/src-tauri/src/rover_engine/provider.rs`
- Modify: `frontend/src-tauri/src/rover_engine/mod.rs`

Bridges `RoverDecoder::decode(&mut self, ...)` (blocking, needs exclusive access) into `TranscriptionProvider::transcribe(&self, ...)` (async, shared access) via `tokio::sync::Mutex` + `block_in_place` — the same blocking pattern already used by `AsrEngine::transcribe_audio`.

- [ ] **Step 1: Implement**

Create `frontend/src-tauri/src/rover_engine/provider.rs`:

```rust
use crate::audio::transcription::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use crate::rover_engine::engine::RoverDecoder;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

pub struct RoverProvider {
    decoder: Arc<TokioMutex<RoverDecoder>>,
    family_a_id: String,
    family_b_id: String,
}

impl RoverProvider {
    pub fn new(decoder: Arc<TokioMutex<RoverDecoder>>, family_a_id: String, family_b_id: String) -> Self {
        Self {
            decoder,
            family_a_id,
            family_b_id,
        }
    }
}

#[async_trait]
impl TranscriptionProvider for RoverProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        _language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        let decoder = self.decoder.clone();
        let result = tokio::task::block_in_place(move || {
            let mut guard = decoder.blocking_lock();
            guard.decode(&audio, 16000.0)
        })
        .map_err(|e| TranscriptionError::EngineFailed(e.to_string()))?;

        // Average word confidence — real data Phase A/B already computed, not currently
        // surfaced anywhere else (the sherpa-onnx path always returns None here).
        let confidence = if result.words.is_empty() {
            None
        } else {
            Some(
                result.words.iter().map(|w| w.word.confidence).sum::<f32>()
                    / result.words.len() as f32,
            )
        };

        Ok(TranscriptResult {
            text: result.text,
            confidence,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        true
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(format!("rover:{}+{}", self.family_a_id, self.family_b_id))
    }

    fn provider_name(&self) -> &'static str {
        "rover-vi"
    }
}
```

- [ ] **Step 2: Register the module**

In `frontend/src-tauri/src/rover_engine/mod.rs`, find:

```rust
pub mod commands;
pub mod engine;
pub mod merge;
pub mod normalize;
```

Replace with:

```rust
pub mod commands;
pub mod engine;
pub mod merge;
pub mod normalize;
pub mod provider;
```

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: same pre-existing error as before, nothing new.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/rover_engine/provider.rs frontend/src-tauri/src/rover_engine/mod.rs
git commit -m "feat(rover): add RoverProvider bridging RoverDecoder into TranscriptionProvider"
```

---

### Task 5: Register commands in `lib.rs`, branch `audio/transcription/engine.rs`

**Files:**
- Modify: `frontend/src-tauri/src/lib.rs`
- Modify: `frontend/src-tauri/src/audio/transcription/engine.rs`

**Why this file, not `worker.rs`:** `worker.rs` already calls `get_or_init_transcription_engine`/`validate_transcription_model_ready` and receives back an `Arc<dyn TranscriptionProvider>` — it has no idea whether that's `AsrProvider` or `RoverProvider`. Putting the branch here means `worker.rs` needs zero changes.

- [ ] **Step 1: Register the 5 new commands in `lib.rs`**

Find where `asr_engine::commands::asr_init` and its siblings are listed in the `tauri::generate_handler![...]` macro call, and add nearby:

```rust
rover_engine::commands::rover_init,
rover_engine::commands::rover_load_model,
rover_engine::commands::rover_is_model_loaded,
rover_engine::commands::rover_get_current_config,
rover_engine::commands::rover_validate_model_ready,
```

- [ ] **Step 2: Add the branching helper and update both public functions**

In `frontend/src-tauri/src/audio/transcription/engine.rs`, find:

```rust
use super::asr_provider::AsrProvider;
use super::provider::TranscriptionProvider;
use log::{info, warn};
use std::sync::Arc;
use tauri::{AppHandle, Runtime};
```

Replace with:

```rust
use super::asr_provider::AsrProvider;
use super::provider::TranscriptionProvider;
use log::{info, warn};
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime};

async fn is_rover_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(app_state) = app.try_state::<crate::state::AppState>() else {
        return false;
    };
    crate::database::repositories::setting::SettingsRepository::get_transcript_config(
        app_state.db_manager.pool(),
    )
    .await
    .ok()
    .flatten()
    .map(|c| c.rover_enabled)
    .unwrap_or(false)
}
```

Then find the body of `validate_transcription_model_ready`:

```rust
pub async fn validate_transcription_model_ready<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    info!("🔍 Validating Vietnamese ASR model...");

    if let Err(e) = crate::asr_engine::commands::asr_init().await {
        warn!("❌ Failed to initialize ASR engine: {}", e);
        return Err(format!("Failed to initialize speech recognition: {}", e));
    }

    match crate::asr_engine::commands::asr_validate_model_ready(
        app.clone(),
        None,
        None,
        None,
        None,
    )
    .await
    {
        Ok(name) => {
            info!("✅ ASR model ready: {}", name);
            Ok(())
        }
        Err(e) => {
            warn!("❌ ASR model validation failed: {}", e);
            Err(e)
        }
    }
}
```

Replace with:

```rust
pub async fn validate_transcription_model_ready<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    info!("🔍 Validating Vietnamese ASR model...");

    if is_rover_enabled(app).await {
        crate::rover_engine::commands::rover_init()
            .await
            .map_err(|e| format!("Failed to initialize ROVER: {}", e))?;
        return match crate::rover_engine::commands::rover_validate_model_ready(app.clone()).await {
            Ok(name) => {
                info!("✅ ROVER models ready: {}", name);
                Ok(())
            }
            Err(e) => {
                warn!("❌ ROVER model validation failed: {}", e);
                Err(e)
            }
        };
    }

    if let Err(e) = crate::asr_engine::commands::asr_init().await {
        warn!("❌ Failed to initialize ASR engine: {}", e);
        return Err(format!("Failed to initialize speech recognition: {}", e));
    }

    match crate::asr_engine::commands::asr_validate_model_ready(
        app.clone(),
        None,
        None,
        None,
        None,
    )
    .await
    {
        Ok(name) => {
            info!("✅ ASR model ready: {}", name);
            Ok(())
        }
        Err(e) => {
            warn!("❌ ASR model validation failed: {}", e);
            Err(e)
        }
    }
}
```

Then find the body of `get_or_init_transcription_engine`:

```rust
pub async fn get_or_init_transcription_engine<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<TranscriptionEngine, String> {
    info!("🎤 Initializing ASR transcription engine");

    crate::asr_engine::commands::asr_validate_model_ready(app.clone(), None, None, None, None)
        .await?;

    let engine = crate::asr_engine::commands::get_engine_arc()?;
    let provider = Arc::new(AsrProvider::new(engine));
    Ok(TranscriptionEngine::Provider(provider))
}
```

Replace with:

```rust
pub async fn get_or_init_transcription_engine<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<TranscriptionEngine, String> {
    info!("🎤 Initializing ASR transcription engine");

    if is_rover_enabled(app).await {
        let name = crate::rover_engine::commands::rover_validate_model_ready(app.clone()).await?;
        let engine = crate::rover_engine::commands::get_engine_arc()?;
        let (family_a, family_b) = name
            .split_once('+')
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .unwrap_or((name.clone(), String::new()));
        let provider = Arc::new(crate::rover_engine::provider::RoverProvider::new(
            engine, family_a, family_b,
        ));
        return Ok(TranscriptionEngine::Provider(provider));
    }

    crate::asr_engine::commands::asr_validate_model_ready(app.clone(), None, None, None, None)
        .await?;

    let engine = crate::asr_engine::commands::get_engine_arc()?;
    let provider = Arc::new(AsrProvider::new(engine));
    Ok(TranscriptionEngine::Provider(provider))
}
```

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: same pre-existing `api.rs` call-site error, nothing new.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/lib.rs frontend/src-tauri/src/audio/transcription/engine.rs
git commit -m "feat(rover): register rover_* commands, branch live-recording path on roverEnabled"
```

---

### Task 6: `audio/import.rs`

**Files:**
- Modify: `frontend/src-tauri/src/audio/import.rs`

- [ ] **Step 1: Branch the engine initialization**

Find:

```rust
    // Initialize ASR engine
    crate::asr_engine::commands::asr_init().await
        .map_err(|e| anyhow!("Failed to init ASR: {}", e))?;
    crate::asr_engine::commands::asr_validate_model_ready(app.clone(), None, None, None, None)
        .await
        .map_err(|e| anyhow!("{}", e))?;
    let asr = crate::asr_engine::commands::get_engine_arc()
        .map_err(|e| anyhow!("{}", e))?;
```

Replace with:

```rust
    // Initialize ASR engine (ROVER or single-model, per saved config)
    let rover_enabled = {
        let app_state = app
            .try_state::<AppState>()
            .ok_or_else(|| anyhow!("App state not available"))?;
        crate::database::repositories::setting::SettingsRepository::get_transcript_config(
            app_state.db_manager.pool(),
        )
        .await
        .ok()
        .flatten()
        .map(|c| c.rover_enabled)
        .unwrap_or(false)
    };

    let (asr, rover): (Option<std::sync::Arc<crate::asr_engine::engine::AsrEngine>>, Option<std::sync::Arc<tokio::sync::Mutex<crate::rover_engine::engine::RoverDecoder>>>) =
        if rover_enabled {
            crate::rover_engine::commands::rover_init().await
                .map_err(|e| anyhow!("Failed to init ROVER: {}", e))?;
            crate::rover_engine::commands::rover_validate_model_ready(app.clone())
                .await
                .map_err(|e| anyhow!("{}", e))?;
            let rover = crate::rover_engine::commands::get_engine_arc()
                .map_err(|e| anyhow!("{}", e))?;
            (None, Some(rover))
        } else {
            crate::asr_engine::commands::asr_init().await
                .map_err(|e| anyhow!("Failed to init ASR: {}", e))?;
            crate::asr_engine::commands::asr_validate_model_ready(app.clone(), None, None, None, None)
                .await
                .map_err(|e| anyhow!("{}", e))?;
            let asr = crate::asr_engine::commands::get_engine_arc()
                .map_err(|e| anyhow!("{}", e))?;
            (Some(asr), None)
        };
```

- [ ] **Step 2: Branch the per-segment transcribe call**

Find:

```rust
        // Transcribe with ASR
        let text = asr
            .transcribe_audio(segment.samples.clone())
            .await
            .map_err(|e| anyhow!("ASR transcription failed on segment {}: {}", i, e))?;
```

Replace with:

```rust
        // Transcribe with ASR or ROVER, per the branch resolved above
        let text = if let Some(rover) = &rover {
            let rover = rover.clone();
            let samples = segment.samples.clone();
            tokio::task::block_in_place(move || {
                let mut guard = rover.blocking_lock();
                guard.decode(&samples, 16000.0)
            })
            .map(|r| r.text)
            .map_err(|e| anyhow!("ROVER transcription failed on segment {}: {}", i, e))?
        } else {
            asr.as_ref()
                .expect("asr must be Some when rover is None")
                .transcribe_audio(segment.samples.clone())
                .await
                .map_err(|e| anyhow!("ASR transcription failed on segment {}: {}", i, e))?
        };
```

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: same pre-existing `api.rs` error, nothing new from this file. If `AppState` or `Runtime`/`Manager` traits aren't already imported in `import.rs`, add them — check the top of the file first; `use crate::state::AppState;` is already confirmed present.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/audio/import.rs
git commit -m "feat(rover): branch file-import transcription on roverEnabled"
```

---

### Task 7: `audio/retranscription.rs`

**Files:**
- Modify: `frontend/src-tauri/src/audio/retranscription.rs`

Same pattern as Task 6, applied to this file's near-identical structure.

- [ ] **Step 1: Branch the engine initialization**

Find:

```rust
    // Ensure ASR engine is ready
    crate::asr_engine::commands::asr_init().await
        .map_err(|e| anyhow!("Failed to init ASR: {}", e))?;
    crate::asr_engine::commands::asr_validate_model_ready(app.clone(), None, None, None, None)
        .await
        .map_err(|e| anyhow!("{}", e))?;
    let engine = crate::asr_engine::commands::get_engine_arc()
        .map_err(|e| anyhow!("{}", e))?;
```

Replace with:

```rust
    // Ensure ASR engine is ready (ROVER or single-model, per saved config)
    let rover_enabled = {
        let app_state = app
            .try_state::<AppState>()
            .ok_or_else(|| anyhow!("App state not available"))?;
        crate::database::repositories::setting::SettingsRepository::get_transcript_config(
            app_state.db_manager.pool(),
        )
        .await
        .ok()
        .flatten()
        .map(|c| c.rover_enabled)
        .unwrap_or(false)
    };

    let (engine, rover): (Option<std::sync::Arc<crate::asr_engine::engine::AsrEngine>>, Option<std::sync::Arc<tokio::sync::Mutex<crate::rover_engine::engine::RoverDecoder>>>) =
        if rover_enabled {
            crate::rover_engine::commands::rover_init().await
                .map_err(|e| anyhow!("Failed to init ROVER: {}", e))?;
            crate::rover_engine::commands::rover_validate_model_ready(app.clone())
                .await
                .map_err(|e| anyhow!("{}", e))?;
            let rover = crate::rover_engine::commands::get_engine_arc()
                .map_err(|e| anyhow!("{}", e))?;
            (None, Some(rover))
        } else {
            crate::asr_engine::commands::asr_init().await
                .map_err(|e| anyhow!("Failed to init ASR: {}", e))?;
            crate::asr_engine::commands::asr_validate_model_ready(app.clone(), None, None, None, None)
                .await
                .map_err(|e| anyhow!("{}", e))?;
            let engine = crate::asr_engine::commands::get_engine_arc()
                .map_err(|e| anyhow!("{}", e))?;
            (Some(engine), None)
        };
```

- [ ] **Step 2: Branch the per-segment transcribe call**

Find:

```rust
        let text = engine
            .transcribe_audio(segment.samples.clone())
            .await
            .map_err(|e| anyhow!("ZipFormer transcription failed on segment {}: {}", i, e))?;
```

Replace with:

```rust
        let text = if let Some(rover) = &rover {
            let rover = rover.clone();
            let samples = segment.samples.clone();
            tokio::task::block_in_place(move || {
                let mut guard = rover.blocking_lock();
                guard.decode(&samples, 16000.0)
            })
            .map(|r| r.text)
            .map_err(|e| anyhow!("ROVER transcription failed on segment {}: {}", i, e))?
        } else {
            engine
                .as_ref()
                .expect("engine must be Some when rover is None")
                .transcribe_audio(segment.samples.clone())
                .await
                .map_err(|e| anyhow!("ASR transcription failed on segment {}: {}", i, e))?
        };
```

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: same pre-existing `api.rs` error, nothing new.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/audio/retranscription.rs
git commit -m "feat(rover): branch retranscription on roverEnabled"
```

---

### Task 8: `api/api.rs`

**Files:**
- Modify: `frontend/src-tauri/src/api/api.rs`

This is where the pre-existing `save_transcript_config` call-site error from Task 1 finally gets fixed.

- [ ] **Step 1: Extend `TranscriptConfig`**

Find:

```rust
pub struct TranscriptConfig {
    pub provider: String,
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(rename = "asrVariant")]
    pub asr_variant: Option<String>,
    #[serde(rename = "decodingMethod")]
    pub decoding_method: Option<String>,
    #[serde(rename = "numActivePaths")]
    pub num_active_paths: Option<i32>,
    #[serde(rename = "maxSegmentSeconds")]
    pub max_segment_seconds: Option<i32>,
}
```

Replace with:

```rust
pub struct TranscriptConfig {
    pub provider: String,
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(rename = "asrVariant")]
    pub asr_variant: Option<String>,
    #[serde(rename = "decodingMethod")]
    pub decoding_method: Option<String>,
    #[serde(rename = "numActivePaths")]
    pub num_active_paths: Option<i32>,
    #[serde(rename = "maxSegmentSeconds")]
    pub max_segment_seconds: Option<i32>,
    #[serde(rename = "roverEnabled")]
    pub rover_enabled: bool,
    #[serde(rename = "roverFamilyB")]
    pub rover_family_b: Option<String>,
    #[serde(rename = "roverVariantB")]
    pub rover_variant_b: Option<String>,
}
```

- [ ] **Step 2: Populate the new fields in `api_get_transcript_config`**

Find:

```rust
            Ok(Some(TranscriptConfig {
                provider,
                model,
                api_key: None,
                asr_variant: Some(config.asr_variant.clone()),
                decoding_method: Some(config.decoding_method.clone()),
                num_active_paths: Some(config.num_active_paths),
                max_segment_seconds: Some(config.max_segment_seconds),
            }))
        }
        Ok(None) => {
            log_info!("No transcript config found, returning default.");
            Ok(Some(TranscriptConfig {
                provider: "asr".to_string(),
                model: crate::config::ZIPFORMER_MODEL_NAME.to_string(),
                api_key: None,
                asr_variant: Some("int8".to_string()),
                decoding_method: Some("modified_beam_search".to_string()),
                num_active_paths: Some(15),
                max_segment_seconds: Some(crate::audio::common::DEFAULT_MAX_SEGMENT_SECONDS as i32),
            }))
        }
```

Replace with:

```rust
            Ok(Some(TranscriptConfig {
                provider,
                model,
                api_key: None,
                asr_variant: Some(config.asr_variant.clone()),
                decoding_method: Some(config.decoding_method.clone()),
                num_active_paths: Some(config.num_active_paths),
                max_segment_seconds: Some(config.max_segment_seconds),
                rover_enabled: config.rover_enabled,
                rover_family_b: config.rover_family_b.clone(),
                rover_variant_b: config.rover_variant_b.clone(),
            }))
        }
        Ok(None) => {
            log_info!("No transcript config found, returning default.");
            Ok(Some(TranscriptConfig {
                provider: "asr".to_string(),
                model: crate::config::ZIPFORMER_MODEL_NAME.to_string(),
                api_key: None,
                asr_variant: Some("int8".to_string()),
                decoding_method: Some("modified_beam_search".to_string()),
                num_active_paths: Some(15),
                max_segment_seconds: Some(crate::audio::common::DEFAULT_MAX_SEGMENT_SECONDS as i32),
                rover_enabled: false,
                rover_family_b: None,
                rover_variant_b: None,
            }))
        }
```

- [ ] **Step 3: Extend `api_save_transcript_config`**

Find:

```rust
pub async fn api_save_transcript_config<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    provider: String,
    model: String,
    api_key: Option<String>,
    asr_variant: Option<String>,
    decoding_method: Option<String>,
    num_active_paths: Option<i32>,
    max_segment_seconds: Option<i32>,
    _auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
```

Replace with:

```rust
pub async fn api_save_transcript_config<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    provider: String,
    model: String,
    api_key: Option<String>,
    asr_variant: Option<String>,
    decoding_method: Option<String>,
    num_active_paths: Option<i32>,
    max_segment_seconds: Option<i32>,
    rover_enabled: Option<bool>,
    rover_family_b: Option<String>,
    rover_variant_b: Option<String>,
    _auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
```

Then find:

```rust
    if let Err(e) = SettingsRepository::save_transcript_config(
        pool, "asr", &model, variant, dm, paths, max_seg as i32,
    )
    .await
    {
        log_error!("Failed to save transcript config: {}", e);
```

Replace with:

```rust
    let rover_on = rover_enabled.unwrap_or(false);
    let rover_variant_b_resolved = rover_variant_b.as_deref().unwrap_or("int8");

    if let Err(e) = SettingsRepository::save_transcript_config(
        pool,
        "asr",
        &model,
        variant,
        dm,
        paths,
        max_seg as i32,
        rover_on,
        rover_family_b.as_deref(),
        if rover_on { Some(rover_variant_b_resolved) } else { None },
    )
    .await
    {
        log_error!("Failed to save transcript config: {}", e);
```

- [ ] **Step 4: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors — this was the last unresolved call site.

- [ ] **Step 5: Run the full test suite**

Run: `cd frontend/src-tauri && cargo test rover_engine rnnt_decoder asr_engine -- --nocapture`
Expected: all previously-passing tests (Phase A: 9, Phase B: 13, third-family: 8) still PASS. No test in this plan touches these modules' logic, only their callers — a regression here would indicate a mistake in this task.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/api/api.rs
git commit -m "feat(rover): thread roverEnabled/roverFamilyB/roverVariantB through the save/get API"
```

---

### Task 9: `lib/asr.ts`

**Files:**
- Modify: `frontend/src/lib/asr.ts`

- [ ] **Step 1: Add ROVER fields to the config shape and the save/load calls**

Find:

```typescript
export const AsrAPI = {
  init: (): Promise<void> => invoke('asr_init'),
  getModelStatus: (): Promise<ModelStatus> => invoke('asr_get_model_status'),
  isModelLoaded: (): Promise<boolean> => invoke('asr_is_model_loaded'),
  getModelsDirectory: (): Promise<string> => invoke('asr_get_models_directory'),
  downloadModel: (family: AsrModelFamily, variant: ModelVariant): Promise<void> =>
    invoke('asr_download_model', { family, variant }),
  loadModel: (
    family: AsrModelFamily,
    variant: ModelVariant,
    decodingMethod: DecodingMethod,
    numActivePaths: number
  ): Promise<void> =>
    invoke('asr_load_model', { family, variant, decodingMethod, numActivePaths }),
  getVariantStatus: (family: AsrModelFamily, variant: ModelVariant): Promise<VariantStatus> =>
    invoke('asr_get_variant_status', { family, variant }),
  validateModelReady: (
    family?: AsrModelFamily,
    variant?: ModelVariant,
    decodingMethod?: DecodingMethod,
    numActivePaths?: number
  ): Promise<string> =>
    invoke('asr_validate_model_ready', { family, variant, decodingMethod, numActivePaths }),
};
```

Replace with:

```typescript
export const AsrAPI = {
  init: (): Promise<void> => invoke('asr_init'),
  getModelStatus: (): Promise<ModelStatus> => invoke('asr_get_model_status'),
  isModelLoaded: (): Promise<boolean> => invoke('asr_is_model_loaded'),
  getModelsDirectory: (): Promise<string> => invoke('asr_get_models_directory'),
  downloadModel: (family: AsrModelFamily, variant: ModelVariant): Promise<void> =>
    invoke('asr_download_model', { family, variant }),
  loadModel: (
    family: AsrModelFamily,
    variant: ModelVariant,
    decodingMethod: DecodingMethod,
    numActivePaths: number
  ): Promise<void> =>
    invoke('asr_load_model', { family, variant, decodingMethod, numActivePaths }),
  getVariantStatus: (family: AsrModelFamily, variant: ModelVariant): Promise<VariantStatus> =>
    invoke('asr_get_variant_status', { family, variant }),
  validateModelReady: (
    family?: AsrModelFamily,
    variant?: ModelVariant,
    decodingMethod?: DecodingMethod,
    numActivePaths?: number
  ): Promise<string> =>
    invoke('asr_validate_model_ready', { family, variant, decodingMethod, numActivePaths }),
};

export const RoverAPI = {
  isModelLoaded: (): Promise<boolean> => invoke('rover_is_model_loaded'),
  getCurrentConfig: (): Promise<{
    isLoaded: boolean;
    familyA?: AsrModelFamily;
    variantA?: ModelVariant;
    familyB?: AsrModelFamily;
    variantB?: ModelVariant;
  }> => invoke('rover_get_current_config'),
  validateModelReady: (): Promise<string> => invoke('rover_validate_model_ready'),
};
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: no new errors (`RoverAPI` is unused so far — fine, `AsrModelManager.tsx` uses it starting Task 10).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/asr.ts
git commit -m "feat(rover): add RoverAPI wrapper for rover_* Tauri commands"
```

---

### Task 10: `AsrModelManager.tsx` — ROVER toggle and second model picker

**Files:**
- Modify: `frontend/src/components/AsrModelManager.tsx`

- [ ] **Step 1: Add ROVER state**

Find:

```typescript
  const [selectedFamily, setSelectedFamily] = useState<AsrModelFamily>(DEFAULT_FAMILY);
  const [selectedVariant, setSelectedVariant] = useState<ModelVariant>(DEFAULT_VARIANT);
  const [decodingMethod, setDecodingMethod] = useState<DecodingMethod>(DEFAULT_DECODING);
  const [numActivePaths, setNumActivePaths] = useState<number>(DEFAULT_PATHS);
  const [maxSegmentSeconds, setMaxSegmentSeconds] = useState<number>(DEFAULT_MAX_SEGMENT_SECONDS);
```

Replace with:

```typescript
  const [selectedFamily, setSelectedFamily] = useState<AsrModelFamily>(DEFAULT_FAMILY);
  const [selectedVariant, setSelectedVariant] = useState<ModelVariant>(DEFAULT_VARIANT);
  const [decodingMethod, setDecodingMethod] = useState<DecodingMethod>(DEFAULT_DECODING);
  const [numActivePaths, setNumActivePaths] = useState<number>(DEFAULT_PATHS);
  const [maxSegmentSeconds, setMaxSegmentSeconds] = useState<number>(DEFAULT_MAX_SEGMENT_SECONDS);

  const [roverEnabled, setRoverEnabled] = useState(false);
  const [roverFamilyB, setRoverFamilyB] = useState<AsrModelFamily>('gipformer-65m-rnnt');
  const [roverVariantB, setRoverVariantB] = useState<ModelVariant>('int8');
  const [roverVariantBStatus, setRoverVariantBStatus] = useState<VariantStatus>({
    hasFiles: false,
    isLoaded: false,
  });
  const [roverDownloadState, setRoverDownloadState] = useState<DownloadState>({
    downloading: false,
    progress: 0,
    error: null,
  });

  const roverModelBInfo = ASR_MODELS.find((m) => m.id === roverFamilyB);
  const roverAvailableVariantsB = VARIANT_OPTIONS.filter((v) =>
    roverModelBInfo ? roverModelBInfo.availableVariants.includes(v.id) : true
  );
```

- [ ] **Step 2: Auto-correct model B's variant when it changes families, and refresh its status**

Find:

```typescript
  useEffect(() => {
    if (!selectedModelInfo) return;
    if (!selectedModelInfo.availableVariants.includes(selectedVariant)) {
      setSelectedVariant(selectedModelInfo.availableVariants[0]);
    }
  }, [selectedFamily, selectedModelInfo, selectedVariant]);
```

Replace with:

```typescript
  useEffect(() => {
    if (!selectedModelInfo) return;
    if (!selectedModelInfo.availableVariants.includes(selectedVariant)) {
      setSelectedVariant(selectedModelInfo.availableVariants[0]);
    }
  }, [selectedFamily, selectedModelInfo, selectedVariant]);

  useEffect(() => {
    if (!roverModelBInfo) return;
    if (!roverModelBInfo.availableVariants.includes(roverVariantB)) {
      setRoverVariantB(roverModelBInfo.availableVariants[0]);
    }
  }, [roverFamilyB, roverModelBInfo, roverVariantB]);

  const refreshRoverVariantBStatus = useCallback(async () => {
    try {
      setRoverVariantBStatus(await AsrAPI.getVariantStatus(roverFamilyB, roverVariantB));
    } catch {
      setRoverVariantBStatus({ hasFiles: false, isLoaded: false });
    }
  }, [roverFamilyB, roverVariantB]);

  useEffect(() => {
    if (roverEnabled) {
      refreshRoverVariantBStatus();
    }
  }, [roverEnabled, refreshRoverVariantBStatus]);
```

- [ ] **Step 3: Load `roverEnabled`/`roverFamilyB`/`roverVariantB` from saved config**

Find:

```typescript
      const config = await invoke<{
        model?: string;
        asrVariant?: string;
        decodingMethod?: string;
        numActivePaths?: number;
        maxSegmentSeconds?: number;
      } | null>('api_get_transcript_config');
      if (config) {
        if (
          config.model === 'zipformer-vi-30m' ||
          config.model === 'gipformer-65m-rnnt' ||
          config.model === 'sherpa-onnx-zipformer-vi-2025-04-20'
        ) {
          setSelectedFamily(config.model);
        }
```

Replace with:

```typescript
      const config = await invoke<{
        model?: string;
        asrVariant?: string;
        decodingMethod?: string;
        numActivePaths?: number;
        maxSegmentSeconds?: number;
        roverEnabled?: boolean;
        roverFamilyB?: string;
        roverVariantB?: string;
      } | null>('api_get_transcript_config');
      if (config) {
        if (
          config.model === 'zipformer-vi-30m' ||
          config.model === 'gipformer-65m-rnnt' ||
          config.model === 'sherpa-onnx-zipformer-vi-2025-04-20'
        ) {
          setSelectedFamily(config.model);
        }
        if (typeof config.roverEnabled === 'boolean') {
          setRoverEnabled(config.roverEnabled);
        }
        if (
          config.roverFamilyB === 'zipformer-vi-30m' ||
          config.roverFamilyB === 'gipformer-65m-rnnt' ||
          config.roverFamilyB === 'sherpa-onnx-zipformer-vi-2025-04-20'
        ) {
          setRoverFamilyB(config.roverFamilyB);
        }
        if (config.roverVariantB === 'int8' || config.roverVariantB === 'full') {
          setRoverVariantB(config.roverVariantB);
        }
```

(This is inserted as additional statements inside the existing `if (config) { ... }` block — the rest of that block, e.g. the `asrVariant`/`decodingMethod`/`numActivePaths` handling below it, is unchanged.)

- [ ] **Step 4: Add a download handler for model B**

Find:

```typescript
  const handleDownload = async () => {
    setDownloadState({ downloading: true, progress: 0, error: null });
    try {
      await AsrAPI.downloadModel(selectedFamily, selectedVariant);
    } catch (e) {
      setDownloadState((prev) => ({
        ...prev,
        downloading: false,
        error: String(e),
      }));
    }
  };
```

Replace with:

```typescript
  const handleDownload = async () => {
    setDownloadState({ downloading: true, progress: 0, error: null });
    try {
      await AsrAPI.downloadModel(selectedFamily, selectedVariant);
    } catch (e) {
      setDownloadState((prev) => ({
        ...prev,
        downloading: false,
        error: String(e),
      }));
    }
  };

  const handleDownloadRoverB = async () => {
    setRoverDownloadState({ downloading: true, progress: 0, error: null });
    try {
      await AsrAPI.downloadModel(roverFamilyB, roverVariantB);
      await refreshRoverVariantBStatus();
      setRoverDownloadState({ downloading: false, progress: 100, error: null });
    } catch (e) {
      setRoverDownloadState((prev) => ({
        ...prev,
        downloading: false,
        error: String(e),
      }));
    }
  };
```

(The download-progress *event* for model B reuses the same `asr-model-download-progress`/`-complete`/`-error` events already listened to for model A — `asr_download_model` doesn't distinguish which "slot" a download belongs to, so this is a known simplification: while a model-B download is in progress, the model-A progress bar's event listener will also fire. This is acceptable for the initial version — both progress bars target the same underlying download system and there's no concurrent A+B download in the UI flow (the buttons aren't both clickable mid-download; downloading is inherently one-at-a-time here since there is only one `asr_download_model` in flight at once from this component).)

- [ ] **Step 5: Include ROVER fields when saving**

Find:

```typescript
      await invoke('api_save_transcript_config', {
        provider: 'asr',
        model: selectedFamily,
        apiKey: null,
        asrVariant: selectedVariant,
        decodingMethod,
        numActivePaths,
        maxSegmentSeconds,
      });

      const status = variantStatuses[selectedVariant];
      if (status.hasFiles) {
        await AsrAPI.loadModel(selectedFamily, selectedVariant, decodingMethod, numActivePaths);
        await refreshAllVariantStatuses(selectedFamily);
      }
```

Replace with:

```typescript
      await invoke('api_save_transcript_config', {
        provider: 'asr',
        model: selectedFamily,
        apiKey: null,
        asrVariant: selectedVariant,
        decodingMethod,
        numActivePaths,
        maxSegmentSeconds,
        roverEnabled,
        roverFamilyB: roverEnabled ? roverFamilyB : null,
        roverVariantB: roverEnabled ? roverVariantB : null,
      });

      if (roverEnabled) {
        if (roverVariantBStatus.hasFiles) {
          await RoverAPI.validateModelReady();
        }
      } else {
        const status = variantStatuses[selectedVariant];
        if (status.hasFiles) {
          await AsrAPI.loadModel(selectedFamily, selectedVariant, decodingMethod, numActivePaths);
          await refreshAllVariantStatuses(selectedFamily);
        }
      }
```

- [ ] **Step 6: Update the import to bring in `RoverAPI`**

Find:

```typescript
import {
  ASR_MODELS,
  AsrAPI,
  AsrModelFamily,
  DecodingMethod,
  ModelVariant,
  VariantStatus,
} from '../lib/asr';
```

Replace with:

```typescript
import {
  ASR_MODELS,
  AsrAPI,
  AsrModelFamily,
  DecodingMethod,
  ModelVariant,
  RoverAPI,
  VariantStatus,
} from '../lib/asr';
```

- [ ] **Step 7: Add the ROVER toggle and model-B picker to the JSX**

Find (the block right after the Model ASR family selector and before the "Variant selector" comment):

```typescript
      {/* Variant selector */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Biến thể
        </label>
```

Replace with:

```typescript
      {/* ROVER toggle */}
      <div className="flex items-center justify-between p-3 rounded-lg border border-gray-200 dark:border-gray-700">
        <div>
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Bật ROVER (kết hợp 2 model)
          </label>
          <p className="text-xs text-gray-500 dark:text-gray-400">
            ROVER dùng gấp đôi RAM/CPU so với 1 model — khuyến nghị dùng biến thể int8 cho cả 2 phía.
          </p>
        </div>
        <input
          type="checkbox"
          checked={roverEnabled}
          onChange={(e) => setRoverEnabled(e.target.checked)}
          disabled={disabled}
          className="w-5 h-5 accent-blue-500 disabled:opacity-50"
        />
      </div>

      {roverEnabled && (
        <div className="space-y-2 p-3 rounded-lg border border-blue-200 dark:border-blue-800">
          <p className="text-xs font-medium text-gray-700 dark:text-gray-300">
            Model B (phụ) — Model A (chính) là model chọn ở trên
          </p>
          <select
            value={roverFamilyB}
            onChange={(e) => setRoverFamilyB(e.target.value as AsrModelFamily)}
            disabled={disabled}
            className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
          >
            {ASR_MODELS.filter((m) => m.id !== selectedFamily).map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </select>
          <select
            value={roverVariantB}
            onChange={(e) => setRoverVariantB(e.target.value as ModelVariant)}
            disabled={disabled || roverAvailableVariantsB.length <= 1}
            className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
          >
            {roverAvailableVariantsB.map((v) => {
              const size = v.id === 'int8' ? roverModelBInfo?.int8Size : roverModelBInfo?.fullSize;
              return (
                <option key={v.id} value={v.id}>
                  {v.label} ({size})
                </option>
              );
            })}
          </select>
          <div className="flex items-center justify-between p-2 rounded-md bg-gray-50 dark:bg-gray-800">
            <span className="text-xs text-gray-600 dark:text-gray-300">
              {roverVariantBStatus.hasFiles ? '✓ Đã tải' : 'Chưa tải'}
            </span>
            {!roverVariantBStatus.hasFiles && !roverDownloadState.downloading && (
              <button
                onClick={handleDownloadRoverB}
                disabled={disabled}
                className="px-3 py-1.5 text-xs rounded-md bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white font-medium transition-colors"
              >
                Tải xuống
              </button>
            )}
          </div>
          {roverDownloadState.downloading && (
            <p className="text-xs text-gray-500">Đang tải model B...</p>
          )}
          {roverDownloadState.error && (
            <p className="text-xs text-red-500 dark:text-red-400">{roverDownloadState.error}</p>
          )}
        </div>
      )}

      {/* Variant selector */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Biến thể
        </label>
```

- [ ] **Step 8: Hide the decoding-method and num-active-paths controls when ROVER is on**

Find:

```typescript
      {/* Decoding method */}
      <div className="space-y-2">
```

Replace with:

```typescript
      {/* Decoding method — sherpa-onnx-specific, not applicable to ROVER's custom decoder */}
      {!roverEnabled && (
      <div className="space-y-2">
```

Find the matching closing tag right after that block (before the "Num active paths" comment):

```typescript
      </div>

      {/* Num active paths — only for beam search */}
      {decodingMethod === 'modified_beam_search' && (
```

Replace with:

```typescript
      </div>
      )}

      {/* Num active paths — only for beam search, and not applicable to ROVER */}
      {!roverEnabled && decodingMethod === 'modified_beam_search' && (
```

- [ ] **Step 9: Verify TypeScript compiles**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: no errors.

- [ ] **Step 10: Commit**

```bash
git add frontend/src/components/AsrModelManager.tsx
git commit -m "feat(rover): add ROVER toggle and model B picker to AsrModelManager"
```

---

### Task 11: Manual E2E (required before merge)

**Files:** none (verification only)

- [ ] **Step 1: Full build**

Run: `cd frontend/src-tauri && cargo build`
Expected: `Finished` with no errors.

- [ ] **Step 2: Start the app and enable ROVER**

Settings → Nhận dạng → bật "Bật ROVER" → Model A = ZipFormer 30M int8 (or whatever's already selected) → Model B = Gipformer 65M int8 → Tải xuống nếu cần → Lưu cấu hình.
Expected: no errors in the terminal log; "Đã lưu cấu hình thành công" appears.

- [ ] **Step 3: Live recording through ROVER**

Record ~10 seconds of Vietnamese speech. Expected: transcript appears, UI stays responsive during decode (both models decode on background OS threads via `block_in_place`, not blocking the UI thread — but do watch for any noticeable stutter, since this is running on CPU and doubles the work per segment).

- [ ] **Step 4: File import through ROVER**

Import a short Vietnamese audio file. Expected: transcript completes without error.

- [ ] **Step 5: Retranscribe through ROVER**

Retranscribe an existing meeting. Expected: completes without error.

- [ ] **Step 6: Turn ROVER off, confirm the old path still works**

Settings → turn off "Bật ROVER" → Lưu → record/import/retranscribe once more.
Expected: behaves exactly as before Phase C — single-model path unaffected.

- [ ] **Step 7: Confirm CAPU still runs after ROVER**

Check any transcript produced via ROVER in Steps 3-5 has punctuation/capitalization applied — `audio/post_asr.rs`'s `process_asr_text` call sites in `import.rs`/`retranscription.rs` are unchanged by this plan, but worth a final visual confirmation that the ROVER branch's `text` output feeds into the same CAPU call as before.

---

## Spec coverage checklist

| Spec requirement | Task |
|---|---|
| 3 new DB columns, reusing `model`/`asrVariant` as family A | Task 1 |
| No duplicated download logic | Confirmed — Task 3's `rover_engine::commands` has no download code; Task 10's UI reuses `AsrAPI.downloadModel` |
| `tokio::sync::Mutex` for `RoverDecoder` state | Task 3 |
| `resolve_models_base_dir` reused, not duplicated | Task 2 |
| `RoverProvider` bridges `&mut self` decode into `&self` trait via `block_in_place` | Task 4 |
| `worker.rs` unchanged | Task 5 (branch lives in `engine.rs`, confirmed no edits to `worker.rs` anywhere in this plan) |
| `import.rs`/`retranscription.rs` each get their own branch, not unified onto the trait | Tasks 6, 7 |
| `TranscriptResult.confidence` populated for ROVER | Task 4 |
| UI toggle + dual picker, hide sherpa-onnx-specific controls | Task 10 |
| Default off | Confirmed — DB default `0`, frontend `useState(false)` |
| Manual E2E across live/import/retranscribe, plus ROVER-off regression check | Task 11 |

---

## Notes for whoever executes this (e.g. via Cursor)

- Task 1 through Task 7 will each leave one known, pre-existing compile error (the `api.rs` call site) until Task 8 — this is intentional and called out at each step's "Expected" line so it isn't mistaken for a mistake in that step.
- Every Rust snippet in this plan was checked against the actual current file content, not reconstructed from the earlier Gipformer-era plan — `import.rs`/`retranscription.rs` in particular have evolved since then (e.g. `max_segment_seconds` splitting, CAPU trailing-context handling) and this plan's diffs match what's actually there now.
- If `AppState`, `Manager`, or `Runtime` aren't already imported in a file this plan edits, add them — check the top of the file before assuming an import is missing.
