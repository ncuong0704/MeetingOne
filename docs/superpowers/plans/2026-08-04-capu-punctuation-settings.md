# CAPU Settings (CPU threads, punctuation/case level) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 3 user-facing settings to Meetily's CAPU (Vietnamese punctuation + capitalization restoration) engine — Số luồng CPU (CPU thread count), Mức độ thêm dấu (punctuation level), Mức độ tự viết hoa (auto-capitalization level) — ported from a reference PyQt6 app that already validated these exact formulas/defaults with real users.

**Architecture:** `CapuEngine::infer_once` currently argmaxes raw ONNX logits with no way to tune behavior. This plan adds a softmax step and a per-level probability bias (ported verbatim from the reference app's `tab_file.py`/`gec_model.py`) applied to the `$KEEP` label (punctuation level) and to all `$TRANSFORM_CASE_*` labels (case level). CPU thread count is baked into the ONNX session at build time (`Session::builder().with_intra_threads(...)`), so changing it requires rebuilding the session — unlike the two level settings, which mutate the already-loaded engine in place. Settings persist in the existing `transcript_settings` table and apply through `api_save_transcript_config`, mirroring the exact pattern already used there for hotwords (push live to the running engine on save, no separate "apply" command).

**Tech Stack:** Rust (`ort` 2.0.0-rc.10, `sysinfo` 0.32 — both already dependencies, no new ones needed), sqlx/SQLite migration, TypeScript/React.

**Reference spec:** `docs/superpowers/specs/2026-08-04-capu-punctuation-settings-design.md`

**Verified before writing this plan (not assumed):**
- `ort::session::builder::SessionBuilder::with_intra_threads(self, usize) -> Result<Self>` exists in the vendored `ort 2.0.0-rc.10` source (`session/builder/impl_options.rs:51`).
- `sysinfo::System::physical_core_count(&self) -> Option<usize>` is an **instance** method (not associated/static) in the vendored `sysinfo 0.32.1` source (`common/system.rs:517`) — computed fresh on every call, no prior `refresh_*` strictly required for it, though `cpus()` (used for the logical-thread count) does need `refresh_cpu_all()` first.
- `sysinfo = "0.32"` and `ort = "2.0.0-rc.10"` are both already in `frontend/src-tauri/Cargo.toml` with no extra features needed (`sysinfo`'s default features include `system`, which is what `physical_core_count`/`cpus` need).
- Every Rust snippet below is a diff against the actual current content of `capu_engine/capu_engine.rs`, `capu_engine/commands.rs`, `capu_engine/mod.rs`, `audio/post_asr.rs`, `database/models.rs`, `database/repositories/setting.rs`, `api/api.rs`, `lib.rs`, `lib/asr.ts`, and `AsrModelManager.tsx` — all re-read fresh on the current `feat/vietnamese-capu-punctuation` branch, not reconstructed from the original CAPU port plan.
- The exact `Action` enum variant names (`TransformCaseCapital`, `TransformCaseUpper`, `TransformCaseLower`, `TransformCaseCapital1`, `TransformCaseUpperMinus1`) come from the real `capu_engine/tokenizer.rs`, not guessed.

---

## File map

| File | Change |
|---|---|
| `frontend/src-tauri/src/capu_engine/cpu_topology.rs` | New: `detect_cpu_topology()` — physical/logical core detection via `sysinfo` |
| `frontend/src-tauri/src/capu_engine/mod.rs` | Register `cpu_topology` submodule |
| `frontend/src-tauri/src/capu_engine/capu_engine.rs` | `CapuEngine` gets thread/level fields + getters/setters; `infer_once` gets softmax+bias decode; `load()` takes threads/levels |
| `frontend/src-tauri/src/audio/post_asr.rs` | Bypass CAPU entirely when punctuation level is 1 |
| `frontend/src-tauri/migrations/20260804200000_add_capu_settings.sql` | New: 3 columns on `transcript_settings` |
| `frontend/src-tauri/src/database/models.rs` | `TranscriptSetting` gets 3 new fields |
| `frontend/src-tauri/src/database/repositories/setting.rs` | `save_transcript_config` gets 3 new params/columns (`get_transcript_config` needs no change — it's a `SELECT *` populated via `FromRow`, so Task 4's new `TranscriptSetting` fields flow through automatically) |
| `frontend/src-tauri/src/capu_engine/commands.rs` | `capu_init` resolves settings from DB; new `apply_settings_after_save` (live-update + conditional rebuild); new `capu_get_cpu_topology` command |
| `frontend/src-tauri/src/lib.rs` | Register `capu_get_cpu_topology` |
| `frontend/src-tauri/src/api/api.rs` | `TranscriptConfig` gets 3 fields; save handler calls `apply_settings_after_save` |
| `frontend/src/lib/asr.ts` | New `CapuAPI.getCpuTopology` |
| `frontend/src/components/AsrModelManager.tsx` | 3 new sliders in Settings |

No changes to `asr_engine` (ASR's own hardcoded `num_threads = 2` stays untouched — out of scope, see spec) or `rover_engine`/`rnnt_decoder`.

---

### Task 1: CPU topology detection

**Files:**
- Create: `frontend/src-tauri/src/capu_engine/cpu_topology.rs`
- Modify: `frontend/src-tauri/src/capu_engine/mod.rs`

- [ ] **Step 1: Write the failing test**

Create `frontend/src-tauri/src/capu_engine/cpu_topology.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_topology_is_internally_consistent() {
        let (physical, logical) = detect_cpu_topology();
        assert!(physical >= 1, "physical core count should be at least 1");
        assert!(logical >= 1, "logical thread count should be at least 1");
        assert!(physical <= logical, "physical cores can't exceed logical threads");
    }
}
```

- [ ] **Step 2: Register the module**

In `frontend/src-tauri/src/capu_engine/mod.rs`, find:

```rust
pub mod vocabulary;
pub mod edits;
pub mod tokenizer;
pub mod capu_engine;
pub mod commands;

pub use capu_engine::CapuEngine;
```

Replace with:

```rust
pub mod vocabulary;
pub mod edits;
pub mod tokenizer;
pub mod cpu_topology;
pub mod capu_engine;
pub mod commands;

pub use capu_engine::CapuEngine;
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd frontend/src-tauri && cargo test capu_engine::cpu_topology -- --nocapture`
Expected: compile error — `detect_cpu_topology` doesn't exist yet.

- [ ] **Step 4: Implement**

Prepend this to `frontend/src-tauri/src/capu_engine/cpu_topology.rs`, above the `#[cfg(test)]` block:

```rust
use sysinfo::System;

/// Returns `(physical_cores, logical_threads)`. Falls back to `logical / 2` (min 1) if the
/// OS doesn't report a physical core count — mirrors the reference app's own fallback
/// (`core/config.py:_detect_cpu_topology`). Unlike that reference implementation, no manual
/// VM-detection heuristic is needed here: `sysinfo` queries real OS topology, and a vCPU's
/// physical/logical counts already come back equal at that layer.
pub fn detect_cpu_topology() -> (usize, usize) {
    let mut sys = System::new();
    sys.refresh_cpu_all();
    let logical = sys.cpus().len().max(1);
    let physical = sys.physical_core_count().unwrap_or((logical / 2).max(1));
    (physical, logical)
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd frontend/src-tauri && cargo test capu_engine::cpu_topology -- --nocapture`
Expected: 1 test PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/capu_engine/cpu_topology.rs frontend/src-tauri/src/capu_engine/mod.rs
git commit -m "feat(capu): add CPU topology detection via sysinfo"
```

---

### Task 2: `CapuEngine` — softmax + confidence-bias decoding, thread-aware load

**Files:**
- Modify: `frontend/src-tauri/src/capu_engine/capu_engine.rs`

- [ ] **Step 1: Write the failing tests**

Add this at the very end of `frontend/src-tauri/src/capu_engine/capu_engine.rs` (after the existing `#[cfg(test)] mod integration_tests { ... }` block, so as a new sibling module):

```rust

#[cfg(test)]
mod bias_tests {
    use super::*;

    #[test]
    fn punctuation_confidence_matches_reference_formula_at_key_levels() {
        // Ported verbatim from the reference app's tab_file.py get_config():
        // confidence = 0.5 - (slider_val - 1) * (1.3 / 9)
        assert!((punctuation_confidence(1) - 0.5).abs() < 1e-6);
        assert!((punctuation_confidence(7) - (-0.366667)).abs() < 1e-4);
        assert!((punctuation_confidence(10) - (-0.8)).abs() < 1e-6);
    }

    #[test]
    fn case_confidence_matches_reference_formula_at_key_levels() {
        // case_confidence = -1.5 + (case_val - 1) * (2.0 / 9)
        assert!((case_confidence(1) - (-1.5)).abs() < 1e-6);
        assert!((case_confidence(3) - (-1.055556)).abs() < 1e-4);
        assert!((case_confidence(10) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn decode_row_min_punctuation_level_pulls_a_close_call_toward_keep() {
        // Without bias, softmax([0.5, 0.6, 0.0]) narrowly favors index 1 over KEEP (index 0).
        let logits = vec![0.5, 0.6, 0.0];
        let idx = decode_row(&logits, 0, &[2], 1, 3);
        assert_eq!(idx, 0, "level=1 (+0.5 to KEEP) should flip this close call toward KEEP");
    }

    #[test]
    fn decode_row_max_punctuation_level_pulls_a_close_call_away_from_keep() {
        // Without bias, softmax([0.6, 0.5, 0.0]) narrowly favors KEEP (index 0).
        let logits = vec![0.6, 0.5, 0.0];
        let idx = decode_row(&logits, 0, &[2], 10, 3);
        assert_eq!(idx, 1, "level=10 (-0.8 to KEEP) should flip this close call away from KEEP");
    }

    #[test]
    fn decode_row_max_case_level_favors_a_trailing_case_action() {
        // KEEP (index 0) narrowly ahead of a case action (index 2); punctuation_level=4 gives
        // KEEP only a small +0.0667 boost, isolating the case-bias effect.
        let logits = vec![0.6, 0.0, 0.5];
        let idx = decode_row(&logits, 0, &[2], 4, 10);
        assert_eq!(idx, 2, "level=10 case bias (+0.5) should flip this close call toward the case action");
    }

    #[test]
    fn decode_row_min_case_level_suppresses_a_trailing_case_action() {
        let logits = vec![0.6, 0.0, 0.5];
        let idx = decode_row(&logits, 0, &[2], 4, 1);
        assert_eq!(idx, 0, "level=1 case bias (-1.5) should keep KEEP winning");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend/src-tauri && cargo test capu_engine::capu_engine::bias_tests -- --nocapture`
Expected: compile error — `punctuation_confidence`, `case_confidence`, `decode_row` don't exist yet.

- [ ] **Step 3: Implement the pure bias/decode functions**

In `frontend/src-tauri/src/capu_engine/capu_engine.rs`, find:

```rust
use super::edits::apply_actions;
use super::tokenizer::CapuTokenizer;
use super::vocabulary::{load_action_labels, Action};
use crate::config::{CAPU_MAX_ITERATIONS, CAPU_MAX_SEQ_LEN, CAPU_TRAILING_CONTEXT_WORDS};
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;
```

Replace with:

```rust
use super::edits::apply_actions;
use super::tokenizer::CapuTokenizer;
use super::vocabulary::{load_action_labels, Action};
use crate::config::{CAPU_MAX_ITERATIONS, CAPU_MAX_SEQ_LEN, CAPU_TRAILING_CONTEXT_WORDS};
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

/// Converts a punctuation-level UI slider value (1..10, default 7) into the probability
/// bias added to the `$KEEP` (no-op) label before argmax. Formula ported verbatim from the
/// reference app's `tab_file.py:get_config()`. Higher level -> more negative bias -> $KEEP
/// suppressed -> the model adds punctuation more aggressively. Level 1 is handled by the
/// caller as a full bypass (see `post_asr.rs`), not by this bias alone.
fn punctuation_confidence(level: u8) -> f32 {
    let level = level.clamp(1, 10) as f32;
    0.5 - (level - 1.0) * (1.3 / 9.0)
}

/// Same idea as `punctuation_confidence`, but for every `$TRANSFORM_CASE_*` label
/// (case-level slider, 1..10, default 3). Higher level -> more positive bias -> case
/// transforms fire more readily.
fn case_confidence(level: u8) -> f32 {
    let level = level.clamp(1, 10) as f32;
    -1.5 + (level - 1.0) * (2.0 / 9.0)
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.into_iter().map(|x| x / sum).collect()
}

/// Picks the winning label index for one word's logits row: softmax -> add
/// `punctuation_confidence(punctuation_level)` to `keep_index`'s probability -> add
/// `case_confidence(case_level)` to every index in `case_label_indices` -> argmax. A pure
/// function (no ONNX involved), so it's unit-testable with fixture logits — see `bias_tests`.
fn decode_row(
    row_logits: &[f32],
    keep_index: usize,
    case_label_indices: &[usize],
    punctuation_level: u8,
    case_level: u8,
) -> usize {
    let mut probs = softmax(row_logits);
    probs[keep_index] += punctuation_confidence(punctuation_level);
    for &idx in case_label_indices {
        probs[idx] += case_confidence(case_level);
    }
    probs
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(idx, _)| idx)
        .unwrap()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend/src-tauri && cargo test capu_engine::capu_engine::bias_tests -- --nocapture`
Expected: 5 tests PASS.

- [ ] **Step 5: Add engine fields for threads/levels, computed once at load**

Find:

```rust
pub struct CapuEngine {
    session: Session,
    tokenizer: CapuTokenizer,
    labels: Vec<Action>,
}
```

Replace with:

```rust
pub struct CapuEngine {
    session: Session,
    tokenizer: CapuTokenizer,
    labels: Vec<Action>,
    /// Index of `Action::Keep` in `labels` — found once at `load()` time.
    keep_index: usize,
    /// Indices of every `Action::TransformCase*` variant in `labels` — found once at
    /// `load()` time.
    case_label_indices: Vec<usize>,
    /// Number of ONNX intra-op threads the current `session` was built with. Compared
    /// against the caller's requested thread count to decide whether a rebuild is needed
    /// (see `capu_engine::commands::apply_settings_after_save`).
    threads: usize,
    punctuation_level: u8,
    case_level: u8,
}
```

- [ ] **Step 6: Thread `threads`/`punctuation_level`/`case_level` through `load()`**

Find:

```rust
impl CapuEngine {
    pub fn load(model_path: &Path, vocab_path: &Path, labels_path: &Path) -> Result<Self> {
        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| anyhow!("Non-UTF8 model path: {:?}", model_path))?;
        let session = Session::builder()
            .map_err(|e| anyhow!("Failed to create ONNX session builder: {}", e))?
            .commit_from_file(model_path_str)
            .map_err(|e| anyhow!("Failed to load CAPU model {:?}: {}", model_path, e))?;

        let tokenizer = CapuTokenizer::from_vocab_file(vocab_path)?;
        let labels = load_action_labels(labels_path)?;

        Ok(Self {
            session,
            tokenizer,
            labels,
        })
    }
```

Replace with:

```rust
impl CapuEngine {
    pub fn load(
        model_path: &Path,
        vocab_path: &Path,
        labels_path: &Path,
        threads: usize,
        punctuation_level: u8,
        case_level: u8,
    ) -> Result<Self> {
        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| anyhow!("Non-UTF8 model path: {:?}", model_path))?;
        let session = Session::builder()
            .map_err(|e| anyhow!("Failed to create ONNX session builder: {}", e))?
            .with_intra_threads(threads.max(1))
            .map_err(|e| anyhow!("Failed to set CAPU intra-op threads: {}", e))?
            .commit_from_file(model_path_str)
            .map_err(|e| anyhow!("Failed to load CAPU model {:?}: {}", model_path, e))?;

        let tokenizer = CapuTokenizer::from_vocab_file(vocab_path)?;
        let labels = load_action_labels(labels_path)?;
        let keep_index = labels
            .iter()
            .position(|a| *a == Action::Keep)
            .ok_or_else(|| anyhow!("Label file has no $KEEP action"))?;
        let case_label_indices = labels
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                matches!(
                    a,
                    Action::TransformCaseCapital
                        | Action::TransformCaseUpper
                        | Action::TransformCaseLower
                        | Action::TransformCaseCapital1
                        | Action::TransformCaseUpperMinus1
                )
            })
            .map(|(i, _)| i)
            .collect();

        Ok(Self {
            session,
            tokenizer,
            labels,
            keep_index,
            case_label_indices,
            threads: threads.max(1),
            punctuation_level: punctuation_level.clamp(1, 10),
            case_level: case_level.clamp(1, 10),
        })
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    pub fn punctuation_level(&self) -> u8 {
        self.punctuation_level
    }

    pub fn set_punctuation_level(&mut self, level: u8) {
        self.punctuation_level = level.clamp(1, 10);
    }

    pub fn set_case_level(&mut self, level: u8) {
        self.case_level = level.clamp(1, 10);
    }
```

- [ ] **Step 7: Use the bias in `infer_once`**

Find:

```rust
        // Real words are offsets[1..num_offsets-1] — skip CLS (index 0) and SEP (last).
        let mut actions = Vec::with_capacity(words.len());
        for row in 1..(num_offsets - 1) {
            let row_start = row * num_classes;
            let row_logits = &logits_data[row_start..row_start + num_classes];
            let (best_idx, _) = row_logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .ok_or_else(|| anyhow!("Empty logits row"))?;
            actions.push(self.labels[best_idx]);
        }
```

Replace with:

```rust
        // Real words are offsets[1..num_offsets-1] — skip CLS (index 0) and SEP (last).
        let mut actions = Vec::with_capacity(words.len());
        for row in 1..(num_offsets - 1) {
            let row_start = row * num_classes;
            let row_logits = &logits_data[row_start..row_start + num_classes];
            let best_idx = decode_row(
                row_logits,
                self.keep_index,
                &self.case_label_indices,
                self.punctuation_level,
                self.case_level,
            );
            actions.push(self.labels[best_idx]);
        }
```

- [ ] **Step 8: Fix the existing ignored integration test's `load()` call**

Find:

```rust
        let mut engine = CapuEngine::load(
            &dir.join("vibert-capu.int8.onnx"),
            &dir.join("vocab.txt"),
            &dir.join("vocabulary/labels.txt"),
        )
        .expect("load model");
```

Replace with:

```rust
        let mut engine = CapuEngine::load(
            &dir.join("vibert-capu.int8.onnx"),
            &dir.join("vocab.txt"),
            &dir.join("vocabulary/labels.txt"),
            4,
            7,
            3,
        )
        .expect("load model");
```

- [ ] **Step 9: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: error in `capu_engine/commands.rs` (`CapuEngine::load` now needs 6 args, only 3 given) — expected, fixed in Task 6. Confirm no *other* errors.

- [ ] **Step 10: Re-run this file's tests**

Run: `cd frontend/src-tauri && cargo test capu_engine::capu_engine -- --nocapture`
Expected: `bias_tests`' 5 tests PASS; `integration_tests::restore_punctuation_on_real_model` shows as `ignored` (unchanged — still gated behind a real downloaded model).

- [ ] **Step 11: Commit**

```bash
git add frontend/src-tauri/src/capu_engine/capu_engine.rs
git commit -m "feat(capu): add softmax + confidence-bias decoding, thread-aware session build"
```

---

### Task 3: Bypass CAPU entirely at punctuation level 1

**Files:**
- Modify: `frontend/src-tauri/src/audio/post_asr.rs`

- [ ] **Step 1: Add the bypass check**

Find (the full current file):

```rust
/// Apply ITN then CAPU to raw ASR text. Falls back gracefully on any failure.
pub fn process_asr_text(raw: &str, capu_trailing: &mut Vec<String>) -> String {
    let lowered = raw.to_lowercase();
    let after_itn = crate::itn_engine::engine::inverse_normalize_or_pass(&lowered);

    match crate::capu_engine::commands::get_engine_arc() {
        Some(engine_arc) => {
            let mut engine = engine_arc.lock().unwrap();
            match engine.restore_punctuation(capu_trailing, &after_itn) {
                Ok((restored, next_context)) => {
                    *capu_trailing = next_context;
                    restored
                }
                Err(e) => {
                    log::warn!("CAPU failed after ITN: {}", e);
                    after_itn
                }
            }
        }
        None => after_itn,
    }
}
```

Replace with:

```rust
/// Apply ITN then CAPU to raw ASR text. Falls back gracefully on any failure. When the
/// punctuation level is at its minimum (1), CAPU is skipped entirely — matching the
/// reference app's `bypass_restorer` behavior — rather than running inference with an
/// extreme bias.
pub fn process_asr_text(raw: &str, capu_trailing: &mut Vec<String>) -> String {
    let lowered = raw.to_lowercase();
    let after_itn = crate::itn_engine::engine::inverse_normalize_or_pass(&lowered);

    match crate::capu_engine::commands::get_engine_arc() {
        Some(engine_arc) => {
            let mut engine = engine_arc.lock().unwrap();
            if engine.punctuation_level() <= 1 {
                return after_itn;
            }
            match engine.restore_punctuation(capu_trailing, &after_itn) {
                Ok((restored, next_context)) => {
                    *capu_trailing = next_context;
                    restored
                }
                Err(e) => {
                    log::warn!("CAPU failed after ITN: {}", e);
                    after_itn
                }
            }
        }
        None => after_itn,
    }
}
```

- [ ] **Step 2: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: same single expected error as Task 2 Step 9 (`capu_engine/commands.rs`), nothing new from this file.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/audio/post_asr.rs
git commit -m "feat(capu): bypass punctuation restoration entirely at level 1"
```

---

### Task 4: Database — migration and model

**Files:**
- Create: `frontend/src-tauri/migrations/20260804200000_add_capu_settings.sql`
- Modify: `frontend/src-tauri/src/database/models.rs`

- [ ] **Step 1: Create the migration**

```sql
ALTER TABLE transcript_settings ADD COLUMN capuCpuThreads INTEGER;
ALTER TABLE transcript_settings ADD COLUMN capuPunctuationLevel INTEGER NOT NULL DEFAULT 7;
ALTER TABLE transcript_settings ADD COLUMN capuCaseLevel INTEGER NOT NULL DEFAULT 3;
```

`capuCpuThreads` stays nullable with no static default — "auto" means "this machine's physical
core count," which isn't a fixed SQL constant; it's resolved at read time (Task 6). The two level
columns get a real `NOT NULL DEFAULT`, same as the existing `maxSegmentSeconds INTEGER NOT NULL
DEFAULT 25`, so their Rust fields can be plain non-optional integers.

- [ ] **Step 2: Extend `TranscriptSetting`**

In `frontend/src-tauri/src/database/models.rs`, find:

```rust
    #[sqlx(rename = "roverVariantB")]
    #[serde(rename = "roverVariantB")]
    pub rover_variant_b: Option<String>,
    pub hotwords: Option<String>,
}
```

Replace with:

```rust
    #[sqlx(rename = "roverVariantB")]
    #[serde(rename = "roverVariantB")]
    pub rover_variant_b: Option<String>,
    pub hotwords: Option<String>,
    #[sqlx(rename = "capuCpuThreads")]
    #[serde(rename = "capuCpuThreads")]
    pub capu_cpu_threads: Option<i32>,
    #[sqlx(rename = "capuPunctuationLevel")]
    #[serde(rename = "capuPunctuationLevel")]
    pub capu_punctuation_level: i32,
    #[sqlx(rename = "capuCaseLevel")]
    #[serde(rename = "capuCaseLevel")]
    pub capu_case_level: i32,
}
```

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: the same Task 2 error, plus nothing new here — `TranscriptSetting` isn't constructed by hand anywhere (it's populated by `sqlx::query_as`), so adding fields alone doesn't break other call sites.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/migrations/20260804200000_add_capu_settings.sql frontend/src-tauri/src/database/models.rs
git commit -m "feat(capu): add CAPU settings columns to transcript_settings"
```

---

### Task 5: Repository — `SettingsRepository::save_transcript_config`

**Files:**
- Modify: `frontend/src-tauri/src/database/repositories/setting.rs`

- [ ] **Step 1: Extend the function signature and SQL**

Find:

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
        hotwords: Option<&str>,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO transcript_settings
                (id, provider, model, asrVariant, decodingMethod, numActivePaths, maxSegmentSeconds, roverEnabled, roverFamilyB, roverVariantB, hotwords)
            VALUES ('1', $1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                asrVariant = excluded.asrVariant,
                decodingMethod = excluded.decodingMethod,
                numActivePaths = excluded.numActivePaths,
                maxSegmentSeconds = excluded.maxSegmentSeconds,
                roverEnabled = excluded.roverEnabled,
                roverFamilyB = excluded.roverFamilyB,
                roverVariantB = excluded.roverVariantB,
                hotwords = excluded.hotwords
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
        .bind(hotwords)
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
        hotwords: Option<&str>,
        capu_cpu_threads: Option<i32>,
        capu_punctuation_level: i32,
        capu_case_level: i32,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO transcript_settings
                (id, provider, model, asrVariant, decodingMethod, numActivePaths, maxSegmentSeconds, roverEnabled, roverFamilyB, roverVariantB, hotwords, capuCpuThreads, capuPunctuationLevel, capuCaseLevel)
            VALUES ('1', $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                asrVariant = excluded.asrVariant,
                decodingMethod = excluded.decodingMethod,
                numActivePaths = excluded.numActivePaths,
                maxSegmentSeconds = excluded.maxSegmentSeconds,
                roverEnabled = excluded.roverEnabled,
                roverFamilyB = excluded.roverFamilyB,
                roverVariantB = excluded.roverVariantB,
                hotwords = excluded.hotwords,
                capuCpuThreads = excluded.capuCpuThreads,
                capuPunctuationLevel = excluded.capuPunctuationLevel,
                capuCaseLevel = excluded.capuCaseLevel
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
        .bind(hotwords)
        .bind(capu_cpu_threads)
        .bind(capu_punctuation_level)
        .bind(capu_case_level)
        .execute(pool)
        .await?;

        Ok(())
    }
```

- [ ] **Step 2: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: new error at the one call site in `api.rs` (`save_transcript_config` now needs 3 more args) added to the existing Task 2 error — both fixed in Task 8.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/database/repositories/setting.rs
git commit -m "feat(capu): thread CAPU settings through SettingsRepository::save_transcript_config"
```

---

### Task 6: `capu_engine::commands` — settings-aware init, live apply, CPU topology command

**Files:**
- Modify: `frontend/src-tauri/src/capu_engine/commands.rs`

- [ ] **Step 1: Resolve settings from DB (best-effort) and use them in `capu_init`**

Find:

```rust
#[tauri::command]
pub async fn capu_init<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = resolve_capu_dir(&app).ok_or_else(|| "Could not resolve app data directory".to_string())?;

    {
        let guard = CAPU_ENGINE.lock().unwrap();
        if guard.is_some() {
            return Ok(());
        }
    }

    let model_path = dir.join(CAPU_MODEL_FILE);
    let vocab_path = dir.join(CAPU_VOCAB_FILE);
    let labels_path = dir.join(CAPU_LABELS_FILE);

    if !model_path.exists() || !vocab_path.exists() || !labels_path.exists() {
        return Err("CAPU model files are missing.".to_string());
    }

    let engine = CapuEngine::load(&model_path, &vocab_path, &labels_path)
        .map_err(|e| e.to_string())?;

    let mut guard = CAPU_ENGINE.lock().unwrap();
    *guard = Some(Arc::new(Mutex::new(engine)));
    info!("CAPU engine initialized");
    Ok(())
}
```

Replace with:

```rust
/// Resolves `(threads, punctuation_level, case_level)` for the CAPU engine: reads the saved
/// `TranscriptSetting` row if the app's DB state is already available (`app.try_state` —
/// this may run before database setup completes at startup, matching the same best-effort
/// pattern `asr_load_model` already uses to read hotwords), falling back to physical-core
/// count / level 7 / level 3 otherwise. `capu_cpu_threads` is clamped to this machine's
/// physical core count — never trust a stored value blindly, hardware can differ across
/// runs (e.g. a DB copied from a different machine).
async fn resolve_capu_settings<R: Runtime>(app: &AppHandle<R>) -> (usize, u8, u8) {
    let (physical_cores, _) = crate::capu_engine::cpu_topology::detect_cpu_topology();

    if let Some(state) = app.try_state::<crate::state::AppState>() {
        if let Ok(Some(config)) =
            crate::database::repositories::setting::SettingsRepository::get_transcript_config(
                state.db_manager.pool(),
            )
            .await
        {
            let threads = config
                .capu_cpu_threads
                .filter(|&t| t > 0)
                .map(|t| (t as usize).min(physical_cores))
                .unwrap_or(physical_cores);
            let punct = config.capu_punctuation_level.clamp(1, 10) as u8;
            let case = config.capu_case_level.clamp(1, 10) as u8;
            return (threads, punct, case);
        }
    }

    (physical_cores, 7, 3)
}

#[tauri::command]
pub async fn capu_init<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = resolve_capu_dir(&app).ok_or_else(|| "Could not resolve app data directory".to_string())?;

    {
        let guard = CAPU_ENGINE.lock().unwrap();
        if guard.is_some() {
            return Ok(());
        }
    }

    let model_path = dir.join(CAPU_MODEL_FILE);
    let vocab_path = dir.join(CAPU_VOCAB_FILE);
    let labels_path = dir.join(CAPU_LABELS_FILE);

    if !model_path.exists() || !vocab_path.exists() || !labels_path.exists() {
        return Err("CAPU model files are missing.".to_string());
    }

    let (threads, punctuation_level, case_level) = resolve_capu_settings(&app).await;

    let engine = CapuEngine::load(
        &model_path,
        &vocab_path,
        &labels_path,
        threads,
        punctuation_level,
        case_level,
    )
    .map_err(|e| e.to_string())?;

    let mut guard = CAPU_ENGINE.lock().unwrap();
    *guard = Some(Arc::new(Mutex::new(engine)));
    info!(
        "CAPU engine initialized ({} threads, punctuation level {}, case level {})",
        threads, punctuation_level, case_level
    );
    Ok(())
}

/// Best-effort: applies new settings to the already-loaded CAPU engine (if any) when
/// `api_save_transcript_config` saves. Punctuation/case levels update immediately, no
/// rebuild needed. The ONNX session is only rebuilt (unload + reload) when `threads` is set
/// and differs from what's currently loaded — and if that reload fails (e.g. model files
/// were deleted), the previous working engine is left untouched rather than torn down.
pub(crate) async fn apply_settings_after_save<R: Runtime>(
    app: AppHandle<R>,
    threads: Option<i32>,
    punctuation_level: u8,
    case_level: u8,
) {
    let engine_arc = match get_engine_arc() {
        Some(e) => e,
        None => return, // not loaded yet — capu_init will pick up saved settings next time it runs
    };

    let (physical_cores, _) = crate::capu_engine::cpu_topology::detect_cpu_topology();
    let requested_threads = threads
        .filter(|&t| t > 0)
        .map(|t| (t as usize).min(physical_cores));

    let mut reload_threads: Option<usize> = None;
    {
        let mut engine = engine_arc.lock().unwrap();
        engine.set_punctuation_level(punctuation_level);
        engine.set_case_level(case_level);
        if let Some(t) = requested_threads {
            if t != engine.threads() {
                reload_threads = Some(t);
            }
        }
    }

    let threads = match reload_threads {
        Some(t) => t,
        None => return,
    };

    let dir = match resolve_capu_dir(&app) {
        Some(d) => d,
        None => return,
    };
    let model_path = dir.join(CAPU_MODEL_FILE);
    let vocab_path = dir.join(CAPU_VOCAB_FILE);
    let labels_path = dir.join(CAPU_LABELS_FILE);

    match CapuEngine::load(
        &model_path,
        &vocab_path,
        &labels_path,
        threads,
        punctuation_level,
        case_level,
    ) {
        Ok(new_engine) => {
            let mut guard = CAPU_ENGINE.lock().unwrap();
            *guard = Some(Arc::new(Mutex::new(new_engine)));
            info!("CAPU engine reloaded with {} threads", threads);
        }
        Err(e) => {
            error!(
                "Failed to reload CAPU engine with {} threads: {} — keeping previous engine",
                threads, e
            );
        }
    }
}

#[derive(serde::Serialize)]
pub struct CpuTopology {
    #[serde(rename = "physicalCores")]
    pub physical_cores: usize,
    #[serde(rename = "logicalThreads")]
    pub logical_threads: usize,
}

#[tauri::command]
pub async fn capu_get_cpu_topology() -> Result<CpuTopology, String> {
    let (physical_cores, logical_threads) = crate::capu_engine::cpu_topology::detect_cpu_topology();
    Ok(CpuTopology {
        physical_cores,
        logical_threads,
    })
}
```

- [ ] **Step 2: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors — this task's changes resolve both the Task 2 (`CapuEngine::load` arg count) and Task 5 (`save_transcript_config` arg count doesn't touch this file, but confirm) errors that were expected up to this point, since `capu_init`'s call site is the one that needed updating. If `api.rs` still shows an error about `save_transcript_config` or `TranscriptConfig`, that's expected — fixed in Task 8.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/capu_engine/commands.rs
git commit -m "feat(capu): resolve settings from DB on init, live-apply on save, expose CPU topology"
```

---

### Task 7: Register `capu_get_cpu_topology` in `lib.rs`

**Files:**
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Add it to the invoke handler**

Find:

```rust
            // CAPU Vietnamese punctuation restoration commands
            capu_engine::commands::capu_get_models_directory,
            capu_engine::commands::capu_is_model_downloaded,
            capu_engine::commands::capu_download_model,
            capu_engine::commands::capu_init,
```

Replace with:

```rust
            // CAPU Vietnamese punctuation restoration commands
            capu_engine::commands::capu_get_models_directory,
            capu_engine::commands::capu_is_model_downloaded,
            capu_engine::commands::capu_download_model,
            capu_engine::commands::capu_init,
            capu_engine::commands::capu_get_cpu_topology,
```

- [ ] **Step 2: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: same state as Task 6 Step 3 — this is a registration-only change, doesn't affect compilation of the command itself.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/lib.rs
git commit -m "feat(capu): register capu_get_cpu_topology command"
```

---

### Task 8: `api/api.rs` — thread CAPU settings through save/get, apply live on save

**Files:**
- Modify: `frontend/src-tauri/src/api/api.rs`

- [ ] **Step 1: Extend `TranscriptConfig`**

Find:

```rust
    #[serde(rename = "roverVariantB")]
    pub rover_variant_b: Option<String>,
    pub hotwords: Option<String>,
}
```

Replace with:

```rust
    #[serde(rename = "roverVariantB")]
    pub rover_variant_b: Option<String>,
    pub hotwords: Option<String>,
    #[serde(rename = "capuCpuThreads")]
    pub capu_cpu_threads: Option<i32>,
    #[serde(rename = "capuPunctuationLevel")]
    pub capu_punctuation_level: i32,
    #[serde(rename = "capuCaseLevel")]
    pub capu_case_level: i32,
}
```

- [ ] **Step 2: Populate it in `api_get_transcript_config`**

Find:

```rust
                rover_enabled: config.rover_enabled,
                rover_family_b: config.rover_family_b.clone(),
                rover_variant_b: config.rover_variant_b.clone(),
                hotwords: crate::asr_engine::hotwords::display_hotwords_text(
                    config.hotwords.as_deref(),
                    bundled_hotwords.as_deref(),
                ),
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
                hotwords: bundled_hotwords.clone(),
            }))
        }
```

Replace with:

```rust
                rover_enabled: config.rover_enabled,
                rover_family_b: config.rover_family_b.clone(),
                rover_variant_b: config.rover_variant_b.clone(),
                hotwords: crate::asr_engine::hotwords::display_hotwords_text(
                    config.hotwords.as_deref(),
                    bundled_hotwords.as_deref(),
                ),
                capu_cpu_threads: config.capu_cpu_threads,
                capu_punctuation_level: config.capu_punctuation_level,
                capu_case_level: config.capu_case_level,
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
                hotwords: bundled_hotwords.clone(),
                capu_cpu_threads: None,
                capu_punctuation_level: 7,
                capu_case_level: 3,
            }))
        }
```

- [ ] **Step 3: Extend `api_save_transcript_config`'s parameters**

Find:

```rust
    rover_enabled: Option<bool>,
    rover_family_b: Option<String>,
    rover_variant_b: Option<String>,
    hotwords: Option<String>,
    _auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
```

Replace with:

```rust
    rover_enabled: Option<bool>,
    rover_family_b: Option<String>,
    rover_variant_b: Option<String>,
    hotwords: Option<String>,
    capu_cpu_threads: Option<i32>,
    capu_punctuation_level: Option<i32>,
    capu_case_level: Option<i32>,
    _auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
```

- [ ] **Step 4: Resolve and save the CAPU fields**

Find:

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
        if rover_on {
            Some(rover_variant_b_resolved)
        } else {
            None
        },
        hotwords.as_deref(),
    )
    .await
    {
        log_error!("Failed to save transcript config: {}", e);
        return Err(e.to_string());
    }
```

Replace with:

```rust
    let rover_on = rover_enabled.unwrap_or(false);
    let rover_variant_b_resolved = rover_variant_b.as_deref().unwrap_or("int8");

    let capu_threads_resolved = capu_cpu_threads.filter(|&t| t > 0);
    let capu_punct_resolved = capu_punctuation_level.unwrap_or(7).clamp(1, 10);
    let capu_case_resolved = capu_case_level.unwrap_or(3).clamp(1, 10);

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
        if rover_on {
            Some(rover_variant_b_resolved)
        } else {
            None
        },
        hotwords.as_deref(),
        capu_threads_resolved,
        capu_punct_resolved,
        capu_case_resolved,
    )
    .await
    {
        log_error!("Failed to save transcript config: {}", e);
        return Err(e.to_string());
    }
```

- [ ] **Step 5: Apply live, right after the existing hotwords-push block**

Find:

```rust
    // Best-effort: if the ASR engine is already loaded this session, push the new
    // hotwords into it immediately so the very next transcribe call uses them without
    // requiring a reload.
    if let Ok(engine) = crate::asr_engine::commands::get_engine_arc() {
        let bundled = crate::asr_engine::commands::load_bundled_hotwords_raw(&app);
        let text = crate::asr_engine::hotwords::effective_hotwords_text(
            hotwords.as_deref(),
            bundled.as_deref(),
        );
        engine.set_hotwords(text).await;
    }

    log_info!("Successfully saved transcript configuration.");
```

Replace with:

```rust
    // Best-effort: if the ASR engine is already loaded this session, push the new
    // hotwords into it immediately so the very next transcribe call uses them without
    // requiring a reload.
    if let Ok(engine) = crate::asr_engine::commands::get_engine_arc() {
        let bundled = crate::asr_engine::commands::load_bundled_hotwords_raw(&app);
        let text = crate::asr_engine::hotwords::effective_hotwords_text(
            hotwords.as_deref(),
            bundled.as_deref(),
        );
        engine.set_hotwords(text).await;
    }

    // Best-effort: apply CAPU settings to the already-loaded engine (if any) — levels
    // update immediately; the ONNX session only rebuilds if the thread count changed.
    crate::capu_engine::commands::apply_settings_after_save(
        app.clone(),
        capu_threads_resolved,
        capu_punct_resolved as u8,
        capu_case_resolved as u8,
    )
    .await;

    log_info!("Successfully saved transcript configuration.");
```

- [ ] **Step 6: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors.

- [ ] **Step 7: Run the full Rust test suite for the touched crates**

Run: `cd frontend/src-tauri && cargo test capu_engine -- --nocapture`
Expected: all `capu_engine::cpu_topology` and `capu_engine::capu_engine::bias_tests` tests still PASS (Tasks 1-2), `integration_tests::restore_punctuation_on_real_model` still `ignored`. No regressions.

- [ ] **Step 8: Commit**

```bash
git add frontend/src-tauri/src/api/api.rs
git commit -m "feat(capu): thread CAPU settings through save/get API, apply live on save"
```

---

### Task 9: `lib/asr.ts` — `CapuAPI.getCpuTopology`

**Files:**
- Modify: `frontend/src/lib/asr.ts`

- [ ] **Step 1: Add the API**

Find:

```typescript
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

Replace with:

```typescript
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

export interface CpuTopology {
  physicalCores: number;
  logicalThreads: number;
}

export const CapuAPI = {
  getCpuTopology: (): Promise<CpuTopology> => invoke('capu_get_cpu_topology'),
};
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/asr.ts
git commit -m "feat(capu): add CapuAPI.getCpuTopology"
```

---

### Task 10: `AsrModelManager.tsx` — 3 new sliders

**Files:**
- Modify: `frontend/src/components/AsrModelManager.tsx`

- [ ] **Step 1: Import `CapuAPI`**

Find:

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

Replace with:

```typescript
import {
  ASR_MODELS,
  AsrAPI,
  AsrModelFamily,
  CapuAPI,
  DecodingMethod,
  ModelVariant,
  RoverAPI,
  VariantStatus,
} from '../lib/asr';
```

- [ ] **Step 2: Add the level-label lookup near the other module-level constants**

Find:

```typescript
const DEFAULT_FAMILY: AsrModelFamily = 'zipformer-vi-30m';
const DEFAULT_VARIANT: ModelVariant = 'int8';
const DEFAULT_DECODING: DecodingMethod = 'modified_beam_search';
const DEFAULT_PATHS = 15;
const DEFAULT_MAX_SEGMENT_SECONDS = 25;
const MIN_MAX_SEGMENT_SECONDS = 5;
const MAX_MAX_SEGMENT_SECONDS = 30;
```

Replace with:

```typescript
const DEFAULT_FAMILY: AsrModelFamily = 'zipformer-vi-30m';
const DEFAULT_VARIANT: ModelVariant = 'int8';
const DEFAULT_DECODING: DecodingMethod = 'modified_beam_search';
const DEFAULT_PATHS = 15;
const DEFAULT_MAX_SEGMENT_SECONDS = 25;
const MIN_MAX_SEGMENT_SECONDS = 5;
const MAX_MAX_SEGMENT_SECONDS = 30;
const DEFAULT_CAPU_PUNCTUATION_LEVEL = 7;
const DEFAULT_CAPU_CASE_LEVEL = 3;
const FALLBACK_PHYSICAL_CORES = 4;

// Exact-value lookup, no interpolation between mid-points — matches the reference app's
// own `labels.get(value, str(value))` (values without a label just show the raw number).
const LEVEL_LABELS: Record<number, string> = {
  1: 'Rất ít',
  3: 'Ít',
  5: 'Vừa',
  7: 'Nhiều',
  10: 'Rất nhiều',
};
const levelLabel = (v: number) => LEVEL_LABELS[v] ?? String(v);
```

- [ ] **Step 3: Add state**

Find:

```typescript
  const [roverEnabled, setRoverEnabled] = useState(false);
  const [hotwords, setHotwords] = useState('');
```

Replace with:

```typescript
  const [roverEnabled, setRoverEnabled] = useState(false);
  const [hotwords, setHotwords] = useState('');
  const [physicalCores, setPhysicalCores] = useState(FALLBACK_PHYSICAL_CORES);
  const [capuThreads, setCapuThreads] = useState(FALLBACK_PHYSICAL_CORES);
  const [capuPunctuationLevel, setCapuPunctuationLevel] = useState(DEFAULT_CAPU_PUNCTUATION_LEVEL);
  const [capuCaseLevel, setCapuCaseLevel] = useState(DEFAULT_CAPU_CASE_LEVEL);
```

- [ ] **Step 4: Fetch CPU topology once on mount**

Find:

```typescript
  useEffect(() => {
    AsrAPI.init().catch(console.error);
    loadSavedConfig();
  }, []);
```

Replace with:

```typescript
  useEffect(() => {
    AsrAPI.init().catch(console.error);
    loadSavedConfig();
    CapuAPI.getCpuTopology()
      .then(({ physicalCores: cores }) => {
        setPhysicalCores(cores);
        setCapuThreads((prev) => Math.min(prev, cores));
      })
      .catch(() => {
        // Fall back to FALLBACK_PHYSICAL_CORES already set as initial state — don't block
        // the rest of the settings panel on this.
      });
  }, []);
```

- [ ] **Step 5: Load saved values in `loadSavedConfig`**

Find:

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
        hotwords?: string;
      } | null>('api_get_transcript_config');
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
        hotwords?: string;
        capuCpuThreads?: number | null;
        capuPunctuationLevel?: number;
        capuCaseLevel?: number;
      } | null>('api_get_transcript_config');
```

Then find:

```typescript
        if (typeof config.hotwords === 'string') {
          setHotwords(config.hotwords);
        }
      }
    } catch (e) {
      console.error('Failed to load ASR config:', e);
    }
  };
```

Replace with:

```typescript
        if (typeof config.hotwords === 'string') {
          setHotwords(config.hotwords);
        }
        if (typeof config.capuCpuThreads === 'number') {
          setCapuThreads(config.capuCpuThreads);
        }
        if (typeof config.capuPunctuationLevel === 'number') {
          setCapuPunctuationLevel(config.capuPunctuationLevel);
        }
        if (typeof config.capuCaseLevel === 'number') {
          setCapuCaseLevel(config.capuCaseLevel);
        }
      }
    } catch (e) {
      console.error('Failed to load ASR config:', e);
    }
  };
```

- [ ] **Step 6: Include the fields when saving**

Find:

```typescript
      await invoke('api_save_transcript_config', {
        provider: 'asr',
        model: selectedFamily,
        apiKey: null,
        asrVariant: effectiveVariant,
        decodingMethod,
        numActivePaths,
        maxSegmentSeconds,
        roverEnabled,
        roverFamilyB: roverEnabled ? roverFamilyB : null,
        roverVariantB: roverEnabled ? roverEffectiveVariantB : null,
        hotwords,
      });
```

Replace with:

```typescript
      await invoke('api_save_transcript_config', {
        provider: 'asr',
        model: selectedFamily,
        apiKey: null,
        asrVariant: effectiveVariant,
        decodingMethod,
        numActivePaths,
        maxSegmentSeconds,
        roverEnabled,
        roverFamilyB: roverEnabled ? roverFamilyB : null,
        roverVariantB: roverEnabled ? roverEffectiveVariantB : null,
        hotwords,
        capuCpuThreads: capuThreads,
        capuPunctuationLevel,
        capuCaseLevel,
      });
```

- [ ] **Step 7: Add the 3 sliders to the JSX, after the hotwords block**

Find:

```typescript
        <textarea
          value={hotwords}
          onChange={(e) => setHotwords(e.target.value)}
          disabled={disabled}
          rows={6}
          placeholder={'ỦY BAN NHÂN DÂN :2.5\nCHUYỂN ĐỔI SỐ\n# Tên riêng\nANH MINH'}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white font-mono focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        />
      </div>

      {/* Save button */}
      <div className="flex items-center gap-3 pt-1">
```

Replace with:

```typescript
        <textarea
          value={hotwords}
          onChange={(e) => setHotwords(e.target.value)}
          disabled={disabled}
          rows={6}
          placeholder={'ỦY BAN NHÂN DÂN :2.5\nCHUYỂN ĐỔI SỐ\n# Tên riêng\nANH MINH'}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white font-mono focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        />
      </div>

      {/* CAPU: CPU threads */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Số luồng CPU (thêm dấu câu)
          </label>
          <span className="text-sm font-mono text-gray-900 dark:text-white w-8 text-right">
            {capuThreads}
          </span>
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400">
          Số luồng CPU dành cho model thêm dấu câu/viết hoa. Đổi giá trị này sẽ tải lại model khi lưu
          (mất khoảng 1-2 giây).
        </p>
        <div className="flex items-center gap-3">
          <span className="text-xs text-gray-400">1</span>
          <input
            type="range"
            min={1}
            max={physicalCores}
            step={1}
            value={capuThreads}
            onChange={(e) => setCapuThreads(Number(e.target.value))}
            disabled={disabled}
            className="flex-1 accent-blue-500 disabled:opacity-50"
          />
          <span className="text-xs text-gray-400">{physicalCores}</span>
        </div>
      </div>

      {/* CAPU: punctuation level */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Mức độ thêm dấu
          </label>
          <span className="text-sm font-mono text-gray-900 dark:text-white w-16 text-right">
            {levelLabel(capuPunctuationLevel)}
          </span>
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400">
          Mức 1 tắt hoàn toàn việc thêm dấu câu (giữ nguyên văn bản thô từ nhận dạng giọng nói).
        </p>
        <div className="flex items-center gap-3">
          <span className="text-xs text-gray-400">1</span>
          <input
            type="range"
            min={1}
            max={10}
            step={1}
            value={capuPunctuationLevel}
            onChange={(e) => setCapuPunctuationLevel(Number(e.target.value))}
            disabled={disabled}
            className="flex-1 accent-blue-500 disabled:opacity-50"
          />
          <span className="text-xs text-gray-400">10</span>
        </div>
      </div>

      {/* CAPU: case level */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Mức độ tự viết hoa
          </label>
          <span className="text-sm font-mono text-gray-900 dark:text-white w-16 text-right">
            {levelLabel(capuCaseLevel)}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-xs text-gray-400">1</span>
          <input
            type="range"
            min={1}
            max={10}
            step={1}
            value={capuCaseLevel}
            onChange={(e) => setCapuCaseLevel(Number(e.target.value))}
            disabled={disabled}
            className="flex-1 accent-blue-500 disabled:opacity-50"
          />
          <span className="text-xs text-gray-400">10</span>
        </div>
      </div>

      {/* Save button */}
      <div className="flex items-center gap-3 pt-1">
```

- [ ] **Step 8: Verify TypeScript compiles**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add frontend/src/components/AsrModelManager.tsx
git commit -m "feat(capu): add CPU threads / punctuation level / case level sliders to Settings"
```

---

### Task 11: Manual E2E (required before merge)

**Files:** none (verification only)

- [ ] **Step 1: Full build**

Run: `cd frontend/src-tauri && cargo build`
Expected: `Finished` with no errors.

- [ ] **Step 2: Baseline — default levels**

With CAPU model already downloaded (Settings → Nhận dạng giọng nói tiếng Việt shows the 3 new sliders at their saved/default positions — 7 and 3), record a short Vietnamese sentence. Confirm punctuation and capitalization appear roughly as they did before this plan (default levels 7/3 should behave close to the pre-existing hardcoded decode, since level 7's bias is the reference app's own tuned default).

- [ ] **Step 3: Punctuation level = 1 (bypass)**

Drag "Mức độ thêm dấu" to 1, "Lưu cấu hình". Record the same sentence again.
Expected: transcript comes back with **no punctuation at all** — confirms the bypass in `post_asr.rs` (Task 3) is actually reached, not just compiled.

- [ ] **Step 4: Punctuation level = 10**

Drag to 10, save, record the same sentence again.
Expected: noticeably more aggressive punctuation than the level-7 baseline from Step 2.

- [ ] **Step 5: Case level = 1 and = 10**

Repeat Steps 3-4's save/record cycle for "Mức độ tự viết hoa" at 1 and 10 (leave punctuation level back at a non-1 value so CAPU actually runs). Expected: visibly less/more capitalization respectively.

- [ ] **Step 6: CPU threads — reload doesn't break anything**

Drag "Số luồng CPU" to a different value than its current one, save. Expected: no crash/hang during the ~1-2s reload; the very next transcription still works correctly using the new setting (check app logs for `CAPU engine reloaded with N threads`).

- [ ] **Step 7: CPU threads while recording**

Start a recording, then (if the UI allows it — the `disabled` prop is tied to `isRecording` for all these controls, same as the existing ASR settings) attempt to change CPU threads. Expected: control is disabled during recording, exactly like the other ASR settings — confirms no new bypass of the existing recording-lock convention was introduced.

- [ ] **Step 8: Retranscription uses the saved settings**

Re-transcribe an existing meeting (Meeting Details → Retranscribe) after changing levels in Step 3-5. Expected: the retranscribed output reflects the currently-saved punctuation/case levels, not stale defaults — confirms `post_asr::process_asr_text` (shared by `retranscription.rs`, `import.rs`, and `worker.rs`) picks up the live engine state correctly on all 3 paths.

- [ ] **Step 9: Fresh app restart honors previously-saved settings**

Fully quit and relaunch the app (don't just reload the window) after having saved non-default levels/threads in earlier steps. Record a sentence immediately, without opening Settings first.
Expected: the transcript reflects the previously-saved levels — confirms `capu_init`'s DB-read path (Task 6) works, not just the save-time live-apply path (Task 8).

---

## Spec coverage checklist

| Spec requirement | Task |
|---|---|
| CPU thread detection (physical/logical, via `sysinfo`) | Task 1 |
| `CapuEngine` softmax + `$KEEP` bias (punctuation level) | Task 2 |
| `CapuEngine` case-label bias (case level) | Task 2 |
| `CapuEngine::load` takes `threads` and configures `with_intra_threads` | Task 2 |
| Bypass CAPU entirely at punctuation level 1 | Task 3 |
| DB columns: `capuCpuThreads` (nullable, no static default), `capuPunctuationLevel`/`capuCaseLevel` (`NOT NULL DEFAULT`) | Task 4 |
| Repository threads the 3 fields through | Task 5 |
| `capu_init` resolves settings from DB (best-effort, physical-core clamp) | Task 6 |
| `apply_settings_after_save`: live-update levels always, rebuild only if threads changed, never tears down a working engine on reload failure | Task 6 |
| `capu_get_cpu_topology` command | Task 6, registered in Task 7 |
| `api_save_transcript_config` calls `apply_settings_after_save` inline (matches the existing hotwords pattern, no separate "apply" command) | Task 8 |
| `CapuAPI.getCpuTopology` on the frontend | Task 9 |
| 3 sliders in `AsrModelManager.tsx`, exact-value level labels (no interpolation) | Task 10 |
| Manual verification: bypass, both directions of both levels, thread reload (idle + during recording), retranscription, fresh-restart persistence | Task 11 |
| Out of scope: `asr_engine`'s own thread count, `Live`/`File`-separate settings, auto-tuning, GECToR gate thresholds | Confirmed — no task touches those |

---

## Notes for whoever executes this (e.g. via Cursor)

- Tasks 1-3 are pure Rust engine logic and fully unit-tested without needing the real ONNX model downloaded — safe to implement and verify in any environment. Tasks 4-10 are plumbing (DB/API/UI) diffed against exact current file content; if a `Find` block doesn't match exactly (e.g. this branch moved further since this plan was written), re-read the target file fresh before editing rather than fuzzy-matching — the codebase has evolved fast on this branch (see the `git log` history of `docs/superpowers/plans/` for how many features landed just in the days before this one).
- The dependency order matters for the "expected compile error" steps: Task 2 breaks `capu_engine/commands.rs` until Task 6; Task 5 breaks `api.rs` until Task 8. Don't be alarmed by `cargo check` failing partway through — each task's "Verify compile" step says exactly what error is expected and which task fixes it.
- Task 11 Step 9 (fresh app restart) is the one most likely to get skipped by accident and is genuinely important — it's the only step that exercises `capu_init`'s DB-read path (Task 6) rather than the save-time live-apply path (Task 8). A bug where settings apply on save but don't survive a restart would pass every other manual check and only show up here.
- If `with_intra_threads` in Task 2 doesn't compile against the actual vendored `ort` crate on the machine running this plan (e.g. a lockfile drift pulled a different `ort` patch version than `2.0.0-rc.10`), check `frontend/src-tauri/Cargo.lock` for the resolved `ort` version and re-verify the method signature in `~/.cargo/registry/src/*/ort-<version>/src/session/builder/impl_options.rs` before assuming the plan is wrong.
