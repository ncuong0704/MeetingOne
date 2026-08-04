# Custom RNN-T Decoder Core Implementation Plan (ROVER Phase A)

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking. Execute tasks in order; each task ends with a commit. Do not skip "run and verify" steps.

**Goal:** Build a standalone, additive Rust module (`rnnt_decoder`) that decodes 16kHz audio through any existing ZipFormer-family ONNX triple (encoder/decoder/joiner + tokens.txt) using a hand-rolled RNN-T beam search, producing text and per-word confidence (margin, Tsallis entropy) — the exact algorithm the reference Vietnamese ASR app uses, faithfully ported. Nothing in the app calls this module yet; it is verified standalone. Phase B (a separate spec) will run two of these concurrently and merge their output for ROVER.

**Architecture:** Six focused files under `frontend/src-tauri/src/rnnt_decoder/`: `features.rs` (fbank via the new `kaldi-native-fbank` crate), `vocab.rs` (tokens.txt + BPE→word merge), `confidence.rs` (pure-math margin/Tsallis, no ONNX), `sessions.rs` (encoder/decoder/joiner ONNX I/O via `ort`, already a dependency), `beam_search.rs` (hypothesis state + modified beam search — the same algorithm sherpa-onnx calls `"modified_beam_search"`, just reimplemented to expose raw joiner logits), `engine.rs` (the `RnntDecoder` facade). Built in verifiable stages: fbank → greedy decode (first readable-text milestone) → full beam search → confidence.

**Tech Stack:** Rust, `ort` 2.0.0-rc.10 (existing dependency, same pattern as `capu_engine`), `kaldi-native-fbank` 0.1 (new dependency, pure-Rust fbank — no native library to vendor).

**Reference spec:** `docs/superpowers/specs/2026-08-04-rnnt-decoder-core-design.md`

---

## File map

| File | Responsibility |
|---|---|
| `frontend/src-tauri/Cargo.toml` | Add `kaldi-native-fbank` dependency |
| `frontend/src-tauri/src/rnnt_decoder/mod.rs` | Module exports |
| `frontend/src-tauri/src/rnnt_decoder/features.rs` | 80-dim log-mel fbank extraction |
| `frontend/src-tauri/src/rnnt_decoder/vocab.rs` | tokens.txt loader + BPE piece→word merge |
| `frontend/src-tauri/src/rnnt_decoder/confidence.rs` | Per-token margin/Tsallis entropy, word aggregation |
| `frontend/src-tauri/src/rnnt_decoder/sessions.rs` | Encoder/decoder/joiner ONNX session wrappers |
| `frontend/src-tauri/src/rnnt_decoder/beam_search.rs` | Greedy decode + modified beam search |
| `frontend/src-tauri/src/rnnt_decoder/engine.rs` | `RnntDecoder` facade tying everything together |
| `frontend/src-tauri/src/lib.rs` | Register `pub mod rnnt_decoder;` (no commands — nothing user-facing yet) |

No frontend files change. No database migration. No existing file is modified except `Cargo.toml` and `lib.rs`'s module list.

---

### Task 1: Add the `kaldi-native-fbank` dependency and confirm its real API

**Why first:** the design spec explicitly flags this crate's exact Rust field names as unverified from the outside — this task's first step resolves that before any code depends on guessed names.

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`

- [ ] **Step 1: Add the dependency**

In `frontend/src-tauri/Cargo.toml`, find:

```toml
ort = "2.0.0-rc.10"
tokenizers = "0.23"
```

Replace with:

```toml
ort = "2.0.0-rc.10"
tokenizers = "0.23"
kaldi-native-fbank = "0.1"
```

- [ ] **Step 2: Fetch and read the crate's real source**

Run: `cd frontend/src-tauri && cargo fetch` (pulls the crate into the local registry cache), then read these three files from the cache path it prints (or `~/.cargo/registry/src/index.crates.io-*/kaldi-native-fbank-0.1.0/src/`):
- `lib.rs` — confirm what's re-exported at the crate root vs. only available via submodule path.
- `fbank.rs` — confirm `FbankOptions` and `FbankComputer` field names and the `FbankComputer::new` signature.
- `window.rs` — confirm `FrameOptions` field names.
- `mel.rs` — confirm `MelOptions` field names, and note it is **not** re-exported at crate root (must be imported as `kaldi_native_fbank::mel::MelOptions`).
- `online.rs` — confirm `OnlineFeature` and `FeatureComputer` (the latter also not re-exported at root — import as `kaldi_native_fbank::online::FeatureComputer`) and their `accept_waveform`/`input_finished`/`num_frames_ready`/`get_frame` methods.

At the time this plan was written, the confirmed shape was:

```rust
// Re-exported at crate root:
pub use kaldi_native_fbank::{FbankComputer, FbankOptions, FrameOptions, OnlineFeature};
// NOT re-exported at root — must import from submodule:
use kaldi_native_fbank::mel::MelOptions;
use kaldi_native_fbank::online::FeatureComputer;

pub struct FrameOptions {
    pub samp_freq: f32,
    pub frame_shift_ms: f32,
    pub frame_length_ms: f32,
    pub dither: f32,
    pub preemph_coeff: f32,
    pub remove_dc_offset: bool,
    pub window_type: String,
    pub round_to_power_of_two: bool,
    pub blackman_coeff: f32,
    pub snip_edges: bool,
}
// Default: dither=0.00003, snip_edges=true — BOTH must be overridden (see Task 2).

pub struct MelOptions {
    pub num_bins: usize,
    pub low_freq: f32,
    pub high_freq: f32,
    pub vtln_low: f32,
    pub vtln_high: f32,
    pub htk_mode: bool,
    pub is_librosa: bool,
    pub use_slaney_mel_scale: bool,
    pub norm: String,
    pub floor_to_int_bin: bool,
    pub debug_mel: bool,
}
// Default: num_bins=25 — must override to 80.

pub struct FbankOptions {
    pub frame_opts: FrameOptions,
    pub mel_opts: MelOptions,
    pub use_energy: bool,       // Default: true — MUST override to false, or output is 81-dim not 80
    pub raw_energy: bool,
    pub htk_compat: bool,
    pub energy_floor: f32,
    pub use_log_fbank: bool,
    pub use_power: bool,
}
```

If the actual source you read differs from this (crate version bumped, API changed), use what you actually read — the values in Task 2's code block below are what must be *achieved*, not the literal text to paste blindly if the struct shape has changed.

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors (dependency added but unused so far).

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/Cargo.toml frontend/src-tauri/Cargo.lock
git commit -m "build: add kaldi-native-fbank dependency for custom RNN-T decoder"
```

---

### Task 2: Feature extraction (`features.rs`)

**Files:**
- Create: `frontend/src-tauri/src/rnnt_decoder/mod.rs`
- Create: `frontend/src-tauri/src/rnnt_decoder/features.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `frontend/src-tauri/src/rnnt_decoder/features.rs` with just the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_fbank_on_one_second_of_silence_produces_80_dim_frames() {
        let samples = vec![0.0f32; 16000]; // 1 second of silence at 16kHz
        let frames = compute_fbank(&samples, 16000.0).expect("fbank should succeed on silence");
        // 25ms frame / 10ms shift over 1000ms, snip_edges=false → ~100 frames.
        assert!(frames.len() > 90 && frames.len() < 110, "got {} frames", frames.len());
        for frame in &frames {
            assert_eq!(frame.len(), FBANK_DIM);
        }
    }

    #[test]
    fn compute_fbank_on_empty_audio_returns_no_frames() {
        let frames = compute_fbank(&[], 16000.0).expect("fbank should not error on empty input");
        assert!(frames.is_empty());
    }
}
```

- [ ] **Step 2: Create `mod.rs` and register the module**

Create `frontend/src-tauri/src/rnnt_decoder/mod.rs`:

```rust
pub mod features;
```

In `frontend/src-tauri/src/lib.rs`, find where other engine modules are declared (e.g. `pub mod capu_engine;`) and add nearby:

```rust
pub mod rnnt_decoder;
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd frontend/src-tauri && cargo test rnnt_decoder::features -- --nocapture`
Expected: compile error — `compute_fbank` and `FBANK_DIM` don't exist yet.

- [ ] **Step 4: Implement `compute_fbank`**

Prepend this to `frontend/src-tauri/src/rnnt_decoder/features.rs`, above the `#[cfg(test)]` block:

```rust
use anyhow::{anyhow, Result};
use kaldi_native_fbank::mel::MelOptions;
use kaldi_native_fbank::online::FeatureComputer;
use kaldi_native_fbank::{FbankComputer, FbankOptions, FrameOptions, OnlineFeature};

pub const FBANK_DIM: usize = 80;

/// Computes 80-dim log mel filterbank features matching the exact parameters used by
/// the reference Vietnamese ASR app (`compute_fbank_ort` in its `core/asr_engine.py`):
/// 16kHz, 25ms/10ms frames, povey window, no dither, no snip_edges, mel range 20-7600Hz,
/// energy channel disabled (the encoder expects exactly 80 dims, not 81).
pub fn compute_fbank(samples: &[f32], sample_rate: f32) -> Result<Vec<Vec<f32>>> {
    let opts = FbankOptions {
        frame_opts: FrameOptions {
            samp_freq: sample_rate,
            frame_shift_ms: 10.0,
            frame_length_ms: 25.0,
            dither: 0.0,
            window_type: "povey".to_string(),
            snip_edges: false,
            ..Default::default()
        },
        mel_opts: MelOptions {
            num_bins: FBANK_DIM,
            low_freq: 20.0,
            high_freq: 7600.0,
            ..Default::default()
        },
        use_energy: false,
        energy_floor: 1.0,
        ..Default::default()
    };

    let computer = FbankComputer::new(opts).map_err(|e| anyhow!("Failed to create FbankComputer: {}", e))?;
    let mut online = OnlineFeature::new(FeatureComputer::Fbank(computer));
    online.accept_waveform(sample_rate, samples);
    online.input_finished();

    let n = online.num_frames_ready();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let frame = online
            .get_frame(i)
            .ok_or_else(|| anyhow!("Missing fbank frame {}", i))?;
        if frame.len() != FBANK_DIM {
            return Err(anyhow!(
                "Unexpected fbank frame dim: {} (expected {})",
                frame.len(),
                FBANK_DIM
            ));
        }
        out.push(frame.to_vec());
    }
    Ok(out)
}
```

(If Task 1's source read turned up different field/type names than assumed here, adjust this block to match what you actually found — the target behavior, not the literal syntax, is what must hold.)

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd frontend/src-tauri && cargo test rnnt_decoder::features -- --nocapture`
Expected: 2 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/rnnt_decoder/
git add frontend/src-tauri/src/lib.rs
git commit -m "feat(rnnt): add fbank feature extraction"
```

---

### Task 3: Vocabulary and word segmentation (`vocab.rs`)

**Files:**
- Create: `frontend/src-tauri/src/rnnt_decoder/vocab.rs`
- Modify: `frontend/src-tauri/src/rnnt_decoder/mod.rs`

Confirmed `tokens.txt` format (read directly from an existing downloaded model): one `<piece> <id>` pair per line, space-separated, e.g. `▁HAI 3`. Blank token is always id `0` (`<blk> 0`), confirming the `BLANK_ID = 0` assumption from the design spec.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src-tauri/src/rnnt_decoder/vocab.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_tokens(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().expect("create temp file");
        for line in lines {
            writeln!(f, "{}", line).expect("write line");
        }
        f
    }

    #[test]
    fn loads_piece_to_id_mapping() {
        let f = write_temp_tokens(&["<blk> 0", "▁XIN 1", "CHÀO 2"]);
        let vocab = Vocab::from_tokens_file(f.path()).expect("load vocab");
        assert_eq!(vocab.piece(0), Some("<blk>"));
        assert_eq!(vocab.piece(1), Some("▁XIN"));
        assert_eq!(vocab.piece(2), Some("CHÀO"));
        assert_eq!(vocab.vocab_size(), 3);
    }

    #[test]
    fn merges_multi_piece_word_and_splits_on_boundary_marker() {
        let f = write_temp_tokens(&["<blk> 0", "▁UN 1", "HAPPY 2", "▁DAY 3"]);
        let vocab = Vocab::from_tokens_file(f.path()).expect("load vocab");
        let tokens = vec![
            PieceToken { id: 1, frame: 0, margin: 0.9, tsallis_norm: 0.1 },
            PieceToken { id: 2, frame: 1, margin: 0.5, tsallis_norm: 0.4 },
            PieceToken { id: 3, frame: 2, margin: 0.8, tsallis_norm: 0.2 },
        ];
        let words = pieces_to_words(&vocab, &tokens).expect("merge words");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "UNHAPPY");
        assert_eq!(words[0].start_frame, 0);
        assert_eq!(words[0].end_frame, 1);
        // margin_min across the word's tokens: min(0.9, 0.5) = 0.5
        assert!((words[0].margin_min - 0.5).abs() < 1e-6);
        // tsallis_max across the word's tokens: max(0.1, 0.4) = 0.4
        assert!((words[0].tsallis_max - 0.4).abs() < 1e-6);
        assert_eq!(words[1].text, "DAY");
        assert_eq!(words[1].start_frame, 2);
        assert_eq!(words[1].end_frame, 2);
    }
}
```

- [ ] **Step 2: Add `tempfile` as a dev-dependency**

In `frontend/src-tauri/Cargo.toml`, find the `[dev-dependencies]` section (create one at the end of the file if it doesn't exist) and add:

```toml
[dev-dependencies]
tempfile = "3"
```

If `[dev-dependencies]` already exists elsewhere in the file, add `tempfile = "3"` as a line inside that existing section instead of creating a duplicate one.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd frontend/src-tauri && cargo test rnnt_decoder::vocab -- --nocapture`
Expected: compile error — `Vocab`, `PieceToken`, `pieces_to_words` don't exist yet.

- [ ] **Step 4: Implement**

Prepend this to `frontend/src-tauri/src/rnnt_decoder/vocab.rs`, above the `#[cfg(test)]` block:

```rust
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;

/// SentencePiece word-boundary marker ("▁", U+2581). A piece starting with this marker
/// begins a new word; a piece without it is a continuation of the previous word.
const WORD_BOUNDARY_MARKER: char = '\u{2581}';

pub struct Vocab {
    id_to_piece: HashMap<i64, String>,
}

impl Vocab {
    /// Parses a sherpa-onnx-style `tokens.txt`: one `<piece> <id>` pair per line,
    /// space-separated. The piece itself never contains a literal space (BPE pieces use
    /// the boundary marker instead), so splitting on the last space is always correct.
    pub fn from_tokens_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("Failed to read tokens file {:?}: {}", path, e))?;
        let mut id_to_piece = HashMap::new();
        for (line_no, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (piece, id_str) = line
                .rsplit_once(' ')
                .ok_or_else(|| anyhow!("Malformed tokens.txt line {}: {:?}", line_no + 1, line))?;
            let id: i64 = id_str
                .trim()
                .parse()
                .map_err(|e| anyhow!("Bad token id on line {}: {}", line_no + 1, e))?;
            id_to_piece.insert(id, piece.to_string());
        }
        Ok(Self { id_to_piece })
    }

    pub fn piece(&self, id: i64) -> Option<&str> {
        self.id_to_piece.get(&id).map(|s| s.as_str())
    }

    pub fn vocab_size(&self) -> usize {
        self.id_to_piece.len()
    }
}

/// One decoded (non-blank) token, carrying the per-token confidence metrics computed
/// from the joiner logits at the frame it was emitted (see `confidence.rs`).
pub struct PieceToken {
    pub id: i64,
    pub frame: usize,
    pub margin: f32,
    pub tsallis_norm: f32,
}

pub struct Word {
    pub text: String,
    pub start_frame: usize,
    pub end_frame: usize,
    pub margin_min: f32,
    pub tsallis_max: f32,
}

/// Merges a flat sequence of decoded BPE pieces into words, aggregating confidence
/// per word: `margin_min` = min margin across the word's pieces, `tsallis_max` = max
/// Tsallis entropy across the word's pieces (matches `_finalize_word_entropy` in the
/// reference app).
pub fn pieces_to_words(vocab: &Vocab, tokens: &[PieceToken]) -> Result<Vec<Word>> {
    let mut words: Vec<Word> = Vec::new();
    for tok in tokens {
        let piece = vocab
            .piece(tok.id)
            .ok_or_else(|| anyhow!("Unknown token id {}", tok.id))?;
        let starts_new_word = piece.starts_with(WORD_BOUNDARY_MARKER) || words.is_empty();
        let trimmed = piece.trim_start_matches(WORD_BOUNDARY_MARKER);

        if starts_new_word {
            words.push(Word {
                text: trimmed.to_string(),
                start_frame: tok.frame,
                end_frame: tok.frame,
                margin_min: tok.margin,
                tsallis_max: tok.tsallis_norm,
            });
        } else if let Some(last) = words.last_mut() {
            last.text.push_str(trimmed);
            last.end_frame = tok.frame;
            last.margin_min = last.margin_min.min(tok.margin);
            last.tsallis_max = last.tsallis_max.max(tok.tsallis_norm);
        }
    }
    Ok(words)
}
```

- [ ] **Step 5: Register the module**

In `frontend/src-tauri/src/rnnt_decoder/mod.rs`, replace:

```rust
pub mod features;
```

with:

```rust
pub mod features;
pub mod vocab;
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd frontend/src-tauri && cargo test rnnt_decoder::vocab -- --nocapture`
Expected: 2 tests PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/rnnt_decoder/vocab.rs frontend/src-tauri/src/rnnt_decoder/mod.rs frontend/src-tauri/Cargo.toml frontend/src-tauri/Cargo.lock
git commit -m "feat(rnnt): add tokens.txt loader and BPE piece-to-word merge"
```

---

### Task 4: Confidence scoring (`confidence.rs`)

**Files:**
- Create: `frontend/src-tauri/src/rnnt_decoder/confidence.rs`
- Modify: `frontend/src-tauri/src/rnnt_decoder/mod.rs`

Pure math, no ONNX, fully deterministic — the cleanest unit-testable piece in this plan.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src-tauri/src/rnnt_decoder/confidence.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_certain_prediction_has_high_margin_and_low_tsallis() {
        // One class dominates: model is very sure.
        let logits = vec![10.0, 0.0, 0.0, 0.0];
        let conf = compute_token_confidence(&logits);
        assert!(conf.margin > 0.99, "margin = {}", conf.margin);
        assert!(conf.tsallis_norm < 0.15, "tsallis_norm = {}", conf.tsallis_norm);
    }

    #[test]
    fn uniform_prediction_has_zero_margin_and_max_tsallis() {
        // All classes equally likely: model is maximally unsure.
        let logits = vec![0.0, 0.0, 0.0, 0.0];
        let conf = compute_token_confidence(&logits);
        assert!(conf.margin.abs() < 1e-5, "margin = {}", conf.margin);
        assert!((conf.tsallis_norm - 1.0).abs() < 1e-3, "tsallis_norm = {}", conf.tsallis_norm);
    }

    #[test]
    fn word_confidence_combines_margin_and_tsallis() {
        assert!((word_confidence(0.8, 0.2) - 0.64).abs() < 1e-6);
        assert!((word_confidence(0.0, 0.5) - 0.0).abs() < 1e-6);
        assert!((word_confidence(1.0, 0.0) - 1.0).abs() < 1e-6);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend/src-tauri && cargo test rnnt_decoder::confidence -- --nocapture`
Expected: compile error — `compute_token_confidence` and `word_confidence` don't exist yet.

- [ ] **Step 3: Implement**

Prepend this to `frontend/src-tauri/src/rnnt_decoder/confidence.rs`, above the `#[cfg(test)]` block:

```rust
/// Per-token confidence derived from raw joiner logits (pre-softmax). Mirrors
/// `_compute_token_entropy` in the reference app, restricted to the two fields ROVER's
/// word confidence actually consumes.
pub struct TokenConfidence {
    /// top1_prob - top2_prob after softmax. Near 1.0 = very confident, near 0.0 = torn
    /// between at least two candidates.
    pub margin: f32,
    /// Tsallis entropy (alpha = 1/3) normalized to [0, 1] by the maximum possible value
    /// for this vocab size. Near 0.0 = low entropy (confident), near 1.0 = maximum
    /// entropy (uniform distribution, no information).
    pub tsallis_norm: f32,
}

pub fn compute_token_confidence(logits: &[f32]) -> TokenConfidence {
    let vocab_size = logits.len();
    let alpha: f32 = 1.0 / 3.0;

    let tsallis_max_val = if vocab_size > 1 {
        (1.0 / (alpha - 1.0)) * (1.0 - (vocab_size as f32).powf(1.0 - alpha))
    } else {
        1.0
    };

    let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp_shifted: Vec<f32> = logits.iter().map(|&x| (x - max_logit).exp()).collect();
    let sum_exp: f32 = exp_shifted.iter().sum();
    let probs: Vec<f32> = exp_shifted.iter().map(|&x| x / sum_exp).collect();

    let tsallis = if vocab_size > 1 {
        let sum_p_alpha: f32 = probs.iter().map(|&p| p.powf(alpha)).sum();
        (1.0 / (alpha - 1.0)) * (1.0 - sum_p_alpha)
    } else {
        0.0
    };
    let tsallis_norm = if tsallis_max_val > 0.0 { tsallis / tsallis_max_val } else { 0.0 };

    let mut sorted_probs = probs.clone();
    sorted_probs.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let top1 = sorted_probs.first().copied().unwrap_or(0.0);
    let top2 = sorted_probs.get(1).copied().unwrap_or(0.0);

    TokenConfidence {
        margin: top1 - top2,
        tsallis_norm,
    }
}

/// Final per-word confidence, matching `_word_confidence` in the reference app exactly:
/// a word the model is both decisive about (`margin_min` high) and not entropic about
/// (`tsallis_max` low) scores near 1.0; either signal being bad drags it toward 0.0.
pub fn word_confidence(margin_min: f32, tsallis_max: f32) -> f32 {
    margin_min * (1.0 - tsallis_max)
}
```

- [ ] **Step 4: Register the module**

In `frontend/src-tauri/src/rnnt_decoder/mod.rs`, replace:

```rust
pub mod features;
pub mod vocab;
```

with:

```rust
pub mod confidence;
pub mod features;
pub mod vocab;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd frontend/src-tauri && cargo test rnnt_decoder::confidence -- --nocapture`
Expected: 3 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/rnnt_decoder/confidence.rs frontend/src-tauri/src/rnnt_decoder/mod.rs
git commit -m "feat(rnnt): add margin/Tsallis-entropy token and word confidence"
```

---

### Task 5: ONNX sessions (`sessions.rs`)

**Files:**
- Create: `frontend/src-tauri/src/rnnt_decoder/sessions.rs`
- Modify: `frontend/src-tauri/src/rnnt_decoder/mod.rs`

Encoder/decoder/joiner I/O tensor names below follow the standard icefall/k2 transducer ONNX export convention — the same convention the existing `sherpa-onnx`-based `asr_engine` relies on implicitly for all three already-integrated model families. This task has no isolated unit test (it needs real `.onnx` files); it's verified end-to-end in Task 6 (greedy decode) and Task 9 (manual smoke test).

- [ ] **Step 1: Implement**

Create `frontend/src-tauri/src/rnnt_decoder/sessions.rs`:

```rust
use crate::rnnt_decoder::features::FBANK_DIM;
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

pub struct RnntSessions {
    encoder: Session,
    decoder: Session,
    joiner: Session,
}

fn load_session(path: &Path, label: &str) -> Result<Session> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("Non-UTF8 {} path: {:?}", label, path))?;
    Session::builder()
        .map_err(|e| anyhow!("Failed to create {} session builder: {}", label, e))?
        .commit_from_file(path_str)
        .map_err(|e| anyhow!("Failed to load {} {:?}: {}", label, path, e))
}

impl RnntSessions {
    pub fn load(encoder_path: &Path, decoder_path: &Path, joiner_path: &Path) -> Result<Self> {
        Ok(Self {
            encoder: load_session(encoder_path, "encoder")?,
            decoder: load_session(decoder_path, "decoder")?,
            joiner: load_session(joiner_path, "joiner")?,
        })
    }

    /// Runs the encoder over the full fbank feature matrix and returns one Vec<f32> per
    /// output frame. `encoder_out_lens` (not just the raw output shape) determines how
    /// many frames are valid, since some export configurations pad the output.
    pub fn run_encoder(&mut self, fbank: &[Vec<f32>]) -> Result<Vec<Vec<f32>>> {
        let t = fbank.len();
        let mut x_flat: Vec<f32> = Vec::with_capacity(t * FBANK_DIM);
        for frame in fbank {
            x_flat.extend_from_slice(frame);
        }
        let x_tensor = TensorRef::from_array_view(([1usize, t, FBANK_DIM], &*x_flat))
            .map_err(|e| anyhow!("Failed to build encoder input x: {}", e))?;
        let x_lens: Vec<i64> = vec![t as i64];
        let x_lens_tensor = TensorRef::from_array_view(([1usize], &*x_lens))
            .map_err(|e| anyhow!("Failed to build encoder input x_lens: {}", e))?;

        let outputs = self
            .encoder
            .run(ort::inputs!["x" => x_tensor, "x_lens" => x_lens_tensor])
            .map_err(|e| anyhow!("Encoder inference failed: {}", e))?;

        let (enc_shape, enc_data) = outputs["encoder_out"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("Failed to read encoder_out: {}", e))?;
        let (_, lens_data) = outputs["encoder_out_lens"]
            .try_extract_tensor::<i64>()
            .map_err(|e| anyhow!("Failed to read encoder_out_lens: {}", e))?;

        let out_t = lens_data[0] as usize;
        let out_dim = enc_shape[2] as usize;
        let mut frames = Vec::with_capacity(out_t);
        for i in 0..out_t {
            let start = i * out_dim;
            frames.push(enc_data[start..start + out_dim].to_vec());
        }
        Ok(frames)
    }

    /// Runs the stateless, context-size-2 decoder for a batch of context tuples. Each
    /// row is `[y_{t-2}, y_{t-1}]`; the caller may pass `-1` for a not-yet-emitted slot
    /// (icefall convention for the initial context) — clamped to `0` here since the
    /// decoder's embedding table has no negative index.
    pub fn run_decoder(&mut self, contexts: &[[i64; 2]]) -> Result<Vec<Vec<f32>>> {
        let b = contexts.len();
        let mut y_flat: Vec<i64> = Vec::with_capacity(b * 2);
        for ctx in contexts {
            y_flat.push(ctx[0].max(0));
            y_flat.push(ctx[1].max(0));
        }
        let y_tensor = TensorRef::from_array_view(([b, 2usize], &*y_flat))
            .map_err(|e| anyhow!("Failed to build decoder input y: {}", e))?;

        let outputs = self
            .decoder
            .run(ort::inputs!["y" => y_tensor])
            .map_err(|e| anyhow!("Decoder inference failed: {}", e))?;

        let (dec_shape, dec_data) = outputs["decoder_out"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("Failed to read decoder_out: {}", e))?;
        let dim = dec_shape[1] as usize;
        let mut out = Vec::with_capacity(b);
        for i in 0..b {
            let start = i * dim;
            out.push(dec_data[start..start + dim].to_vec());
        }
        Ok(out)
    }

    /// Runs the joiner for a batch of (encoder_out, decoder_out) pairs, returning raw
    /// (pre-softmax) logits per row. Callers must not treat these as probabilities —
    /// `confidence.rs` and `beam_search.rs` both apply their own softmax.
    pub fn run_joiner(&mut self, encoder_outs: &[Vec<f32>], decoder_outs: &[Vec<f32>]) -> Result<Vec<Vec<f32>>> {
        let b = encoder_outs.len();
        let enc_dim = encoder_outs[0].len();
        let dec_dim = decoder_outs[0].len();
        let mut enc_flat: Vec<f32> = Vec::with_capacity(b * enc_dim);
        let mut dec_flat: Vec<f32> = Vec::with_capacity(b * dec_dim);
        for row in encoder_outs {
            enc_flat.extend_from_slice(row);
        }
        for row in decoder_outs {
            dec_flat.extend_from_slice(row);
        }
        let enc_tensor = TensorRef::from_array_view(([b, enc_dim], &*enc_flat))
            .map_err(|e| anyhow!("Failed to build joiner input encoder_out: {}", e))?;
        let dec_tensor = TensorRef::from_array_view(([b, dec_dim], &*dec_flat))
            .map_err(|e| anyhow!("Failed to build joiner input decoder_out: {}", e))?;

        let outputs = self
            .joiner
            .run(ort::inputs!["encoder_out" => enc_tensor, "decoder_out" => dec_tensor])
            .map_err(|e| anyhow!("Joiner inference failed: {}", e))?;

        let (logits_shape, logits_data) = outputs["logits"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("Failed to read joiner logits: {}", e))?;
        let v = logits_shape[1] as usize;
        let mut out = Vec::with_capacity(b);
        for i in 0..b {
            let start = i * v;
            out.push(logits_data[start..start + v].to_vec());
        }
        Ok(out)
    }
}
```

- [ ] **Step 2: Register the module**

In `frontend/src-tauri/src/rnnt_decoder/mod.rs`, replace:

```rust
pub mod confidence;
pub mod features;
pub mod vocab;
```

with:

```rust
pub mod confidence;
pub mod features;
pub mod sessions;
pub mod vocab;
```

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/rnnt_decoder/sessions.rs frontend/src-tauri/src/rnnt_decoder/mod.rs
git commit -m "feat(rnnt): add encoder/decoder/joiner ONNX session wrappers"
```

**If input/output tensor names are wrong:** `cargo check` will pass (names are only validated at runtime), but Task 6's smoke test will fail with an `ort` error like `"no output named 'encoder_out'"`. If that happens, dump the actual graph I/O names — e.g. with a throwaway Python one-liner (`import onnx; m = onnx.load("encoder.onnx"); print([i.name for i in m.graph.input], [o.name for o in m.graph.output])`) against one of the already-downloaded model files under the app's models directory — and adjust the string literals in `run_encoder`/`run_decoder`/`run_joiner` accordingly.

---

### Task 6: Greedy decode — first end-to-end milestone

**Files:**
- Create: `frontend/src-tauri/src/rnnt_decoder/beam_search.rs`
- Modify: `frontend/src-tauri/src/rnnt_decoder/mod.rs`

**Why greedy first:** it isolates encoder/decoder/joiner wiring and tensor shapes from beam-search complexity. If greedy produces garbage, the bug is in fbank params, tensor shapes, or context/blank handling — not in beam search logic, which doesn't exist yet.

This task has no automated unit test (needs real model files); it is its own manual milestone.

- [ ] **Step 1: Implement greedy decode**

Create `frontend/src-tauri/src/rnnt_decoder/beam_search.rs`:

```rust
use crate::rnnt_decoder::sessions::RnntSessions;
use anyhow::Result;

pub const BLANK_ID: i64 = 0;
pub const CONTEXT_SIZE: usize = 2;

/// Simplest possible decode: argmax at each encoder frame, no beam, no confidence.
/// Exists purely as a wiring-verification milestone before `modified_beam_search`.
pub fn greedy_decode(sessions: &mut RnntSessions, encoder_frames: &[Vec<f32>]) -> Result<Vec<i64>> {
    let mut ys: Vec<i64> = vec![-1, BLANK_ID];
    let mut token_ids: Vec<i64> = Vec::new();

    let mut decoder_out = sessions
        .run_decoder(&[[ys[ys.len() - 2], ys[ys.len() - 1]]])?
        .remove(0);

    for enc_frame in encoder_frames {
        let logits = sessions
            .run_joiner(&[enc_frame.clone()], &[decoder_out.clone()])?
            .remove(0);
        let (best_idx, _) = logits
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .expect("joiner logits must be non-empty");
        let token = best_idx as i64;

        if token != BLANK_ID {
            token_ids.push(token);
            ys.push(token);
            let n = ys.len();
            decoder_out = sessions.run_decoder(&[[ys[n - 2], ys[n - 1]]])?.remove(0);
        }
    }

    Ok(token_ids)
}
```

- [ ] **Step 2: Register the module**

In `frontend/src-tauri/src/rnnt_decoder/mod.rs`, replace:

```rust
pub mod confidence;
pub mod features;
pub mod sessions;
pub mod vocab;
```

with:

```rust
pub mod beam_search;
pub mod confidence;
pub mod features;
pub mod sessions;
pub mod vocab;
```

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors.

- [ ] **Step 4: Manual smoke test — greedy decode produces readable text**

This is the first real correctness checkpoint for the whole module. Write a small throwaway test binary or `#[test]` (with `#[ignore]` so it doesn't run in CI, since it needs real model files on disk) in `beam_search.rs`:

```rust
#[cfg(test)]
mod manual_smoke_tests {
    use super::*;
    use crate::rnnt_decoder::{features::compute_fbank, sessions::RnntSessions, vocab::Vocab};
    use std::path::PathBuf;

    /// Ignored by default — run explicitly with a real model directory and WAV file:
    /// `cargo test --release rnnt_decoder::beam_search::manual_smoke_tests -- --ignored --nocapture`
    /// Point `MODEL_DIR` at an already-downloaded ZipFormer 30M int8 directory
    /// (e.g. the app data dir's `models/zipformer-vi-int8/`) and `WAV_PATH` at a short
    /// 16kHz mono Vietnamese speech clip.
    #[test]
    #[ignore]
    fn greedy_decode_on_real_audio() {
        let model_dir = PathBuf::from(std::env::var("RNNT_MODEL_DIR").expect("set RNNT_MODEL_DIR"));
        let wav_path = std::env::var("RNNT_WAV_PATH").expect("set RNNT_WAV_PATH");

        let mut sessions = RnntSessions::load(
            &model_dir.join("encoder-epoch-20-avg-10.int8.onnx"),
            &model_dir.join("decoder-epoch-20-avg-10.int8.onnx"),
            &model_dir.join("joiner-epoch-20-avg-10.int8.onnx"),
        )
        .expect("load sessions");
        let vocab = Vocab::from_tokens_file(&model_dir.join("config.json")).expect("load vocab");

        let (samples, sample_rate) = crate::audio::common::read_wav_mono_f32(&wav_path)
            .expect("read wav — adjust this call to whatever this repo's existing WAV-reading helper is");
        let fbank = compute_fbank(&samples, sample_rate as f32).expect("fbank");
        let encoder_frames = sessions.run_encoder(&fbank).expect("encoder");
        let token_ids = greedy_decode(&mut sessions, &encoder_frames).expect("greedy decode");

        let text: String = token_ids
            .iter()
            .filter_map(|&id| vocab.piece(id))
            .collect::<Vec<_>>()
            .join(" ");
        println!("Greedy decode output: {}", text);
        assert!(!text.is_empty(), "greedy decode produced no tokens");
    }
}
```

Adjust the WAV-reading call to whatever helper this codebase already uses to load a 16kHz mono `f32` buffer (check `frontend/src-tauri/src/audio/common.rs` — it already has WAV/audio decoding used elsewhere in the pipeline; reuse it rather than adding a new one).

Run it manually:

```bash
cd frontend/src-tauri
RNNT_MODEL_DIR="<path to your zipformer-vi-int8 models dir>" RNNT_WAV_PATH="<path to a short Vietnamese WAV>" cargo test --release rnnt_decoder::beam_search::manual_smoke_tests -- --ignored --nocapture
```

Expected: the printed text is readable Vietnamese BPE pieces roughly matching what the app's existing `AsrEngine::transcribe_audio` produces for the same file on the same model (compare by running the app normally on that file). It will **not** be identical — greedy vs. the app's `modified_beam_search` via sherpa-onnx will diverge in places — but it must be recognizably the same sentence, not silence, not repeated garbage, not a wall of blanks-turned-into-noise.

If it's garbled: the bug is almost certainly in fbank parameters (re-check Task 2's `use_energy`/`dither`/`snip_edges` against the real crate defaults) or in the encoder/decoder/joiner tensor names (see Task 5's fallback note).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/rnnt_decoder/beam_search.rs frontend/src-tauri/src/rnnt_decoder/mod.rs
git commit -m "feat(rnnt): add greedy decode as a wiring-verification milestone"
```

---

### Task 7: Modified beam search — full algorithm

**Files:**
- Modify: `frontend/src-tauri/src/rnnt_decoder/beam_search.rs`

This is deliberately the same algorithm sherpa-onnx itself calls `"modified_beam_search"` (already the default decoding method used elsewhere in this codebase's `asr_engine`) — reimplemented over raw ONNX so the raw joiner logits are observable for confidence scoring. Per frame: log-softmax every active hypothesis's logits, take the global top-k across all (hypothesis × vocab) pairs, merge hypotheses that collapse to the same token sequence via log-sum-exp, cache decoder output by context tuple to avoid redundant decoder forward passes, and pick the final hypothesis by length-normalized log-probability. No hotword context graph (out of scope — this project has no hotword feature at all).

- [ ] **Step 1: Append the beam search implementation**

Add this to the end of `frontend/src-tauri/src/rnnt_decoder/beam_search.rs`, before the `#[cfg(test)]` module:

```rust
use std::collections::HashMap;

#[derive(Clone)]
struct Hypothesis {
    ys: Vec<i64>,
    log_prob: f32,
    emitted_frames: Vec<usize>,
    emitted_logits: Vec<Vec<f32>>,
}

impl Hypothesis {
    fn initial() -> Self {
        Self {
            ys: vec![-1, BLANK_ID],
            log_prob: 0.0,
            emitted_frames: Vec::new(),
            emitted_logits: Vec::new(),
        }
    }

    fn context(&self) -> [i64; CONTEXT_SIZE] {
        let n = self.ys.len();
        [self.ys[n - 2], self.ys[n - 1]]
    }
}

/// Numerically stable log(exp(a) + exp(b)), used to merge two beam-search hypotheses
/// that have collapsed onto the same token sequence.
fn log_add(a: f32, b: f32) -> f32 {
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    let diff = lo - hi;
    if diff < -36.0 {
        hi
    } else {
        hi + diff.exp().ln_1p()
    }
}

pub struct BeamSearchResult {
    pub token_ids: Vec<i64>,
    pub frames: Vec<usize>,
    /// Raw joiner logits at the exact step each token was emitted — feed these straight
    /// into `confidence::compute_token_confidence`.
    pub logits: Vec<Vec<f32>>,
}

pub fn modified_beam_search(
    sessions: &mut RnntSessions,
    encoder_frames: &[Vec<f32>],
    beam_size: usize,
    vocab_size: usize,
) -> Result<BeamSearchResult> {
    let mut hyps: HashMap<Vec<i64>, Hypothesis> = HashMap::new();
    let init = Hypothesis::initial();
    hyps.insert(init.ys.clone(), init);

    let mut decoder_cache: HashMap<[i64; CONTEXT_SIZE], Vec<f32>> = HashMap::new();

    for (t, enc_frame) in encoder_frames.iter().enumerate() {
        let prev: Vec<Hypothesis> = hyps.values().cloned().collect();
        let b = prev.len();

        let missing_contexts: Vec<[i64; CONTEXT_SIZE]> = prev
            .iter()
            .map(|h| h.context())
            .filter(|ctx| !decoder_cache.contains_key(ctx))
            .collect();
        if !missing_contexts.is_empty() {
            let results = sessions.run_decoder(&missing_contexts)?;
            for (ctx, out) in missing_contexts.iter().zip(results.into_iter()) {
                decoder_cache.insert(*ctx, out);
            }
        }
        let decoder_outs: Vec<Vec<f32>> = prev.iter().map(|h| decoder_cache[&h.context()].clone()).collect();
        let encoder_outs: Vec<Vec<f32>> = std::iter::repeat(enc_frame.clone()).take(b).collect();

        let logits_batch = sessions.run_joiner(&encoder_outs, &decoder_outs)?;

        let mut flat_scores: Vec<f32> = Vec::with_capacity(b * vocab_size);
        for (hi, logits) in logits_batch.iter().enumerate() {
            let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let sum_exp: f32 = logits.iter().map(|&x| (x - max_logit).exp()).sum();
            let log_sum_exp = max_logit + sum_exp.ln();
            for &l in logits {
                flat_scores.push(l - log_sum_exp + prev[hi].log_prob);
            }
        }

        let k = beam_size.min(flat_scores.len());
        let mut indices: Vec<usize> = (0..flat_scores.len()).collect();
        indices.sort_unstable_by(|&a, &b| flat_scores[b].partial_cmp(&flat_scores[a]).unwrap());
        indices.truncate(k);

        let mut new_hyps: HashMap<Vec<i64>, Hypothesis> = HashMap::new();
        for &idx in &indices {
            let hi = idx / vocab_size;
            let token = (idx % vocab_size) as i64;
            let score = flat_scores[idx];
            let base = &prev[hi];

            let mut new_hyp = base.clone();
            new_hyp.log_prob = score;
            if token != BLANK_ID {
                new_hyp.ys.push(token);
                new_hyp.emitted_frames.push(t);
                new_hyp.emitted_logits.push(logits_batch[hi].clone());
            }

            match new_hyps.get_mut(&new_hyp.ys) {
                Some(existing) => existing.log_prob = log_add(existing.log_prob, new_hyp.log_prob),
                None => {
                    new_hyps.insert(new_hyp.ys.clone(), new_hyp);
                }
            }
        }
        hyps = new_hyps;
    }

    let best = hyps
        .values()
        .max_by(|a, b| {
            let na = a.ys.len().max(1) as f32;
            let nb = b.ys.len().max(1) as f32;
            (a.log_prob / na).partial_cmp(&(b.log_prob / nb)).unwrap()
        })
        .ok_or_else(|| anyhow::anyhow!("Beam search produced no hypotheses"))?;

    Ok(BeamSearchResult {
        token_ids: best.ys[CONTEXT_SIZE..].to_vec(),
        frames: best.emitted_frames.clone(),
        logits: best.emitted_logits.clone(),
    })
}
```

- [ ] **Step 2: Write a small deterministic unit test for `log_add`**

Add to the `#[cfg(test)]` module at the bottom of `beam_search.rs` (create it if this is the first test in the file):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_add_matches_naive_log_sum_exp_for_moderate_values() {
        let a = -1.0_f32;
        let b = -2.0_f32;
        let naive = (a.exp() + b.exp()).ln();
        let via_log_add = log_add(a, b);
        assert!((naive - via_log_add).abs() < 1e-5, "naive={} log_add={}", naive, via_log_add);
    }

    #[test]
    fn log_add_returns_larger_value_when_other_is_negligible() {
        let a = 0.0_f32;
        let b = -100.0_f32; // exp(-100) is effectively 0
        assert!((log_add(a, b) - a).abs() < 1e-4);
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cd frontend/src-tauri && cargo test rnnt_decoder::beam_search::tests -- --nocapture`
Expected: 2 tests PASS.

- [ ] **Step 4: Manual smoke test — beam search vs. greedy**

Update the `RNNT_MODEL_DIR`/`RNNT_WAV_PATH` smoke test from Task 6 (or add a sibling `#[test] #[ignore]`) to also run `modified_beam_search` with `beam_size = 4` (matching the reference app's own default for this specific decoder) and print both outputs side by side:

```rust
    #[test]
    #[ignore]
    fn beam_search_matches_or_beats_greedy_on_real_audio() {
        // ... same setup as greedy_decode_on_real_audio ...
        let greedy_tokens = greedy_decode(&mut sessions, &encoder_frames).expect("greedy");
        let beam_result = modified_beam_search(&mut sessions, &encoder_frames, 4, vocab.vocab_size())
            .expect("beam search");

        let greedy_text: String = greedy_tokens.iter().filter_map(|&id| vocab.piece(id)).collect::<Vec<_>>().join(" ");
        let beam_text: String = beam_result.token_ids.iter().filter_map(|&id| vocab.piece(id)).collect::<Vec<_>>().join(" ");
        println!("Greedy: {}\nBeam:   {}", greedy_text, beam_text);
        assert!(!beam_text.is_empty());
    }
```

Run it the same way as Task 6's smoke test. Expected: beam search output reads at least as well as greedy, ideally better (fewer obviously-wrong words). If beam search is dramatically *worse* than greedy or empty, the bug is in the top-k/merge/length-normalization logic, not in the encoder/decoder/joiner wiring (already verified in Task 6).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/rnnt_decoder/beam_search.rs
git commit -m "feat(rnnt): implement modified beam search with decoder caching and log-sum-exp merge"
```

---

### Task 8: `RnntDecoder` facade (`engine.rs`)

**Files:**
- Create: `frontend/src-tauri/src/rnnt_decoder/engine.rs`
- Modify: `frontend/src-tauri/src/rnnt_decoder/mod.rs`

Ties `features` + `sessions` + `beam_search` + `confidence` + `vocab` into the one public entry point Phase B will call.

- [ ] **Step 1: Implement**

Create `frontend/src-tauri/src/rnnt_decoder/engine.rs`:

```rust
use crate::rnnt_decoder::beam_search::modified_beam_search;
use crate::rnnt_decoder::confidence::{compute_token_confidence, word_confidence};
use crate::rnnt_decoder::features::compute_fbank;
use crate::rnnt_decoder::sessions::RnntSessions;
use crate::rnnt_decoder::vocab::{pieces_to_words, PieceToken, Vocab};
use anyhow::Result;
use std::path::Path;

pub struct WordResult {
    pub text: String,
    /// Seconds from the start of the decoded clip.
    pub start: f32,
    pub end: f32,
    pub margin_min: f32,
    pub tsallis_max: f32,
    /// `margin_min * (1.0 - tsallis_max)` — the single score ROVER's merge (Phase B)
    /// will compare between two models' competing words.
    pub confidence: f32,
}

pub struct DecodeResult {
    pub text: String,
    pub words: Vec<WordResult>,
}

pub struct RnntDecoder {
    sessions: RnntSessions,
    vocab: Vocab,
    frame_shift_ms: f32,
    beam_size: usize,
}

impl RnntDecoder {
    /// `beam_size` defaults to 4 in the reference app for this exact decoder; pass a
    /// different value to tune quality vs. speed.
    pub fn load(
        encoder_path: &Path,
        decoder_path: &Path,
        joiner_path: &Path,
        tokens_path: &Path,
        beam_size: usize,
    ) -> Result<Self> {
        let sessions = RnntSessions::load(encoder_path, decoder_path, joiner_path)?;
        let vocab = Vocab::from_tokens_file(tokens_path)?;
        Ok(Self {
            sessions,
            vocab,
            frame_shift_ms: 10.0,
            beam_size,
        })
    }

    pub fn decode(&mut self, samples: &[f32], sample_rate: f32) -> Result<DecodeResult> {
        let fbank = compute_fbank(samples, sample_rate)?;
        let encoder_frames = self.sessions.run_encoder(&fbank)?;

        let result = modified_beam_search(
            &mut self.sessions,
            &encoder_frames,
            self.beam_size,
            self.vocab.vocab_size(),
        )?;

        let mut pieces: Vec<PieceToken> = Vec::with_capacity(result.token_ids.len());
        for (i, &id) in result.token_ids.iter().enumerate() {
            let conf = compute_token_confidence(&result.logits[i]);
            pieces.push(PieceToken {
                id,
                frame: result.frames[i],
                margin: conf.margin,
                tsallis_norm: conf.tsallis_norm,
            });
        }

        let words = pieces_to_words(&self.vocab, &pieces)?;
        let frame_shift_s = self.frame_shift_ms / 1000.0;

        let word_results: Vec<WordResult> = words
            .into_iter()
            .map(|w| WordResult {
                text: w.text,
                start: w.start_frame as f32 * frame_shift_s,
                end: w.end_frame as f32 * frame_shift_s,
                confidence: word_confidence(w.margin_min, w.tsallis_max),
                margin_min: w.margin_min,
                tsallis_max: w.tsallis_max,
            })
            .collect();

        let text = word_results
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        Ok(DecodeResult { text, words: word_results })
    }
}
```

- [ ] **Step 2: Register the module**

In `frontend/src-tauri/src/rnnt_decoder/mod.rs`, replace:

```rust
pub mod beam_search;
pub mod confidence;
pub mod features;
pub mod sessions;
pub mod vocab;
```

with:

```rust
pub mod beam_search;
pub mod confidence;
pub mod engine;
pub mod features;
pub mod sessions;
pub mod vocab;
```

- [ ] **Step 3: Verify compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: `Finished` with no errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/rnnt_decoder/engine.rs frontend/src-tauri/src/rnnt_decoder/mod.rs
git commit -m "feat(rnnt): add RnntDecoder facade tying decode + confidence together"
```

---

### Task 9: Manual smoke test across all three families (required before this is considered done)

**Files:** none (verification only)

This is the gate before Phase B (ROVER merge) can build on top of `rnnt_decoder` — it must produce sane, confidence-annotated output on every model family it will need to pair up.

- [ ] **Step 1: Full build and automated test suite**

Run: `cd frontend/src-tauri && cargo test rnnt_decoder -- --nocapture`
Expected: all non-`#[ignore]`d tests PASS (features: 2, vocab: 2, confidence: 3, beam_search: 2 — 9 total).

- [ ] **Step 2: `RnntDecoder::decode` end-to-end on ZipFormer 30M int8**

Write and run one more `#[ignore]`d manual test (or reuse Task 7's smoke test structure) that calls `RnntDecoder::load(...).decode(&samples, sample_rate)` on the already-downloaded ZipFormer 30M int8 model files, on a short Vietnamese WAV. Print `result.text` and, for each `WordResult`, its `text`, `start`, `end`, and `confidence`.

Expected:
- `result.text` is readable Vietnamese, consistent with Task 7's beam search output.
- Confidence values are plausible: clearly-articulated words score noticeably higher than mumbled/quiet/background-noise words if the sample has any of the latter. All values fall within `[0, 1]` (mathematically guaranteed by the formula, but eyeball it — a bug could produce NaN or out-of-range values that still "compile" but are meaningless).

- [ ] **Step 3: Repeat for Gipformer 65M int8 and Sherpa-ONNX Zipformer VI 2025 (full)**

Same test, pointed at the other two families' already-downloaded model directories (Gipformer needs its `tokens.txt`/`config.json` fallback resolved the same way `asr_engine` already resolves it — just point directly at whichever file exists; Sherpa VI 2025 only has a `full` variant, no `int8`).

**This is where `CONTEXT_SIZE = 2` / `BLANK_ID = 0` get their real verification**, per the design spec's explicit risk note — if either assumption is wrong for a given family, decode will fail loudly (shape mismatch from `ort`, or a `panic` on an out-of-range token id in `Vocab::piece`) rather than silently producing wrong output. If that happens for one specific family, that family's actual context size / blank id needs to be determined from its ONNX graph (e.g. the decoder's `y` input shape) and threaded through as a per-family parameter — small, contained follow-up, not a redesign.

- [ ] **Step 4: Record findings**

No file changes required, but note in the PR description (or wherever this work is being tracked) which families passed cleanly and which needed a `CONTEXT_SIZE`/`BLANK_ID` adjustment, since Phase B's design will need to know whether that parameter is universal or per-family.

---

## Spec coverage checklist

| Spec requirement | Task |
|---|---|
| `rnnt_decoder` module, fully decoupled from `asr_engine`/sherpa-onnx | Task 2 (mod.rs structure), confirmed throughout — no file imports `asr_engine` or `sherpa_onnx` |
| Fbank matching exact reference-app parameters | Task 2 |
| `kaldi-native-fbank` field names verified, not guessed | Task 1 |
| Encoder/decoder/joiner tensor contract | Task 5, with fallback verification path if names are wrong |
| Modified beam search (global top-k, log-sum-exp merge, decoder cache, length-normalized pick) | Task 7 |
| Staged build order (fbank → greedy → beam search → confidence) | Verification milestones land in Task 2 (fbank), Task 6 (greedy text), Task 7 (beam search text), Task 9 (confidence values eyeballed against real audio) — in that order. Task 3 (vocab) and Task 4 (confidence math) are implemented earlier than their *verification* stage because both are pure functions with no ONNX dependency and zero risk of being affected by beam-search bugs; only their real-world correctness check waits its turn. |
| Per-token margin/Tsallis confidence | Task 4 |
| Word-level aggregation (margin_min/tsallis_max) + `▁` boundary segmentation | Task 3 |
| No hotword/context graph | Confirmed absent from Task 7's beam search — no `ctx_graph` field anywhere |
| No UI/DB/call-site changes (Phase A is standalone) | Confirmed — only `Cargo.toml` and `lib.rs`'s module list change outside `rnnt_decoder/` |
| Manual verification across all 3 families, context_size/blank_id treated as needing per-family confirmation | Task 9 |

---

## Notes for whoever executes this (e.g. via Cursor)

- Tasks 1–5 and 8 are fully mechanical — the code is complete and can be applied as-is (Task 1's fbank field names pending your own confirmation from the crate source, per its Step 2).
- Tasks 6, 7, and 9 have real manual verification gates that need actual model files and a WAV sample already present on disk (this repo already downloads ZipFormer 30M int8 on first use — reuse that). These aren't optional formalities; they're the only thing standing between "compiles" and "actually decodes Vietnamese speech correctly." Don't skip straight to Task 8 without having watched Task 6 and Task 7 print real, readable text first.
- If you get stuck on an `ort` shape-mismatch error, the most common cause in transducer ports like this is an off-by-one in `CONTEXT_SIZE`/`BLANK_ID` or a batch-dimension mismatch between `encoder_outs`/`decoder_outs` fed to the joiner — re-read Task 7's `modified_beam_search` against the Python reference algorithm described in the design spec rather than guessing.
