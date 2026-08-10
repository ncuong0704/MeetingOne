# Speaker Diarization (File Import) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add "who spoke when" to file-import transcripts — an opt-in checkbox runs offline speaker
diarization (pyannote segmentation + CAM++ embedding + agglomerative clustering, all via `ort`, no
sherpa-onnx C++ dependency) alongside ROVER/CAPU, then aligns the result onto the finished
transcript so each segment carries a renamable, colored speaker label.

**Architecture:** New `diarization_engine/` module (mirrors `capu_engine/`/`rover_engine/`) runs
independently of ASR on the same decoded 16kHz samples, producing `Vec<SpeakerTurn>`. A pure
alignment function maps those turns onto the already-finalized `TranscriptSegment`s by time
overlap. A new `meeting_speakers` table holds renamable display names; `transcripts.speaker_id`
links each row to one. Frontend groups consecutive same-speaker segments into labeled blocks in
`FlowingTranscriptView`.

**Tech Stack:** Rust (`ort` for ONNX inference, `bzip2-rs`+`tar` for model extraction, `sqlx`),
TypeScript/React (Tauri commands via `invoke`).

**Spec:** `docs/superpowers/specs/2026-08-10-speaker-diarization-design.md`

---

## Before you start

Four spots below require reading real, current code before writing any implementation — **do not
implement from memory of how diarization pipelines "usually" work, and do not guess at code this
plan couldn't verify directly**:

- **Tasks 4, 5, 8** port genuinely intricate reference logic (pyannote-segmentation window
  reconciliation, sample-index extraction, and result finalization) from sherpa-onnx's C++
  implementation. Each task's Step 1 is "fetch and read the real source." The exact index
  arithmetic in the reference is non-obvious (verified by direct inference against the real ONNX
  models, not just reading code) and must be ported faithfully or clustering quality silently
  degrades.
- **Task 14 Step 7** flags an unresolved timing problem (the meeting id diarization needs to attach
  speaker rows to doesn't exist yet at the point in the pipeline where diarization naturally runs)
  — read the real call sequence between `batch_transcribe` and `TranscriptsRepository::save_transcript`
  before writing that step's code, per the note inline there.

Do Task 14 before Task 13 (Task 13's `diarization_init` reads the settings fields Task 14 adds).

All other tasks (ONNX I/O shapes, feature-extraction parameters, clustering algorithm, DB schema,
download pattern) are already fully verified below — implement those directly.

---

### Task 1: Model acquisition — download, extract, verify

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`
- Modify: `frontend/src-tauri/src/config.rs`
- Create: `frontend/src-tauri/src/diarization_engine/mod.rs`
- Create: `frontend/src-tauri/src/diarization_engine/commands.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Add the `bzip2-rs` dependency and confirm its real API**

The segmentation model ships only as a `.tar.bz2` archive (no standalone `.onnx` on GitHub
releases); the embedding model is a plain `.onnx` file, no extraction needed. `tar = "0.4"` is
already a workspace dependency (used for Linux packaging) but doesn't decompress bzip2 — add a
pure-Rust bzip2 decoder (avoids a new native/C build dependency on top of the existing
Windows/macOS/Linux build matrix):

```bash
cd frontend/src-tauri
cargo add bzip2-rs
```

Confirmed real API (`docs.rs/bzip2-rs`, crate root re-export): `bzip2_rs::DecoderReader::new(reader)`
implements `std::io::Read`, wrapping any `Read` source and yielding decompressed bytes — pure Rust,
no C library. Verify this compiles standalone before building on it:

```rust
// scratch check, delete after confirming — not part of the final module
use bzip2_rs::DecoderReader;
fn _check<R: std::io::Read>(r: R) -> impl std::io::Read {
    DecoderReader::new(r)
}
```

- [ ] **Step 2: Model constants in `config.rs`**

Both URLs and both file sizes below are verified directly (`gh release view speaker-segmentation-models/speaker-recongition-models --repo k2-fsa/sherpa-onnx`, and by extracting the real archive and checking the extracted file size — not guessed):

```rust
// Speaker diarization — pyannote segmentation-3.0 (int8) + 3D-Speaker CAM++ embedding,
// both from the k2-fsa/sherpa-onnx model zoo. See
// docs/superpowers/specs/2026-08-10-speaker-diarization-design.md for why these two models.
pub const DIARIZATION_SUBDIR: &str = "diarization-vi";

pub const DIARIZATION_SEGMENTATION_ARCHIVE_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2";
/// Path of the int8 model *inside* the tar archive — verified via `tar -tjf` on the real
/// download, not guessed. The archive also contains the fp32 model, a LICENSE, and several
/// Python export scripts we don't need.
pub const DIARIZATION_SEGMENTATION_ARCHIVE_MEMBER: &str =
    "sherpa-onnx-pyannote-segmentation-3-0/model.int8.onnx";
/// Whole-archive download size, for the progress bar (matches `CAPU_MODEL_SIZE_BYTES`'s role).
pub const DIARIZATION_SEGMENTATION_ARCHIVE_SIZE_BYTES: u64 = 6_958_444;
/// Extracted member size — used to sanity-check extraction succeeded, not for the progress bar.
pub const DIARIZATION_SEGMENTATION_EXTRACTED_SIZE_BYTES: u64 = 1_540_506;
/// Local filename after extraction (renamed from `model.int8.onnx` for clarity next to the
/// embedding model in the same directory).
pub const DIARIZATION_SEGMENTATION_MODEL_FILE: &str = "segmentation.int8.onnx";

pub const DIARIZATION_EMBEDDING_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx";
pub const DIARIZATION_EMBEDDING_MODEL_FILE: &str = "embedding-campplus.onnx";
pub const DIARIZATION_EMBEDDING_SIZE_BYTES: u64 = 28_281_164;

// Segmentation model I/O (verified by direct ONNX inference + reading the model's own
// `metadata_props`, not assumed):
/// Input "x": f32 [1, 1, N] raw waveform @ 16kHz, one 10s window per call.
pub const DIARIZATION_SEG_WINDOW_SAMPLES: usize = 160_000;
/// 90% overlap between consecutive windows (1s shift).
pub const DIARIZATION_SEG_WINDOW_SHIFT_SAMPLES: usize = 16_000;
/// Output "y": f32 [1, 589, 7] log-probabilities (final graph node is LogSoftmax) — 589 is
/// exact for a full 160_000-sample window (every window is zero-padded to exactly this size
/// before inference, so this is a constant, not a formula to recompute per-chunk).
pub const DIARIZATION_SEG_OUTPUT_FRAMES: usize = 589;
pub const DIARIZATION_SEG_NUM_CLASSES: usize = 7;
/// Frame→sample grid used when reconciling overlapping chunks into one global timeline.
pub const DIARIZATION_RECEPTIVE_FIELD_SHIFT: usize = 270;
pub const DIARIZATION_NUM_LOCAL_SPEAKERS: usize = 3;

// Embedding model I/O:
/// Input "x": f32 [1, T, 80] — 80-dim fbank, NOT raw waveform (see Task 2).
pub const DIARIZATION_EMBEDDING_DIM: usize = 192;
/// (chunk, local-speaker) pairs with fewer active frames than this are skipped entirely —
/// matches the reference's "skip segments less than 10 frames" comment.
pub const DIARIZATION_MIN_ACTIVE_FRAMES: usize = 10;

// Clustering / finalization (reference defaults, from `fast-clustering-config` and
// `offline-speaker-diarization.h`):
pub const DIARIZATION_DEFAULT_CLUSTER_THRESHOLD: f32 = 0.5;
pub const DIARIZATION_MIN_DURATION_ON_SEC: f64 = 0.3;
pub const DIARIZATION_MIN_DURATION_OFF_SEC: f64 = 0.5;
```

- [ ] **Step 3: `diarization_engine/commands.rs` — download + extract**

Mirror `capu_engine/commands.rs`'s `resolve_capu_dir`/`capu_is_model_downloaded`/`download_capu_files`
exactly, adapted for two files from two different URLs (not one HF-repo-prefix pattern) and one
archive extraction:

```rust
use crate::config::{
    DIARIZATION_EMBEDDING_MODEL_FILE, DIARIZATION_EMBEDDING_SIZE_BYTES, DIARIZATION_EMBEDDING_URL,
    DIARIZATION_SEGMENTATION_ARCHIVE_MEMBER, DIARIZATION_SEGMENTATION_ARCHIVE_SIZE_BYTES,
    DIARIZATION_SEGMENTATION_ARCHIVE_URL, DIARIZATION_SEGMENTATION_EXTRACTED_SIZE_BYTES,
    DIARIZATION_SEGMENTATION_MODEL_FILE, DIARIZATION_SUBDIR,
};
use anyhow::{anyhow, Result};
use bzip2_rs::DecoderReader;
use futures_util::StreamExt;
use log::{error, info};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};

fn resolve_diarization_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("models").join(DIARIZATION_SUBDIR))
}

#[tauri::command]
pub async fn diarization_is_model_downloaded<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    let dir = resolve_diarization_dir(&app).ok_or_else(|| "Could not resolve app data directory".to_string())?;
    Ok(dir.join(DIARIZATION_SEGMENTATION_MODEL_FILE).exists()
        && dir.join(DIARIZATION_EMBEDDING_MODEL_FILE).exists())
}

#[tauri::command]
pub async fn diarization_download_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = resolve_diarization_dir(&app).ok_or_else(|| "Could not resolve app data directory".to_string())?;
    let app_clone = app.clone();

    tauri::async_runtime::spawn(async move {
        match download_diarization_files(&dir, &app_clone).await {
            Ok(()) => {
                info!("Diarization model download complete");
                let _ = app_clone.emit("diarization-model-download-complete", ());
            }
            Err(e) => {
                error!("Diarization model download failed: {}", e);
                let _ = app_clone.emit(
                    "diarization-model-download-error",
                    serde_json::json!({ "error": e.to_string() }),
                );
            }
        }
    });

    Ok(())
}

async fn download_to_bytes(url: &str, client: &reqwest::Client) -> Result<Vec<u8>> {
    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        anyhow::bail!("HTTP {} for {}", response.status(), url);
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(bytes)
}

/// Extracts exactly one member from a `.tar.bz2` archive already in memory, returning its bytes.
/// Errors if the member isn't found — a mismatched archive layout should fail loudly, not
/// silently produce a missing model file that only surfaces as a confusing load error later.
fn extract_tar_bz2_member(archive_bytes: &[u8], member_path: &str) -> Result<Vec<u8>> {
    let decoder = DecoderReader::new(archive_bytes);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().to_string();
        if path == member_path {
            let mut out = Vec::new();
            entry.read_to_end(&mut out)?;
            return Ok(out);
        }
    }
    Err(anyhow!("Member {} not found in archive", member_path))
}

async fn download_diarization_files<R: Runtime>(dir: &Path, app: &AppHandle<R>) -> Result<()> {
    tokio::fs::create_dir_all(dir).await?;

    let seg_dest = dir.join(DIARIZATION_SEGMENTATION_MODEL_FILE);
    let emb_dest = dir.join(DIARIZATION_EMBEDDING_MODEL_FILE);
    if seg_dest.exists() && emb_dest.exists() {
        let _ = app.emit("diarization-model-download-progress", serde_json::json!({ "progress": 100 }));
        return Ok(());
    }

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .build()?;

    let total_bytes = DIARIZATION_SEGMENTATION_ARCHIVE_SIZE_BYTES + DIARIZATION_EMBEDDING_SIZE_BYTES;
    let mut done: u64 = 0;

    if !seg_dest.exists() {
        let archive_bytes = download_to_bytes(crate::config::DIARIZATION_SEGMENTATION_ARCHIVE_URL, &client).await?;
        let onnx_bytes = extract_tar_bz2_member(&archive_bytes, DIARIZATION_SEGMENTATION_ARCHIVE_MEMBER)?;
        if (onnx_bytes.len() as u64) < DIARIZATION_SEGMENTATION_EXTRACTED_SIZE_BYTES / 2 {
            anyhow::bail!(
                "Extracted segmentation model suspiciously small ({} bytes) — archive layout may have changed",
                onnx_bytes.len()
            );
        }
        let tmp = dir.join(format!("{}.tmp", DIARIZATION_SEGMENTATION_MODEL_FILE));
        tokio::fs::write(&tmp, &onnx_bytes).await?;
        tokio::fs::rename(&tmp, &seg_dest).await?;
        done += DIARIZATION_SEGMENTATION_ARCHIVE_SIZE_BYTES;
        let pct = ((done * 100) / total_bytes.max(1)).min(99) as u8;
        let _ = app.emit("diarization-model-download-progress", serde_json::json!({ "progress": pct }));
    } else {
        done += DIARIZATION_SEGMENTATION_ARCHIVE_SIZE_BYTES;
    }

    if !emb_dest.exists() {
        let bytes = download_to_bytes(DIARIZATION_EMBEDDING_URL, &client).await?;
        let tmp = dir.join(format!("{}.tmp", DIARIZATION_EMBEDDING_MODEL_FILE));
        tokio::fs::write(&tmp, &bytes).await?;
        tokio::fs::rename(&tmp, &emb_dest).await?;
    }

    let _ = app.emit("diarization-model-download-progress", serde_json::json!({ "progress": 100 }));
    Ok(())
}
```

- [ ] **Step 4: `mod.rs`, register module in `lib.rs`**

```rust
// diarization_engine/mod.rs
pub mod commands;
```

In `lib.rs`, add `pub mod diarization_engine;` next to `pub mod capu_engine;` (line 57), and add
`diarization_engine::commands::diarization_is_model_downloaded` +
`diarization_engine::commands::diarization_download_model` to the `generate_handler!` list next to
the `capu_download_model`/`capu_init` entries (line ~498-499).

- [ ] **Step 5: Verify compile**

```bash
cd frontend/src-tauri && cargo check
```

- [ ] **Step 6: Integration test — real download + extraction**

```rust
// bottom of diarization_engine/commands.rs
#[cfg(test)]
mod tests {
    use super::*;

    /// Requires network access. Run with:
    /// cargo test --manifest-path frontend/src-tauri/Cargo.toml -- --ignored diarization_engine::commands
    #[tokio::test]
    #[ignore = "requires network access, downloads ~35MB"]
    async fn download_and_extract_real_models() {
        let dir = std::env::temp_dir().join(format!("diarization-test-{}", std::process::id()));
        let client = reqwest::Client::new();

        let archive_bytes = download_to_bytes(crate::config::DIARIZATION_SEGMENTATION_ARCHIVE_URL, &client)
            .await
            .expect("download segmentation archive");
        let onnx_bytes =
            extract_tar_bz2_member(&archive_bytes, DIARIZATION_SEGMENTATION_ARCHIVE_MEMBER)
                .expect("extract model.int8.onnx");
        assert_eq!(onnx_bytes.len() as u64, DIARIZATION_SEGMENTATION_EXTRACTED_SIZE_BYTES);

        let emb_bytes = download_to_bytes(DIARIZATION_EMBEDDING_URL, &client)
            .await
            .expect("download embedding model");
        assert_eq!(emb_bytes.len() as u64, DIARIZATION_EMBEDDING_SIZE_BYTES);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

Run it once manually (`cargo test --manifest-path frontend/src-tauri/Cargo.toml -- --ignored diarization_engine::commands::tests::download_and_extract_real_models --nocapture`) before moving on — the exact byte-size assertions catch a silently-changed upstream file immediately.

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/Cargo.toml frontend/src-tauri/Cargo.lock frontend/src-tauri/src/config.rs frontend/src-tauri/src/diarization_engine frontend/src-tauri/src/lib.rs
git commit -m "feat(diarization): add model download/extraction for segmentation + embedding models"
```

---

### Task 2: Feature extraction for the embedding model (`diarization_engine/features.rs`)

**Files:**
- Create: `frontend/src-tauri/src/diarization_engine/features.rs`
- Modify: `frontend/src-tauri/src/diarization_engine/mod.rs`

The CAM++ embedding model needs 80-dim fbank, **not raw waveform** — verified from the embedding
ONNX graph's input shape `[N, T, 80]` and from `speaker-embedding-extractor-model.cc`'s
`SpeakerEmbeddingExtractorGeneralImpl::Compute`. Its exact preprocessing differs from the existing
`rnnt_decoder::features::compute_fbank` (used for ASR) in several parameters — verified against
`sherpa-onnx`'s `CreateStream()`/`InitFbank` and cross-checked against
`kaldi-native-fbank-0.1.0`'s actual `FrameOptions`/`MelOptions` struct fields (not assumed to
exist): `frame_length_ms=25.0` (crate default, matches), `frame_shift_ms=10.0` (crate default,
matches), `dither=0.0` (override — crate default is `0.00003`), `high_freq=-400.0` (override — crate
default is `0.0`, meaning Nyquist; `-400.0` means `sample_rate/2 - 400 = 7600Hz`), `low_freq=20.0`
(crate default, matches), `snip_edges=false` (override — crate default is `true`),
`window_type="povey"` (crate default, matches), `remove_dc_offset=true` (crate default, matches),
`round_to_power_of_two=true` (crate default, matches), `preemph_coeff=0.97` (crate default,
matches). **Plus mandatory post-processing** the ASR path doesn't do: per-mel-bin global mean
subtraction (CMN) over the segment's own frames (`feature_normalize_type=global-mean` in the
model's metadata).

- [ ] **Step 1: Write the failing tests**

```rust
use anyhow::Result;
use kaldi_native_fbank::mel::MelOptions;
use kaldi_native_fbank::online::FeatureComputer;
use kaldi_native_fbank::{FbankComputer, FbankOptions, FrameOptions, OnlineFeature};

pub const EMBEDDING_FBANK_DIM: usize = 80;

pub fn compute_embedding_fbank(samples: &[f32], sample_rate: f32) -> Result<Vec<Vec<f32>>> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_80_dim_frames_with_zero_mean_per_bin() {
        let samples: Vec<f32> = (0..32000).map(|i| (i as f32 * 0.02).sin() * 0.2).collect();
        let frames = compute_embedding_fbank(&samples, 16000.0).expect("fbank should succeed");
        assert!(!frames.is_empty());
        for frame in &frames {
            assert_eq!(frame.len(), EMBEDDING_FBANK_DIM);
        }
        // Global mean subtraction: each column (mel bin) across all frames must average ~0.
        for bin in 0..EMBEDDING_FBANK_DIM {
            let mean: f32 = frames.iter().map(|f| f[bin]).sum::<f32>() / frames.len() as f32;
            assert!(mean.abs() < 1e-3, "bin {} mean {} not ~0 after CMN", bin, mean);
        }
    }

    #[test]
    fn produces_no_nan_or_inf_values() {
        let samples = vec![0.0f32; 16000];
        let frames = compute_embedding_fbank(&samples, 16000.0).expect("fbank should succeed on silence");
        for frame in &frames {
            for &v in frame {
                assert!(v.is_finite(), "non-finite fbank value: {}", v);
            }
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail** (compile error on `todo!()` / assertion failure)

- [ ] **Step 3: Implement**

```rust
pub fn compute_embedding_fbank(samples: &[f32], sample_rate: f32) -> Result<Vec<Vec<f32>>> {
    let frame_opts = FrameOptions {
        samp_freq: sample_rate,
        dither: 0.0,
        snip_edges: false,
        ..Default::default()
    };
    let mel_opts = MelOptions {
        num_bins: EMBEDDING_FBANK_DIM,
        high_freq: -400.0,
        ..Default::default()
    };
    let opts = FbankOptions {
        frame_opts,
        mel_opts,
        use_energy: false,
        ..Default::default()
    };

    let computer = FbankComputer::new(opts)
        .map_err(|e| anyhow::anyhow!("Failed to create fbank computer: {}", e))?;
    let mut online = OnlineFeature::new(FeatureComputer::Fbank(computer));
    online.accept_waveform(sample_rate, samples);
    online.input_finished();

    let mut frames = online.features;
    subtract_global_mean(&mut frames);
    Ok(frames)
}

/// Per-mel-bin mean subtraction across all frames of `frames` (in place) — the embedding
/// model's own `feature_normalize_type == "global-mean"` requirement. "Global" here means
/// global to the one segment being embedded, not to the whole recording.
fn subtract_global_mean(frames: &mut [Vec<f32>]) {
    if frames.is_empty() {
        return;
    }
    let dim = frames[0].len();
    let mut means = vec![0.0f32; dim];
    for frame in frames.iter() {
        for (i, &v) in frame.iter().enumerate() {
            means[i] += v;
        }
    }
    let n = frames.len() as f32;
    for m in means.iter_mut() {
        *m /= n;
    }
    for frame in frames.iter_mut() {
        for (i, v) in frame.iter_mut().enumerate() {
            *v -= means[i];
        }
    }
}
```

- [ ] **Step 4: Register module, run tests to verify they pass**

Add `pub mod features;` to `diarization_engine/mod.rs`. Run:
```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml diarization_engine::features
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/diarization_engine
git commit -m "feat(diarization): CAM++ fbank feature extraction with global-mean CMN"
```

---

### Task 3: Segmentation model — single-window inference + powerset decode (`diarization_engine/segmentation.rs`)

**Files:**
- Create: `frontend/src-tauri/src/diarization_engine/segmentation.rs`
- Modify: `frontend/src-tauri/src/diarization_engine/mod.rs`

Powerset table verified by deriving it from `InitPowersetMapping`'s loop (`num_speakers=3`,
`powerset_max_classes=2`, index `k` starts at 1 so class 0 is silence):

| class | active local speakers |
|---|---|
| 0 | (none — silence) |
| 1 | {0} |
| 2 | {1} |
| 3 | {2} |
| 4 | {0,1} |
| 5 | {0,2} |
| 6 | {1,2} |

- [ ] **Step 1: Write the failing tests** (pure powerset-decode logic first — no ONNX needed)

```rust
use crate::config::{DIARIZATION_NUM_LOCAL_SPEAKERS, DIARIZATION_SEG_NUM_CLASSES};

/// class index -> which of the 3 local speakers are active, for pyannote-segmentation-3.0's
/// powerset encoding (num_speakers=3, powerset_max_classes=2). Verified by deriving it from
/// the reference's `InitPowersetMapping` loop, not assumed.
pub const POWERSET_TABLE: [[bool; DIARIZATION_NUM_LOCAL_SPEAKERS]; DIARIZATION_SEG_NUM_CLASSES] = [
    [false, false, false], // 0: silence
    [true, false, false],  // 1: {0}
    [false, true, false],  // 2: {1}
    [false, false, true],  // 3: {2}
    [true, true, false],   // 4: {0,1}
    [true, false, true],   // 5: {0,2}
    [false, true, true],   // 6: {1,2}
];

/// Argmax per frame over the model's raw output, then powerset table lookup. `log_probs` is
/// flattened `[num_frames * num_classes]`, row-major (matches how `outputs["y"]` is read).
/// Works directly on log-probabilities — argmax is monotonic under `exp()`, no need to
/// exponentiate first (the reference doesn't either).
pub fn to_multi_label(log_probs: &[f32], num_frames: usize, num_classes: usize) -> Vec<[bool; DIARIZATION_NUM_LOCAL_SPEAKERS]> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_multi_label_decodes_silence_and_single_speaker_frames() {
        // frame 0: class 0 (silence) wins; frame 1: class 2 ({1}) wins.
        let mut log_probs = vec![-10.0f32; 2 * 7];
        log_probs[0] = -0.1; // frame 0, class 0
        log_probs[1 * 7 + 2] = -0.1; // frame 1, class 2
        let labels = to_multi_label(&log_probs, 2, 7);
        assert_eq!(labels, vec![[false, false, false], [false, true, false]]);
    }

    #[test]
    fn to_multi_label_decodes_overlap_frame() {
        let mut log_probs = vec![-10.0f32; 7];
        log_probs[5] = -0.1; // class 5 = {0,2}
        let labels = to_multi_label(&log_probs, 1, 7);
        assert_eq!(labels, vec![[true, false, true]]);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

- [ ] **Step 3: Implement `to_multi_label`**

```rust
pub fn to_multi_label(log_probs: &[f32], num_frames: usize, num_classes: usize) -> Vec<[bool; DIARIZATION_NUM_LOCAL_SPEAKERS]> {
    let mut out = Vec::with_capacity(num_frames);
    for frame in 0..num_frames {
        let row = &log_probs[frame * num_classes..(frame + 1) * num_classes];
        let (best_class, _) = row
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        out.push(POWERSET_TABLE[best_class]);
    }
    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

- [ ] **Step 5: ONNX session wrapper for one 10-second window**

```rust
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use crate::config::{DIARIZATION_SEG_OUTPUT_FRAMES, DIARIZATION_SEG_NUM_CLASSES, DIARIZATION_SEG_WINDOW_SAMPLES};

pub struct SegmentationEngine {
    session: Session,
}

impl SegmentationEngine {
    pub fn load(model_path: &std::path::Path, threads: usize) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("Failed to create segmentation session builder: {}", e))?
            .with_intra_threads(threads.max(1))
            .map_err(|e| anyhow!("Failed to set segmentation intra-op threads: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("Failed to load segmentation model: {}", e))?;
        Ok(Self { session })
    }

    /// Runs one 10s window through the model. `window` must be exactly
    /// `DIARIZATION_SEG_WINDOW_SAMPLES` long — zero-pad shorter windows before calling (the
    /// last window of a recording is virtually always shorter than 10s).
    pub fn run_window(&mut self, window: &[f32]) -> Result<Vec<[bool; 3]>> {
        if window.len() != DIARIZATION_SEG_WINDOW_SAMPLES {
            return Err(anyhow!(
                "Segmentation window must be exactly {} samples, got {}",
                DIARIZATION_SEG_WINDOW_SAMPLES,
                window.len()
            ));
        }
        let x = TensorRef::from_array_view(([1usize, 1usize, DIARIZATION_SEG_WINDOW_SAMPLES], window))
            .map_err(|e| anyhow!("Failed to build segmentation input tensor: {}", e))?;
        let outputs = self
            .session
            .run(ort::inputs![x])
            .map_err(|e| anyhow!("Segmentation inference failed: {}", e))?;
        let (shape, data) = outputs["y"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("Failed to read segmentation output: {}", e))?;
        if shape.len() != 3 || shape[1] as usize != DIARIZATION_SEG_OUTPUT_FRAMES || shape[2] as usize != DIARIZATION_SEG_NUM_CLASSES {
            return Err(anyhow!("Unexpected segmentation output shape {:?}", shape));
        }
        Ok(super::segmentation::to_multi_label(data, DIARIZATION_SEG_OUTPUT_FRAMES, DIARIZATION_SEG_NUM_CLASSES))
    }
}
```

- [ ] **Step 6: Register module, verify compile**

Add `pub mod segmentation;` to `mod.rs`. `cargo check --manifest-path frontend/src-tauri/Cargo.toml`.

- [ ] **Step 7: Integration test on real audio (requires downloaded model)**

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    fn model_path() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap())
            .join("AppData/Roaming/com.meetingone.app/models/diarization-vi/segmentation.int8.onnx")
    }

    #[test]
    #[ignore = "requires downloaded diarization model on disk"]
    fn run_window_on_silence_yields_all_silence_frames() {
        let mut engine = SegmentationEngine::load(&model_path(), 2).expect("load segmentation model");
        let window = vec![0.0f32; crate::config::DIARIZATION_SEG_WINDOW_SAMPLES];
        let labels = engine.run_window(&window).expect("run window");
        assert_eq!(labels.len(), crate::config::DIARIZATION_SEG_OUTPUT_FRAMES);
        let active_frames = labels.iter().filter(|l| l.iter().any(|&x| x)).count();
        // Pure digital silence should overwhelmingly decode as class 0 — allow a small margin
        // for model noise rather than requiring literally zero.
        assert!(
            (active_frames as f32) < (labels.len() as f32) * 0.05,
            "{} / {} frames marked active on silence",
            active_frames,
            labels.len()
        );
    }
}
```

- [ ] **Step 8: Run the integration test manually, confirm it passes**

```bash
cargo test --manifest-path frontend/src-tauri/Cargo.toml -- --ignored diarization_engine::segmentation::integration_tests --nocapture
```

- [ ] **Step 9: Commit**

```bash
git add frontend/src-tauri/src/diarization_engine
git commit -m "feat(diarization): segmentation model wrapper + powerset decode"
```

---

### Task 4: Segmentation windowing + cross-chunk reconciliation (`diarization_engine/windowing.rs`)

**Files:**
- Create: `frontend/src-tauri/src/diarization_engine/windowing.rs`
- Modify: `frontend/src-tauri/src/diarization_engine/mod.rs`

- [ ] **Step 1: Fetch and read the real reference source before writing any Rust**

```bash
curl -s https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/sherpa-onnx/csrc/offline-speaker-diarization-impl.cc -o /tmp/offline-speaker-diarization-impl.cc
```

Read the whole file. Find and understand, specifically:
- The windowing loop that slides `DIARIZATION_SEG_WINDOW_SAMPLES`-sized windows across the full
  recording at `DIARIZATION_SEG_WINDOW_SHIFT_SAMPLES` stride, including how the last
  (necessarily shorter) window is zero-padded rather than dropped.
- `ComputeSpeakersPerFrame` (or equivalently named function) — how each chunk's independent
  589-frame `[bool;3]` output gets placed onto a **global** per-frame timeline gridded at
  `DIARIZATION_RECEPTIVE_FIELD_SHIFT` (270-sample) resolution, and how overlapping chunks'
  predictions at the same global frame get combined (the report from prior research described this
  as an *averaged, rounded* combination — confirm the exact rounding/threshold from the source
  itself, don't rely on that paraphrase).
- The exact frame-index-within-chunk → global-frame-index mapping formula, including the detail
  that it's proportional (`chunk_frame / 589 * window_size`) rather than a flat
  `receptive_field_shift` multiplication, which is why 589 frames over a 160000-sample window
  don't map 1:1 onto `160000/270 ≈ 592.6` — this mismatch is real and must be reproduced exactly
  as the reference computes it, not "fixed" to look more consistent.

- [ ] **Step 2: Define the types**

```rust
use crate::config::{
    DIARIZATION_NUM_LOCAL_SPEAKERS, DIARIZATION_RECEPTIVE_FIELD_SHIFT,
    DIARIZATION_SEG_OUTPUT_FRAMES, DIARIZATION_SEG_WINDOW_SAMPLES, DIARIZATION_SEG_WINDOW_SHIFT_SAMPLES,
};
use super::segmentation::SegmentationEngine;
use anyhow::Result;

/// One 10s window's decoded per-frame local-speaker activity, plus where it sits in the
/// original recording.
pub struct ChunkLabels {
    pub chunk_index: usize,
    pub sample_offset: usize,
    pub labels: Vec<[bool; DIARIZATION_NUM_LOCAL_SPEAKERS]>, // len == DIARIZATION_SEG_OUTPUT_FRAMES
}

/// Global per-frame speaker activity after reconciling all overlapping chunks, gridded at
/// `DIARIZATION_RECEPTIVE_FIELD_SHIFT`-sample resolution. `speaker_count[frame][local_speaker]`
/// is `true` once reconciliation (per Step 1's exact algorithm) decides that speaker is active
/// at that global frame.
pub struct GlobalFrameLabels {
    pub frame_shift_samples: usize, // == DIARIZATION_RECEPTIVE_FIELD_SHIFT
    pub frames: Vec<[bool; DIARIZATION_NUM_LOCAL_SPEAKERS]>,
}
```

- [ ] **Step 3: Write the failing tests**

Cover, using small synthetic `ChunkLabels` (not real audio — this is pure reconciliation logic):
- A single chunk (no overlap to reconcile) round-trips its own labels onto the global grid at the
  expected sample positions.
- Two overlapping chunks that agree at a shared global frame produce that same label.
- Two overlapping chunks that disagree at a shared global frame resolve according to whatever
  exact averaging/rounding rule Step 1 found (write the test to match the real rule, not a guess).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_chunk_reconciles_to_itself() {
        // fill in once Step 1's exact mapping is known
    }

    #[test]
    fn overlapping_chunks_agreeing_at_shared_frame_stay_active() {
        // fill in once Step 1's exact mapping is known
    }

    #[test]
    fn overlapping_chunks_disagreeing_at_shared_frame_resolve_per_reference_rule() {
        // fill in once Step 1's exact mapping is known
    }
}
```

- [ ] **Step 4: Implement `slide_windows` and `compute_speakers_per_frame`**

```rust
/// Slides `DIARIZATION_SEG_WINDOW_SAMPLES`-sized windows over `samples` at
/// `DIARIZATION_SEG_WINDOW_SHIFT_SAMPLES` stride, zero-padding the final (necessarily
/// shorter) window rather than dropping it, and runs the segmentation model on each.
pub fn run_segmentation_windows(
    engine: &mut SegmentationEngine,
    samples: &[f32],
) -> Result<Vec<ChunkLabels>> {
    todo!("port the exact windowing loop found in Task 4 Step 1")
}

/// Reconciles all chunks' independent per-frame predictions into one global per-frame
/// timeline. Must reproduce the reference's exact frame->sample mapping and averaging rule
/// (see Task 4 Step 1) — this is the step most likely to silently diverge from correct
/// behavior if implemented from general understanding instead of the actual source.
pub fn compute_speakers_per_frame(chunks: &[ChunkLabels], total_samples: usize) -> GlobalFrameLabels {
    todo!("port the exact reconciliation found in Task 4 Step 1")
}
```

- [ ] **Step 5: Run tests to verify they pass**

- [ ] **Step 6: Register module, verify compile**

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/diarization_engine
git commit -m "feat(diarization): sliding-window segmentation + cross-chunk reconciliation"
```

---

### Task 5: Sample-index extraction for embedding (`diarization_engine/sample_indexes.rs`)

**Files:**
- Create: `frontend/src-tauri/src/diarization_engine/sample_indexes.rs`
- Modify: `frontend/src-tauri/src/diarization_engine/mod.rs`

- [ ] **Step 1: Fetch and read the real reference source**

**Correction from Task 4** (verified by the controller by re-fetching and reading the file
directly, not just trusting Task 4's implementer report): the plan originally pointed at
`offline-speaker-diarization-impl.cc`, but that file is only a 60-line factory with no diarization
logic in it. Everything lives in the header:

```bash
curl -s https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/sherpa-onnx/csrc/offline-speaker-diarization-pyannote-impl.h -o /tmp/pyannote-impl.h
```

Read `GetChunkSpeakerSampleIndexes` (header, confirmed at line ~412) and `ExcludeOverlap` (header,
confirmed at line ~487, called from inside `GetChunkSpeakerSampleIndexes` at its very first line —
port both together, `ExcludeOverlap` is not a separate task). Understand:
- For each `(chunk_index, local_speaker)` pair, how contiguous runs of "this speaker active" frames
  within that chunk get converted into sample ranges `(start_sample, end_sample)` in the *original*
  audio — this uses the same frame→sample mapping convention confirmed in Task 4.
- The **overlap exclusion** rule: frames where 2+ local speakers are simultaneously active must be
  *excluded* before they reach the embedding step (`ExcludeOverlap` zeroes those frames out) —
  overlapping speech never gets embedded as any one speaker's voice print.
- The **minimum active-frame filter**: `(chunk, speaker)` pairs with fewer than
  `DIARIZATION_MIN_ACTIVE_FRAMES` (10) active frames in that chunk are skipped entirely (too little
  signal for a reliable embedding).

- [ ] **Step 2: Write the failing tests**

Using small synthetic `GlobalFrameLabels`/per-chunk data:
- A chunk with one clearly active speaker for >10 frames produces one sample range for that
  speaker.
- A chunk where a speaker is active for fewer than 10 frames produces no sample range for them.
- A chunk with overlapping speakers (2+ active at some frames) excludes those specific frames from
  every speaker's range, per the exact rule found in Step 1.

- [ ] **Step 3: Implement**

```rust
use crate::config::DIARIZATION_MIN_ACTIVE_FRAMES;

/// Sample ranges (in the original recording) where exactly this one local speaker (within
/// this one chunk) is active, with overlapping-speech frames already excluded.
pub struct ChunkSpeakerSamples {
    pub chunk_index: usize,
    pub local_speaker: usize,
    pub sample_ranges: Vec<(usize, usize)>, // (start, end), end-exclusive
}

pub fn get_chunk_speaker_sample_indexes(
    chunks: &[super::windowing::ChunkLabels],
) -> Vec<ChunkSpeakerSamples> {
    todo!("port the exact extraction found in Task 5 Step 1, including overlap exclusion and the >= 10 active-frame filter")
}
```

- [ ] **Step 4: Run tests to verify they pass**

- [ ] **Step 5: Register module, verify compile**

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/diarization_engine
git commit -m "feat(diarization): per-chunk-speaker sample-range extraction with overlap exclusion"
```

---

### Task 6: Embedding model wrapper (`diarization_engine/embedding.rs`)

**Files:**
- Create: `frontend/src-tauri/src/diarization_engine/embedding.rs`
- Modify: `frontend/src-tauri/src/diarization_engine/mod.rs`

- [ ] **Step 1: Implement**

```rust
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use crate::config::DIARIZATION_EMBEDDING_DIM;
use super::features::compute_embedding_fbank;

pub struct EmbeddingEngine {
    session: Session,
}

impl EmbeddingEngine {
    pub fn load(model_path: &std::path::Path, threads: usize) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("Failed to create embedding session builder: {}", e))?
            .with_intra_threads(threads.max(1))
            .map_err(|e| anyhow!("Failed to set embedding intra-op threads: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("Failed to load embedding model: {}", e))?;
        Ok(Self { session })
    }

    /// Computes a raw (NOT L2-normalized — verified: measured embedding norms of ~13-16, not
    /// 1.0; normalize at the clustering stage instead, matching the reference) 192-dim voice
    /// embedding for `samples` (16kHz mono f32, any length — typically a few seconds).
    pub fn compute_embedding(&mut self, samples: &[f32], sample_rate: f32) -> Result<[f32; DIARIZATION_EMBEDDING_DIM]> {
        let frames = compute_embedding_fbank(samples, sample_rate)?;
        if frames.is_empty() {
            return Err(anyhow!("No fbank frames produced for embedding input of {} samples", samples.len()));
        }
        let num_frames = frames.len();
        let feat_dim = frames[0].len();
        let flat: Vec<f32> = frames.into_iter().flatten().collect();

        let x = TensorRef::from_array_view(([1usize, num_frames, feat_dim], flat.as_slice()))
            .map_err(|e| anyhow!("Failed to build embedding input tensor: {}", e))?;
        let outputs = self
            .session
            .run(ort::inputs![x])
            .map_err(|e| anyhow!("Embedding inference failed: {}", e))?;
        let (shape, data) = outputs["embedding"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("Failed to read embedding output: {}", e))?;
        if shape != [1, DIARIZATION_EMBEDDING_DIM as i64] {
            return Err(anyhow!("Unexpected embedding output shape {:?}", shape));
        }
        let mut out = [0.0f32; DIARIZATION_EMBEDDING_DIM];
        out.copy_from_slice(data);
        Ok(out)
    }
}
```

- [ ] **Step 2: Register module, verify compile**

- [ ] **Step 3: Integration test + empirical same/different-speaker sanity check**

This is the load-bearing correctness check for Task 2's feature-extraction parameters — if the
fbank config is subtly wrong, embeddings will still have the right *shape* but be useless for
clustering, which a shape-only test would miss entirely.

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    fn model_path() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap())
            .join("AppData/Roaming/com.meetingone.app/models/diarization-vi/embedding-campplus.onnx")
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (na * nb)
    }

    /// Requires two short WAV clips checked in or pointed to via env vars: one pair of
    /// same-speaker clips, one different-speaker clip. Set SAME_SPEAKER_CLIP_A,
    /// SAME_SPEAKER_CLIP_B, DIFFERENT_SPEAKER_CLIP (16kHz mono WAV, few seconds each — e.g.
    /// two halves of one VAD segment from a known single speaker, and one segment from a
    /// different speaker in the same or another recording).
    #[test]
    #[ignore = "requires downloaded embedding model + real speaker audio clips on disk"]
    fn same_speaker_embeddings_are_more_similar_than_different_speaker() {
        let a_path = std::env::var("SAME_SPEAKER_CLIP_A").expect("set SAME_SPEAKER_CLIP_A");
        let b_path = std::env::var("SAME_SPEAKER_CLIP_B").expect("set SAME_SPEAKER_CLIP_B");
        let c_path = std::env::var("DIFFERENT_SPEAKER_CLIP").expect("set DIFFERENT_SPEAKER_CLIP");

        let mut engine = EmbeddingEngine::load(&model_path(), 2).expect("load embedding model");
        let decode = |p: &str| crate::audio::decoder::load_audio_for_file_pipeline(std::path::Path::new(p), None)
            .expect("decode clip").0;

        let emb_a = engine.compute_embedding(&decode(&a_path), 16000.0).expect("embed a");
        let emb_b = engine.compute_embedding(&decode(&b_path), 16000.0).expect("embed b");
        let emb_c = engine.compute_embedding(&decode(&c_path), 16000.0).expect("embed c");

        let sim_same = cosine_similarity(&emb_a, &emb_b);
        let sim_diff = cosine_similarity(&emb_a, &emb_c);
        println!("same-speaker cosine similarity: {:.4}", sim_same);
        println!("different-speaker cosine similarity: {:.4}", sim_diff);
        assert!(
            sim_same > sim_diff,
            "same-speaker similarity ({:.4}) should exceed different-speaker similarity ({:.4}) — \
             if this fails, re-check the fbank config in features.rs against the reference",
            sim_same,
            sim_diff
        );
    }
}
```

- [ ] **Step 4: Run the integration test manually with real clips, confirm same > different**

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/diarization_engine
git commit -m "feat(diarization): CAM++ embedding model wrapper"
```

---

### Task 7: Clustering (`diarization_engine/clustering.rs`)

**Files:**
- Create: `frontend/src-tauri/src/diarization_engine/clustering.rs`
- Modify: `frontend/src-tauri/src/diarization_engine/mod.rs`

Reference (`fast-clustering.cc`, read for the *algorithm choice*, not exact index arithmetic — this
is a standard, well-defined algorithm unlike Tasks 4-5): agglomerative clustering, **complete
linkage** (distance between two clusters = the *maximum* pairwise distance between their members,
not average or minimum), **cosine dissimilarity** (`1 - cosine_similarity`, clamped to `>= 0`) as
the base distance, embeddings **L2-normalized before clustering** (verified: raw embeddings are
NOT normalized coming out of the model). Two cut modes from the same dendrogram: fixed-`k` and
distance-threshold (default `0.5`, `DIARIZATION_DEFAULT_CLUSTER_THRESHOLD`). For meeting-scale
input (well under 1000 embeddings), a plain O(n³) "repeatedly merge the closest pair" is
performance-fine — no need for the reference's nn-chain optimization.

- [ ] **Step 1: Write the failing tests**

```rust
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Dendrogram {
    /// Each merge: (left_cluster_id, right_cluster_id, distance_at_merge). Cluster ids
    /// `0..n` are the original points; ids `>= n` are merged clusters created during the
    /// process, in creation order (standard scipy/fastcluster linkage-matrix convention).
    pub merges: Vec<(usize, usize, f32)>,
    pub num_points: usize,
}

/// L2-normalizes each row, builds the cosine-dissimilarity distance matrix, and runs
/// complete-linkage agglomerative clustering. Returns the full dendrogram — cut it with
/// `cutree_k` or `cutree_cdist` afterward depending on whether the caller knows the speaker
/// count.
pub fn hierarchical_cluster(embeddings: &[[f32; 192]]) -> Dendrogram {
    todo!()
}

/// Cuts the dendrogram to produce exactly `k` clusters. `k` must be `1..=num_points`.
pub fn cutree_k(dendrogram: &Dendrogram, k: usize) -> Vec<usize> {
    todo!()
}

/// Cuts the dendrogram at the first merge whose distance exceeds `threshold` — clusters
/// merged below the threshold stay merged, everything above stays separate. Matches the
/// reference default of 0.5.
pub fn cutree_cdist(dendrogram: &Dendrogram, threshold: f32) -> Vec<usize> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(mostly_dim: usize) -> [f32; 192] {
        let mut v = [0.01f32; 192];
        v[mostly_dim] = 10.0;
        v
    }

    #[test]
    fn hierarchical_cluster_then_cutree_k_separates_two_clear_clusters() {
        // 3 points near dim 0, 3 points near dim 100 — two well-separated clusters.
        let embeddings: Vec<[f32; 192]> = vec![
            unit(0), unit(0), unit(0),
            unit(100), unit(100), unit(100),
        ];
        let dendrogram = hierarchical_cluster(&embeddings);
        let labels = cutree_k(&dendrogram, 2);
        assert_eq!(labels.len(), 6);
        assert_eq!(labels[0], labels[1]);
        assert_eq!(labels[1], labels[2]);
        assert_eq!(labels[3], labels[4]);
        assert_eq!(labels[4], labels[5]);
        assert_ne!(labels[0], labels[3]);
    }

    #[test]
    fn cutree_cdist_with_low_threshold_keeps_similar_points_together() {
        let embeddings: Vec<[f32; 192]> = vec![unit(0), unit(0), unit(100)];
        let dendrogram = hierarchical_cluster(&embeddings);
        let labels = cutree_cdist(&dendrogram, 0.5);
        assert_eq!(labels[0], labels[1]);
        assert_ne!(labels[0], labels[2]);
    }

    #[test]
    fn single_point_yields_one_cluster() {
        let embeddings: Vec<[f32; 192]> = vec![unit(0)];
        let dendrogram = hierarchical_cluster(&embeddings);
        assert_eq!(cutree_k(&dendrogram, 1), vec![0]);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

- [ ] **Step 3: Implement**

```rust
fn l2_normalize(v: &[f32; 192]) -> [f32; 192] {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    let mut out = [0.0f32; 192];
    for (o, &x) in out.iter_mut().zip(v.iter()) {
        *o = x / norm;
    }
    out
}

fn cosine_dissimilarity(a: &[f32; 192], b: &[f32; 192]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    (1.0 - dot).max(0.0)
}

pub fn hierarchical_cluster(embeddings: &[[f32; 192]]) -> Dendrogram {
    let n = embeddings.len();
    let normalized: Vec<[f32; 192]> = embeddings.iter().map(l2_normalize).collect();

    // `members[cluster_id]` = original point indices belonging to that cluster; grows past
    // `n` as merges create new cluster ids, complete-linkage style (max pairwise distance).
    let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let mut active: Vec<usize> = (0..n).collect();
    let mut merges = Vec::with_capacity(n.saturating_sub(1));

    while active.len() > 1 {
        let mut best = (0usize, 0usize, f32::INFINITY);
        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let (ci, cj) = (active[i], active[j]);
                let mut max_dist = 0.0f32;
                for &pi in &members[ci] {
                    for &pj in &members[cj] {
                        let d = cosine_dissimilarity(&normalized[pi], &normalized[pj]);
                        if d > max_dist {
                            max_dist = d;
                        }
                    }
                }
                if max_dist < best.2 {
                    best = (i, j, max_dist);
                }
            }
        }
        let (idx_i, idx_j, dist) = best;
        let (ci, cj) = (active[idx_i], active[idx_j]);
        let new_id = members.len();
        let mut combined = members[ci].clone();
        combined.extend(members[cj].clone());
        members.push(combined);
        merges.push((ci, cj, dist));

        // Remove the higher index first so the lower one's position doesn't shift.
        active.remove(idx_j.max(idx_i));
        active.remove(idx_j.min(idx_i));
        active.push(new_id);
    }

    Dendrogram { merges, num_points: n }
}

/// Union-find over cluster ids `0..num_points + merges.len()`, replaying merges in order up
/// to (but not including) `stop_before` of them, then mapping each original point to its
/// final root, relabeled to consecutive `0..k` ids in first-seen order for determinism.
fn labels_after_merges(dendrogram: &Dendrogram, stop_before: usize) -> Vec<usize> {
    let n = dendrogram.num_points;
    let mut parent: Vec<usize> = (0..n + dendrogram.merges.len()).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }
    for (i, &(a, b, _)) in dendrogram.merges.iter().enumerate() {
        if i >= stop_before {
            break;
        }
        let new_id = n + i;
        let ra = find(&mut parent, a);
        let rb = find(&mut parent, b);
        parent[ra] = new_id;
        parent[rb] = new_id;
        parent[new_id] = new_id;
    }

    let mut relabel = std::collections::HashMap::new();
    let mut next_label = 0usize;
    (0..n)
        .map(|p| {
            let root = find(&mut parent, p);
            *relabel.entry(root).or_insert_with(|| {
                let l = next_label;
                next_label += 1;
                l
            })
        })
        .collect()
}

pub fn cutree_k(dendrogram: &Dendrogram, k: usize) -> Vec<usize> {
    let n = dendrogram.num_points;
    let k = k.clamp(1, n);
    // n points need (n - k) merges applied to end up with exactly k clusters.
    labels_after_merges(dendrogram, n - k)
}

pub fn cutree_cdist(dendrogram: &Dendrogram, threshold: f32) -> Vec<usize> {
    let stop_before = dendrogram
        .merges
        .iter()
        .position(|&(_, _, dist)| dist > threshold)
        .unwrap_or(dendrogram.merges.len());
    labels_after_merges(dendrogram, stop_before)
}
```

- [ ] **Step 4: Run tests to verify they pass**

- [ ] **Step 5: Register module, verify compile**

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/diarization_engine
git commit -m "feat(diarization): complete-linkage agglomerative clustering (fixed-k + threshold modes)"
```

---

### Task 8: Relabel + finalize + compute result (`diarization_engine/finalize.rs`)

**Files:**
- Create: `frontend/src-tauri/src/diarization_engine/finalize.rs`
- Modify: `frontend/src-tauri/src/diarization_engine/mod.rs`
- Modify: `frontend/src-tauri/src/config.rs` (one new constant, see Step 3)

**This task's design was rewritten after Task 4 was implemented.** The original text here assumed
`relabel` would operate on Task 4's `GlobalFrameLabels` and produce `Vec<Option<usize>>` — that
doesn't work and was never implementable: local speaker slots are chunk-local (chunk 0's slot 1 and
chunk 5's slot 1 are unrelated voices), so there is no meaningful global per-local-speaker identity
to relabel. Task 4's implementer caught this by actually reading the reference (see its commit
`2bc432b`'s report) and correctly changed `GlobalFrameLabels` to carry `speakers_per_frame: Vec<usize>`
(a *count*, not identities) instead. The controller independently re-verified the entire reference
chain below by re-fetching and reading the header directly — every function/line reference here is
confirmed against the real source, not carried over from the original (wrong) plan text.

- [ ] **Step 1: Fetch and read the real reference source**

```bash
curl -s https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/sherpa-onnx/csrc/offline-speaker-diarization-pyannote-impl.h -o /tmp/pyannote-impl.h
curl -s https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/sherpa-onnx/csrc/offline-speaker-diarization-result.h -o /tmp/result.h
curl -s https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/sherpa-onnx/csrc/offline-speaker-diarization-result.cc -o /tmp/result.cc
curl -s https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/sherpa-onnx/csrc/math.h -o /tmp/math.h
```

Read, in `pyannote-impl.h`:
- `ReLabel` (~line 593) — takes the **original, un-excluded** per-chunk multi-label matrices (Task
  4's `ChunkLabels.labels` — the same values Task 5 reads too, *not* the overlap-excluded version
  `ExcludeOverlap` produces; that exclusion is local to Task 5's embedding extraction only) plus
  `max_cluster_index` and the `(chunk_index, local_speaker) -> cluster_id` map (built by
  `ConvertChunkSpeakerToCluster`, ~line 574 — a trivial zip of the `(chunk,speaker)` pairs Task 5
  produced against the cluster labels Task 7's clustering produced, in the same order — this part
  belongs in Task 9's orchestrator, not here). For each chunk, per frame, per local speaker: if that
  `(chunk, speaker)` has a cluster mapping, copy that speaker's active/inactive flag at that frame
  into the corresponding **cluster column** of a new per-chunk matrix shaped
  `(num_frames_in_chunk, max_cluster_index + 1)`. A `(chunk, speaker)` with no mapping (filtered out
  earlier for having too little embeddable signal) is simply skipped — its frames stay 0 in every
  cluster column.
- `ComputeSpeakerCount` (~line 641) — reconciles the per-chunk, per-cluster matrices from `ReLabel`
  onto one global `(num_global_frames, num_clusters)` grid, using the **exact same** `chunk_index ->
  start_frame` placement as `ComputeSpeakersPerFrame` (Task 4) — but **summing, not averaging**:
  `count(seq, all).array() += labels[i].array()` (no division by a weight count anywhere in this
  function). Also applies the same trailing-truncation-to-audio-length rule Task 4's
  `compute_speakers_per_frame` already ported (lines ~666-676) — see Step 3 for how to reuse that
  without re-deriving it.
- `FinalizeLabels` (~line 679) — per global frame `i`: let `k = speakers_per_frame[i]` (Task 4's
  output); if `k == 0`, every cluster is inactive at that frame; otherwise pick the `k`
  highest-count cluster columns at that frame active (`TopkIndex`, `math.h` line ~112 — descending
  by count, via `std::partial_sort`, which does **not** guarantee a specific order among tied
  values; when porting to Rust, break ties by lower cluster index — this is a deliberate, documented
  minor divergence from the reference's technically-unspecified tie order, not an attempt to match
  it bit-for-bit).
- `ComputeResult` (~line 704) — per cluster column, scan the finalized per-frame activation and find
  contiguous active runs. Time conversion: `scale = receptive_field_shift / sample_rate`,
  `scale_offset = 0.5 * receptive_field_size / sample_rate`; `start_time = start_frame * scale +
  scale_offset`, `end_time = end_frame * scale + scale_offset` (`end_frame` is the first frame where
  the run stops being active — i.e. a half-open `[start, end)` in frame terms). Then
  `MergeSegments` (~line 236 of `pyannote-impl.h`, calling `Segment::Merge` —
  `offline-speaker-diarization-result.cc` line ~34): repeatedly merge adjacent same-cluster segments
  whose gap is `<= min_duration_off` (`this.end + gap >= other.start`, merged span is
  `(this.start, other.end)`), until no more merges apply. Finally keep only segments with
  `Duration() > min_duration_on` (**strict** `>`, matching `result.cc`/`pyannote-impl.h`'s check —
  not `>=`).

- [ ] **Step 2: Add the missing `DIARIZATION_RECEPTIVE_FIELD_SIZE` constant**

`config.rs` already has `DIARIZATION_RECEPTIVE_FIELD_SHIFT` but not the model's
`receptive_field_size` metadata value — confirmed needed by `ComputeResult`'s `scale_offset` above
(the segmentation model's own ONNX metadata, previously assumed unused, verified in this task to
actually be load-bearing). Add next to `DIARIZATION_RECEPTIVE_FIELD_SHIFT`:
```rust
/// Used only by `ComputeResult`'s time-offset calculation (half a receptive field, so a
/// detected frame's timestamp lands at the center of what it covers, not its leading edge).
pub const DIARIZATION_RECEPTIVE_FIELD_SIZE: usize = 991;
```

- [ ] **Step 3: Define the types and reuse Task 4's helpers**

```rust
use super::windowing::{chunk_start_frame, ChunkLabels};
use crate::config::{DIARIZATION_MIN_DURATION_OFF_SEC, DIARIZATION_MIN_DURATION_ON_SEC, DIARIZATION_RECEPTIVE_FIELD_SIZE};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct SpeakerTurn {
    pub start_sec: f64,
    pub end_sec: f64,
    pub cluster_index: usize,
}

/// One chunk's per-frame activation, recolumned from chunk-local speaker slots to global
/// cluster ids. `frames[j][c]` is true iff cluster `c` is active at this chunk's frame `j`.
pub struct RelabeledChunk {
    pub chunk_index: usize,
    pub frames: Vec<Vec<bool>>, // len == chunk's frame count; each inner Vec has len == num_clusters
}
```

**Important — do not re-derive the trailing-truncation formula.** Task 4's
`compute_speakers_per_frame` already produces a correctly-truncated `speakers_per_frame: Vec<usize>`
whose length is the ground truth for how many global frames actually exist. `compute_speaker_count`
below must build its grid to *that same length* (pass `speakers_per_frame.len()` in directly as
`target_num_frames`, dropping any placed frame `>= target_num_frames`) instead of independently
recomputing `total_samples`-based truncation a second time — this guarantees the two arrays are
always the same length (required for `finalize_labels` to index them together) without duplicating
error-prone arithmetic.

- [ ] **Step 4: Write the failing tests**

Using small synthetic `ChunkLabels` + a hand-built `chunk_speaker_to_cluster` map (not real audio):
- `relabel`: a chunk with one local speaker mapped to cluster 2 (out of e.g. 3 clusters) produces a
  matrix where only column 2 carries that speaker's frame activity, columns 0/1 stay all-false; a
  local speaker with no mapping entry contributes nothing (all its frames stay false in every
  column).
- `compute_speaker_count`: two overlapping chunks whose relabeled activity both mark the same
  cluster active at the same global frame sum to `2` at that (frame, cluster) cell, not `1` — this
  is the key behavioral difference from Task 4's *averaging* reconciliation, worth a test that would
  fail if someone later "fixes" this to average by mistake.
- `finalize_labels`: a frame with `speakers_per_frame[i] == 1` and cluster counts `[5, 2, 8]` picks
  cluster 2 (highest count) active, clusters 0/1 inactive; a frame with `speakers_per_frame[i] == 0`
  has every cluster inactive regardless of counts; a tie (`[3, 3]`, k=1) picks the lower index
  deterministically (documenting the accepted tie-break divergence from Step 1).
- `compute_result`: a short (<0.3s) isolated active run gets dropped; two same-cluster runs
  separated by a gap `<= min_duration_off` merge into one turn spanning both; a gap strictly greater
  than `min_duration_off` keeps them separate; a straightforward multi-cluster sequence with no edge
  cases produces the expected turn list with correctly scaled start/end times.

- [ ] **Step 5: Implement**

Signatures (fill in bodies per Step 1's exact algorithms — this is intentionally not handed to you
as ready-made code, unlike earlier tasks, because getting here required correcting the plan itself;
implement from the verified algorithm description above, and if anything is still ambiguous once
you're looking at the real source yourself, stop and ask rather than guessing):

```rust
pub fn relabel(
    chunks: &[ChunkLabels],
    max_cluster_index: usize,
    chunk_speaker_to_cluster: &HashMap<(usize, usize), usize>,
) -> Vec<RelabeledChunk> { ... }

/// Sums (not averages) relabeled per-chunk activity onto a global `(target_num_frames,
/// num_clusters)` grid, placed via `chunk_start_frame` — the same placement Task 4 uses.
pub fn compute_speaker_count(
    relabeled: &[RelabeledChunk],
    num_clusters: usize,
    target_num_frames: usize,
) -> Vec<Vec<usize>> { ... }

pub fn finalize_labels(count: &[Vec<usize>], speakers_per_frame: &[usize]) -> Vec<Vec<bool>> { ... }

pub fn compute_result(
    final_labels: &[Vec<bool>],
    frame_shift_samples: usize,
    sample_rate: f64,
) -> Vec<SpeakerTurn> { ... }
```

- [ ] **Step 6: Run tests to verify they pass**

- [ ] **Step 7: Register module, verify compile**

- [ ] **Step 8: Commit**

```bash
git add frontend/src-tauri/src/diarization_engine frontend/src-tauri/src/config.rs
git commit -m "feat(diarization): relabel + min-duration smoothing + final speaker turns"
```

---

### Task 9: Engine orchestrator + real-audio integration test (`diarization_engine/engine.rs`)

**Files:**
- Create: `frontend/src-tauri/src/diarization_engine/engine.rs`
- Modify: `frontend/src-tauri/src/diarization_engine/mod.rs`

- [ ] **Step 1: Implement**

Ties Tasks 3-8 together. `num_clusters: Option<usize>` — `None` uses
`cutree_cdist(_, DIARIZATION_DEFAULT_CLUSTER_THRESHOLD)` (auto), `Some(k)` uses `cutree_k(_, k)`
(user specified the speaker count). Uses Task 8's corrected 4-function finalize API
(`relabel` → `compute_speaker_count` → `finalize_labels` → `compute_result`), not the original
2-function design that turned out not to be implementable — see Task 8's header note for why.

```rust
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use super::clustering::{cutree_cdist, cutree_k, hierarchical_cluster};
use super::embedding::EmbeddingEngine;
use super::finalize::{compute_result, compute_speaker_count, finalize_labels, relabel, SpeakerTurn};
use super::sample_indexes::get_chunk_speaker_sample_indexes;
use super::segmentation::SegmentationEngine;
use super::windowing::{compute_speakers_per_frame, run_segmentation_windows};
use crate::config::{DIARIZATION_DEFAULT_CLUSTER_THRESHOLD, DIARIZATION_RECEPTIVE_FIELD_SHIFT};

pub struct DiarizationEngine {
    segmentation: SegmentationEngine,
    embedding: EmbeddingEngine,
}

impl DiarizationEngine {
    pub fn load(segmentation_model: &Path, embedding_model: &Path, threads: usize) -> Result<Self> {
        Ok(Self {
            segmentation: SegmentationEngine::load(segmentation_model, threads)?,
            embedding: EmbeddingEngine::load(embedding_model, threads)?,
        })
    }

    /// `samples` must be 16kHz mono f32 — the same decoded samples ASR uses, no separate
    /// decode needed. `num_clusters`: `None` = auto-detect via distance threshold, `Some(k)` =
    /// exactly `k` speakers.
    pub fn diarize(&mut self, samples: &[f32], num_clusters: Option<usize>) -> Result<Vec<SpeakerTurn>> {
        let chunks = run_segmentation_windows(&mut self.segmentation, samples)?;
        let global_frames = compute_speakers_per_frame(&chunks, samples.len());
        let chunk_speaker_samples = get_chunk_speaker_sample_indexes(&chunks);

        let mut pair_order: Vec<(usize, usize)> = Vec::new();
        let mut embeddings: Vec<[f32; 192]> = Vec::new();
        for entry in &chunk_speaker_samples {
            let mut concatenated: Vec<f32> = Vec::new();
            for &(start, end) in &entry.sample_ranges {
                concatenated.extend_from_slice(&samples[start..end.min(samples.len())]);
            }
            if concatenated.is_empty() {
                continue;
            }
            match self.embedding.compute_embedding(&concatenated, 16000.0) {
                Ok(emb) => {
                    pair_order.push((entry.chunk_index, entry.local_speaker));
                    embeddings.push(emb);
                }
                Err(e) => {
                    log::warn!(
                        "Diarization: embedding failed for chunk {} speaker {}: {} — skipping",
                        entry.chunk_index,
                        entry.local_speaker,
                        e
                    );
                }
            }
        }

        if embeddings.is_empty() {
            return Ok(Vec::new());
        }

        let dendrogram = hierarchical_cluster(&embeddings);
        let labels = match num_clusters {
            Some(k) => cutree_k(&dendrogram, k),
            None => cutree_cdist(&dendrogram, DIARIZATION_DEFAULT_CLUSTER_THRESHOLD),
        };
        // Cutting always relabels to consecutive 0..k ids (see Task 7's `labels_after_merges`),
        // so the highest id actually assigned is a safe stand-in for "how many clusters".
        let max_cluster_index = *labels.iter().max().unwrap_or(&0);
        let num_clusters_found = max_cluster_index + 1;

        let chunk_speaker_to_cluster: HashMap<(usize, usize), usize> = pair_order
            .into_iter()
            .zip(labels)
            .collect();

        let relabeled = relabel(&chunks, max_cluster_index, &chunk_speaker_to_cluster);
        let count = compute_speaker_count(
            &relabeled,
            num_clusters_found,
            global_frames.speakers_per_frame.len(),
        );
        let final_labels = finalize_labels(&count, &global_frames.speakers_per_frame);
        Ok(compute_result(&final_labels, DIARIZATION_RECEPTIVE_FIELD_SHIFT, 16000.0))
    }
}
```

- [ ] **Step 2: Register module, verify compile**

- [ ] **Step 3: Integration test on real multi-speaker audio**

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    fn models_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap())
            .join("AppData/Roaming/com.meetingone.app/models/diarization-vi")
    }

    /// Requires the diarization models on disk and a real multi-speaker WAV pointed to by
    /// DIARIZATION_TEST_WAV (16kHz mono; e.g. a short clip with 2-3 known speakers).
    #[test]
    #[ignore = "requires downloaded diarization models + a real multi-speaker audio file"]
    fn diarize_real_multi_speaker_audio_produces_plausible_turns() {
        let wav_path = std::env::var("DIARIZATION_TEST_WAV").expect("set DIARIZATION_TEST_WAV");
        let (samples, _duration) = crate::audio::decoder::load_audio_for_file_pipeline(
            std::path::Path::new(&wav_path),
            None,
        )
        .expect("decode audio");

        let dir = models_dir();
        let mut engine = DiarizationEngine::load(
            &dir.join("segmentation.int8.onnx"),
            &dir.join("embedding-campplus.onnx"),
            4,
        )
        .expect("load diarization engine");

        let turns = engine.diarize(&samples, None).expect("diarize");
        println!("Detected {} speaker turns:", turns.len());
        for t in &turns {
            println!("  [{:.2}s - {:.2}s] speaker {}", t.start_sec, t.end_sec, t.cluster_index);
        }
        assert!(!turns.is_empty());
        let distinct_speakers: std::collections::HashSet<_> = turns.iter().map(|t| t.cluster_index).collect();
        println!("Distinct speakers detected: {}", distinct_speakers.len());
    }
}
```

- [ ] **Step 4: Run the integration test manually against a real multi-speaker recording, read the printed turns, and sanity-check them by ear against the actual audio**

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/diarization_engine
git commit -m "feat(diarization): engine orchestrator tying segmentation, embedding, and clustering together"
```

---

### Task 10: Align speaker turns onto transcript segments (`audio/speaker_align.rs`)

**Files:**
- Create: `frontend/src-tauri/src/audio/speaker_align.rs`
- Modify: `frontend/src-tauri/src/audio/mod.rs` (register module)

- [ ] **Step 1: Write the failing tests**

```rust
use crate::diarization_engine::finalize::SpeakerTurn;

/// One segment's approximate time span, decoupled from `TranscriptSegment`/`api::TranscriptSegment`
/// so this module doesn't need to depend on the API layer — callers convert both directions.
pub struct SegmentSpan {
    pub start_sec: f64,
    pub end_sec: f64,
}

/// For each segment, returns the `cluster_index` of the `SpeakerTurn` it overlaps most (by
/// duration of overlap) — `None` if it overlaps no turn at all (e.g. diarization found no
/// speech there). Pure function: no DB/IO, easy to test exhaustively.
pub fn align_speakers_to_segments(turns: &[SpeakerTurn], segments: &[SegmentSpan]) -> Vec<Option<usize>> {
    segments
        .iter()
        .map(|seg| {
            turns
                .iter()
                .map(|t| {
                    let overlap = (seg.end_sec.min(t.end_sec) - seg.start_sec.max(t.start_sec)).max(0.0);
                    (t.cluster_index, overlap)
                })
                .filter(|&(_, overlap)| overlap > 0.0)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(cluster, _)| cluster)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(start: f64, end: f64, cluster: usize) -> SpeakerTurn {
        SpeakerTurn { start_sec: start, end_sec: end, cluster_index: cluster }
    }
    fn span(start: f64, end: f64) -> SegmentSpan {
        SegmentSpan { start_sec: start, end_sec: end }
    }

    #[test]
    fn segment_fully_inside_one_turn_gets_that_speaker() {
        let turns = vec![turn(0.0, 10.0, 0), turn(10.0, 20.0, 1)];
        let segments = vec![span(2.0, 5.0)];
        assert_eq!(align_speakers_to_segments(&turns, &segments), vec![Some(0)]);
    }

    #[test]
    fn segment_spanning_a_speaker_change_gets_the_majority_overlap_speaker() {
        let turns = vec![turn(0.0, 10.0, 0), turn(10.0, 20.0, 1)];
        // segment [8, 15]: 2s with speaker 0, 5s with speaker 1 -> speaker 1 wins.
        let segments = vec![span(8.0, 15.0)];
        assert_eq!(align_speakers_to_segments(&turns, &segments), vec![Some(1)]);
    }

    #[test]
    fn segment_with_no_overlapping_turn_gets_none() {
        let turns = vec![turn(0.0, 5.0, 0)];
        let segments = vec![span(10.0, 12.0)];
        assert_eq!(align_speakers_to_segments(&turns, &segments), vec![None]);
    }

    #[test]
    fn empty_turns_gives_none_for_every_segment() {
        let segments = vec![span(0.0, 1.0), span(1.0, 2.0)];
        assert_eq!(align_speakers_to_segments(&[], &segments), vec![None, None]);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail** (module doesn't exist yet — compile error)

- [ ] **Step 3: Register module** (`pub mod speaker_align;` in `audio/mod.rs`), run tests to verify they pass

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/audio/speaker_align.rs frontend/src-tauri/src/audio/mod.rs
git commit -m "feat(diarization): max-overlap alignment of speaker turns onto transcript segments"
```

---

### Task 11: Database — `meeting_speakers` table + `speaker_id` column

**Files:**
- Create: `frontend/src-tauri/migrations/20260810000000_add_meeting_speakers.sql`
- Modify: `frontend/src-tauri/src/database/models.rs`
- Create: `frontend/src-tauri/src/database/repositories/meeting_speaker.rs`
- Modify: `frontend/src-tauri/src/database/repositories/mod.rs`

- [ ] **Step 1: Migration**

IDs follow the existing house convention (`String`/UUID-prefixed, e.g.
`format!("transcript-{}", Uuid::new_v4())` in `transcript.rs`) — **not** `INTEGER AUTOINCREMENT`,
matching `meetings.id`/`transcripts.id`/`meeting_documents.id` exactly:

```sql
CREATE TABLE meeting_speakers (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id),
    cluster_index INTEGER NOT NULL,
    display_name TEXT NOT NULL,
    color TEXT NOT NULL
);

ALTER TABLE transcripts ADD COLUMN speaker_id TEXT REFERENCES meeting_speakers(id);
```

- [ ] **Step 2: `database/models.rs`**

```rust
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MeetingSpeaker {
    pub id: String,
    pub meeting_id: String,
    pub cluster_index: i64,
    pub display_name: String,
    pub color: String,
}
```

Add `pub speaker_id: Option<String>,` to `Transcript` (after `duration`, matching field-addition
style already used there).

- [ ] **Step 3: `database/repositories/meeting_speaker.rs`**

Color palette: 8 fixed, visually distinct hex colors, assigned `cluster_index % 8` — matches the
scale test_asr and most chat UIs use (rarely more than a handful of simultaneous distinguishable
colors are useful anyway).

```rust
use crate::database::models::MeetingSpeaker;
use sqlx::{Error as SqlxError, SqlitePool};
use uuid::Uuid;

const SPEAKER_COLOR_PALETTE: [&str; 8] = [
    "#2563eb", "#dc2626", "#16a34a", "#d97706",
    "#9333ea", "#0891b2", "#db2777", "#65a30d",
];

pub struct MeetingSpeakersRepository;

impl MeetingSpeakersRepository {
    /// Creates one row per distinct `cluster_index` found in `cluster_indexes`, in ascending
    /// order, with default display names ("Người nói 1", "Người nói 2", ...) and palette
    /// colors. Returns the created rows, ordered the same way — callers build a
    /// `cluster_index -> MeetingSpeaker.id` map from this to attach to transcript segments.
    pub async fn create_for_clusters(
        pool: &SqlitePool,
        meeting_id: &str,
        cluster_indexes: &[i64],
    ) -> Result<Vec<MeetingSpeaker>, SqlxError> {
        let mut sorted = cluster_indexes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        let mut created = Vec::with_capacity(sorted.len());
        for cluster_index in sorted {
            let id = format!("speaker-{}", Uuid::new_v4());
            let display_name = format!("Người nói {}", cluster_index + 1);
            let color = SPEAKER_COLOR_PALETTE[(cluster_index as usize) % SPEAKER_COLOR_PALETTE.len()];

            sqlx::query(
                "INSERT INTO meeting_speakers (id, meeting_id, cluster_index, display_name, color)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(meeting_id)
            .bind(cluster_index)
            .bind(&display_name)
            .bind(color)
            .execute(pool)
            .await?;

            created.push(MeetingSpeaker {
                id,
                meeting_id: meeting_id.to_string(),
                cluster_index,
                display_name,
                color: color.to_string(),
            });
        }
        Ok(created)
    }

    pub async fn list_by_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<MeetingSpeaker>, SqlxError> {
        sqlx::query_as::<_, MeetingSpeaker>(
            "SELECT id, meeting_id, cluster_index, display_name, color
             FROM meeting_speakers WHERE meeting_id = ? ORDER BY cluster_index ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    /// Renames a speaker — applies to every transcript segment pointing at this
    /// `speaker_id` at once, since they all share the same row.
    pub async fn rename(pool: &SqlitePool, speaker_id: &str, new_name: &str) -> Result<bool, SqlxError> {
        let result = sqlx::query("UPDATE meeting_speakers SET display_name = ? WHERE id = ?")
            .bind(new_name)
            .bind(speaker_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Re-points exactly one transcript segment's `speaker_id` to the speaker of the segment
    /// immediately before it (by `audio_start_time`, same meeting). Returns `Ok(false)` if
    /// there's no earlier segment (nothing to merge with) or the earlier segment itself has
    /// no speaker assigned yet — the caller should hide/disable the merge action rather than
    /// call this in that case, but this is the source of truth for that check too.
    pub async fn merge_segment_with_previous(pool: &SqlitePool, transcript_id: &str) -> Result<bool, SqlxError> {
        let row: Option<(String, String, f64)> = sqlx::query_as(
            "SELECT meeting_id, id, audio_start_time FROM transcripts WHERE id = ?",
        )
        .bind(transcript_id)
        .fetch_optional(pool)
        .await?
        .map(|(meeting_id, id, start): (String, String, Option<f64>)| (meeting_id, id, start.unwrap_or(0.0)));

        let Some((meeting_id, _, start_time)) = row else {
            return Ok(false);
        };

        let previous_speaker_id: Option<String> = sqlx::query_scalar(
            "SELECT speaker_id FROM transcripts
             WHERE meeting_id = ? AND audio_start_time < ? AND speaker_id IS NOT NULL
             ORDER BY audio_start_time DESC LIMIT 1",
        )
        .bind(&meeting_id)
        .bind(start_time)
        .fetch_optional(pool)
        .await?
        .flatten();

        let Some(previous_speaker_id) = previous_speaker_id else {
            return Ok(false);
        };

        let result = sqlx::query("UPDATE transcripts SET speaker_id = ? WHERE id = ?")
            .bind(&previous_speaker_id)
            .bind(transcript_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Convenience wrapper `create_for_clusters` + resolve, used by both `import.rs` and
    /// `retranscription.rs` (Task 14) right before they save transcripts — takes the raw
    /// per-segment cluster assignments diarization/alignment produced and a now-known
    /// `meeting_id`, creates one `meeting_speakers` row per distinct cluster, and returns a
    /// parallel `Vec<Option<String>>` (same length/order as `cluster_assignments`) ready to
    /// bind directly as each segment's `speaker_id`. Kept as one shared function specifically
    /// so the two callers can't drift out of sync on this logic.
    pub async fn resolve_cluster_assignments(
        pool: &SqlitePool,
        meeting_id: &str,
        cluster_assignments: &[Option<i64>],
    ) -> Result<Vec<Option<String>>, SqlxError> {
        let distinct: Vec<i64> = {
            let mut v: Vec<i64> = cluster_assignments.iter().flatten().copied().collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        if distinct.is_empty() {
            return Ok(vec![None; cluster_assignments.len()]);
        }
        let speakers = Self::create_for_clusters(pool, meeting_id, &distinct).await?;
        let id_by_cluster: std::collections::HashMap<i64, String> =
            speakers.into_iter().map(|s| (s.cluster_index, s.id)).collect();
        Ok(cluster_assignments
            .iter()
            .map(|c| c.and_then(|c| id_by_cluster.get(&c).cloned()))
            .collect())
    }
}
```

- [ ] **Step 4: Register repository module** (`pub mod meeting_speaker;` in `database/repositories/mod.rs`)

- [ ] **Step 5: Verify compile + migration applies cleanly**

```bash
cargo check --manifest-path frontend/src-tauri/Cargo.toml
```
(Migrations run automatically on next app/test DB init via `sqlx::migrate!` — confirm no error in
the log on first run after this change.)

- [ ] **Step 6: Write repository tests**

Follow the existing test pattern in this codebase for repositories that touch a real (in-memory or
temp-file) SQLite pool — find and mirror whatever `meeting_document.rs` or `transcript.rs` tests
already do for pool setup (search for `#[sqlx::test]` or a shared test-pool helper in
`database/repositories/mod.rs` or a `tests` module) rather than inventing a new setup pattern.
Cover: `create_for_clusters` creates one row per distinct cluster with correct default names/colors
and dedupes repeated cluster indexes; `rename` updates the row and returns `true`, returns `false`
for an unknown id; `merge_segment_with_previous` re-points the target segment's `speaker_id` and
leaves every other segment untouched, returns `false` when there's no earlier speaker-labeled
segment; `resolve_cluster_assignments` returns a same-length `Vec<Option<String>>` with `None`s
preserved in place for `None` inputs, and returns all-`None` (no DB writes) for an all-`None`
input slice.

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/migrations frontend/src-tauri/src/database
git commit -m "feat(diarization): meeting_speakers table + rename/merge repository operations"
```

---

### Task 12: Wire `speaker_id` through transcript save/read paths

**Files:**
- Modify: `frontend/src-tauri/src/api/api.rs`
- Modify: `frontend/src-tauri/src/database/repositories/transcript.rs`
- Modify: `frontend/src-tauri/src/audio/import.rs`
- Modify: `frontend/src-tauri/src/audio/retranscription.rs`
- Modify: `frontend/src-tauri/src/database/repositories/meeting.rs`

Purely mechanical plumbing in this task — every site here just needs to accept and bind an
`Option<String>` it's handed. **Where that value actually comes from (resolving a diarization
cluster index to a real `meeting_speakers.id`) is Task 14's job, not this one** — after this task,
every write site is ready to receive a real `speaker_id`, but none of them populate one yet
(`segment.speaker_id` is always `None` until Task 14 wires the diarization pipeline in).

There are **three separate places** that `INSERT INTO transcripts`, confirmed by reading each file
directly — this surprised initial assumptions, so don't skip re-confirming while editing:
`TranscriptsRepository::save_transcript` (`transcript.rs:50`, used by the live-recording finalize
path), `import.rs`'s own local `create_meeting_with_transcripts` (`import.rs:886`, file import —
does **not** go through `TranscriptsRepository`), and `retranscription.rs`'s own inline insert
(`retranscription.rs:339`, re-transcription — also does not go through `TranscriptsRepository`).
All three need the new column.

- [ ] **Step 1: `TranscriptSegment` struct (`api/api.rs`)**

Add `pub speaker_id: Option<String>,` (with `#[serde(skip_serializing_if = "Option::is_none")]`)
next to `duration` in `TranscriptSegment` (around line 222-233) — the one shared struct all three
insert sites read from.

- [ ] **Step 2: `TranscriptsRepository::save_transcript` (`transcript.rs:50-59`)**

```rust
sqlx::query(
    "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker_id)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
)
.bind(&transcript_id)
.bind(&meeting_id)
.bind(&segment.text)
.bind(&segment.timestamp)
.bind(segment.audio_start_time)
.bind(segment.audio_end_time)
.bind(segment.duration)
.bind(&segment.speaker_id)
.execute(&mut *transaction)
.await;
```

- [ ] **Step 3: `import.rs::create_meeting_with_transcripts` (`import.rs:884-899`)**

```rust
sqlx::query(
    "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker_id)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
)
.bind(&segment.id)
.bind(&meeting_id)
.bind(&segment.text)
.bind(&segment.timestamp)
.bind(segment.audio_start_time)
.bind(segment.audio_end_time)
.bind(segment.duration)
.bind(&segment.speaker_id)
.execute(&mut *tx)
.await
.map_err(|e| anyhow!("Failed to insert transcript: {}", e))?;
```

- [ ] **Step 4: `retranscription.rs`'s inline insert (`retranscription.rs:337-350`)**

Same change as Step 3 — add `speaker_id` to the column list, placeholders, and
`.bind(&segment.speaker_id)`.

- [ ] **Step 5: `MeetingTranscript` (read path, `api/api.rs`)**

Add speaker fields to the struct at line 171-182 (same optional-field pattern as
`audio_start_time`):

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub speaker_id: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub speaker_name: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub speaker_color: Option<String>,
```

At the conversion site (`api.rs:1024-1034`), this needs the meeting's speaker list to resolve
`speaker_id -> (name, color)` — fetch it once per call via
`MeetingSpeakersRepository::list_by_meeting`, build a `HashMap<String, &MeetingSpeaker>`, then:

```rust
let speakers = MeetingSpeakersRepository::list_by_meeting(pool, &meeting_id).await.unwrap_or_default();
let speaker_by_id: std::collections::HashMap<&str, &MeetingSpeaker> =
    speakers.iter().map(|s| (s.id.as_str(), s)).collect();

let meeting_transcripts = transcripts
    .into_iter()
    .map(|t| {
        let speaker = t.speaker_id.as_deref().and_then(|id| speaker_by_id.get(id));
        MeetingTranscript {
            id: t.id,
            text: t.transcript,
            timestamp: t.timestamp,
            audio_start_time: t.audio_start_time,
            audio_end_time: t.audio_end_time,
            duration: t.duration,
            speaker_id: t.speaker_id.clone(),
            speaker_name: speaker.map(|s| s.display_name.clone()),
            speaker_color: speaker.map(|s| s.color.clone()),
        }
    })
    .collect::<Vec<_>>();
```

- [ ] **Step 6: Check `database/repositories/meeting.rs:92`'s parallel conversion site**

This is a second, separate place `Transcript` rows get read (a denormalized "get full meeting"
path, distinct from the paginated fetch touched in Step 5) — read the surrounding function fully
and apply the same `speaker_id`/`speaker_name`/`speaker_color` treatment there if it constructs the
same or an equivalent output struct. If it turns out to serve a response shape that doesn't include
per-segment speaker info at all (e.g. a metadata-only or text-only summary), leave it unchanged and
note why in the commit message instead of forcing a fit.

- [ ] **Step 7: Verify compile**

- [ ] **Step 8: Commit**

```bash
git add frontend/src-tauri/src/database frontend/src-tauri/src/api frontend/src-tauri/src/audio
git commit -m "feat(diarization): thread speaker_id/name/color through all transcript save and read paths"
```

---

### Task 13: Tauri commands — download/init/rename/merge

**Files:**
- Modify: `frontend/src-tauri/src/diarization_engine/commands.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Engine singleton + init command**

Unlike CAPU (always used if its model is present — no on/off setting), diarization is opt-in, so
`diarization_init` must itself check the saved `diarization_enabled` flag and no-op (leave the
engine unloaded) when it's off — that way callers (`import.rs`/`retranscription.rs`, Task 14) can
call it unconditionally as best-effort, exactly like they already do for `capu_init`, without each
needing their own copy of the enabled/disabled branching logic. The speaker-count setting is read
here too and cached alongside the engine, mirroring how `rover_engine::commands::ROVER_CONFIG`
caches config separately from the engine instance itself:

```rust
use super::engine::DiarizationEngine;
use std::sync::{Arc, Mutex};

pub(crate) static DIARIZATION_ENGINE: Mutex<Option<Arc<Mutex<DiarizationEngine>>>> = Mutex::new(None);
/// `Some(Some(k))` = user specified exactly `k` speakers. `Some(None)` = enabled, auto-detect.
/// `None` = never initialized (or disabled) this run — `get_num_speakers()` and
/// `get_engine_arc()` agree: both are only meaningfully `Some` together.
pub(crate) static DIARIZATION_NUM_SPEAKERS: Mutex<Option<Option<usize>>> = Mutex::new(None);

#[tauri::command]
pub async fn diarization_init<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    {
        let guard = DIARIZATION_ENGINE.lock().unwrap();
        if guard.is_some() {
            return Ok(());
        }
    }

    let app_state = app.try_state::<crate::state::AppState>().ok_or("App state not available")?;
    let config = crate::database::repositories::setting::SettingsRepository::get_transcript_config(
        app_state.db_manager.pool(),
    )
    .await
    .map_err(|e| e.to_string())?;
    let Some(config) = config else { return Ok(()) }; // no settings row yet -> disabled
    if !config.diarization_enabled {
        return Ok(()); // opt-in feature, off by default — no-op, not an error
    }
    let num_speakers = config
        .diarization_num_speakers
        .filter(|&n| n > 0)
        .map(|n| n as usize);

    let dir = resolve_diarization_dir(&app).ok_or_else(|| "Could not resolve app data directory".to_string())?;
    let seg_path = dir.join(DIARIZATION_SEGMENTATION_MODEL_FILE);
    let emb_path = dir.join(DIARIZATION_EMBEDDING_MODEL_FILE);
    if !seg_path.exists() || !emb_path.exists() {
        return Err("Diarization model files are missing.".to_string());
    }
    let (physical_cores, _) = crate::capu_engine::cpu_topology::detect_cpu_topology();
    let engine = DiarizationEngine::load(&seg_path, &emb_path, physical_cores)
        .map_err(|e| e.to_string())?;

    let mut engine_guard = DIARIZATION_ENGINE.lock().unwrap();
    *engine_guard = Some(Arc::new(Mutex::new(engine)));
    *DIARIZATION_NUM_SPEAKERS.lock().unwrap() = Some(num_speakers);
    info!(
        "Diarization engine initialized ({} threads, num_speakers={:?})",
        physical_cores, num_speakers
    );
    Ok(())
}

/// Used internally by the file-import pipeline — not a Tauri command. `None` means either
/// never initialized or disabled; callers treat both the same way (skip diarization).
pub(crate) fn get_engine_arc() -> Option<Arc<Mutex<DiarizationEngine>>> {
    DIARIZATION_ENGINE.lock().unwrap().as_ref().cloned()
}

pub(crate) fn get_num_speakers() -> Option<usize> {
    DIARIZATION_NUM_SPEAKERS.lock().unwrap().flatten()
}
```

`get_transcript_config`'s exact return type (`Option<TranscriptSetting>` vs a `Result` without
`Option`) and whether `diarization_enabled`/`diarization_num_speakers` land on that exact struct
depend on Task 14 Step 2's implementation — if Task 14 is done first (recommended: do Task 14
before this one, or swap their order during execution), copy the real field names/types instead of
the sketch above. If done in this order instead, come back and fix this snippet up once Task 14
Step 2 exists.

- [ ] **Step 2: Rename/merge commands**

```rust
#[tauri::command]
pub async fn rename_meeting_speaker(
    app: AppHandle<impl Runtime>,
    speaker_id: String,
    new_name: String,
) -> Result<bool, String> {
    let state = app.try_state::<crate::state::AppState>().ok_or("App state not available")?;
    crate::database::repositories::meeting_speaker::MeetingSpeakersRepository::rename(
        state.db_manager.pool(),
        &speaker_id,
        &new_name,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn merge_speaker_segment(
    app: AppHandle<impl Runtime>,
    transcript_id: String,
) -> Result<bool, String> {
    let state = app.try_state::<crate::state::AppState>().ok_or("App state not available")?;
    crate::database::repositories::meeting_speaker::MeetingSpeakersRepository::merge_segment_with_previous(
        state.db_manager.pool(),
        &transcript_id,
    )
    .await
    .map_err(|e| e.to_string())
}
```

(Check the exact `AppHandle<impl Runtime>` vs `AppHandle<R>` generic-parameter style other commands
in this codebase use — e.g. `capu_get_models_directory<R: Runtime>(app: AppHandle<R>)` — and match
it exactly rather than introducing `impl Runtime` if the codebase consistently uses the explicit
generic form.)

- [ ] **Step 3: Register in `lib.rs`**

Add `diarization_engine::commands::diarization_init`,
`diarization_engine::commands::rename_meeting_speaker`,
`diarization_engine::commands::merge_speaker_segment` to `generate_handler!`, next to the Task 1
download commands already added there.

- [ ] **Step 4: Verify compile**

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/diarization_engine frontend/src-tauri/src/lib.rs
git commit -m "feat(diarization): init/rename/merge Tauri commands"
```

---

### Task 14: Settings schema + wire diarization into the import pipeline

**Files:**
- Create: `frontend/src-tauri/migrations/20260810010000_add_diarization_settings.sql`
- Modify: `frontend/src-tauri/src/database/models.rs` (`TranscriptSetting`, confirmed struct name —
  `get_transcript_config` runs `SELECT * FROM transcript_settings LIMIT 1` into it, so any new
  column added here is picked up automatically, no query changes needed)
- Modify: `frontend/src-tauri/src/database/repositories/setting.rs` (`save_transcript_config`)
- Modify: `frontend/src-tauri/src/audio/import.rs`
- Modify: `frontend/src-tauri/src/audio/retranscription.rs`
- Modify: `frontend/src-tauri/src/audio/batch_transcribe.rs`

Do this task **before Task 13** — Task 13's `diarization_init` reads the two fields added here.

- [ ] **Step 1: Migration** (mirrors `20260804100000_add_hotwords.sql`'s single-column style)

```sql
ALTER TABLE transcript_settings ADD COLUMN diarizationEnabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE transcript_settings ADD COLUMN diarizationNumSpeakers INTEGER;
```

- [ ] **Step 2: `TranscriptSetting` model**

Add, next to `hotwords`/`capuPunctuationLevel` in `database/models.rs`'s `TranscriptSetting`
(confirmed real fields at lines 121-150+):

```rust
#[sqlx(rename = "diarizationEnabled")]
#[serde(rename = "diarizationEnabled")]
pub diarization_enabled: bool,
#[sqlx(rename = "diarizationNumSpeakers")]
#[serde(rename = "diarizationNumSpeakers")]
pub diarization_num_speakers: Option<i32>,
```

- [ ] **Step 3: `SettingsRepository::save_transcript_config`**

This function takes a long list of **positional** parameters (confirmed: `provider: &str, model:
&str, asr_variant: &str, ...`), not a struct — read the full current parameter list and `INSERT`/
`UPDATE` statement in `setting.rs` before editing (it's long; don't guess where `hotwords`-style
optional params slot in). Add `diarization_enabled: bool, diarization_num_speakers: Option<i32>`
as two new parameters (end of the list, matching how `capu_punctuation_level`/`capu_case_level`
were appended when they were added), and bind them in both the column list and the `?`
placeholders of the existing `INSERT ... ON CONFLICT`/`UPDATE` statement.

- [ ] **Step 4: `diarization_init` unconditional best-effort call in `import.rs`**

Right next to the existing CAPU best-effort init (`import.rs:722-728`):

```rust
if crate::diarization_engine::commands::diarization_is_model_downloaded(app.clone())
    .await
    .unwrap_or(false)
{
    let _ = crate::diarization_engine::commands::diarization_init(app.clone()).await;
}
```

`diarization_init` itself (Task 13) already checks `diarization_enabled` and no-ops when it's
off — `import.rs` doesn't need to read that flag itself, exactly matching how it never checks any
CAPU-specific setting before calling `capu_init` either.

- [ ] **Step 5: Same call in `retranscription.rs`**

`retranscription.rs` has its own independent copy of the CAPU best-effort-init call (confirmed at
lines 277-281, separate from `import.rs`'s) because it has its own settings-read and its own call
into `batch_transcribe` (line 294) — add the identical `diarization_is_model_downloaded`/
`diarization_init` pair there too, right next to its existing CAPU init call. Skipping this file
would silently leave diarization working on first import but not on re-transcribe.

Confirmed by reading both callers directly: `batch_transcribe` never has a real `meeting_id` to
work with (`import.rs`'s local `create_meeting_with_transcripts` generates one at
`import.rs:860`, strictly *after* `batch_transcribe` already returned at `import.rs:739`), while
`retranscription.rs` already has `meeting_id` in scope *before* it calls `batch_transcribe`
(`retranscription.rs:293`, re-transcribing an existing meeting). Because of this asymmetry,
`batch_transcribe` itself must stay meeting-id-agnostic: it returns raw cluster assignments, not
resolved `speaker_id`s, and each caller resolves them (via Task 11's
`resolve_cluster_assignments`) at whatever point *it* first has a real `meeting_id`.

- [ ] **Step 6: Thread the full (pre-VAD) samples into `batch_transcribe`, change its return type**

Diarization needs the *original* decoded audio, not the VAD-segmented `segments: Vec<SpeechSegment>`
`batch_transcribe` already takes (VAD strips silence between segments; diarization needs to see
real gaps to place turn boundaries correctly).

```rust
pub async fn batch_transcribe<R: Runtime>(
    app: &AppHandle<R>,
    segments: Vec<SpeechSegment>,
    leading_context_samples: Vec<usize>,
    primary: PrimaryEngine,
    full_samples: &[f32], // NEW — the whole decoded file, for diarization only; ASR still uses `segments`
    mut on_progress: impl FnMut(usize, usize),
    is_cancelled: impl Fn() -> bool + Send + Sync + Clone + 'static,
) -> Result<(Vec<TranscriptSegment>, Vec<Option<i64>>)> { // NEW — segments, parallel per-segment cluster index (None = no speaker assigned)
```

Every existing early return in the function (the non-Rover branch, empty-input cases, etc.) needs
updating to return `(segments, vec![None; segments.len()])` instead of bare `segments` — grep the
function body for every `return Ok(` / trailing expression once inside it, there are several
branches (parallel vs sequential, Rover vs Single).

Update both call sites (`import.rs:739`, `retranscription.rs:294`) to pass their already-decoded
`audio_samples`/equivalent local variable as `full_samples`, and destructure the new tuple return.

- [ ] **Step 7: Run diarization + alignment inside the `PrimaryEngine::Rover` branch**

Right before `raw_timed_results_to_segments(raw_results)` returns (`batch_transcribe.rs:685-688`):

```rust
let segments = raw_timed_results_to_segments(raw_results);

let cluster_assignments = match crate::diarization_engine::commands::get_engine_arc() {
    Some(engine_arc) => {
        let num_speakers = crate::diarization_engine::commands::get_num_speakers();
        let diarize_result = {
            let mut engine = engine_arc.lock().unwrap();
            engine.diarize(full_samples, num_speakers)
        };
        match diarize_result {
            Ok(turns) if !turns.is_empty() => {
                let spans: Vec<crate::audio::speaker_align::SegmentSpan> = segments
                    .iter()
                    .map(|s| crate::audio::speaker_align::SegmentSpan {
                        start_sec: s.audio_start_time.unwrap_or(0.0),
                        end_sec: s.audio_end_time.unwrap_or(0.0),
                    })
                    .collect();
                crate::audio::speaker_align::align_speakers_to_segments(&turns, &spans)
                    .into_iter()
                    .map(|c| c.map(|c| c as i64))
                    .collect()
            }
            Ok(_) => vec![None; segments.len()], // no turns detected
            Err(e) => {
                log::warn!("Diarization failed: {} — continuing without speaker labels", e);
                vec![None; segments.len()]
            }
        }
    }
    None => vec![None; segments.len()], // diarization disabled or model not loaded
};

return Ok((segments, cluster_assignments));
```

The non-Rover (`PrimaryEngine::Single`) branch and any early-return branches just pair their
existing segments with `vec![None; segments.len()]` — diarization only runs for
`PrimaryEngine::Rover`, matching the spec's ROVER-only scope.

- [ ] **Step 8: Resolve cluster assignments to real `speaker_id`s in each caller**

`import.rs` (around line 739-789): destructure `let (segments, cluster_assignments) = batch_transcribe(...).await?;`, pass `cluster_assignments` as a new parameter into `create_meeting_with_transcripts`. Inside that function, right after `meeting_id` is generated (`import.rs:860`) and before the insert loop:
```rust
let speaker_ids = crate::database::repositories::meeting_speaker::MeetingSpeakersRepository::resolve_cluster_assignments(
    pool, &meeting_id, cluster_assignments,
)
.await
.unwrap_or_else(|e| {
    log::warn!("Diarization: failed to create speaker rows: {} — leaving segments unlabeled", e);
    vec![None; segments.len()]
});
```
then in the existing `for segment in segments` insert loop, zip `segments.iter().zip(&speaker_ids)` instead of iterating `segments` alone, binding `speaker_id` (the zipped value) instead of `&segment.speaker_id` (which is always `None` at this point, per Task 12).

`retranscription.rs` (around line 294-313): since `meeting_id` is already in scope, resolve right after `batch_transcribe` returns, same pattern, then zip into the existing insert loop the same way.

On any failure at any point in this optional path (model not initialized, `diarize()` errors,
speaker-row-creation failing) — log a warning and continue with every segment's resolved
`speaker_id` as `None`, exactly like `CapuBatcher::flush`'s fallback. **Never fail the whole import
over diarization.**

- [ ] **Step 9: Manual test — diarization off (default), confirm import behaves identically to before this task**

- [ ] **Step 10: Commit**

```bash
git add frontend/src-tauri/migrations frontend/src-tauri/src/database frontend/src-tauri/src/audio
git commit -m "feat(diarization): wire diarization into file-import and retranscription pipelines, off by default"
```

---

### Task 15: Frontend — settings UI (checkbox + speaker count)

**Files:**
- Modify: `frontend/src/lib/asr.ts`
- Modify: `frontend/src/components/SharedTranscriptPanel.tsx`

- [ ] **Step 1: `asr.ts` config type + save call**

Add `diarizationEnabled?: boolean` and `diarizationNumSpeakers?: number | null` next to the existing
`hotwords` field (line ~48, ~165) — same optional-field style already used for `hotwords`.

- [ ] **Step 2: `SharedTranscriptPanel.tsx` UI**

Next to the existing `hotwords` textarea (state pattern at line 23, 33, 55, 90), add a checkbox
"Phân biệt người nói" and, conditionally rendered when checked, a number input "Số người nói (để
trống nếu không biết)" clamped 1-20. Follow the exact state-management pattern already used for
`hotwords` in this file (local `useState`, populated from `config` on load, included in the save
payload) rather than introducing a different pattern.

- [ ] **Step 3: Manual test — toggle checkbox, save, reload settings, confirm it persists**

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/asr.ts frontend/src/components/SharedTranscriptPanel.tsx
git commit -m "feat(diarization): settings UI for enabling diarization and speaker count"
```

---

### Task 16: Frontend — speaker display, rename, merge

**Files:**
- Modify: `frontend/src/types/index.ts`
- Modify: `frontend/src/hooks/usePaginatedTranscripts.ts`
- Modify: `frontend/src/components/FlowingTranscriptView.tsx`

- [ ] **Step 1: Types**

Add to `Transcript` (`types/index.ts:7-19`):
```typescript
speaker_id?: string;
speaker_name?: string;
speaker_color?: string;
```
Add to `TranscriptSegmentData` (`types/index.ts:104-112`):
```typescript
speakerId?: string;
speakerName?: string;
speakerColor?: string;
```

- [ ] **Step 2: `usePaginatedTranscripts.ts` conversion**

Extend `convertTranscriptsToSegments` (line 33-41):
```typescript
function convertTranscriptsToSegments(transcripts: Transcript[]): TranscriptSegmentData[] {
    return transcripts.map(t => ({
        id: t.id,
        timestamp: t.audio_start_time ?? 0,
        endTime: t.audio_end_time,
        text: t.text,
        confidence: t.confidence,
        speakerId: t.speaker_id,
        speakerName: t.speaker_name,
        speakerColor: t.speaker_color,
    }));
}
```

- [ ] **Step 3: `FlowingTranscriptView.tsx` — group consecutive same-speaker segments**

Before mapping `segments` to rendered items, group consecutive runs sharing the same
non-undefined `speakerId` (segments with `speakerId === undefined` — the no-diarization case —
render exactly as today, ungrouped). For each group, render a header block: colored dot/badge
(`speakerColor`) + `speakerName` text, clickable to open the rename popover (reuse the existing
`Popover`/`PopoverAnchor`/`PopoverContent` import already used for segment-text editing in this
file), followed by the group's segments flowing as they do today. On hover over a group (except
the meeting's first group), show a small "Gộp với người nói trước" button that calls
`invoke('merge_speaker_segment', { transcriptId: <first segment id in this group> })` and, on
success, triggers whatever refetch/refresh mechanism this component's parent already uses after an
edit (check how `onSegmentEdit`'s callers refresh data and reuse that, rather than inventing a new
refresh path).

- [ ] **Step 4: Rename popover**

On badge click, open a `Popover` with a text input pre-filled with `speakerName`, submit calls
`invoke('rename_meeting_speaker', { speakerId, newName })`, same refresh-on-success pattern as
Step 3.

- [ ] **Step 5: Manual verification**

Run the app (`pnpm run tauri:dev`), import a real multi-speaker audio file with the diarization
checkbox enabled, confirm: speaker badges appear with distinct colors, rename persists after
reload, merge moves a segment to the previous speaker and persists after reload, a meeting imported
*without* diarization enabled still displays exactly as it did before this plan (no badges, no
regressions).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/types/index.ts frontend/src/hooks/usePaginatedTranscripts.ts frontend/src/components/FlowingTranscriptView.tsx
git commit -m "feat(diarization): speaker badges, rename, and merge in the transcript view"
```

---

### Task 17: End-to-end verification (required before this is considered done)

**Files:** none (verification only)

- [ ] **Step 1:** Import a real recording with 2+ known, identifiable speakers (diarization
  checkbox on, speaker count left blank for auto-detect). Confirm the detected speaker count is
  reasonable and turn boundaries roughly line up with actual speaker changes when checked by ear
  against the audio.
- [ ] **Step 2:** Re-import the same file with the speaker count explicitly entered. Confirm the
  result is at least as good as auto-detect (per the spec's expectation that a known count improves
  accuracy).
- [ ] **Step 3:** Rename a speaker, reload the meeting, confirm the new name persists and applies to
  every segment from that speaker.
- [ ] **Step 4:** Find one clearly mis-clustered segment, merge it into the previous speaker,
  reload, confirm it stuck and no other segment changed.
- [ ] **Step 5:** Import a single-speaker recording with diarization enabled — confirm it doesn't
  crash and produces one speaker (or degrades gracefully if the model can't find a real cluster
  boundary in genuinely single-speaker audio).
- [ ] **Step 6:** Import any file with the diarization checkbox left off (default) — confirm import
  time and output are unchanged from before this plan (no accidental always-on regression).
- [ ] **Step 7:** Note any issues found in this task directly to the user rather than silently
  patching around them — this is the point where subjective diarization quality gets judged, not
  just pass/fail tests.
