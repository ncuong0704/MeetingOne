# Live Mic Raw Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Live microphone samples reach mix/ASR as raw PCM (matching test ASR), without HPF or EBU R128 on the capture callback.

**Architecture:** Add a documented live-mic pass-through used by `AudioCapture::process_audio_data`. Stop constructing/applying `HighPassFilter` and `LoudnessNormalizer` on the live microphone device. File-import preprocessing stays on `preprocess_file_audio`.

**Tech Stack:** Rust (`frontend/src-tauri`), existing `#[cfg(test)]` in `audio_processing.rs` / `pipeline.rs`.

---

### Task 1: Unit tests for raw live mic vs EBU delay

**Files:**
- Modify: `frontend/src-tauri/src/audio/audio_processing.rs`

- [x] **Step 1: Write the failing tests** (function not yet in production path)

Add next to `loudness_normalizer_tests`:

```rust
#[cfg(test)]
mod live_mic_capture_tests {
    use super::*;

    #[test]
    fn ebu_limiter_delays_an_impulse_unlike_test_asr_raw_path() {
        let sample_rate = 48000u32;
        let lookahead = (sample_rate as usize * 10) / 1000;
        let mut samples = vec![0.0f32; lookahead + 8];
        samples[0] = 0.5;
        let mut normalizer = LoudnessNormalizer::new(1, sample_rate).expect("create");
        let out = normalizer.normalize_loudness(&samples);
        assert!(
            (out[0]).abs() < 1e-6,
            "limiter lookahead should delay the impulse; got {}",
            out[0]
        );
    }

    #[test]
    fn live_microphone_capture_samples_are_raw_identity() {
        let input: Vec<f32> = (0..800).map(|i| if i == 0 { 0.8 } else { 0.01 }).collect();
        let out = live_microphone_capture_samples(&input);
        assert_eq!(out, input);
    }
}
```

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml --lib audio::audio_processing::live_mic_capture_tests -- --nocapture`

Expected: compile fail — `live_microphone_capture_samples` not found.

- [ ] **Step 3: Minimal implementation**

In `audio_processing.rs`, after `LoudnessNormalizer` impl:

```rust
/// Live microphone path (test ASR): raw PCM after format conversion.
/// File import still uses `HighPassFilter` + `LoudnessNormalizer`.
pub fn live_microphone_capture_samples(samples: &[f32]) -> Vec<f32> {
    samples.to_vec()
}
```

- [ ] **Step 4: Run tests — GREEN**

Same cargo command. Expected: both tests PASS.

---

### Task 2: Wire AudioCapture — skip HPF/EBU on live mic

**Files:**
- Modify: `frontend/src-tauri/src/audio/pipeline.rs`

- [ ] **Step 1: Fail if enhancement still mutates live mic**

After constructing `AudioCapture` fields, `process_audio_data` must call `live_microphone_capture_samples` for `DeviceType::Microphone` instead of HPF+EBU.

- [ ] **Step 2: Implementation**

- Do not init `HighPassFilter` / `LoudnessNormalizer` in `AudioCapture::new`.
- Remove those two fields from `AudioCapture` (RNNoise field may stay behind the existing flag).
- In `process_audio_data`, after resample, for microphone: `mono_data = super::audio_processing::live_microphone_capture_samples(&mono_data);` (or simply skip mutation — must not call `normalize_loudness` / `HighPassFilter::process`).
- Drop `LoudnessNormalizer` / `HighPassFilter` from `pipeline.rs` imports if unused.
- Update mixer comment that claimed mic is already −23 LUFS.

- [ ] **Step 3: Run surrounding tests**

```
cargo test --manifest-path frontend/src-tauri/Cargo.toml --lib audio::audio_processing::
cargo test --manifest-path frontend/src-tauri/Cargo.toml --lib audio::pipeline::
cargo test --manifest-path frontend/src-tauri/Cargo.toml --lib audio::file_batch_prepare::
```

Expected: all PASS. File-import HPF+EBU tests still exist and pass.

---

### Task 3: Verify + restart app

- [ ] Run the three `cargo test` commands above.
- [ ] Restart Windows Tauri app via `frontend/clean_run_windows.bat`.
