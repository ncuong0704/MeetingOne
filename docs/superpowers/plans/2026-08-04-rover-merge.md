# ROVER Merge Implementation Plan (Phase B)

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking. Execute tasks in order; each task ends with a commit. Do not skip "run and verify" steps.

**Goal:** Build `rover_engine`, a module that runs two `RnntDecoder`s (from Phase A) concurrently on the same audio and merges their word sequences into one result by confidence — the same algorithm as the reference app's `rover_merge_words`, minus the hotword bonus this project has no use for.

**Architecture:** Three focused files under `frontend/src-tauri/src/rover_engine/`: `normalize.rs` (word normalization for comparison — lowercase, NFC, strip punctuation), `merge.rs` (the alignment + confidence-based selection, a pure function with no ONNX dependency — the highest-value unit-testable piece in this plan), `engine.rs` (`RoverDecoder`, which owns two `RnntDecoder`s, runs them in parallel via `std::thread::scope`, and calls the merge). Nothing in the app calls `RoverDecoder` yet — verified standalone, same posture as Phase A. Phase C (a separate spec) wires it into settings/UI/the recording pipeline.

**Tech Stack:** Rust, `similar` 3.x (Myers diff, verified against its actual source — `DiffOp`/`DiffTag`/`capture_diff_slices`), `unicode-normalization` 0.1 (NFC), plus everything Phase A already added (`rnnt_decoder`, `ort`, `kaldi-native-fbank`).

**Reference spec:** `docs/superpowers/specs/2026-08-04-rover-merge-design.md`

**Verified before writing this plan:** Phase A's `RnntDecoder` was run end-to-end against real downloaded models for all three families (ZipFormer 30M int8, Gipformer 65M int8, Sherpa VI 2025 full) on the same real Vietnamese audio clip. All three produced the identical 22-word sentence. One real bug was found and fixed along the way: the joiner's ONNX output is named `logit` (singular), not `logits` — already corrected in `frontend/src-tauri/src/rnnt_decoder/sessions.rs`. `CONTEXT_SIZE = 2` and `BLANK_ID = 0` are confirmed correct for all three families (checked directly via each model's ONNX graph I/O, not assumed).

---

## File map

| File | Responsibility |
|---|---|
| `frontend/src-tauri/Cargo.toml` | Add `similar` and `unicode-normalization` dependencies |
| `frontend/src-tauri/src/rnnt_decoder/engine.rs` | Add `#[derive(Clone)]` to `WordResult` (Phase B needs to copy words out of borrowed slices) |
| `frontend/src-tauri/src/rover_engine/mod.rs` | Module exports |
| `frontend/src-tauri/src/rover_engine/normalize.rs` | Word normalization for comparison |
| `frontend/src-tauri/src/rover_engine/merge.rs` | `rover_merge_words` — the alignment/selection algorithm |
| `frontend/src-tauri/src/rover_engine/engine.rs` | `RoverDecoder` facade — parallel decode + merge |
| `frontend/src-tauri/src/lib.rs` | Register `pub mod rover_engine;` |

No frontend files change. No database migration. No existing call site is wired to this module yet.

---

### Task 1: Dependencies and the `WordResult: Clone` retrofit

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`
- Modify: `frontend/src-tauri/src/rnnt_decoder/engine.rs`

- [ ] **Step 1: Add dependencies**

In `frontend/src-tauri/Cargo.toml`, find:

```toml
ort = "2.0.0-rc.10"
tokenizers = "0.23"
kaldi-native-fbank = "0.1"
```

Replace with:

```toml
ort = "2.0.0-rc.10"
tokenizers = "0.23"
kaldi-native-fbank = "0.1"
similar = "3"
unicode-normalization = "0.1"
```

- [ ] **Step 2: Add `Clone` to `WordResult`**

In `frontend/src-tauri/src/rnnt_decoder/engine.rs`, find:

```rust
pub struct WordResult {
```

Replace with:

```rust
#[derive(Clone)]
pub struct WordResult {
```

This is safe and behavior-preserving — `WordResult` only contains `String` and `f32` fields, both trivially `Clone`. Phase A's own tests are unaffected.

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/Cargo.toml frontend/src-tauri/Cargo.lock frontend/src-tauri/src/rnnt_decoder/engine.rs
git commit -m "build: add similar and unicode-normalization deps; make WordResult Clone for ROVER merge"
```

---

### Task 2: Word normalization (`normalize.rs`)

**Files:**
- Create: `frontend/src-tauri/src/rover_engine/mod.rs`
- Create: `frontend/src-tauri/src/rover_engine/normalize.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `frontend/src-tauri/src/rover_engine/normalize.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_strips_whitespace() {
        assert_eq!(normalize_word("  HAI  "), "hai");
    }

    #[test]
    fn strips_punctuation() {
        assert_eq!(normalize_word("dấu?"), "dấu");
        assert_eq!(normalize_word("chào,"), "chào");
    }

    #[test]
    fn nfc_normalizes_combining_diacritics() {
        // "ế" as a single precomposed codepoint (U+1EBF) vs "e" + combining
        // circumflex (U+0302) + combining acute (U+0301) must normalize equal —
        // ASR output and hand-typed test strings can differ in which form they use.
        let precomposed = "\u{1EBF}"; // "ế"
        let decomposed = "e\u{0302}\u{0301}"; // "e" + combining circumflex + combining acute
        assert_eq!(normalize_word(precomposed), normalize_word(decomposed));
    }

    #[test]
    fn empty_string_normalizes_to_empty() {
        assert_eq!(normalize_word(""), "");
        assert_eq!(normalize_word("   "), "");
    }
}
```

- [ ] **Step 2: Create `mod.rs` and register the module**

Create `frontend/src-tauri/src/rover_engine/mod.rs`:

```rust
pub mod normalize;
```

In `frontend/src-tauri/src/lib.rs`, find where `pub mod rnnt_decoder;` was added (Phase A) and add nearby:

```rust
pub mod rover_engine;
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd frontend/src-tauri && cargo test rover_engine::normalize -- --nocapture`
Expected: compile error — `normalize_word` doesn't exist yet.

- [ ] **Step 4: Implement**

Prepend this to `frontend/src-tauri/src/rover_engine/normalize.rs`, above the `#[cfg(test)]` block:

```rust
use unicode_normalization::UnicodeNormalization;

/// Normalizes a word for cross-model comparison: lowercase, trim, NFC-normalize
/// (so precomposed and decomposed Vietnamese diacritics compare equal), then keep
/// only alphanumeric characters. Mirrors `normalize_word_for_overlap` in the
/// reference app.
pub fn normalize_word(word: &str) -> String {
    let lowered = word.to_lowercase();
    let trimmed = lowered.trim();
    let nfc_normalized: String = trimmed.nfc().collect();
    nfc_normalized.chars().filter(|c| c.is_alphanumeric()).collect()
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd frontend/src-tauri && cargo test rover_engine::normalize -- --nocapture`
Expected: 4 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/rover_engine/
git add frontend/src-tauri/src/lib.rs
git commit -m "feat(rover): add word normalization for cross-model comparison"
```

---

### Task 3: The merge algorithm (`merge.rs`)

**Files:**
- Create: `frontend/src-tauri/src/rover_engine/merge.rs`
- Modify: `frontend/src-tauri/src/rover_engine/mod.rs`

This is the core of Phase B and the most valuable test coverage in this plan — a pure function over two `Vec<WordResult>`, no ONNX, no model files needed to test any of its branches.

Confirmed against the real `similar` 3.x source (not guessed): `capture_diff_slices::<T: Eq + Hash>(Algorithm::Myers, old: &[T], new: &[T]) -> Vec<DiffOp>`, and `DiffOp::as_tag_tuple(&self) -> (DiffTag, Range<usize>, Range<usize>)` where the two ranges index into `old`/`new` respectively — this is a direct match for the reference app's `difflib.SequenceMatcher(...).get_opcodes()` (`tag, i1, i2, j1, j2`) that `rover_merge_words` is built on.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src-tauri/src/rover_engine/merge.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start: f32, confidence: f32) -> WordResult {
        WordResult {
            text: text.to_string(),
            start,
            end: start,
            margin_min: confidence,
            tsallis_max: 0.0,
            confidence,
        }
    }

    #[test]
    fn full_agreement_keeps_a_with_no_disagreement() {
        let a = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.9), word("ba", 1.0, 0.9)];
        let b = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.9), word("ba", 1.0, 0.9)];

        let merged = rover_merge_words(&a, &b);

        assert_eq!(merged.len(), 3);
        let texts: Vec<&str> = merged.iter().map(|m| m.word.text.as_str()).collect();
        assert_eq!(texts, vec!["một", "hai", "ba"]);
        assert!(merged.iter().all(|m| !m.disagree));
    }

    #[test]
    fn replace_keeps_a_when_a_more_confident() {
        let a = vec![word("một", 0.0, 0.95)];
        let b = vec![word("mốt", 0.0, 0.40)];

        let merged = rover_merge_words(&a, &b);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].word.text, "một");
        assert!(!merged[0].disagree);
    }

    #[test]
    fn replace_picks_b_when_b_more_confident_and_marks_disagreement() {
        let a = vec![word("một", 0.0, 0.30)];
        let b = vec![word("mốt", 0.0, 0.92)];

        let merged = rover_merge_words(&a, &b);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].word.text, "mốt");
        assert!(merged[0].disagree);
    }

    #[test]
    fn insert_above_threshold_is_included() {
        let a = vec![word("một", 0.0, 0.9), word("ba", 1.0, 0.9)];
        let b = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.50), word("ba", 1.0, 0.9)];

        let merged = rover_merge_words(&a, &b);

        let texts: Vec<&str> = merged.iter().map(|m| m.word.text.as_str()).collect();
        assert_eq!(texts, vec!["một", "hai", "ba"]);
        assert!(merged.iter().find(|m| m.word.text == "hai").unwrap().disagree);
    }

    #[test]
    fn insert_below_threshold_is_dropped() {
        let a = vec![word("một", 0.0, 0.9), word("ba", 1.0, 0.9)];
        let b = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.05), word("ba", 1.0, 0.9)];

        let merged = rover_merge_words(&a, &b);

        let texts: Vec<&str> = merged.iter().map(|m| m.word.text.as_str()).collect();
        assert_eq!(texts, vec!["một", "ba"]);
    }

    #[test]
    fn near_duplicate_insert_supplement_is_deduped() {
        // "hai" already present via A/Equal at t=0.50; B supplies the same
        // normalized word 0.05s away via a spurious Insert — must not double up.
        let a = vec![word("một", 0.0, 0.9), word("hai", 0.50, 0.9), word("ba", 1.0, 0.9)];
        let b = vec![
            word("một", 0.0, 0.9),
            word("hai", 0.50, 0.9),
            word("hai", 0.55, 0.60), // spurious near-duplicate
            word("ba", 1.0, 0.9),
        ];

        let merged = rover_merge_words(&a, &b);

        let hai_count = merged.iter().filter(|m| m.word.text == "hai").count();
        assert_eq!(hai_count, 1, "duplicate 'hai' supplement should have been deduped");
    }

    #[test]
    fn empty_a_takes_all_of_b_above_threshold() {
        let a: Vec<WordResult> = vec![];
        let b = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.05)];

        let merged = rover_merge_words(&a, &b);

        let texts: Vec<&str> = merged.iter().map(|m| m.word.text.as_str()).collect();
        assert_eq!(texts, vec!["một"]);
    }

    #[test]
    fn empty_b_keeps_all_of_a() {
        let a = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.9)];
        let b: Vec<WordResult> = vec![];

        let merged = rover_merge_words(&a, &b);

        assert_eq!(merged.len(), 2);
        assert!(merged.iter().all(|m| !m.disagree));
    }

    #[test]
    fn both_empty_returns_empty() {
        let merged = rover_merge_words(&[], &[]);
        assert!(merged.is_empty());
    }
}
```

- [ ] **Step 2: Register the module**

In `frontend/src-tauri/src/rover_engine/mod.rs`, replace:

```rust
pub mod normalize;
```

with:

```rust
pub mod merge;
pub mod normalize;
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd frontend/src-tauri && cargo test rover_engine::merge -- --nocapture`
Expected: compile error — `rover_merge_words`, `WordResult` (unimported), `MergedWord` don't exist yet in this file.

- [ ] **Step 4: Implement**

Prepend this to `frontend/src-tauri/src/rover_engine/merge.rs`, above the `#[cfg(test)]` block:

```rust
use crate::rnnt_decoder::engine::WordResult;
use crate::rover_engine::normalize::normalize_word;
use similar::{capture_diff_slices, Algorithm, DiffTag};

/// A `B`-only word is only accepted into the merge if its own confidence clears
/// this bar — matches the reference app's `_word_confidence(wb) > 0.20` check.
const INSERT_CONFIDENCE_THRESHOLD: f32 = 0.20;
/// Two words within this many seconds of each other, with the same normalized
/// text, are treated as the same word for dedup purposes.
const DEDUP_TIME_WINDOW_SECONDS: f32 = 0.15;

pub struct MergedWord {
    pub word: WordResult,
    /// True if this word came from B overriding A in a Replace block, or from a
    /// B-only Insert. False for anything both models agreed on, or anything kept
    /// from A by default (Equal, Delete, or a Replace A won).
    pub disagree: bool,
}

struct Candidate {
    word: WordResult,
    disagree: bool,
    is_supplement: bool,
}

fn block_confidence(words: &[WordResult]) -> f32 {
    if words.is_empty() {
        return 0.0;
    }
    let sum: f32 = words.iter().map(|w| w.confidence).sum();
    sum / words.len() as f32
}

/// Aligns two independently-decoded word sequences and merges them by confidence.
/// Mirrors `rover_merge_words` in the reference app, minus the hotword bonus (this
/// project has no hotword feature).
pub fn rover_merge_words(words_a: &[WordResult], words_b: &[WordResult]) -> Vec<MergedWord> {
    let norm_a: Vec<String> = words_a.iter().map(|w| normalize_word(&w.text)).collect();
    let norm_b: Vec<String> = words_b.iter().map(|w| normalize_word(&w.text)).collect();

    let ops = capture_diff_slices(Algorithm::Myers, &norm_a, &norm_b);

    let mut candidates: Vec<Candidate> = Vec::new();

    for op in &ops {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        match tag {
            DiffTag::Equal | DiffTag::Delete => {
                for w in &words_a[old_range] {
                    candidates.push(Candidate {
                        word: w.clone(),
                        disagree: false,
                        is_supplement: false,
                    });
                }
            }
            DiffTag::Replace => {
                let block_a = &words_a[old_range];
                let block_b = &words_b[new_range];
                let conf_a = block_confidence(block_a);
                let conf_b = block_confidence(block_b);
                let (chosen, disagree) = if conf_b > conf_a {
                    (block_b, true)
                } else {
                    (block_a, false)
                };
                for w in chosen {
                    candidates.push(Candidate {
                        word: w.clone(),
                        disagree,
                        is_supplement: false,
                    });
                }
            }
            DiffTag::Insert => {
                for w in &words_b[new_range] {
                    if w.confidence > INSERT_CONFIDENCE_THRESHOLD {
                        candidates.push(Candidate {
                            word: w.clone(),
                            disagree: true,
                            is_supplement: true,
                        });
                    }
                }
            }
        }
    }

    candidates.sort_by(|a, b| a.word.start.partial_cmp(&b.word.start).unwrap());

    let mut result: Vec<MergedWord> = Vec::with_capacity(candidates.len());
    for cand in candidates {
        if cand.is_supplement {
            let is_duplicate = result.iter().any(|kept: &MergedWord| {
                (kept.word.start - cand.word.start).abs() < DEDUP_TIME_WINDOW_SECONDS
                    && normalize_word(&kept.word.text) == normalize_word(&cand.word.text)
            });
            if is_duplicate {
                continue;
            }
        }
        result.push(MergedWord {
            word: cand.word,
            disagree: cand.disagree,
        });
    }

    result
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd frontend/src-tauri && cargo test rover_engine::merge -- --nocapture`
Expected: 9 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/rover_engine/merge.rs frontend/src-tauri/src/rover_engine/mod.rs
git commit -m "feat(rover): implement confidence-based word alignment merge"
```

---

### Task 4: `RoverDecoder` facade (`engine.rs`)

**Files:**
- Create: `frontend/src-tauri/src/rover_engine/engine.rs`
- Modify: `frontend/src-tauri/src/rover_engine/mod.rs`

Runs both underlying decoders in parallel. Safe to do so: `ort::Session` is `Send + Sync` (confirmed directly in the `ort` 2.0.0-rc.10 source — `unsafe impl Send for Session {}` / `unsafe impl Sync for Session {}` in `session/mod.rs`), and `self.decoder_a`/`self.decoder_b` are disjoint fields, so Rust's borrow checker allows two separate `&mut` borrows into the same `std::thread::scope` block.

- [ ] **Step 1: Implement**

Create `frontend/src-tauri/src/rover_engine/engine.rs`:

```rust
use crate::rnnt_decoder::engine::RnntDecoder;
use crate::rover_engine::merge::{rover_merge_words, MergedWord};
use anyhow::{anyhow, Result};
use std::path::Path;

pub struct RoverDecodeResult {
    pub text: String,
    pub words: Vec<MergedWord>,
}

pub struct RoverDecoder {
    decoder_a: RnntDecoder,
    decoder_b: RnntDecoder,
}

impl RoverDecoder {
    /// Each `(encoder, decoder, joiner, tokens)` tuple identifies one family's model
    /// files, exactly as `RnntDecoder::load` already takes them — `rover_engine` does
    /// not know about `ModelFamily`; that mapping is Phase C's job.
    pub fn load(
        family_a: (&Path, &Path, &Path, &Path),
        family_b: (&Path, &Path, &Path, &Path),
        beam_size: usize,
    ) -> Result<Self> {
        let decoder_a = RnntDecoder::load(family_a.0, family_a.1, family_a.2, family_a.3, beam_size)?;
        let decoder_b = RnntDecoder::load(family_b.0, family_b.1, family_b.2, family_b.3, beam_size)?;
        Ok(Self { decoder_a, decoder_b })
    }

    pub fn decode(&mut self, samples: &[f32], sample_rate: f32) -> Result<RoverDecodeResult> {
        let (result_a, result_b) = std::thread::scope(|scope| {
            let decoder_a = &mut self.decoder_a;
            let decoder_b = &mut self.decoder_b;
            let handle_a = scope.spawn(move || decoder_a.decode(samples, sample_rate));
            let handle_b = scope.spawn(move || decoder_b.decode(samples, sample_rate));
            let result_a = handle_a.join().map_err(|_| anyhow!("Decoder A thread panicked"));
            let result_b = handle_b.join().map_err(|_| anyhow!("Decoder B thread panicked"));
            (result_a, result_b)
        });

        let decode_a = result_a??;
        let decode_b = result_b??;

        let merged = rover_merge_words(&decode_a.words, &decode_b.words);
        let text = merged
            .iter()
            .map(|m| m.word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        Ok(RoverDecodeResult { text, words: merged })
    }
}
```

- [ ] **Step 2: Register the module**

In `frontend/src-tauri/src/rover_engine/mod.rs`, replace:

```rust
pub mod merge;
pub mod normalize;
```

with:

```rust
pub mod engine;
pub mod merge;
pub mod normalize;
```

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/rover_engine/engine.rs frontend/src-tauri/src/rover_engine/mod.rs
git commit -m "feat(rover): add RoverDecoder facade running two decoders in parallel"
```

---

### Task 5: Manual smoke test on real audio

**Files:** none (verification only, following the same pattern as Phase A's Task 9)

- [ ] **Step 1: Add an ignored manual test**

Add to `frontend/src-tauri/src/rover_engine/engine.rs`, below the existing code (a new `#[cfg(test)]` module):

```rust
#[cfg(test)]
mod manual_smoke_tests {
    use super::*;
    use std::path::PathBuf;

    fn resolve_tokens_path(model_dir: &PathBuf) -> PathBuf {
        let tokens = model_dir.join("tokens.txt");
        if tokens.exists() {
            tokens
        } else {
            model_dir.join("config.json")
        }
    }

    /// Runs ROVER over ZipFormer 30M int8 + Gipformer 65M int8 on real audio.
    /// Set ROVER_A_DIR, ROVER_B_DIR, ROVER_WAV_PATH. Filenames follow each
    /// family's own convention — adjust the join()s below if pointing at a
    /// different pair than ZipFormer/Gipformer int8.
    #[test]
    #[ignore]
    fn rover_decode_on_real_audio() {
        let dir_a = PathBuf::from(std::env::var("ROVER_A_DIR").expect("set ROVER_A_DIR"));
        let dir_b = PathBuf::from(std::env::var("ROVER_B_DIR").expect("set ROVER_B_DIR"));
        let wav_path = std::env::var("ROVER_WAV_PATH").expect("set ROVER_WAV_PATH");

        let enc_a = dir_a.join("encoder-epoch-20-avg-10.int8.onnx");
        let dec_a = dir_a.join("decoder-epoch-20-avg-10.int8.onnx");
        let joi_a = dir_a.join("joiner-epoch-20-avg-10.int8.onnx");
        let tok_a = resolve_tokens_path(&dir_a);

        let enc_b = dir_b.join("encoder-epoch-35-avg-6.int8.onnx");
        let dec_b = dir_b.join("decoder-epoch-35-avg-6.int8.onnx");
        let joi_b = dir_b.join("joiner-epoch-35-avg-6.int8.onnx");
        let tok_b = resolve_tokens_path(&dir_b);

        let mut rover = RoverDecoder::load(
            (&enc_a, &dec_a, &joi_a, &tok_a),
            (&enc_b, &dec_b, &joi_b, &tok_b),
            4,
        )
        .expect("load RoverDecoder");

        let decoded = crate::audio::decoder::decode_audio_file(PathBuf::from(&wav_path).as_path())
            .expect("decode audio file");

        let result = rover.decode(&decoded.samples, decoded.sample_rate as f32).expect("rover decode");

        println!("Merged text: {}", result.text);
        let disagreements = result.words.iter().filter(|w| w.disagree).count();
        println!("{} / {} words came from a disagreement (B overrode A, or B-only supplement)", disagreements, result.words.len());
        for w in &result.words {
            let marker = if w.disagree { "*" } else { " " };
            println!("  {}{} [{:.2}-{:.2}s] conf={:.3}", marker, w.word.text, w.word.start, w.word.end, w.word.confidence);
        }

        assert!(!result.text.is_empty());
    }
}
```

- [ ] **Step 2: Run it**

Reuse the same real model directories and WAV clip already used to verify Phase A:

```bash
cd frontend/src-tauri
ROVER_A_DIR="<path to zipformer-vi-int8>" ROVER_B_DIR="<path to gipformer-vi-int8>" ROVER_WAV_PATH="<path to the same WAV used in Phase A>" cargo test --release rover_engine::engine::manual_smoke_tests -- --ignored --nocapture
```

Expected: `result.text` reads as the same sentence Phase A already validated for this clip on all three families individually. Since all three models independently agreed on this clip's transcription in Phase A, there's no real disagreement for ROVER to arbitrate here — a 0-disagreement, matching-text result is the correct outcome, not a weak test. It confirms the merge doesn't corrupt an already-correct pair of inputs. The Replace/Insert/dedup *decision logic* itself is already covered by Task 3's synthetic unit tests — this step is specifically checking real-world plumbing (thread spawning, real `WordResult` shapes, real timing values), not re-testing the algorithm's branches.

If you want to see the disagreement path exercised on real audio (optional, not required to consider this phase done): try a noisier or longer clip, or a clip where the three families' Phase A outputs differed even slightly — check whether any manual notes from Phase A's Task 9 mentioned a family producing different text on some other sample.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/rover_engine/engine.rs
git commit -m "test(rover): add manual smoke test for RoverDecoder on real audio"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|---|---|
| `rover_engine` module structure | Tasks 2-4 (mod.rs built up incrementally) |
| Word normalization (lowercase, NFC, strip punctuation) | Task 2 |
| Diff-based alignment via `similar` (verified real API) | Task 3 |
| Equal → keep A | Task 3 (`full_agreement_keeps_a_with_no_disagreement`) |
| Replace → higher block confidence wins, `disagree` flag | Task 3 (`replace_keeps_a_when_a_more_confident`, `replace_picks_b_when_b_more_confident_and_marks_disagreement`) |
| Delete → keep A | Task 3 (covered by the same `Equal \| Delete` branch; both discard nothing from A) |
| Insert → B word only if confidence > 0.20 | Task 3 (`insert_above_threshold_is_included`, `insert_below_threshold_is_dropped`) |
| Sort by timestamp + dedup near-duplicate supplements | Task 3 (`near_duplicate_insert_supplement_is_deduped`) |
| No hotword bonus | Confirmed absent — no `ctx_graph`/hotword code anywhere in `merge.rs` |
| Concurrent decode, `Session: Send + Sync` verified | Task 4 |
| No UI/DB/pipeline wiring (Phase C's job) | Confirmed — only `Cargo.toml`, `lib.rs`'s module list, and Phase A's `WordResult` derive change outside `rover_engine/` |
| Manual verification on real audio | Task 5 |

---

## Notes for whoever executes this (e.g. via Cursor)

- Tasks 1-4 are fully mechanical and complete as written — every type/field name (`similar::DiffOp`, `DiffTag`, `capture_diff_slices`) was checked against the actual crate source, not guessed, so there should be no `sessions.rs`-style surprise this time.
- Task 3's unit tests are where the real correctness confidence comes from — if you're pressed for time, that's the task not to shortcut. Task 5's manual test is comparatively low-signal on its own (this specific clip has no disagreement to arbitrate) but confirms the plumbing.
- Reuse the exact same WAV clip and model directories from Phase A's manual verification for Task 5 — no need to source new audio.
