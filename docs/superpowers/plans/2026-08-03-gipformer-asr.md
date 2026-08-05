# Gipformer 65M ASR — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add [g-group-ai-lab/gipformer-65M-rnnt](https://huggingface.co/g-group-ai-lab/gipformer-65M-rnnt) as a second selectable Vietnamese ASR model alongside the existing ZipFormer 30M, via a unified `asr_engine` module and Settings UI.

**Architecture:** Refactor `zipformer_engine/` → `asr_engine/` with a `ModelFamily` enum (`ZipFormer30M`, `Gipformer65M`) × `ModelVariant` (`Int8`, `Full`). One global `AsrEngine` instance loads exactly one family+variant at a time through sherpa-onnx `OfflineRecognizer`. CAPU pipeline is unchanged — it runs after ASR regardless of family.

**Tech Stack:** Rust, sherpa-onnx (existing), Tauri 2.x commands, React/TypeScript frontend, SQLite migrations.

**Reference spec:** `docs/superpowers/specs/2026-08-03-gipformer-asr-design.md`

---

## File map (before you start)

| File | Responsibility after refactor |
|---|---|
| `src/config.rs` | ZipFormer + Gipformer HF URLs, filenames, sizes |
| `src/asr_engine/model_family.rs` | `ModelFamily` + `ModelVariant` lookup (paths, files, URLs) |
| `src/asr_engine/engine.rs` | Download, load, unload, transcribe (was `zipformer_engine.rs`) |
| `src/asr_engine/commands.rs` | Tauri `asr_*` commands (was `zipformer_engine/commands.rs`) |
| `src/asr_engine/mod.rs` | Module exports |
| `src/audio/transcription/asr_provider.rs` | `TranscriptionProvider` impl (was `zipformer_provider.rs`) |
| `src/audio/transcription/engine.rs` | `validate_transcription_model_ready`, `get_or_init_transcription_engine` |
| `migrations/20260803000000_add_asr_family.sql` | Rename column, update provider |
| `frontend/src/lib/asr.ts` | TypeScript API (was `zipformer.ts`) |
| `frontend/src/components/AsrModelManager.tsx` | Unified family+variant UI (was `ZipFormerModelManager.tsx`) |

**Delete after migration:** `zipformer_engine/` directory, `lib/zipformer.ts`, `ZipFormerModelManager.tsx`.

---

### Task 1: Add Gipformer constants to `config.rs`

**Files:**
- Modify: `frontend/src-tauri/src/config.rs`

- [ ] **Step 1: Append Gipformer block after ZipFormer constants**

After line 29 (`pub const ZIPFORMER_VOCAB`), add:

```rust
/// Application configuration constants — Gipformer 65M Vietnamese ASR

pub const GIPFORMER_MODEL_NAME: &str = "gipformer-65m-rnnt";

pub const GIPFORMER_INT8_HF_URL: &str =
    "https://huggingface.co/g-group-ai-lab/gipformer-65M-rnnt/resolve/main";
pub const GIPFORMER_INT8_SUBDIR: &str = "gipformer-vi-int8";
pub const GIPFORMER_INT8_ENCODER: &str = "encoder-epoch-35-avg-6.int8.onnx";
pub const GIPFORMER_INT8_DECODER: &str = "decoder-epoch-35-avg-6.int8.onnx";
pub const GIPFORMER_INT8_JOINER: &str = "joiner-epoch-35-avg-6.int8.onnx";
pub const GIPFORMER_INT8_SIZE_BYTES: u64 = 71_000_000;

pub const GIPFORMER_FULL_HF_URL: &str =
    "https://huggingface.co/g-group-ai-lab/gipformer-65M-rnnt/resolve/main";
pub const GIPFORMER_FULL_SUBDIR: &str = "gipformer-vi-full";
pub const GIPFORMER_FULL_ENCODER: &str = "encoder-epoch-35-avg-6.onnx";
pub const GIPFORMER_FULL_DECODER: &str = "decoder-epoch-35-avg-6.onnx";
pub const GIPFORMER_FULL_JOINER: &str = "joiner-epoch-35-avg-6.onnx";
pub const GIPFORMER_FULL_SIZE_BYTES: u64 = 261_000_000;

pub const GIPFORMER_BPE: &str = "bpe.model";
pub const GIPFORMER_TOKENS: &str = "tokens.txt";
pub const GIPFORMER_VOCAB_FALLBACK: &str = "config.json";
```

- [ ] **Step 2: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors.

---

### Task 2: Create `ModelFamily` with unit tests

**Files:**
- Create: `frontend/src-tauri/src/asr_engine/model_family.rs`
- Create: `frontend/src-tauri/src/asr_engine/mod.rs` (stub)
- Modify: `frontend/src-tauri/src/lib.rs` — add `pub mod asr_engine;` (keep `zipformer_engine` for now)

- [ ] **Step 1: Create `asr_engine/mod.rs`**

```rust
pub mod model_family;
```

- [ ] **Step 2: Create `model_family.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelFamily {
    ZipFormer30M,
    Gipformer65M,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelVariant {
    #[default]
    Int8,
    Full,
}

impl ModelFamily {
    pub fn from_id(s: &str) -> Self {
        match s {
            crate::config::GIPFORMER_MODEL_NAME => ModelFamily::Gipformer65M,
            _ => ModelFamily::ZipFormer30M,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => crate::config::ZIPFORMER_MODEL_NAME,
            ModelFamily::Gipformer65M => crate::config::GIPFORMER_MODEL_NAME,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => "ZipFormer 30M",
            ModelFamily::Gipformer65M => "Gipformer 65M",
        }
    }

    pub fn variant_subdir(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_SUBDIR,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_SUBDIR,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_SUBDIR,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_SUBDIR,
        }
    }

    pub fn hf_url(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_HF_URL,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_HF_URL,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_HF_URL,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_HF_URL,
        }
    }

    pub fn encoder_file(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_ENCODER,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_ENCODER,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_ENCODER,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_ENCODER,
        }
    }

    pub fn decoder_file(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_DECODER,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_DECODER,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_DECODER,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_DECODER,
        }
    }

    pub fn joiner_file(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_JOINER,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_JOINER,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_JOINER,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_JOINER,
        }
    }

    pub fn bpe_file(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => crate::config::ZIPFORMER_BPE,
            ModelFamily::Gipformer65M => crate::config::GIPFORMER_BPE,
        }
    }

    pub fn token_file(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => crate::config::ZIPFORMER_VOCAB,
            ModelFamily::Gipformer65M => crate::config::GIPFORMER_TOKENS,
        }
    }

    pub fn encoder_size_bytes(self, variant: ModelVariant) -> u64 {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_SIZE_BYTES,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_SIZE_BYTES,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_SIZE_BYTES,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_SIZE_BYTES,
        }
    }

    pub fn model_files(self, variant: ModelVariant) -> [&'static str; 5] {
        [
            self.encoder_file(variant),
            self.decoder_file(variant),
            self.joiner_file(variant),
            self.bpe_file(),
            self.token_file(),
        ]
    }

    pub fn total_size_bytes(self, variant: ModelVariant) -> u64 {
        let shared: u64 = 268_000 + 50_000 + 1_310_000;
        self.encoder_size_bytes(variant) + shared
    }
}

impl ModelVariant {
    pub fn from_str(s: &str) -> Self {
        match s {
            "full" => ModelVariant::Full,
            _ => ModelVariant::Int8,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ModelVariant::Int8 => crate::config::ZIPFORMER_VARIANT_INT8,
            ModelVariant::Full => crate::config::ZIPFORMER_VARIANT_FULL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zipformer_int8_files_match_existing_layout() {
        let files = ModelFamily::ZipFormer30M.model_files(ModelVariant::Int8);
        assert_eq!(files[0], "encoder-epoch-20-avg-10.int8.onnx");
        assert_eq!(files[4], "config.json");
        assert_eq!(
            ModelFamily::ZipFormer30M.variant_subdir(ModelVariant::Int8),
            "zipformer-vi-int8"
        );
    }

    #[test]
    fn gipformer_int8_uses_separate_subdir_and_tokens() {
        let files = ModelFamily::Gipformer65M.model_files(ModelVariant::Int8);
        assert_eq!(files[0], "encoder-epoch-35-avg-6.int8.onnx");
        assert_eq!(files[4], "tokens.txt");
        assert_eq!(
            ModelFamily::Gipformer65M.variant_subdir(ModelVariant::Int8),
            "gipformer-vi-int8"
        );
        assert_ne!(
            ModelFamily::Gipformer65M.variant_subdir(ModelVariant::Int8),
            ModelFamily::ZipFormer30M.variant_subdir(ModelVariant::Int8)
        );
    }

    #[test]
    fn from_id_roundtrip() {
        assert_eq!(
            ModelFamily::from_id("gipformer-65m-rnnt"),
            ModelFamily::Gipformer65M
        );
        assert_eq!(
            ModelFamily::from_id("zipformer-vi-30m"),
            ModelFamily::ZipFormer30M
        );
    }
}
```

- [ ] **Step 3: Register module in `lib.rs`**

Add near existing `pub mod zipformer_engine;`:

```rust
pub mod asr_engine;
```

- [ ] **Step 4: Run tests**

Run: `cd frontend/src-tauri && cargo test asr_engine::model_family -- --nocapture`
Expected: 3 tests PASS.

---

### Task 3: Refactor engine — rename module and add `ModelFamily` support

**Files:**
- Create: `frontend/src-tauri/src/asr_engine/engine.rs` (copy from `zipformer_engine/zipformer_engine.rs`, refactor)
- Modify: `frontend/src-tauri/src/asr_engine/mod.rs`
- Delete later in Task 7: `frontend/src-tauri/src/zipformer_engine/`

- [ ] **Step 1: Update `asr_engine/mod.rs`**

```rust
pub mod commands;
pub mod engine;
pub mod model_family;
```

- [ ] **Step 2: Create `engine.rs` from `zipformer_engine.rs` with these key changes**

1. Rename `ZipFormerEngine` → `AsrEngine`
2. Remove the old `ModelVariant` impl block (lines 32–100) — use `model_family::ModelVariant` instead
3. Add field: `current_family: Arc<RwLock<ModelFamily>>` (default `ZipFormer30M`)
4. Change `variant_dir`:

```rust
fn variant_dir(&self, base: &PathBuf, family: &ModelFamily, variant: &ModelVariant) -> PathBuf {
    base.join(family.variant_subdir(*variant))
}
```

5. Update `are_variant_files_present` signature:

```rust
pub async fn are_variant_files_present(
    &self,
    family: &ModelFamily,
    variant: &ModelVariant,
) -> bool {
    let base = self.models_base_dir.read().await.clone();
    if base == PathBuf::new() {
        return false;
    }
    let dir = self.variant_dir(&base, family, variant);
    family.model_files(*variant).iter().all(|f| dir.join(f).exists())
}
```

6. Update `download_model` signature to take `family: ModelFamily`:

```rust
pub async fn download_model(
    &self,
    family: ModelFamily,
    variant: ModelVariant,
    progress_callback: Option<Box<dyn Fn(u8) + Send>>,
) -> Result<()>
```

Inside the function, replace `variant.model_files()` → `family.model_files(variant)`, `variant.hf_url()` → `family.hf_url(variant)`, `variant_dir(&base, &variant)` → `variant_dir(&base, &family, &variant)`, and encoder size from `family.encoder_size_bytes(variant)`.

7. Update `load_model` signature:

```rust
pub async fn load_model(
    &self,
    family: ModelFamily,
    variant: ModelVariant,
    decoding_method: String,
    num_active_paths: i32,
) -> Result<()>
```

Before loading, if a different family/variant is already loaded, call `unload_model().await`.

Token file resolution for Gipformer fallback:

```rust
let token_path = dir.join(family.token_file());
let tokens = if token_path.exists() {
    token_path
} else if family == ModelFamily::Gipformer65M {
    let fallback = dir.join(crate::config::GIPFORMER_VOCAB_FALLBACK);
    if fallback.exists() {
        log::warn!("Gipformer: tokens.txt missing, falling back to config.json");
        fallback
    } else {
        return Err(anyhow!("Missing token file: {}", family.token_file()));
    }
} else {
    token_path
}
.to_string_lossy()
.to_string();
```

8. Update `get_current_model`:

```rust
pub async fn get_current_family(&self) -> ModelFamily {
    self.current_family.read().await.clone()
}

pub async fn get_current_model(&self) -> Option<String> {
    if self.is_model_loaded().await {
        Some(self.current_family.read().await.id().to_string())
    } else {
        None
    }
}
```

9. Update log messages: `"ZipFormer"` → `"ASR"` where generic.

- [ ] **Step 3: Copy and adapt `commands.rs`**

Create `frontend/src-tauri/src/asr_engine/commands.rs` from `zipformer_engine/commands.rs`:

- `ZIPFORMER_ENGINE` → `ASR_ENGINE`
- `ZipFormerEngine` → `AsrEngine`
- All commands renamed: `zipformer_init` → `asr_init`, etc.
- Add `family: String` param to: `asr_download_model`, `asr_load_model`, `asr_get_variant_status`, `asr_validate_model_ready`
- Events: `asr-model-download-progress`, `asr-model-download-complete`, `asr-model-download-error`
- `asr_get_variant_status`: compare both family AND variant for `isLoaded`
- `asr_validate_model_ready`: read `config.model` (family id) and `config.asr_variant` from DB

`asr_validate_model_ready` DB branch:

```rust
let family = ModelFamily::from_id(&config.model);
let variant = ModelVariant::from_str(&config.asr_variant);
```

- [ ] **Step 4: Verify compile (both modules coexist temporarily)**

Run: `cd frontend/src-tauri && cargo check`
Expected: PASS (duplicate symbols OK since old module still registered — we'll remove in Task 7).

---

### Task 4: Database migration + Rust DB layer

**Files:**
- Create: `frontend/src-tauri/migrations/20260803000000_add_asr_family.sql`
- Modify: `frontend/src-tauri/src/database/models.rs`
- Modify: `frontend/src-tauri/src/database/repositories/setting.rs`
- Modify: `frontend/src-tauri/src/api/api.rs`

- [ ] **Step 1: Create migration SQL**

```sql
UPDATE transcript_settings SET provider = 'asr' WHERE provider = 'zipformer';
ALTER TABLE transcript_settings RENAME COLUMN zipformerVariant TO asrVariant;
```

- [ ] **Step 2: Update `TranscriptSetting` in `models.rs`**

```rust
#[sqlx(rename = "asrVariant")]
#[serde(rename = "asrVariant")]
pub asr_variant: String,
```

Remove `zipformer_variant` field.

- [ ] **Step 3: Update `setting.rs`**

Rename parameter `zipformer_variant` → `asr_variant` in `save_transcript_config` and SQL column `asrVariant`.

Update `get_transcript_config` mapping accordingly.

Change provider validation in `save_transcript_api_key`:

```rust
if provider != "asr" {
    return Err(sqlx::Error::Protocol(
        format!("Unsupported transcript provider: {}. Only asr is supported.", provider).into(),
    ));
}
```

- [ ] **Step 4: Update `api.rs` save handler**

Rename serde field `zipformerVariant` → `asrVariant` in the request struct. Ensure `model` field accepts `zipformer-vi-30m` or `gipformer-65m-rnnt`.

- [ ] **Step 5: Verify**

Run: `cd frontend/src-tauri && cargo check`

---

### Task 5: Wire `asr_engine` into `lib.rs` and transcription layer

**Files:**
- Modify: `frontend/src-tauri/src/lib.rs`
- Modify: `frontend/src-tauri/src/audio/transcription/engine.rs`
- Rename: `zipformer_provider.rs` → `asr_provider.rs`
- Modify: `frontend/src-tauri/src/audio/transcription/mod.rs`
- Modify: `frontend/src-tauri/src/audio/import.rs`
- Modify: `frontend/src-tauri/src/audio/retranscription.rs`
- Modify: `frontend/src-tauri/src/audio/recording_commands.rs`
- Modify: `frontend/src-tauri/src/onboarding.rs`
- Modify: `frontend/src-tauri/src/tray.rs`
- Delete: `frontend/src-tauri/src/zipformer_engine/` (entire directory)
- Modify: `frontend/src-tauri/src/lib.rs` — remove `pub mod zipformer_engine;`

- [ ] **Step 1: Register `asr_engine` commands in `lib.rs`**

Replace all `zipformer_engine::commands::zipformer_*` with `asr_engine::commands::asr_*`:

```rust
asr_engine::commands::init_on_startup(&_app.handle());
// ...
asr_engine::commands::asr_init,
asr_engine::commands::asr_get_model_status,
asr_engine::commands::asr_is_model_loaded,
asr_engine::commands::asr_get_models_directory,
asr_engine::commands::asr_download_model,
asr_engine::commands::asr_load_model,
asr_engine::commands::asr_transcribe_audio,
asr_engine::commands::asr_validate_model_ready,
asr_engine::commands::asr_get_variant_status,
asr_engine::commands::asr_get_current_config,
```

- [ ] **Step 2: Create `asr_provider.rs`**

```rust
use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use std::sync::Arc;

pub struct AsrProvider {
    engine: Arc<crate::asr_engine::engine::AsrEngine>,
}

impl AsrProvider {
    pub fn new(engine: Arc<crate::asr_engine::engine::AsrEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl TranscriptionProvider for AsrProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        _language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if !self.engine.is_model_loaded().await {
            return Err(TranscriptionError::ModelNotLoaded);
        }
        let text = self
            .engine
            .transcribe_audio(audio)
            .await
            .map_err(|e| TranscriptionError::EngineFailed(e.to_string()))?;
        Ok(TranscriptResult {
            text,
            confidence: None,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        self.engine.is_model_loaded().await
    }

    async fn get_current_model(&self) -> Option<String> {
        self.engine.get_current_model().await
    }

    fn provider_name(&self) -> &'static str {
        "asr-vi"
    }
}
```

- [ ] **Step 3: Update `transcription/engine.rs`**

Replace all `zipformer_engine` references with `asr_engine`. In `get_or_init_transcription_engine`, load from saved DB config when model not loaded:

```rust
match crate::asr_engine::commands::asr_validate_model_ready(app.clone(), None, None, None, None).await {
```

(Add `family: Option<String>` as first optional param to `asr_validate_model_ready`.)

- [ ] **Step 4: Update `import.rs` and `retranscription.rs`**

Replace:
```rust
crate::zipformer_engine::commands::zipformer_init()
```
with:
```rust
crate::asr_engine::commands::asr_init()
```

And `get_engine_arc()` from `asr_engine::commands`.

When auto-loading model, read family from DB or use `asr_validate_model_ready`.

- [ ] **Step 5: Delete `zipformer_engine/` and remove `pub mod zipformer_engine`**

- [ ] **Step 6: Grep cleanup**

Run: `rg "zipformer_engine|zipformer_" frontend/src-tauri/src --glob "!target"`
Expected: zero matches (except comments/docs if any).

Run: `cd frontend/src-tauri && cargo check`
Expected: PASS.

Run: `cd frontend/src-tauri && cargo test asr_engine -- --nocapture`
Expected: all PASS.

---

### Task 6: Frontend — `lib/asr.ts` + `AsrModelManager`

**Files:**
- Create: `frontend/src/lib/asr.ts`
- Create: `frontend/src/components/AsrModelManager.tsx`
- Modify: `frontend/src/components/TranscriptSettings.tsx`
- Modify: `frontend/src/constants/modelDefaults.ts`
- Delete: `frontend/src/lib/zipformer.ts`, `frontend/src/components/ZipFormerModelManager.tsx`

- [ ] **Step 1: Create `lib/asr.ts`**

```typescript
import { invoke } from '@tauri-apps/api/core';

export type ModelStatus =
  | { type: 'NotLoaded' }
  | { type: 'Downloading'; value: number }
  | { type: 'Ready' }
  | { type: 'Error'; value: string };

export type AsrModelFamily = 'zipformer-vi-30m' | 'gipformer-65m-rnnt';
export type ModelVariant = 'int8' | 'full';
export type DecodingMethod = 'greedy_search' | 'modified_beam_search';

export interface AsrModelInfo {
  id: AsrModelFamily;
  label: string;
  hfRepo: string;
  int8Size: string;
  fullSize: string;
  description: string;
}

export const ASR_MODELS: AsrModelInfo[] = [
  {
    id: 'zipformer-vi-30m',
    label: 'ZipFormer 30M',
    hfRepo: 'hynt/Zipformer-30M-RNNT-6000h',
    int8Size: '~32 MB',
    fullSize: '~100 MB',
    description: 'Nhỏ gọn, tốc độ cao — mặc định',
  },
  {
    id: 'gipformer-65m-rnnt',
    label: 'Gipformer 65M',
    hfRepo: 'g-group-ai-lab/gipformer-65M-rnnt',
    int8Size: '~75 MB',
    fullSize: '~335 MB',
    description: 'Chính xác hơn, cần máy mạnh hơn',
  },
];

export interface VariantStatus {
  hasFiles: boolean;
  isLoaded: boolean;
}

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

- [ ] **Step 2: Create `AsrModelManager.tsx`**

Based on `ZipFormerModelManager.tsx`, add:

1. State `selectedFamily: AsrModelFamily` (default `'zipformer-vi-30m'`)
2. First dropdown — **Model ASR** listing `ASR_MODELS`
3. Second dropdown — **Biến thể** (`int8` / `full`) with size label from selected family
4. `refreshAllVariantStatuses` calls `AsrAPI.getVariantStatus(selectedFamily, variant)` for both variants
5. `handleDownload` → `AsrAPI.downloadModel(selectedFamily, selectedVariant)`
6. `handleSave` → `api_save_transcript_config` with:
   ```typescript
   provider: 'asr',
   model: selectedFamily,
   asrVariant: selectedVariant,
   decodingMethod,
   numActivePaths,
   ```
7. Listen events: `asr-model-download-progress`, `asr-model-download-complete`, `asr-model-download-error`
8. On family change → refresh statuses (do NOT unload until Save)

- [ ] **Step 3: Update `TranscriptSettings.tsx`**

```typescript
import AsrModelManager from './AsrModelManager';

export interface TranscriptModelProps {
  provider: 'asr';
  model: string;
  apiKey?: string | null;
}
// ...
<AsrModelManager />
```

- [ ] **Step 4: Update `modelDefaults.ts`**

```typescript
export const GIPFORMER_MODEL_ID = 'gipformer-65m-rnnt';

export function createDefaultTranscriptModelConfig() {
  return {
    provider: 'asr' as const,
    model: ZIPFORMER_MODEL_ID,
    apiKey: null as string | null,
  };
}
```

- [ ] **Step 5: Delete old files**

Remove `zipformer.ts` and `ZipFormerModelManager.tsx`.

---

### Task 7: Frontend call-site sweep

**Files:**
- Modify: `frontend/src/hooks/useRecordingStart.ts`
- Modify: `frontend/src/hooks/useTranscriptionModels.ts`
- Modify: `frontend/src/contexts/ConfigContext.tsx`
- Modify: `frontend/src/components/Sidebar/index.tsx`
- Modify: `frontend/src/components/MeetingDetails/RetranscribeDialog.tsx`
- Modify: `frontend/src/components/shared/DownloadProgressToast.tsx`
- Modify: `frontend/src/services/transcriptService.ts`
- Modify: `frontend/src/components/LanguageSelection.tsx`
- Modify: `frontend/src/hooks/useModalState.ts`

- [ ] **Step 1: Replace all `zipformer_*` invoke calls with `asr_*`**

In `useRecordingStart.ts`:
```typescript
await invoke('asr_init');
return await invoke<boolean>('asr_is_model_loaded');
```

- [ ] **Step 2: Update `useTranscriptionModels.ts`**

```typescript
export interface ModelOption {
  provider: 'asr';
  name: string;
  displayName: string;
  size_mb: number;
}
```

Add Gipformer to available models list when loaded.

- [ ] **Step 3: Update `RetranscribeDialog.tsx` text**

Change "ZipFormer chỉ hỗ trợ tiếng Việt" → "Mô hình ASR chỉ hỗ trợ tiếng Việt".

- [ ] **Step 4: Grep frontend cleanup**

Run: `rg "zipformer" frontend/src --glob "*.{ts,tsx}"`
Expected: zero matches except possibly comments in docs paths.

- [ ] **Step 5: TypeScript check**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: no errors.

---

### Task 8: Gipformer smoke test + manual E2E

**Files:** none (verification only)

- [ ] **Step 1: Build app**

Run: `cd frontend/src-tauri && cargo build`
Expected: PASS.

- [ ] **Step 2: Smoke test Gipformer int8 load**

1. Start app (`pnpm run tauri:dev:cpu` or `cargo run`)
2. Settings → Nhận dạng → chọn **Gipformer 65M** → **int8** → Tải xuống
3. Wait for download complete (~75 MB)
4. Bấm **Lưu cấu hình**
5. Check terminal log: `ASR model loaded` with `gipformer-65m-rnnt`

- [ ] **Step 3: Transcribe test**

Import a short Vietnamese audio file OR record 10 seconds.
Expected: transcript text appears (raw uppercase from RNNT is OK — CAPU adds punctuation).

- [ ] **Step 4: CAPU after Gipformer**

Verify transcript has punctuation/capitalization after processing.
Expected: CAPU runs (log: no CAPU errors).

- [ ] **Step 5: Switch back to ZipFormer**

Settings → ZipFormer 30M → int8 → Lưu.
Expected: existing `models/zipformer-vi-int8/` still works without re-download.

- [ ] **Step 6: Upgrade path test**

If possible, start from DB with old `provider='zipformer'` and `zipformerVariant` column — run app, confirm migration applies and ZipFormer still works.

---

## Spec coverage checklist

| Spec requirement | Task |
|---|---|
| Two model families | Task 2, 3 |
| Default ZipFormer 30M int8 | Task 6 (`modelDefaults.ts`) |
| Unified Settings UI | Task 6 (`AsrModelManager`) |
| Flat storage dirs | Task 2 (`variant_subdir`) |
| `asr_*` commands, no alias | Task 3, 5 |
| DB migration `asrVariant` | Task 4 |
| Gipformer `tokens.txt` + fallback | Task 3 (`load_model`) |
| CAPU unchanged | No task (verify Task 8) |
| Only one model in RAM | Task 3 (`unload` before load) |
| Block model change while recording | Task 6 (disable UI when `isRecording`) |

---

## Execution handoff

Plan saved. Two execution options:

**1. Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks

**2. Inline Execution** — implement tasks in this session with checkpoints

Which approach?
