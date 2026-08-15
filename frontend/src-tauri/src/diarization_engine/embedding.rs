//! ResNet34-LM embedding — fbank (WeSpeaker) + encoder ONNX + masked stats pool + Gemm.
//! Ported from test ASR `speaker_diarization_pure_ort.py` (`compute_emb_fbank`, encoder path).

use anyhow::{anyhow, Result};
use kaldi_native_fbank::mel::MelOptions;
use kaldi_native_fbank::online::FeatureComputer;
use kaldi_native_fbank::{FbankComputer, FbankOptions, FrameOptions, OnlineFeature};
use ndarray_npy::ReadNpyExt;
use ort::session::Session;
use ort::value::TensorRef;
use std::fs::File;
use std::path::Path;

pub const EMB_DIM: usize = 256;
pub const FBANK_DIM: usize = 80;

pub struct EmbeddingModel {
    session: Session,
    /// Shape (256, 5120)
    weight: Vec<f32>,
    /// Shape (256,)
    bias: Vec<f32>,
}

impl EmbeddingModel {
    pub fn load(onnx_dir: &Path, threads: usize) -> Result<Self> {
        let encoder = onnx_dir.join("embedding_encoder.onnx");
        let w_path = onnx_dir.join("resnet_seg_1_weight.npy");
        let b_path = onnx_dir.join("resnet_seg_1_bias.npy");

        let session = Session::builder()
            .map_err(|e| anyhow!("emb session builder: {e}"))?
            .with_intra_threads(threads.max(1))
            .map_err(|e| anyhow!("emb intra threads: {e}"))?
            .commit_from_file(&encoder)
            .map_err(|e| anyhow!("load embedding encoder {}: {e}", encoder.display()))?;

        let weight: ndarray::Array2<f32> = ndarray::Array2::read_npy(File::open(&w_path)?)
            .map_err(|e| anyhow!("read {}: {e}", w_path.display()))?;
        let bias: ndarray::Array1<f32> = ndarray::Array1::read_npy(File::open(&b_path)?)
            .map_err(|e| anyhow!("read {}: {e}", b_path.display()))?;

        if weight.shape() != [EMB_DIM, 5120] {
            return Err(anyhow!(
                "unexpected weight shape {:?}, expected [{EMB_DIM}, 5120]",
                weight.shape()
            ));
        }
        if bias.len() != EMB_DIM {
            return Err(anyhow!(
                "unexpected bias len {}, expected {EMB_DIM}",
                bias.len()
            ));
        }

        Ok(Self {
            session,
            weight: weight.into_raw_vec_and_offset().0,
            bias: bias.to_vec(),
        })
    }

    /// WeSpeaker-style fbank: scale *32768, hamming, 80 mel, CMVN.
    pub fn compute_fbank(audio: &[f32], sample_rate: f32) -> Result<Vec<Vec<f32>>> {
        let frame_opts = FrameOptions {
            samp_freq: sample_rate,
            dither: 0.0,
            frame_length_ms: 25.0,
            frame_shift_ms: 10.0,
            window_type: "hamming".to_string(),
            snip_edges: true,
            ..Default::default()
        };
        let mel_opts = MelOptions {
            num_bins: FBANK_DIM,
            low_freq: 20.0,
            high_freq: 0.0,
            ..Default::default()
        };
        let opts = FbankOptions {
            frame_opts,
            mel_opts,
            use_energy: false,
            energy_floor: 0.0,
            ..Default::default()
        };
        let computer =
            FbankComputer::new(opts).map_err(|e| anyhow!("fbank computer: {e}"))?;
        let mut online = OnlineFeature::new(FeatureComputer::Fbank(computer));

        let scaled: Vec<f32> = audio.iter().map(|s| s * 32768.0).collect();
        online.accept_waveform(sample_rate, &scaled);
        online.input_finished();

        let mut frames = online.features;
        if frames.is_empty() {
            return Ok(frames);
        }
        // Per-utterance CMVN
        let mut means = vec![0.0f32; FBANK_DIM];
        for f in &frames {
            for (i, &v) in f.iter().enumerate() {
                means[i] += v;
            }
        }
        let n = frames.len() as f32;
        for m in &mut means {
            *m /= n;
        }
        for f in &mut frames {
            for (i, v) in f.iter_mut().enumerate() {
                *v -= means[i];
            }
        }
        Ok(frames)
    }

    /// Encode fbank frames (T, 80) → frame features, then stats-pool + Gemm → 256-d L2-normed.
    pub fn embed_from_fbank(&mut self, fbank: &[Vec<f32>]) -> Result<[f32; EMB_DIM]> {
        if fbank.len() < 9 {
            return Err(anyhow!("need at least 9 fbank frames, got {}", fbank.len()));
        }
        let t = fbank.len();
        let mut flat = vec![0.0f32; t * FBANK_DIM];
        for (i, row) in fbank.iter().enumerate() {
            if row.len() != FBANK_DIM {
                return Err(anyhow!("fbank dim {}", row.len()));
            }
            flat[i * FBANK_DIM..(i + 1) * FBANK_DIM].copy_from_slice(row);
        }

        let tensor = TensorRef::from_array_view(([1usize, t, FBANK_DIM], flat.as_slice()))
            .map_err(|e| anyhow!("emb tensor: {e}"))?;
        let outputs = self
            .session
            .run(ort::inputs!["fbank_features" => tensor])
            .map_err(|e| anyhow!("emb inference: {e}"))?;

        // Copy output out of the session borrow before Gemm.
        let (shape_usize, data): (Vec<usize>, Vec<f32>) =
            if let Ok((shape, data)) = outputs["/resnet/pool/Reshape_output_0"].try_extract_tensor::<f32>()
            {
                (
                    shape.iter().map(|&d| d as usize).collect(),
                    data.to_vec(),
                )
            } else {
                let (_name, val) = outputs
                    .iter()
                    .next()
                    .ok_or_else(|| anyhow!("no emb outputs"))?;
                let (shape, data) = val
                    .try_extract_tensor::<f32>()
                    .map_err(|e| anyhow!("emb extract: {e}"))?;
                (
                    shape.iter().map(|&d| d as usize).collect(),
                    data.to_vec(),
                )
            };
        drop(outputs);

        // Expect (1, feat, frames) or (1, frames, feat) — pure_ort uses (D, F) after [0]
        let (feat_dim, n_frames, frame_major) = match shape_usize.as_slice() {
            &[1, d, f] if d > f => (d, f, false), // (1, D, F)
            &[1, f, d] if d > f => (d, f, true),  // (1, F, D)
            &[1, d, f] => (d, f, false),
            other => return Err(anyhow!("unexpected emb encoder shape {:?}", other)),
        };

        // Stats pool over all frames (no mask) — used for smoke / full-chunk path
        let mut mean = vec![0.0f32; feat_dim];
        for fi in 0..n_frames {
            for di in 0..feat_dim {
                let v = if frame_major {
                    data[fi * feat_dim + di]
                } else {
                    data[di * n_frames + fi]
                };
                mean[di] += v;
            }
        }
        for m in &mut mean {
            *m /= n_frames as f32;
        }
        let mut var = vec![0.0f32; feat_dim];
        for fi in 0..n_frames {
            for di in 0..feat_dim {
                let v = if frame_major {
                    data[fi * feat_dim + di]
                } else {
                    data[di * n_frames + fi]
                };
                let d = v - mean[di];
                var[di] += d * d;
            }
        }
        for v in &mut var {
            *v = (*v / n_frames as f32).sqrt();
        }

        // stats = concat(mean, std) length 2*feat_dim; expect 5120 → feat_dim 2560
        let stats_len = mean.len() + var.len();
        if stats_len != 5120 {
            // Some exports already pool — if data is already 5120, use directly
            if data.len() == 5120 {
                return self.gemm_l2(&data);
            }
            return Err(anyhow!(
                "stats len {stats_len} != 5120 (feat_dim={feat_dim}, frames={n_frames})"
            ));
        }
        let mut stats = Vec::with_capacity(5120);
        stats.extend_from_slice(&mean);
        stats.extend_from_slice(&var);
        self.gemm_l2(&stats)
    }

    fn gemm_l2(&self, stats: &[f32]) -> Result<[f32; EMB_DIM]> {
        let mut out = [0.0f32; EMB_DIM];
        for i in 0..EMB_DIM {
            let mut s = self.bias[i];
            let row = &self.weight[i * 5120..(i + 1) * 5120];
            for j in 0..5120 {
                s += row[j] * stats[j];
            }
            out[i] = s;
        }
        let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt() + 1e-10;
        for v in &mut out {
            *v /= norm;
        }
        Ok(out)
    }

    pub fn embed_audio_chunk(&mut self, audio: &[f32], sample_rate: f32) -> Result<[f32; EMB_DIM]> {
        let fbank = Self::compute_fbank(audio, sample_rate)?;
        self.embed_from_fbank(&fbank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn onnx_dir() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("MEETINGONE_DIARIZATION_ONNX_DIR") {
            let dir = PathBuf::from(p);
            if dir.join("embedding_encoder.onnx").exists() {
                return Some(dir);
            }
        }
        let default = PathBuf::from(r"C:\Users\HP\Desktop\test ASR\models\pyannote-onnx");
        if default.join("embedding_encoder.onnx").exists() {
            Some(default)
        } else {
            None
        }
    }

    #[test]
    fn fbank_produces_80_dim() {
        let samples: Vec<f32> = (0..16000).map(|i| (i as f32 * 0.01).sin() * 0.1).collect();
        let frames = EmbeddingModel::compute_fbank(&samples, 16000.0).unwrap();
        assert!(!frames.is_empty());
        assert!(frames.iter().all(|f| f.len() == 80 && f.iter().all(|v| v.is_finite())));
    }

    #[test]
    fn embed_chunk_smoke() {
        let Some(dir) = onnx_dir() else {
            eprintln!("skip: embedding onnx dir missing");
            return;
        };
        let mut model = EmbeddingModel::load(&dir, 2).expect("load emb");
        // 1s tone
        let samples: Vec<f32> = (0..16000).map(|i| (i as f32 * 0.02).sin() * 0.2).collect();
        let emb = model.embed_audio_chunk(&samples, 16000.0).expect("embed");
        assert!(emb.iter().all(|v| v.is_finite()));
        let n = emb.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-3, "l2 norm {n}");
    }
}
