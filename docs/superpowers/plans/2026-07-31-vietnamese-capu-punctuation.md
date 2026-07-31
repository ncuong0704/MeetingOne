# Vietnamese CAPU Punctuation Restoration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore Vietnamese punctuation and capitalization on ZipFormer ASR output (which comes
out raw, with no punctuation/casing) by porting the `welcomyou/vibert-capu-onnx` GECToR-style ONNX
model natively to Rust, hooked into both the live transcription worker and meeting retranscription.

**Architecture:** A new `capu_engine` module loads the ONNX model (via crate `ort`) and a BERT
WordPiece tokenizer (via crate `tokenizers`, built from `vocab.txt`), runs the GECToR iterative
tag-and-apply loop (max 3 iterations), and exposes `restore_punctuation(trailing_context, text)`.
This is called from `worker.rs` (real-time, per VAD segment, with a small trailing-context window
carried across segments) and reused identically from `retranscription.rs` (batch, per stored
segment). All model I/O semantics (15-action vocabulary, case-transform semantics, CLS/SEP offset
handling) were confirmed by reading the model repo's own reference Python source
(`gec_model.py`, `utils.py`) and vocabulary files — not guessed.

**Tech Stack:** Rust, `ort` 2.0.0-rc.13 (ONNX Runtime bindings), `tokenizers` 0.23 (HuggingFace Rust
tokenizer), Tauri 2.x commands, existing `reqwest`-based streaming download pattern.

**Reference spec:** `docs/superpowers/specs/2026-07-31-vietnamese-capu-punctuation-design.md`

---

## Before you start: key facts this plan depends on (verified, not guessed)

- **Model I/O** (from the model repo's README): inputs `input_ids`, `attention_mask`,
  `token_type_ids` (always 0), `input_offsets` (all `int64`); outputs `logits`
  `(batch, num_offsets, 15)` and `detect_logits` `(batch, num_offsets, 4)` (`float32`).
- **15 labels** (`vocabulary/labels.txt`, confirmed verbatim from the repo), in file order:
  `$KEEP`, `$TRANSFORM_CASE_CAPITAL`, `$APPEND_,`, `$APPEND_.`, `$TRANSFORM_VERB_VB_VBN`,
  `$TRANSFORM_CASE_UPPER`, `$APPEND_:`, `$APPEND_?`, `$TRANSFORM_VERB_VB_VBC`,
  `$TRANSFORM_CASE_LOWER`, `$TRANSFORM_CASE_CAPITAL_1`, `$TRANSFORM_CASE_UPPER_-1`,
  `$MERGE_SPACE`, `@@UNKNOWN@@`, `@@PADDING@@`.
- **Case-transform semantics** (ported verbatim from `utils.py`'s `convert_using_case`):
  - `LOWER` → `token.to_lowercase()`
  - `UPPER` → `token.to_uppercase()`
  - `CAPITAL` → uppercase first char, lowercase the rest (Python `.capitalize()`)
  - `CAPITAL_1` → **keep first char as-is**, then capitalize from index 1 onward (`token[0] +
    token[1:].capitalize()`)
  - `UPPER_-1` → uppercase everything except the **last** char, leave the last char as-is
    (`token[:-1].upper() + token[-1]`)
- **Offset/CLS/SEP handling** (ported verbatim from `gec_model.py`'s offset loop): offsets are
  built by walking `word_ids()` and recording every index where the word id changes from the
  previous token. Because `[CLS]` has `word_id = None` and the transition into `[SEP]` also
  changes the word id, the resulting `offsets` list always has **exactly `words.len() + 2`
  entries** — first entry = `[CLS]` position, last entry = `[SEP]`-boundary position, and the
  `words.len()` entries in between correspond 1:1 with the real words. **Always skip the first and
  last offset** when reading `logits` back into per-word actions.
- **Iteration**: max 3 passes (`gec_model.py` default), stop early once every predicted action for
  the current pass is `$KEEP`.
- **Simplifications made deliberately for this port** (documented, not accidental):
  - We do **not** port GECToR's general `(start, end, label, prob)` edit-tuple machinery or the
    `detect_logits` probability-threshold gating. The reference constructor defaults are
    `min_error_probability=0.0, confidence=0`, which is threshold-off — so with the reference's own
    defaults, gating is a no-op. We implement plain per-word argmax over the 15-way `logits` only.
    If manual testing (Task 11) shows too many spurious edits, revisit with `detect_logits`
    thresholding — do not add it speculatively now (YAGNI).
  - `$TRANSFORM_VERB_VB_VBN` / `$TRANSFORM_VERB_VB_VBC` are implemented as a no-op passthrough
    (return the token unchanged) with a `warn!` log if triggered. These are vestigial GECToR
    actions from the base action template; this capu-finetuned model is not expected to predict
    them. If the warning ever fires in practice, that's the signal to implement real verb
    conjugation — not before.
  - `ort` is added with its **default bundled ONNX Runtime** (not `load-dynamic` sharing with
    `sherpa-onnx`'s copy). This costs ~15-20MB extra in the shipped binary but avoids a fragile
    cross-platform dynamic-linking setup. Revisit only if binary size becomes an actual complaint.
  - The spec listed `config.json` among the files to download. We don't download or parse it: the
    3 values we'd read from it (`max_position_embeddings=512`, `num_labels=15`,
    `num_detect_classes=4`) were already confirmed while researching the spec, are static for this
    model, and `num_labels` is redundantly self-verified anyway (`labels.txt` has 15 lines, checked
    against the ONNX output shape at inference time). One fewer file to download/parse for no loss
    of correctness.
  - The spec's file tree showed a `capu_engine/config.rs`. Following the codebase's actual existing
    convention (ZipFormer's constants live in the shared top-level `src/config.rs`, not inside
    `zipformer_engine/`), CAPU's constants go into that same shared `src/config.rs` instead (Task
    2) — no `capu_engine/config.rs` file is created.

---

### Task 1: Add `ort` and `tokenizers` dependencies

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`

- [ ] **Step 1: Add the dependencies**

In `frontend/src-tauri/Cargo.toml`, under the existing `# ZipFormer Vietnamese ASR via sherpa-onnx`
block (around line 70-72), add a new block right after it:

```toml
# CAPU (punctuation + capitalization restoration) via ONNX Runtime
ort = "2.0.0-rc.13"
tokenizers = "0.23"
```

- [ ] **Step 2: Verify it builds**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles successfully (may take a while the first time as `ort` downloads its bundled
ONNX Runtime binary and `tokenizers`/`esaxx-rs` compile). If `tokenizers` fails to build because of
the `esaxx-rs` patch already in `Cargo.toml` (`[patch.crates-io]` section, line ~160), that patch
already targets a fork with `feat/dynamic-msvc-link` — this exists specifically to fix Windows MSVC
linking for `esaxx-rs`, so it should already cover `tokenizers`' needs. If it still fails, run
`cargo tree -i esaxx-rs` to see what's pulling it in and diagnose from there.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/Cargo.toml
git commit -m "build: add ort and tokenizers deps for CAPU punctuation restoration"
```

---

### Task 2: Add CAPU model constants to shared config

**Files:**
- Modify: `frontend/src-tauri/src/config.rs`

- [ ] **Step 1: Append CAPU constants**

`frontend/src-tauri/src/config.rs` currently only holds ZipFormer constants. Append a new section
at the end of the file:

```rust

/// Application configuration constants — CAPU Vietnamese punctuation restoration

pub const CAPU_MODEL_NAME: &str = "vibert-capu-vi";
pub const CAPU_SUBDIR: &str = "capu-vi";

pub const CAPU_HF_URL: &str = "https://huggingface.co/welcomyou/vibert-capu-onnx/resolve/main";

pub const CAPU_MODEL_FILE: &str = "vibert-capu.int8.onnx";
pub const CAPU_VOCAB_FILE: &str = "vocab.txt";
pub const CAPU_LABELS_FILE: &str = "vocabulary/labels.txt";
pub const CAPU_DTAGS_FILE: &str = "vocabulary/d_tags.txt";

// Approximate sizes, used only for the download progress bar
pub const CAPU_MODEL_SIZE_BYTES: u64 = 110_000_000;
pub const CAPU_VOCAB_SIZE_BYTES: u64 = 500_000;
pub const CAPU_LABELS_SIZE_BYTES: u64 = 300;
pub const CAPU_DTAGS_SIZE_BYTES: u64 = 100;

pub const CAPU_MAX_SEQ_LEN: usize = 512;
pub const CAPU_MAX_ITERATIONS: usize = 3;
pub const CAPU_TRAILING_CONTEXT_WORDS: usize = 15;
```

- [ ] **Step 2: Verify it builds**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles (constants are unused so far — that's fine, later tasks consume them).

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/config.rs
git commit -m "feat(capu): add model/config constants for CAPU punctuation restoration"
```

---

### Task 3: `vocabulary.rs` — Action enum + label file loading

**Files:**
- Create: `frontend/src-tauri/src/capu_engine/mod.rs`
- Create: `frontend/src-tauri/src/capu_engine/vocabulary.rs`
- Test: inline `#[cfg(test)]` module in `vocabulary.rs`

- [ ] **Step 1: Create the module skeleton**

`frontend/src-tauri/src/capu_engine/mod.rs`:

```rust
pub mod vocabulary;
pub mod edits;
pub mod tokenizer;
pub mod capu_engine;
pub mod commands;

pub use capu_engine::CapuEngine;
```

- [ ] **Step 2: Write the failing test for label parsing**

`frontend/src-tauri/src/capu_engine/vocabulary.rs`:

```rust
use anyhow::{anyhow, Result};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Keep,
    TransformCaseCapital,
    AppendComma,
    AppendPeriod,
    TransformVerbVbVbn,
    TransformCaseUpper,
    AppendColon,
    AppendQuestion,
    TransformVerbVbVbc,
    TransformCaseLower,
    TransformCaseCapital1,
    TransformCaseUpperMinus1,
    MergeSpace,
    Unknown,
    Padding,
}

impl Action {
    pub fn from_label(label: &str) -> Action {
        match label {
            "$KEEP" => Action::Keep,
            "$TRANSFORM_CASE_CAPITAL" => Action::TransformCaseCapital,
            "$APPEND_," => Action::AppendComma,
            "$APPEND_." => Action::AppendPeriod,
            "$TRANSFORM_VERB_VB_VBN" => Action::TransformVerbVbVbn,
            "$TRANSFORM_CASE_UPPER" => Action::TransformCaseUpper,
            "$APPEND_:" => Action::AppendColon,
            "$APPEND_?" => Action::AppendQuestion,
            "$TRANSFORM_VERB_VB_VBC" => Action::TransformVerbVbVbc,
            "$TRANSFORM_CASE_LOWER" => Action::TransformCaseLower,
            "$TRANSFORM_CASE_CAPITAL_1" => Action::TransformCaseCapital1,
            "$TRANSFORM_CASE_UPPER_-1" => Action::TransformCaseUpperMinus1,
            "$MERGE_SPACE" => Action::MergeSpace,
            "@@PADDING@@" => Action::Padding,
            _ => Action::Unknown,
        }
    }
}

/// Loads a newline-separated label file (labels.txt or d_tags.txt) and maps each
/// line, in order, to an `Action` via `Action::from_label`. The returned Vec's index
/// corresponds exactly to the model's output class index for that head.
pub fn load_action_labels(path: &Path) -> Result<Vec<Action>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("Failed to read label file {:?}: {}", path, e))?;
    let labels: Vec<Action> = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(Action::from_label)
        .collect();
    if labels.is_empty() {
        return Err(anyhow!("Label file {:?} was empty", path));
    }
    Ok(labels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_real_capu_label_order() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            file,
            "$KEEP\n$TRANSFORM_CASE_CAPITAL\n$APPEND_,\n$APPEND_.\n$TRANSFORM_VERB_VB_VBN\n$TRANSFORM_CASE_UPPER\n$APPEND_:\n$APPEND_?\n$TRANSFORM_VERB_VB_VBC\n$TRANSFORM_CASE_LOWER\n$TRANSFORM_CASE_CAPITAL_1\n$TRANSFORM_CASE_UPPER_-1\n$MERGE_SPACE\n@@UNKNOWN@@\n@@PADDING@@"
        )
        .unwrap();

        let labels = load_action_labels(file.path()).unwrap();

        assert_eq!(labels.len(), 15);
        assert_eq!(labels[0], Action::Keep);
        assert_eq!(labels[2], Action::AppendComma);
        assert_eq!(labels[10], Action::TransformCaseCapital1);
        assert_eq!(labels[11], Action::TransformCaseUpperMinus1);
        assert_eq!(labels[12], Action::MergeSpace);
        assert_eq!(labels[13], Action::Unknown);
        assert_eq!(labels[14], Action::Padding);
    }

    #[test]
    fn unknown_label_string_maps_to_unknown_action() {
        assert_eq!(Action::from_label("$SOMETHING_NEW"), Action::Unknown);
    }
}
```

Note: this references `tempfile`, which is already a `[dev-dependencies]` entry in
`frontend/src-tauri/Cargo.toml` — no new dependency needed.

Also create empty placeholder files so the module compiles (filled in by later tasks):

`frontend/src-tauri/src/capu_engine/edits.rs`:
```rust
// Filled in by Task 4
```

`frontend/src-tauri/src/capu_engine/tokenizer.rs`:
```rust
// Filled in by Task 5
```

`frontend/src-tauri/src/capu_engine/capu_engine.rs`:
```rust
// Filled in by Task 6
pub struct CapuEngine;
```

`frontend/src-tauri/src/capu_engine/commands.rs`:
```rust
// Filled in by Task 7
```

- [ ] **Step 3: Register the module in `lib.rs`**

In `frontend/src-tauri/src/lib.rs`, near the existing `pub mod zipformer_engine;` (line 54), add:

```rust
pub mod capu_engine;
```

- [ ] **Step 4: Run the tests**

Run: `cd frontend/src-tauri && cargo test capu_engine::vocabulary -- --nocapture`
Expected: `loads_real_capu_label_order` and `unknown_label_string_maps_to_unknown_action` both PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/capu_engine frontend/src-tauri/src/lib.rs
git commit -m "feat(capu): add Action vocabulary and label-file loading with tests"
```

---

### Task 4: `edits.rs` — apply actions to a word list (pure logic, no ONNX)

**Files:**
- Modify: `frontend/src-tauri/src/capu_engine/edits.rs`

This is the highest-risk-of-subtle-bugs piece, so it gets the most thorough tests. It implements,
for our closed 15-action set, the equivalent of the reference's `apply_reverse_transformation`
(case transforms) and `get_target_sent_by_edits` (append/merge), specialized to single-word-scoped
edits since every action in this vocabulary applies to exactly one word or one word + its
immediate neighbor (no arbitrary-span edits exist in this vocabulary).

- [ ] **Step 1: Write the failing tests**

`frontend/src-tauri/src/capu_engine/edits.rs`:

```rust
use super::vocabulary::Action;

/// Applies a case-transform action to a single token. Ported verbatim from the
/// reference `convert_using_case()` in the model repo's `utils.py`. Non-case actions
/// return the token unchanged.
pub fn apply_case_transform(token: &str, action: Action) -> String {
    match action {
        Action::TransformCaseLower => token.to_lowercase(),
        Action::TransformCaseUpper => token.to_uppercase(),
        Action::TransformCaseCapital => capitalize(token),
        Action::TransformCaseCapital1 => {
            // token[0] + token[1:].capitalize() in the Python reference:
            // first char untouched, capitalize() applied starting from the second char.
            let mut chars = token.chars();
            match chars.next() {
                Some(first) => {
                    let rest: String = chars.collect();
                    format!("{}{}", first, capitalize(&rest))
                }
                None => token.to_string(),
            }
        }
        Action::TransformCaseUpperMinus1 => {
            // token[:-1].upper() + token[-1] in the Python reference.
            let char_count = token.chars().count();
            if char_count == 0 {
                return token.to_string();
            }
            let mut chars = token.chars();
            let last = chars.next_back().unwrap();
            let head: String = chars.collect();
            format!("{}{}", head.to_uppercase(), last)
        }
        _ => token.to_string(),
    }
}

/// Python's str.capitalize(): first char uppercase, rest lowercase.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => {
            let rest: String = chars.as_str().to_lowercase();
            format!("{}{}", first.to_uppercase(), rest)
        }
        None => String::new(),
    }
}

fn append_char(action: Action) -> Option<char> {
    match action {
        Action::AppendComma => Some(','),
        Action::AppendPeriod => Some('.'),
        Action::AppendColon => Some(':'),
        Action::AppendQuestion => Some('?'),
        _ => None,
    }
}

/// Applies one predicted `Action` per word to reconstruct the corrected word list.
/// `words.len()` must equal `actions.len()`. `$TRANSFORM_VERB_*` actions are a
/// deliberate no-op (see plan notes) — this capu-finetuned model is not expected to
/// predict them; if it does, we log and leave the word unchanged rather than guess.
pub fn apply_actions(words: &[String], actions: &[Action]) -> Vec<String> {
    debug_assert_eq!(words.len(), actions.len());
    let mut output: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if matches!(
            actions[i],
            Action::TransformVerbVbVbn | Action::TransformVerbVbVbc
        ) {
            log::warn!(
                "CAPU predicted an untranslated verb-transform action on word '{}' — leaving unchanged",
                words[i]
            );
        }

        let mut word = apply_case_transform(&words[i], actions[i]);

        if actions[i] == Action::MergeSpace && i + 1 < words.len() {
            let next_word = apply_case_transform(&words[i + 1], actions[i + 1]);
            word.push_str(&next_word);
            if let Some(c) = append_char(actions[i + 1]) {
                word.push(c);
            }
            output.push(word);
            i += 2;
            continue;
        }

        if let Some(c) = append_char(actions[i]) {
            word.push(c);
        }
        output.push(word);
        i += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_leaves_words_unchanged() {
        let words = vec!["xin".to_string(), "chào".to_string()];
        let actions = vec![Action::Keep, Action::Keep];
        assert_eq!(apply_actions(&words, &actions), vec!["xin", "chào"]);
    }

    #[test]
    fn capital_uppercases_first_lowercases_rest() {
        assert_eq!(apply_case_transform("VIỆT", Action::TransformCaseCapital), "Việt");
    }

    #[test]
    fn capital_1_leaves_first_char_untouched() {
        // token[0] + token[1:].capitalize() -> first char kept as-is, second char capitalized
        assert_eq!(
            apply_case_transform("nội", Action::TransformCaseCapital1),
            "nỘi"
        );
    }

    #[test]
    fn upper_minus_1_leaves_last_char_untouched() {
        assert_eq!(
            apply_case_transform("viet", Action::TransformCaseUpperMinus1),
            "VIEt"
        );
    }

    #[test]
    fn append_period_suffixes_the_word() {
        let words = vec!["xong".to_string()];
        let actions = vec![Action::AppendPeriod];
        assert_eq!(apply_actions(&words, &actions), vec!["xong."]);
    }

    #[test]
    fn capitalize_and_append_compose_on_same_word() {
        let words = vec!["chào".to_string(), "bạn".to_string()];
        let actions = vec![Action::TransformCaseCapital, Action::AppendQuestion];
        assert_eq!(apply_actions(&words, &actions), vec!["Chào", "bạn?"]);
    }

    #[test]
    fn merge_space_joins_word_with_next_and_applies_next_actions() {
        let words = vec!["hôm".to_string(), "nay".to_string(), "đẹp".to_string()];
        // MergeSpace on "hôm" merges it with "nay" (which itself gets a period appended)
        let actions = vec![Action::MergeSpace, Action::AppendPeriod, Action::Keep];
        assert_eq!(apply_actions(&words, &actions), vec!["hômnay.", "đẹp"]);
    }

    #[test]
    fn verb_transform_actions_are_untranslated_noop() {
        let words = vec!["đi".to_string()];
        let actions = vec![Action::TransformVerbVbVbn];
        assert_eq!(apply_actions(&words, &actions), vec!["đi"]);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cd frontend/src-tauri && cargo test capu_engine::edits -- --nocapture`
Expected: all 7 tests PASS. (They should pass immediately since we wrote implementation +
tests together here — the point of this step is to catch any typo in the ported case-transform
logic before it's wired to anything else.)

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/capu_engine/edits.rs
git commit -m "feat(capu): port GECToR action application (case transforms, append, merge)"
```

---

### Task 5: `tokenizer.rs` — BERT WordPiece tokenizer + word-offset alignment

**Files:**
- Modify: `frontend/src-tauri/src/capu_engine/tokenizer.rs`

- [ ] **Step 1: Write the failing test**

`frontend/src-tauri/src/capu_engine/tokenizer.rs`:

```rust
use anyhow::{anyhow, Result};
use std::path::Path;
use tokenizers::models::wordpiece::WordPiece;
use tokenizers::normalizers::bert::BertNormalizer;
use tokenizers::pre_tokenizers::bert::BertPreTokenizer;
use tokenizers::processors::bert::BertProcessing;
use tokenizers::{Model, Tokenizer};

/// Encoded representation ready to feed to the ONNX session: `input_offsets` here is
/// the *raw* offsets list (length == words.len() + 2, includes the CLS/SEP sentinel
/// positions) — callers are responsible for skipping the first/last entry when they
/// read per-word predictions back out (see capu_engine.rs).
pub struct CapuEncoding {
    pub input_ids: Vec<i64>,
    pub attention_mask: Vec<i64>,
    pub token_type_ids: Vec<i64>,
    pub input_offsets: Vec<i64>,
}

pub struct CapuTokenizer {
    tokenizer: Tokenizer,
}

impl CapuTokenizer {
    /// Builds a cased BERT WordPiece tokenizer from a raw `vocab.txt` file. Vietnamese
    /// relies heavily on diacritics, so this is explicitly a *cased* tokenizer:
    /// `strip_accents: Some(false)`, `lowercase: false`. These aren't a guess — they
    /// follow directly from the base model being named `vibert-base-cased`.
    pub fn from_vocab_file(vocab_path: &Path) -> Result<Self> {
        let vocab_path_str = vocab_path
            .to_str()
            .ok_or_else(|| anyhow!("Non-UTF8 vocab path: {:?}", vocab_path))?;

        let wordpiece: WordPiece = WordPiece::from_file(vocab_path_str)
            .build()
            .map_err(|e| anyhow!("Failed to build WordPiece from {:?}: {}", vocab_path, e))?;

        let cls_id = wordpiece
            .token_to_id("[CLS]")
            .ok_or_else(|| anyhow!("[CLS] not found in vocab {:?}", vocab_path))?;
        let sep_id = wordpiece
            .token_to_id("[SEP]")
            .ok_or_else(|| anyhow!("[SEP] not found in vocab {:?}", vocab_path))?;

        let mut tokenizer = Tokenizer::new(wordpiece);
        tokenizer.with_normalizer(Some(BertNormalizer::new(true, true, Some(false), false)));
        tokenizer.with_pre_tokenizer(Some(BertPreTokenizer));
        tokenizer.with_post_processor(Some(BertProcessing::new(
            ("[SEP]".to_string(), sep_id),
            ("[CLS]".to_string(), cls_id),
        )));

        Ok(Self { tokenizer })
    }

    /// Encodes a list of whitespace-delimited words into model inputs, computing
    /// `input_offsets` by replicating the reference `gec_model.py` logic: append the
    /// token index every time `word_ids()` changes value versus the previous token.
    pub fn encode_words(&self, words: &[String]) -> Result<CapuEncoding> {
        let text = words.join(" ");
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow!("Tokenization failed: {}", e))?;

        let word_ids = encoding.get_word_ids();
        let mut input_offsets: Vec<i64> = vec![0];
        for i in 1..word_ids.len() {
            if word_ids[i] != word_ids[i - 1] {
                input_offsets.push(i as i64);
            }
        }

        let expected_len = words.len() + 2;
        if input_offsets.len() != expected_len {
            return Err(anyhow!(
                "Offset alignment mismatch: got {} offsets for {} words (expected {})",
                input_offsets.len(),
                words.len(),
                expected_len
            ));
        }

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> = vec![1; input_ids.len()];
        let token_type_ids: Vec<i64> = vec![0; input_ids.len()];

        Ok(CapuEncoding {
            input_ids,
            attention_mask,
            token_type_ids,
            input_offsets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_test_vocab() -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        // [PAD]=0 [UNK]=1 [CLS]=2 [SEP]=3 [MASK]=4, then real (word, subword) tokens.
        // "vietnam" isn't whole-word in vocab, forcing a viet/##nam split so the test
        // exercises multi-subword offset alignment, not just the trivial 1:1 case.
        writeln!(
            file,
            "[PAD]\n[UNK]\n[CLS]\n[SEP]\n[MASK]\nxin\nchào\nviet\n##nam"
        )
        .unwrap();
        file
    }

    #[test]
    fn offsets_align_one_per_word_including_multi_subword_word() {
        let vocab = write_test_vocab();
        let tok = CapuTokenizer::from_vocab_file(vocab.path()).unwrap();

        let words = vec!["xin".to_string(), "chào".to_string(), "vietnam".to_string()];
        let encoding = tok.encode_words(&words).unwrap();

        // [CLS] xin chào viet ##nam [SEP] -> word_ids [None,0,1,2,2,None]
        // offsets computed by "append on word_id change": [0, 1, 2, 3, 5]
        assert_eq!(encoding.input_offsets, vec![0, 1, 2, 3, 5]);
        assert_eq!(encoding.input_offsets.len(), words.len() + 2);
        assert_eq!(encoding.attention_mask.len(), encoding.input_ids.len());
        assert_eq!(encoding.token_type_ids, vec![0; encoding.input_ids.len()]);
    }

    #[test]
    fn unknown_word_falls_back_to_unk_without_erroring() {
        let vocab = write_test_vocab();
        let tok = CapuTokenizer::from_vocab_file(vocab.path()).unwrap();

        let words = vec!["xin".to_string(), "gibberishword".to_string()];
        let encoding = tok.encode_words(&words).unwrap();

        assert_eq!(encoding.input_offsets.len(), words.len() + 2);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cd frontend/src-tauri && cargo test capu_engine::tokenizer -- --nocapture`
Expected: both tests PASS. If `offsets_align_one_per_word_including_multi_subword_word` fails with
a different offset list than `[0, 1, 2, 3, 5]`, do not "fix" the test to match — that means either
the vocab fixture or the `tokenizers` crate's BERT pre-tokenizer/normalizer behaves differently than
assumed here, and the offset-computation logic (which is a verbatim port of the reference) needs to
be re-checked against the actual `word_ids()` values first (add a temporary
`eprintln!("{:?}", word_ids)` to see what's really being produced).

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/capu_engine/tokenizer.rs
git commit -m "feat(capu): add cased BERT WordPiece tokenizer with word-offset alignment"
```

---

### Task 6: `capu_engine.rs` — ONNX session, iterative GECToR loop, trailing-context API

**Files:**
- Modify: `frontend/src-tauri/src/capu_engine/capu_engine.rs`

This task wires the tokenizer (Task 5) and action vocabulary (Task 3/4) to an actual `ort` ONNX
session. It cannot be meaningfully unit-tested without a real model file, so there's no TDD step
here — correctness is instead verified end-to-end in Task 11 after the model is downloaded. Keep
the pure-logic pieces (offset skipping, argmax, iteration stop condition) written as small private
functions so they stay easy to reason about even though they aren't unit-tested in isolation here.

- [ ] **Step 1: Implement the engine**

`frontend/src-tauri/src/capu_engine/capu_engine.rs`:

```rust
use super::edits::apply_actions;
use super::tokenizer::CapuTokenizer;
use super::vocabulary::{load_action_labels, Action};
use crate::config::{CAPU_MAX_ITERATIONS, CAPU_MAX_SEQ_LEN, CAPU_TRAILING_CONTEXT_WORDS};
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

pub struct CapuEngine {
    session: Session,
    tokenizer: CapuTokenizer,
    labels: Vec<Action>,
}

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

    /// Runs one forward pass over `words` and returns one `Action` per word (already
    /// skipping the CLS/SEP sentinel offsets — see the offset-handling note at the top
    /// of this plan).
    fn infer_once(&mut self, words: &[String]) -> Result<Vec<Action>> {
        let encoding = self.tokenizer.encode_words(words)?;
        if encoding.input_ids.len() > CAPU_MAX_SEQ_LEN {
            return Err(anyhow!(
                "Tokenized sequence too long ({} > {}); caller should truncate words",
                encoding.input_ids.len(),
                CAPU_MAX_SEQ_LEN
            ));
        }

        let seq_len = encoding.input_ids.len();
        let num_offsets = encoding.input_offsets.len();

        let ids_tensor = TensorRef::from_array_view(([1usize, seq_len], &*encoding.input_ids))
            .map_err(|e| anyhow!("Failed to build input_ids tensor: {}", e))?;
        let mask_tensor =
            TensorRef::from_array_view(([1usize, seq_len], &*encoding.attention_mask))
                .map_err(|e| anyhow!("Failed to build attention_mask tensor: {}", e))?;
        let type_tensor =
            TensorRef::from_array_view(([1usize, seq_len], &*encoding.token_type_ids))
                .map_err(|e| anyhow!("Failed to build token_type_ids tensor: {}", e))?;
        let offsets_tensor =
            TensorRef::from_array_view(([1usize, num_offsets], &*encoding.input_offsets))
                .map_err(|e| anyhow!("Failed to build input_offsets tensor: {}", e))?;

        let outputs = self
            .session
            .run(ort::inputs![ids_tensor, mask_tensor, type_tensor, offsets_tensor])
            .map_err(|e| anyhow!("ONNX inference failed: {}", e))?;

        let (logits_shape, logits_data) = outputs["logits"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("Failed to read logits output: {}", e))?;

        let num_classes = self.labels.len();
        if logits_shape.len() != 3 || logits_shape[2] as usize != num_classes {
            return Err(anyhow!(
                "Unexpected logits shape {:?} (expected [1, num_offsets, {}])",
                logits_shape,
                num_classes
            ));
        }

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

        if actions.len() != words.len() {
            return Err(anyhow!(
                "Decoded {} actions for {} words — offset/word mismatch",
                actions.len(),
                words.len()
            ));
        }

        Ok(actions)
    }

    /// Runs the GECToR iterative correction loop (max `CAPU_MAX_ITERATIONS` passes,
    /// stopping early once a pass predicts `$KEEP` for every word).
    fn restore_words(&mut self, mut words: Vec<String>) -> Result<Vec<String>> {
        for _ in 0..CAPU_MAX_ITERATIONS {
            if words.is_empty() {
                break;
            }
            let actions = self.infer_once(&words)?;
            if actions.iter().all(|a| *a == Action::Keep) {
                break;
            }
            words = apply_actions(&words, &actions);
        }
        Ok(words)
    }

    /// Restores punctuation/capitalization for `new_text`, using `trailing_context`
    /// (raw words from the tail of the previously processed segment) as left-context so
    /// the model has a chance to see across VAD segment boundaries. Returns the
    /// restored text for `new_text` only (context words are stripped back out) plus the
    /// trailing-context words to pass on the next call.
    pub fn restore_punctuation(
        &mut self,
        trailing_context: &[String],
        new_text: &str,
    ) -> Result<(String, Vec<String>)> {
        let new_words: Vec<String> = new_text.split_whitespace().map(str::to_string).collect();
        if new_words.is_empty() {
            return Ok((String::new(), trailing_context.to_vec()));
        }

        let combined: Vec<String> = trailing_context
            .iter()
            .cloned()
            .chain(new_words.iter().cloned())
            .collect();

        let restored = self.restore_words(combined)?;

        // MERGE_SPACE can reduce word count, so recover the new-segment tail by count
        // from the end rather than assuming a fixed offset from the start.
        let take_from = restored.len().saturating_sub(new_words.len());
        let result_text = restored[take_from..].join(" ");

        let context_start = new_words.len().saturating_sub(CAPU_TRAILING_CONTEXT_WORDS);
        let next_context = new_words[context_start..].to_vec();

        Ok((result_text, next_context))
    }
}
```

- [ ] **Step 2: Verify it builds**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles. Pay attention to the exact `ort` 2.0.0-rc.13 API surface — `Session::builder()`,
`commit_from_file`, `TensorRef::from_array_view`, `session.run(ort::inputs![...])`, and indexing
`SessionOutputs` by name (`outputs["logits"]`) were all confirmed against `ort`'s own
`examples/sentence-transformers` example and API docs at plan-writing time, but this is a
pre-1.0 (`-rc`) crate — if a method name has shifted slightly in whatever patch version `cargo`
actually resolves, fix the call site to match rather than downgrading the crate, and note what
changed in the commit message.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/capu_engine/capu_engine.rs
git commit -m "feat(capu): implement ONNX inference, GECToR iteration loop, trailing-context API"
```

---

### Task 7: `commands.rs` — Tauri commands (download, status, init)

**Files:**
- Modify: `frontend/src-tauri/src/capu_engine/commands.rs`

Mirrors `frontend/src-tauri/src/zipformer_engine/commands.rs` exactly: a process-wide
`Mutex<Option<Arc<Mutex<CapuEngine>>>>` (the inner `Mutex` is needed because `ort::Session::run`
takes `&mut self`, and the engine is shared across the async worker task and Tauri commands), a
models-directory resolver reusing the same `app_data_dir()/models` base as ZipFormer, and a
streaming downloader copied from `ZipFormerEngine::download_model`'s pattern.

- [ ] **Step 1: Implement commands**

`frontend/src-tauri/src/capu_engine/commands.rs`:

```rust
use super::CapuEngine;
use crate::config::{
    CAPU_DTAGS_FILE, CAPU_DTAGS_SIZE_BYTES, CAPU_HF_URL, CAPU_LABELS_FILE, CAPU_LABELS_SIZE_BYTES,
    CAPU_MODEL_FILE, CAPU_MODEL_SIZE_BYTES, CAPU_SUBDIR, CAPU_VOCAB_FILE, CAPU_VOCAB_SIZE_BYTES,
};
use futures_util::StreamExt;
use log::{error, info};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::io::AsyncWriteExt;

pub(crate) static CAPU_ENGINE: Mutex<Option<Arc<Mutex<CapuEngine>>>> = Mutex::new(None);

fn resolve_capu_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("models").join(CAPU_SUBDIR))
}

fn capu_files() -> [(&'static str, u64); 4] {
    [
        (CAPU_MODEL_FILE, CAPU_MODEL_SIZE_BYTES),
        (CAPU_VOCAB_FILE, CAPU_VOCAB_SIZE_BYTES),
        (CAPU_LABELS_FILE, CAPU_LABELS_SIZE_BYTES),
        (CAPU_DTAGS_FILE, CAPU_DTAGS_SIZE_BYTES),
    ]
}

#[tauri::command]
pub async fn capu_get_models_directory<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    resolve_capu_dir(&app)
        .map(|d| d.to_string_lossy().to_string())
        .ok_or_else(|| "Could not resolve app data directory".to_string())
}

#[tauri::command]
pub async fn capu_is_model_downloaded<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    let dir = resolve_capu_dir(&app).ok_or_else(|| "Could not resolve app data directory".to_string())?;
    Ok(capu_files()
        .iter()
        .all(|(name, _)| dir.join(name).exists()))
}

#[tauri::command]
pub async fn capu_download_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = resolve_capu_dir(&app).ok_or_else(|| "Could not resolve app data directory".to_string())?;
    let app_clone = app.clone();

    tauri::async_runtime::spawn(async move {
        match download_capu_files(&dir, &app_clone).await {
            Ok(()) => {
                info!("CAPU model download complete");
                let _ = app_clone.emit("capu-model-download-complete", ());
            }
            Err(e) => {
                error!("CAPU model download failed: {}", e);
                let _ = app_clone.emit(
                    "capu-model-download-error",
                    serde_json::json!({ "error": e.to_string() }),
                );
            }
        }
    });

    Ok(())
}

async fn download_capu_files<R: Runtime>(dir: &PathBuf, app: &AppHandle<R>) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    tokio::fs::create_dir_all(dir.join("vocabulary")).await?;

    let files = capu_files();
    let total_bytes: u64 = files.iter().map(|(_, size)| size).sum();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .build()?;

    let mut bytes_downloaded: u64 = 0;
    let mut last_reported: u8 = 0;

    for (filename, size) in files.iter() {
        let dest = dir.join(filename);
        if dest.exists() {
            bytes_downloaded += size;
            continue;
        }
        let tmp = dir.join(format!("{}.tmp", filename.replace('/', "_")));
        let url = format!("{}/{}", CAPU_HF_URL, filename);

        let response = client.get(&url).send().await?;
        if !response.status().is_success() {
            anyhow::bail!("HTTP {} for {}", response.status(), filename);
        }

        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&tmp).await?;
        let mut file_bytes: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            file_bytes += chunk.len() as u64;

            let cumulative = bytes_downloaded.saturating_add(file_bytes);
            let overall = ((cumulative * 100) / total_bytes.max(1)).min(99) as u8;
            if overall > last_reported {
                last_reported = overall;
                let _ = app.emit(
                    "capu-model-download-progress",
                    serde_json::json!({ "progress": overall }),
                );
            }
        }
        file.flush().await?;
        drop(file);
        tokio::fs::rename(&tmp, &dest).await?;
        bytes_downloaded += file_bytes;
    }

    let _ = app.emit(
        "capu-model-download-progress",
        serde_json::json!({ "progress": 100 }),
    );
    Ok(())
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
        return Err("CAPU model not downloaded. Please download it from Settings → Transcription.".to_string());
    }

    let engine = CapuEngine::load(&model_path, &vocab_path, &labels_path)
        .map_err(|e| e.to_string())?;

    let mut guard = CAPU_ENGINE.lock().unwrap();
    *guard = Some(Arc::new(Mutex::new(engine)));
    info!("CAPU engine initialized");
    Ok(())
}

/// Used internally by `worker.rs` and `retranscription.rs` — not a Tauri command.
pub(crate) fn get_engine_arc() -> Option<Arc<Mutex<CapuEngine>>> {
    CAPU_ENGINE.lock().unwrap().as_ref().cloned()
}
```

- [ ] **Step 2: Verify it builds**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles. `futures_util::StreamExt` is already a dependency (`futures-util = "0.3"` in
Cargo.toml, used by the ZipFormer downloader).

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/capu_engine/commands.rs
git commit -m "feat(capu): add Tauri commands for CAPU model download/init/status"
```

---

### Task 8: Wire CAPU into `lib.rs`

**Files:**
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Register the invoke handlers**

In `frontend/src-tauri/src/lib.rs`, right after the existing ZipFormer command block (after line
470, `zipformer_engine::commands::zipformer_get_variant_status,`), add:

```rust
            // CAPU Vietnamese punctuation restoration commands
            capu_engine::commands::capu_get_models_directory,
            capu_engine::commands::capu_is_model_downloaded,
            capu_engine::commands::capu_download_model,
            capu_engine::commands::capu_init,
```

- [ ] **Step 2: Verify it builds**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/lib.rs
git commit -m "feat(capu): register CAPU Tauri commands in invoke_handler"
```

---

### Task 9: Integrate into `worker.rs` (real-time, with trailing-context)

**Files:**
- Modify: `frontend/src-tauri/src/audio/transcription/worker.rs`

- [ ] **Step 1: Add the trailing-context buffer and reset function**

In `frontend/src-tauri/src/audio/transcription/worker.rs`, near the existing statics (after line
18, `static SPEECH_DETECTED_EMITTED: AtomicBool = AtomicBool::new(false);`), add:

```rust
// Trailing-context words carried across VAD segments within one recording session,
// so CAPU punctuation restoration has left-context across segment boundaries.
static CAPU_TRAILING_CONTEXT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Reset the CAPU trailing-context buffer for a new recording session.
pub fn reset_capu_context() {
    CAPU_TRAILING_CONTEXT.lock().unwrap().clear();
}
```

- [ ] **Step 2: Call the punctuation restoration before building `TranscriptUpdate`**

In the same file, the block that currently reads (around line 226-240):

```rust
                                        // Emit transcript update with NEW recording-relative timestamps

                                        let update = TranscriptUpdate {
                                            text: transcript,
                                            timestamp: format_current_timestamp(), // Wall-clock for reference
```

Change it to:

```rust
                                        // Restore Vietnamese punctuation/capitalization before emitting.
                                        // Falls back to the raw transcript on any failure (model not
                                        // downloaded/loaded, inference error) — never blocks the pipeline.
                                        let punctuated_text = match crate::capu_engine::commands::get_engine_arc() {
                                            Some(engine_arc) => {
                                                let trailing = CAPU_TRAILING_CONTEXT.lock().unwrap().clone();
                                                let mut engine = engine_arc.lock().unwrap();
                                                match engine.restore_punctuation(&trailing, &transcript) {
                                                    Ok((restored, next_context)) => {
                                                        *CAPU_TRAILING_CONTEXT.lock().unwrap() = next_context;
                                                        restored
                                                    }
                                                    Err(e) => {
                                                        error!("Worker {}: CAPU punctuation restoration failed: {}", worker_id, e);
                                                        transcript.clone()
                                                    }
                                                }
                                            }
                                            None => transcript.clone(),
                                        };

                                        // Emit transcript update with NEW recording-relative timestamps

                                        let update = TranscriptUpdate {
                                            text: punctuated_text,
                                            timestamp: format_current_timestamp(), // Wall-clock for reference
```

Note: `engine.lock().unwrap()` is a synchronous `std::sync::Mutex` lock held across the (blocking,
CPU-bound) ONNX inference call — this happens on a `tokio::spawn`ed task, and since `NUM_WORKERS =
1` (confirmed at `worker.rs:67`, "Serial processing ensures transcripts emit in chronological
order"), there is no contention on this lock from other transcription workers. If `NUM_WORKERS` is
ever raised above 1 in the future, revisit this — a `std::sync::Mutex` held across inference would
then serialize what was meant to be parallel work.

- [ ] **Step 3: Reset the CAPU context alongside the existing speech-detected reset**

In `frontend/src-tauri/src/audio/recording_commands.rs`, at both call sites of
`reset_speech_detected_flag();` (lines 308 and 527), add the CAPU reset right after:

```rust
    reset_speech_detected_flag();
    crate::audio::transcription::worker::reset_capu_context();
```

- [ ] **Step 4: Verify it builds**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/transcription/worker.rs frontend/src-tauri/src/audio/recording_commands.rs
git commit -m "feat(capu): hook CAPU punctuation restoration into live transcription worker"
```

---

### Task 10: Integrate into `retranscription.rs`

**Files:**
- Modify: `frontend/src-tauri/src/audio/retranscription.rs`

- [ ] **Step 1: Restore punctuation per segment, reusing the same engine**

In `frontend/src-tauri/src/audio/retranscription.rs`, the segment loop currently reads (around
lines 253-285):

```rust
    let mut all_transcripts: Vec<(String, f64, f64)> = Vec::new();

    for (i, segment) in processable_segments.iter().enumerate() {
        ...
        let text = engine
            .transcribe_audio(segment.samples.clone())
            .await
            .map_err(|e| anyhow!("ZipFormer transcription failed on segment {}: {}", i, e))?;

        let trimmed = text.trim();
        if !trimmed.is_empty() {
            debug!("Segment {}/{}: {:.1}s — '{}'", i + 1, processable_count, segment_duration_sec, trimmed);
            all_transcripts.push((text, segment.start_timestamp_ms, segment.end_timestamp_ms));
        }
    }
```

Change it to:

```rust
    let capu_engine_arc = crate::capu_engine::commands::get_engine_arc();
    let mut capu_trailing_context: Vec<String> = Vec::new();

    let mut all_transcripts: Vec<(String, f64, f64)> = Vec::new();

    for (i, segment) in processable_segments.iter().enumerate() {
        ...
        let text = engine
            .transcribe_audio(segment.samples.clone())
            .await
            .map_err(|e| anyhow!("ZipFormer transcription failed on segment {}: {}", i, e))?;

        let trimmed = text.trim();
        if !trimmed.is_empty() {
            debug!("Segment {}/{}: {:.1}s — '{}'", i + 1, processable_count, segment_duration_sec, trimmed);

            let punctuated = match &capu_engine_arc {
                Some(capu) => {
                    let mut capu = capu.lock().unwrap();
                    match capu.restore_punctuation(&capu_trailing_context, &text) {
                        Ok((restored, next_context)) => {
                            capu_trailing_context = next_context;
                            restored
                        }
                        Err(e) => {
                            warn!("CAPU punctuation restoration failed on segment {}: {}", i, e);
                            text.clone()
                        }
                    }
                }
                None => text.clone(),
            };

            all_transcripts.push((punctuated, segment.start_timestamp_ms, segment.end_timestamp_ms));
        }
    }
```

(The `...` above stands for the unchanged lines already in the file — the cancellation check,
progress emission, and short-segment skip; only the `text`-to-`all_transcripts.push` tail changes.)

- [ ] **Step 2: Verify it builds**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/audio/retranscription.rs
git commit -m "feat(capu): reuse CAPU punctuation restoration in meeting retranscription"
```

---

### Task 11: Manual end-to-end verification

This feature cannot be meaningfully verified by automated tests alone (it needs the real 110MB
ONNX model and a human judgment call on output quality). Do this manually before considering the
feature done.

- [ ] **Step 1: Full build**

Run: `cd frontend/src-tauri && cargo build`
Expected: builds successfully in debug mode.

- [ ] **Step 2: Download the CAPU model**

Start the app (`pnpm run tauri:dev` from `frontend/`), open the developer console, and invoke:
```js
await window.__TAURI__.core.invoke('capu_download_model')
```
Listen for `capu-model-download-progress` / `capu-model-download-complete` events (or poll
`capu_is_model_downloaded`). Expected: 4 files land under
`<app data dir>/models/capu-vi/` (`vibert-capu.int8.onnx`, `vocab.txt`,
`vocabulary/labels.txt`, `vocabulary/d_tags.txt`).

- [ ] **Step 3: Initialize the engine**

Invoke `await window.__TAURI__.core.invoke('capu_init')`. Expected: resolves without error; Rust
log shows `CAPU engine initialized`.

- [ ] **Step 4: Record a short Vietnamese conversation**

Start a recording, speak a few Vietnamese sentences with natural pauses (so VAD produces multiple
segments), stop recording. Expected: live transcript segments show periods/commas/question marks
and capitalized sentence starts, not raw lowercase run-on text. Check the Rust log for any `CAPU
punctuation restoration failed` warnings — there should be none in the happy path.

- [ ] **Step 5: Re-transcribe an existing meeting**

Pick a meeting recorded before this feature existed (or the one from Step 4), trigger
retranscription from the UI. Expected: the resulting transcript also shows punctuation, and reads
consistently with what live transcription would have produced for the same audio.

- [ ] **Step 6: Note quality issues, don't silently tune around them**

If punctuation looks obviously wrong in a specific way (e.g., periods missing at real sentence
ends, or spurious mid-sentence periods), that's the moment to reconsider the "no `detect_logits`
gating" simplification from Task 6 — not before, and not by guessing at a threshold value. If it
looks reasonable, this task — and the feature — is done.
