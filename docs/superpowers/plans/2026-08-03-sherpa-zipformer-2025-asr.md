# Sherpa-ONNX Zipformer VI (2025) — Third ASR Family Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking. Execute tasks in order; each task ends with a commit. Do not skip the "run and verify" steps — they are the only evidence a step worked.

**Goal:** Add `csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20` as a third selectable Vietnamese ASR model family in the existing `asr_engine`, alongside ZipFormer 30M and Gipformer 65M. This family has **no int8 variant** — only full precision — which is the one real deviation from the existing two-family pattern.

**Architecture:** Extend the existing `ModelFamily` enum (in `frontend/src-tauri/src/asr_engine/model_family.rs`) with a third variant. Add `ModelFamily::available_variants()` as the single source of truth for which `ModelVariant`s a family supports, and use it both to gate the frontend variant dropdown and to reject invalid family+variant combinations at the Rust engine layer (`download_model`/`load_model`) before they can reach the exhaustive `match (family, variant)` blocks. No new Tauri commands, no new DB migration — both already store family as a free-form string.

**Tech Stack:** Rust (`asr_engine` module, `anyhow`), TypeScript/React (`lib/asr.ts`, `AsrModelManager.tsx`).

**Reference spec:** `docs/superpowers/specs/2026-08-03-sherpa-zipformer-2025-asr-design.md`

---

## File map (before you start)

| File | Change |
|---|---|
| `frontend/src-tauri/src/config.rs` | Add `SHERPA_VI_2025_*` constants |
| `frontend/src-tauri/src/asr_engine/model_family.rs` | Add `SherpaZipformerVi2025` variant + `available_variants()` |
| `frontend/src-tauri/src/asr_engine/engine.rs` | Reject unsupported family+variant in `download_model`/`load_model` |
| `frontend/src/constants/modelDefaults.ts` | Add `SHERPA_VI_2025_MODEL_ID` constant |
| `frontend/src/lib/asr.ts` | Add family to `AsrModelFamily`, `ASR_MODELS`, `AsrModelInfo.availableVariants` |
| `frontend/src/components/AsrModelManager.tsx` | Gate variant dropdown by `availableVariants` |
| `frontend/src/hooks/useTranscriptionModels.ts` | Add third entry to `ASR_MODEL_OPTIONS` |

No files are deleted. No database migration is needed — `transcript_settings.model` is already a free-form string column (generalized by the Gipformer migration `20260803000000_add_asr_family.sql`).

---

### Task 1: Add config constants for the new model

**Files:**
- Modify: `frontend/src-tauri/src/config.rs`

- [ ] **Step 1: Append the constant block**

Add after the existing Gipformer block (after `pub const GIPFORMER_VOCAB_FALLBACK: &str = "config.json";`):

```rust
/// Application configuration constants — Sherpa-ONNX Zipformer VI 2025 ASR (full precision only)

pub const SHERPA_VI_2025_MODEL_NAME: &str = "sherpa-onnx-zipformer-vi-2025-04-20";

pub const SHERPA_VI_2025_HF_URL: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20/resolve/main";
pub const SHERPA_VI_2025_SUBDIR: &str = "sherpa-vi-2025-full";
pub const SHERPA_VI_2025_ENCODER: &str = "encoder-epoch-12-avg-8.onnx";
pub const SHERPA_VI_2025_DECODER: &str = "decoder-epoch-12-avg-8.onnx";
pub const SHERPA_VI_2025_JOINER: &str = "joiner-epoch-12-avg-8.onnx";
pub const SHERPA_VI_2025_SIZE_BYTES: u64 = 261_000_000;

pub const SHERPA_VI_2025_BPE: &str = "bpe.model";
pub const SHERPA_VI_2025_TOKENS: &str = "tokens.txt";
```

- [ ] **Step 2: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors (new constants are unused so far — that's fine, `pub const` doesn't trigger dead-code warnings).

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/config.rs
git commit -m "feat(asr): add config constants for Sherpa-ONNX Zipformer VI 2025"
```

---

### Task 2: Extend `ModelFamily` with the third variant and `available_variants()`

**Files:**
- Modify: `frontend/src-tauri/src/asr_engine/model_family.rs`

- [ ] **Step 1: Write the failing tests**

Add these three tests inside the existing `#[cfg(test)] mod tests { ... }` block, after `from_id_roundtrip`:

```rust
    #[test]
    fn sherpa_vi_2025_full_files_and_subdir() {
        let files = ModelFamily::SherpaZipformerVi2025.model_files(ModelVariant::Full);
        assert_eq!(files[0], "encoder-epoch-12-avg-8.onnx");
        assert_eq!(files[1], "decoder-epoch-12-avg-8.onnx");
        assert_eq!(files[2], "joiner-epoch-12-avg-8.onnx");
        assert_eq!(files[3], "bpe.model");
        assert_eq!(files[4], "tokens.txt");
        assert_eq!(
            ModelFamily::SherpaZipformerVi2025.variant_subdir(ModelVariant::Full),
            "sherpa-vi-2025-full"
        );
    }

    #[test]
    fn sherpa_vi_2025_has_full_variant_only() {
        assert_eq!(
            ModelFamily::SherpaZipformerVi2025.available_variants(),
            &[ModelVariant::Full]
        );
        assert_eq!(
            ModelFamily::ZipFormer30M.available_variants(),
            &[ModelVariant::Int8, ModelVariant::Full]
        );
        assert_eq!(
            ModelFamily::Gipformer65M.available_variants(),
            &[ModelVariant::Int8, ModelVariant::Full]
        );
    }

    #[test]
    fn from_id_includes_sherpa_vi_2025() {
        assert_eq!(
            ModelFamily::from_id("sherpa-onnx-zipformer-vi-2025-04-20"),
            ModelFamily::SherpaZipformerVi2025
        );
        assert_eq!(ModelFamily::SherpaZipformerVi2025.id(), "sherpa-onnx-zipformer-vi-2025-04-20");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend/src-tauri && cargo test asr_engine::model_family -- --nocapture`
Expected: **compile error** — `no variant or associated item named 'SherpaZipformerVi2025' found for enum 'ModelFamily'` and `no method named 'available_variants' found`. This is the expected red state; the enum variant doesn't exist yet.

- [ ] **Step 3: Replace the whole file with the implementation**

Replace the full contents of `frontend/src-tauri/src/asr_engine/model_family.rs` with:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelFamily {
    ZipFormer30M,
    Gipformer65M,
    SherpaZipformerVi2025,
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
            crate::config::SHERPA_VI_2025_MODEL_NAME => ModelFamily::SherpaZipformerVi2025,
            _ => ModelFamily::ZipFormer30M,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => crate::config::ZIPFORMER_MODEL_NAME,
            ModelFamily::Gipformer65M => crate::config::GIPFORMER_MODEL_NAME,
            ModelFamily::SherpaZipformerVi2025 => crate::config::SHERPA_VI_2025_MODEL_NAME,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => "ZipFormer 30M",
            ModelFamily::Gipformer65M => "Gipformer 65M",
            ModelFamily::SherpaZipformerVi2025 => "Sherpa-ONNX Zipformer VI (2025)",
        }
    }

    /// Which `ModelVariant`s this family actually ships. `SherpaZipformerVi2025` has no
    /// int8 build upstream — callers (UI, engine) must check this before using `Int8`
    /// with that family; the per-variant lookup functions below panic on that combination.
    pub fn available_variants(self) -> &'static [ModelVariant] {
        match self {
            ModelFamily::ZipFormer30M => &[ModelVariant::Int8, ModelVariant::Full],
            ModelFamily::Gipformer65M => &[ModelVariant::Int8, ModelVariant::Full],
            ModelFamily::SherpaZipformerVi2025 => &[ModelVariant::Full],
        }
    }

    pub fn variant_subdir(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_SUBDIR,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_SUBDIR,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_SUBDIR,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_SUBDIR,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_SUBDIR,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn hf_url(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_HF_URL,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_HF_URL,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_HF_URL,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_HF_URL,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_HF_URL,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn encoder_file(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_ENCODER,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_ENCODER,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_ENCODER,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_ENCODER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_ENCODER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn decoder_file(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_DECODER,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_DECODER,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_DECODER,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_DECODER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_DECODER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn joiner_file(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_JOINER,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_JOINER,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_JOINER,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_JOINER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_JOINER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn bpe_file(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => crate::config::ZIPFORMER_BPE,
            ModelFamily::Gipformer65M => crate::config::GIPFORMER_BPE,
            ModelFamily::SherpaZipformerVi2025 => crate::config::SHERPA_VI_2025_BPE,
        }
    }

    pub fn token_file(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => crate::config::ZIPFORMER_VOCAB,
            ModelFamily::Gipformer65M => crate::config::GIPFORMER_TOKENS,
            ModelFamily::SherpaZipformerVi2025 => crate::config::SHERPA_VI_2025_TOKENS,
        }
    }

    pub fn encoder_size_bytes(self, variant: ModelVariant) -> u64 {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_SIZE_BYTES,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_SIZE_BYTES,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_SIZE_BYTES,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_SIZE_BYTES,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_SIZE_BYTES,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
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
    fn zipformer30m_int8_files_match_existing_layout() {
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

    #[test]
    fn sherpa_vi_2025_full_files_and_subdir() {
        let files = ModelFamily::SherpaZipformerVi2025.model_files(ModelVariant::Full);
        assert_eq!(files[0], "encoder-epoch-12-avg-8.onnx");
        assert_eq!(files[1], "decoder-epoch-12-avg-8.onnx");
        assert_eq!(files[2], "joiner-epoch-12-avg-8.onnx");
        assert_eq!(files[3], "bpe.model");
        assert_eq!(files[4], "tokens.txt");
        assert_eq!(
            ModelFamily::SherpaZipformerVi2025.variant_subdir(ModelVariant::Full),
            "sherpa-vi-2025-full"
        );
    }

    #[test]
    fn sherpa_vi_2025_has_full_variant_only() {
        assert_eq!(
            ModelFamily::SherpaZipformerVi2025.available_variants(),
            &[ModelVariant::Full]
        );
        assert_eq!(
            ModelFamily::ZipFormer30M.available_variants(),
            &[ModelVariant::Int8, ModelVariant::Full]
        );
        assert_eq!(
            ModelFamily::Gipformer65M.available_variants(),
            &[ModelVariant::Int8, ModelVariant::Full]
        );
    }

    #[test]
    fn from_id_includes_sherpa_vi_2025() {
        assert_eq!(
            ModelFamily::from_id("sherpa-onnx-zipformer-vi-2025-04-20"),
            ModelFamily::SherpaZipformerVi2025
        );
        assert_eq!(ModelFamily::SherpaZipformerVi2025.id(), "sherpa-onnx-zipformer-vi-2025-04-20");
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend/src-tauri && cargo test asr_engine::model_family -- --nocapture`
Expected: 6 tests PASS (3 existing + 3 new).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/asr_engine/model_family.rs
git commit -m "feat(asr): add SherpaZipformerVi2025 family with available_variants()"
```

---

### Task 3: Reject unsupported family+variant combinations in the engine

**Why:** `model_files()`/`encoder_file()`/etc. now `unreachable!()`-panic if called with `(SherpaZipformerVi2025, Int8)`. That combination must never reach them. The only two entry points that take a caller-supplied `(family, variant)` pair are `AsrEngine::download_model` and `AsrEngine::load_model` — both already return `Result<()>`, so this is a normal early-return, not a new failure mode.

**Files:**
- Modify: `frontend/src-tauri/src/asr_engine/engine.rs`

- [ ] **Step 1: Write the failing test**

Add this test inside the existing `#[cfg(test)] mod tests { ... }` block in `engine.rs`, after `test_unload_model_clears_recognizer_and_status`:

```rust
    #[tokio::test]
    async fn test_load_model_rejects_unsupported_variant() {
        let engine = AsrEngine::new();
        let result = engine
            .load_model(
                ModelFamily::SherpaZipformerVi2025,
                ModelVariant::Int8,
                "modified_beam_search".to_string(),
                15,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not support variant"));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend/src-tauri && cargo test asr_engine::engine::tests::test_load_model_rejects_unsupported_variant -- --nocapture`
Expected: **FAIL** (or panic) — today `load_model` proceeds past the missing-files check and would either return the wrong "Missing model files" error or, worse, panic once it reaches a `family.token_file()`-style call that doesn't hit this specific combination. The important thing is it does NOT fail with the `"does not support variant"` message, because that check doesn't exist yet.

- [ ] **Step 3: Add the guard to `download_model`**

In `frontend/src-tauri/src/asr_engine/engine.rs`, find:

```rust
    pub async fn download_model(
        &self,
        family: ModelFamily,
        variant: ModelVariant,
        progress_callback: Option<Box<dyn Fn(u8) + Send>>,
    ) -> Result<()> {
        let base = self.models_base_dir.read().await.clone();
```

Replace with:

```rust
    pub async fn download_model(
        &self,
        family: ModelFamily,
        variant: ModelVariant,
        progress_callback: Option<Box<dyn Fn(u8) + Send>>,
    ) -> Result<()> {
        if !family.available_variants().contains(&variant) {
            return Err(anyhow!(
                "{} does not support variant '{}' (available: {:?})",
                family.id(),
                variant.as_str(),
                family.available_variants()
            ));
        }

        let base = self.models_base_dir.read().await.clone();
```

- [ ] **Step 4: Add the same guard to `load_model`**

Find:

```rust
    pub async fn load_model(
        &self,
        family: ModelFamily,
        variant: ModelVariant,
        decoding_method: String,
        num_active_paths: i32,
    ) -> Result<()> {
        if self.is_model_loaded().await {
```

Replace with:

```rust
    pub async fn load_model(
        &self,
        family: ModelFamily,
        variant: ModelVariant,
        decoding_method: String,
        num_active_paths: i32,
    ) -> Result<()> {
        if !family.available_variants().contains(&variant) {
            return Err(anyhow!(
                "{} does not support variant '{}' (available: {:?})",
                family.id(),
                variant.as_str(),
                family.available_variants()
            ));
        }

        if self.is_model_loaded().await {
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd frontend/src-tauri && cargo test asr_engine::engine::tests::test_load_model_rejects_unsupported_variant -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Run the full asr_engine test suite**

Run: `cd frontend/src-tauri && cargo test asr_engine -- --nocapture`
Expected: all PASS (model_family's 6 tests + engine's 2 tests).

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/asr_engine/engine.rs
git commit -m "fix(asr): reject unsupported family+variant combos before they can panic"
```

---

### Task 4: Frontend constant

**Files:**
- Modify: `frontend/src/constants/modelDefaults.ts`

- [ ] **Step 1: Add the model id constant**

Find:

```typescript
/** Internal model id for Gipformer 65M (matches Rust GIPFORMER_MODEL_NAME). */
export const GIPFORMER_MODEL_ID = 'gipformer-65m-rnnt';
```

Replace with:

```typescript
/** Internal model id for Gipformer 65M (matches Rust GIPFORMER_MODEL_NAME). */
export const GIPFORMER_MODEL_ID = 'gipformer-65m-rnnt';

/** Internal model id for Sherpa-ONNX Zipformer VI 2025 (matches Rust SHERPA_VI_2025_MODEL_NAME). */
export const SHERPA_VI_2025_MODEL_ID = 'sherpa-onnx-zipformer-vi-2025-04-20';
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/constants/modelDefaults.ts
git commit -m "feat(asr): add SHERPA_VI_2025_MODEL_ID constant"
```

---

### Task 5: `lib/asr.ts` — register the family and its variant constraint

**Files:**
- Modify: `frontend/src/lib/asr.ts`

- [ ] **Step 1: Replace the whole file**

```typescript
import { invoke } from '@tauri-apps/api/core';

export type ModelStatus =
  | { type: 'NotLoaded' }
  | { type: 'Downloading'; value: number }
  | { type: 'Ready' }
  | { type: 'Error'; value: string };

export type AsrModelFamily =
  | 'zipformer-vi-30m'
  | 'gipformer-65m-rnnt'
  | 'sherpa-onnx-zipformer-vi-2025-04-20';
export type ModelVariant = 'int8' | 'full';
export type DecodingMethod = 'greedy_search' | 'modified_beam_search';

export interface AsrModelInfo {
  id: AsrModelFamily;
  label: string;
  hfRepo: string;
  int8Size: string;
  fullSize: string;
  description: string;
  /** Which variants this family actually ships. Must match Rust `ModelFamily::available_variants()`. */
  availableVariants: ModelVariant[];
}

export const ASR_MODELS: AsrModelInfo[] = [
  {
    id: 'zipformer-vi-30m',
    label: 'ZipFormer 30M',
    hfRepo: 'hynt/Zipformer-30M-RNNT-6000h',
    int8Size: '~32 MB',
    fullSize: '~100 MB',
    description: 'Nhỏ gọn, tốc độ cao — mặc định',
    availableVariants: ['int8', 'full'],
  },
  {
    id: 'gipformer-65m-rnnt',
    label: 'Gipformer 65M',
    hfRepo: 'g-group-ai-lab/gipformer-65M-rnnt',
    int8Size: '~75 MB',
    fullSize: '~335 MB',
    description: 'Chính xác hơn, cần máy mạnh hơn',
    availableVariants: ['int8', 'full'],
  },
  {
    id: 'sherpa-onnx-zipformer-vi-2025-04-20',
    label: 'Sherpa-ONNX Zipformer VI (2025)',
    hfRepo: 'csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20',
    int8Size: 'Không có',
    fullSize: '~270 MB',
    description: 'Model cộng đồng, chỉ có bản full precision',
    availableVariants: ['full'],
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

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: no new errors (existing unrelated errors, if any, are out of scope — compare against a baseline run before this task if unsure).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/asr.ts
git commit -m "feat(asr): register Sherpa-ONNX Zipformer VI 2025 with full-only variant constraint"
```

---

### Task 6: `AsrModelManager.tsx` — gate the variant dropdown by family

**Why:** Today the "Biến thể" dropdown always renders both `int8` and `full`, unconditionally. For the new family, `int8` must not be selectable — the file doesn't exist upstream and would 404 or hit Task 3's new Rust-side rejection.

**Files:**
- Modify: `frontend/src/components/AsrModelManager.tsx`

- [ ] **Step 1: Add a computed list of available variant options**

Find:

```typescript
  const selectedModelInfo = ASR_MODELS.find((m) => m.id === selectedFamily);
```

Replace with:

```typescript
  const selectedModelInfo = ASR_MODELS.find((m) => m.id === selectedFamily);
  const availableVariantOptions = VARIANT_OPTIONS.filter((v) =>
    selectedModelInfo ? selectedModelInfo.availableVariants.includes(v.id) : true
  );
```

- [ ] **Step 2: Auto-correct the selected variant when it becomes invalid for the new family**

Find:

```typescript
  useEffect(() => {
    refreshAllVariantStatuses(selectedFamily);
  }, [selectedFamily, refreshAllVariantStatuses]);
```

Replace with:

```typescript
  useEffect(() => {
    refreshAllVariantStatuses(selectedFamily);
  }, [selectedFamily, refreshAllVariantStatuses]);

  useEffect(() => {
    if (!selectedModelInfo) return;
    if (!selectedModelInfo.availableVariants.includes(selectedVariant)) {
      setSelectedVariant(selectedModelInfo.availableVariants[0]);
    }
  }, [selectedFamily, selectedModelInfo, selectedVariant]);
```

- [ ] **Step 3: Update `loadSavedConfig` to recognize the third family id**

Find:

```typescript
        if (config.model === 'zipformer-vi-30m' || config.model === 'gipformer-65m-rnnt') {
          setSelectedFamily(config.model);
        }
```

Replace with:

```typescript
        if (
          config.model === 'zipformer-vi-30m' ||
          config.model === 'gipformer-65m-rnnt' ||
          config.model === 'sherpa-onnx-zipformer-vi-2025-04-20'
        ) {
          setSelectedFamily(config.model);
        }
```

- [ ] **Step 4: Render `availableVariantOptions` instead of the full static list, and disable when there's only one choice**

Find:

```typescript
        <select
          value={selectedVariant}
          onChange={(e) => setSelectedVariant(e.target.value as ModelVariant)}
          disabled={disabled}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        >
          {VARIANT_OPTIONS.map((v) => {
            const size = v.id === 'int8' ? selectedModelInfo?.int8Size : selectedModelInfo?.fullSize;
            return (
              <option key={v.id} value={v.id}>
                {v.label} ({size})
              </option>
            );
          })}
        </select>
```

Replace with:

```typescript
        <select
          value={selectedVariant}
          onChange={(e) => setSelectedVariant(e.target.value as ModelVariant)}
          disabled={disabled || availableVariantOptions.length <= 1}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        >
          {availableVariantOptions.map((v) => {
            const size = v.id === 'int8' ? selectedModelInfo?.int8Size : selectedModelInfo?.fullSize;
            return (
              <option key={v.id} value={v.id}>
                {v.label} ({size})
              </option>
            );
          })}
        </select>
```

- [ ] **Step 5: Manual check — variant dropdown locks when family has one variant**

Run: `cd frontend && pnpm exec tsc --noEmit` (no errors expected)

Then start the app (see Task 9 for the full manual pass) and confirm: selecting "Sherpa-ONNX Zipformer VI (2025)" locks the "Biến thể" dropdown to `full` and disables it; switching back to "ZipFormer 30M" or "Gipformer 65M" re-enables it with both options.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/AsrModelManager.tsx
git commit -m "feat(asr): gate variant dropdown by family's available variants"
```

---

### Task 7: `useTranscriptionModels.ts` — list the third model as an available option

**Files:**
- Modify: `frontend/src/hooks/useTranscriptionModels.ts`

- [ ] **Step 1: Add the import and the list entry**

Find:

```typescript
import { GIPFORMER_MODEL_ID, ZIPFORMER_MODEL_ID } from '@/constants/modelDefaults';
```

Replace with:

```typescript
import {
  GIPFORMER_MODEL_ID,
  SHERPA_VI_2025_MODEL_ID,
  ZIPFORMER_MODEL_ID,
} from '@/constants/modelDefaults';
```

Find:

```typescript
const ASR_MODEL_OPTIONS: ModelOption[] = [
  {
    provider: 'asr',
    name: ZIPFORMER_MODEL_ID,
    displayName: '🇻🇳 ZipFormer 30M Vietnamese ASR (~30 MB)',
    size_mb: 30,
  },
  {
    provider: 'asr',
    name: GIPFORMER_MODEL_ID,
    displayName: '🇻🇳 Gipformer 65M Vietnamese ASR (~65 MB)',
    size_mb: 65,
  },
];
```

Replace with:

```typescript
const ASR_MODEL_OPTIONS: ModelOption[] = [
  {
    provider: 'asr',
    name: ZIPFORMER_MODEL_ID,
    displayName: '🇻🇳 ZipFormer 30M Vietnamese ASR (~30 MB)',
    size_mb: 30,
  },
  {
    provider: 'asr',
    name: GIPFORMER_MODEL_ID,
    displayName: '🇻🇳 Gipformer 65M Vietnamese ASR (~65 MB)',
    size_mb: 65,
  },
  {
    provider: 'asr',
    name: SHERPA_VI_2025_MODEL_ID,
    displayName: '🇻🇳 Sherpa-ONNX Zipformer VI 2025 (~270 MB)',
    size_mb: 270,
  },
];
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: no new errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/useTranscriptionModels.ts
git commit -m "feat(asr): list Sherpa-ONNX Zipformer VI 2025 in transcription model options"
```

---

### Task 8: Repo-wide sweep and full build

**Files:** none (verification only)

- [ ] **Step 1: Grep for any other hardcoded two-family lists**

Run: `rg "zipformer-vi-30m.*gipformer-65m-rnnt|Gipformer65M.*=>" frontend/src frontend/src-tauri/src --glob "!target"`
Expected: only matches inside files already modified in Tasks 1–7. If a new match turns up elsewhere (e.g. a component that lists exactly two options by name), add the third option there too before continuing.

- [ ] **Step 2: Full Rust build**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors or new warnings.

- [ ] **Step 3: Full Rust test suite**

Run: `cd frontend/src-tauri && cargo test asr_engine -- --nocapture`
Expected: all 8 tests PASS (6 in `model_family`, 2 in `engine`).

- [ ] **Step 4: Full TypeScript check**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: no errors.

---

### Task 9: Manual smoke test (required before merge)

**Files:** none (verification only)

- [ ] **Step 1: Build and start the app**

Run: `cd frontend/src-tauri && cargo build`, then start with `pnpm run tauri:dev:cpu` (or your platform's dev command) from `frontend/`.

- [ ] **Step 2: Select the new model in Settings**

Settings → Nhận dạng giọng nói → **Model ASR** → chọn "Sherpa-ONNX Zipformer VI (2025)".
Expected: "Biến thể" dropdown shows only `full (precision)` and is disabled (can't change it).

- [ ] **Step 3: Download**

Click "Tải xuống". Expected: progress bar runs to 100% (~270 MB), no errors in the terminal log.

- [ ] **Step 4: Save and auto-load**

Click "Lưu cấu hình". Expected: terminal log shows `ASR model loaded` with family `sherpa-onnx-zipformer-vi-2025-04-20`.

- [ ] **Step 5: Transcribe**

Import a short Vietnamese audio file, or record ~10 seconds. Expected: transcript text appears.

- [ ] **Step 6: CAPU still runs after this family**

Expected: the transcript has capitalization/punctuation applied (CAPU pipeline is unmodified — this just confirms nothing about the new family breaks it).

- [ ] **Step 7: Switch across all three families**

Settings → switch ZipFormer 30M → Gipformer 65M → Sherpa-ONNX Zipformer VI (2025) → back to ZipFormer 30M. Confirm each switch loads without requiring re-download of already-downloaded families, and the variant dropdown correctly re-enables for the two int8-capable families.

- [ ] **Step 8: Retranscribe**

Retranscribe an existing meeting using the new model via the retranscription dialog. Expected: completes without error.

---

## Spec coverage checklist

| Spec requirement | Task |
|---|---|
| Third `ModelFamily` variant, correct HF repo/files | Task 1, 2 |
| Full-only variant (no int8) | Task 2 (`available_variants`), Task 6 (UI gating) |
| No dead "int8" option / no silent Full-data-for-Int8-request bugs | Task 2 (`unreachable!` per combination), Task 3 (reject before reaching them) |
| No new DB migration | N/A — verified in design; `model` column already free-form |
| CAPU / streaming / hotwords unchanged | No task touches those files |
| Manual E2E across all three families | Task 9 |

---

## Notes for whoever executes this (e.g. via Cursor)

- Every code block in this plan is the exact, complete replacement or exact match-and-replace text — there are no partial diffs or "similar to above" shortcuts. Apply them literally.
- Tasks are ordered so `cargo check`/`tsc --noEmit` stays green after each task's commit — safe to stop between any two tasks.
- If Task 2's exact-match replace fails because the working file has drifted from what's quoted above (e.g. someone touched `model_family.rs` in the meantime), re-read the current file first and reapply the same *logical* change (new enum variant, `available_variants()`, one match arm per function, two new test cases) rather than forcing the literal text match.
