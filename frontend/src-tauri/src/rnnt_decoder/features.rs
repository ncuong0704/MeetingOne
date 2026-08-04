// frontend/src-tauri/src/rnnt_decoder/features.rs
//
// Fbank (mel-filterbank) feature extraction for the hand-written RNNT decoder path,
// via the `kaldi-native-fbank` crate (already a workspace dependency). 80-dim, no
// energy term, no dither — matches the standard icefall/k2 Zipformer training recipe
// (samples are expected to already be 16kHz mono f32, per `sample_rate` passed in).
//
// NOTE: this file was reconstructed after accidental deletion of the untracked
// original during development (see git history around 2026-08-04). The public
// interface (`FBANK_DIM`, `compute_fbank`) matches what `sessions.rs`/`engine.rs`
// already expect; the exact `kaldi_native_fbank` option values below are a
// best-effort match to the standard Zipformer fbank config, not a byte-for-byte
// restoration of the original file.

use anyhow::{anyhow, Result};
use kaldi_native_fbank::mel::MelOptions;
use kaldi_native_fbank::online::FeatureComputer;
use kaldi_native_fbank::{FbankComputer, FbankOptions, FrameOptions, OnlineFeature};

pub const FBANK_DIM: usize = 80;

/// Computes 80-dim log mel-filterbank features for a full utterance. `samples` must
/// already be mono f32 at `sample_rate` Hz (16kHz for every model family in this
/// codebase). Returns one `Vec<f32>` of length `FBANK_DIM` per output frame.
pub fn compute_fbank(samples: &[f32], sample_rate: f32) -> Result<Vec<Vec<f32>>> {
    let frame_opts = FrameOptions {
        samp_freq: sample_rate,
        dither: 0.0, // deterministic output for decode (no train-time-style noise)
        ..Default::default()
    };
    let mel_opts = MelOptions {
        num_bins: FBANK_DIM,
        ..Default::default()
    };
    let opts = FbankOptions {
        frame_opts,
        mel_opts,
        use_energy: false, // dim() == num_bins exactly, matching FBANK_DIM
        ..Default::default()
    };

    let computer =
        FbankComputer::new(opts).map_err(|e| anyhow!("Failed to create fbank computer: {}", e))?;
    let mut online = OnlineFeature::new(FeatureComputer::Fbank(computer));
    online.accept_waveform(sample_rate, samples);
    online.input_finished();

    Ok(online.features)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_fbank_produces_80_dim_frames_for_1_second_of_silence() {
        let samples = vec![0.0f32; 16000];
        let frames = compute_fbank(&samples, 16000.0).expect("fbank should succeed on silence");
        assert!(!frames.is_empty());
        for frame in &frames {
            assert_eq!(frame.len(), FBANK_DIM);
        }
    }

    #[test]
    fn compute_fbank_produces_no_nan_or_inf_values() {
        let samples: Vec<f32> = (0..16000).map(|i| (i as f32 * 0.01).sin() * 0.1).collect();
        let frames = compute_fbank(&samples, 16000.0).expect("fbank should succeed");
        for frame in &frames {
            for &v in frame {
                assert!(v.is_finite(), "fbank produced non-finite value: {}", v);
            }
        }
    }
}
