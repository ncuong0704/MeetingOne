//! CAM++ 192-dim speaker embedding (3D-Speaker).
//! Input ONNX: `feats` [N, T, 80]; output `embs` [N, 192].
//! Fbank: povey window, 80 mel, energy_floor=1.0, per-utterance CMVN — Senko/test ASR.

use anyhow::{anyhow, Result};
use kaldi_native_fbank::mel::MelOptions;
use kaldi_native_fbank::online::FeatureComputer;
use kaldi_native_fbank::{FbankComputer, FbankOptions, FrameOptions, OnlineFeature};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

pub const EMB_DIM: usize = 192;
pub const FBANK_DIM: usize = 80;

pub struct EmbeddingModel {
    session: Session,
}

impl EmbeddingModel {
    pub fn load(onnx_path: &Path, threads: usize) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("cam++ session builder: {e}"))?
            .with_intra_threads(threads.max(1))
            .map_err(|e| anyhow!("cam++ intra threads: {e}"))?
            .commit_from_file(onnx_path)
            .map_err(|e| anyhow!("load CAM++ {}: {e}", onnx_path.display()))?;
        Ok(Self { session })
    }

    pub fn compute_fbank(audio: &[f32], sample_rate: f32) -> Result<Vec<Vec<f32>>> {
        let frame_opts = FrameOptions {
            samp_freq: sample_rate,
            dither: 0.0,
            frame_length_ms: 25.0,
            frame_shift_ms: 10.0,
            window_type: "povey".to_string(),
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
            energy_floor: 1.0,
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
        let mut means = vec![0.0f32; FBANK_DIM];
        for f in &frames {
            for (i, &v) in f.iter().enumerate().take(FBANK_DIM) {
                means[i] += v;
            }
        }
        let n = frames.len() as f32;
        for m in &mut means {
            *m /= n;
        }
        for f in &mut frames {
            if f.len() > FBANK_DIM {
                f.truncate(FBANK_DIM);
            }
            for (i, v) in f.iter_mut().enumerate().take(FBANK_DIM) {
                *v -= means[i];
            }
        }
        Ok(frames)
    }

    /// Batch embeddings. Each item is a fbank matrix (T, 80). Returns L2-normalized 192-d rows.
    pub fn embed_batch(&mut self, fbanks: &[Vec<Vec<f32>>]) -> Result<Vec<[f32; EMB_DIM]>> {
        if fbanks.is_empty() {
            return Ok(Vec::new());
        }
        let max_t = fbanks.iter().map(|f| f.len()).max().unwrap_or(0);
        if max_t < 10 {
            return Ok(Vec::new());
        }
        let n = fbanks.len();
        let mut flat = vec![0.0f32; n * max_t * FBANK_DIM];
        for (i, fb) in fbanks.iter().enumerate() {
            for (t, row) in fb.iter().enumerate() {
                let dim = row.len().min(FBANK_DIM);
                let off = (i * max_t + t) * FBANK_DIM;
                flat[off..off + dim].copy_from_slice(&row[..dim]);
            }
        }
        let tensor =
            TensorRef::from_array_view(([n, max_t, FBANK_DIM], flat.as_slice()))
                .map_err(|e| anyhow!("cam++ tensor: {e}"))?;
        let outputs = self
            .session
            .run(ort::inputs!["feats" => tensor])
            .map_err(|e| anyhow!("cam++ inference: {e}"))?;
        let (shape, data) = outputs["embs"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("cam++ extract embs: {e}"))?;
        let rows = shape[0] as usize;
        let cols = if shape.len() >= 2 { shape[1] as usize } else { EMB_DIM };
        let mut out = Vec::with_capacity(rows);
        for i in 0..rows {
            let mut emb = [0.0f32; EMB_DIM];
            let take = cols.min(EMB_DIM);
            for j in 0..take {
                emb[j] = data[i * cols + j];
            }
            let nrm = emb.iter().map(|v| v * v).sum::<f32>().sqrt();
            if nrm > 1e-10 {
                for v in &mut emb {
                    *v /= nrm;
                }
            }
            out.push(emb);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fbank_produces_80_dim() {
        let audio: Vec<f32> = (0..16000)
            .map(|i| 0.1 * ((i as f32) * 0.02).sin())
            .collect();
        let frames = EmbeddingModel::compute_fbank(&audio, 16000.0).unwrap();
        assert!(!frames.is_empty());
        assert_eq!(frames[0].len(), FBANK_DIM);
    }
}
