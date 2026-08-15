//! Senko CAM++ orchestrator — energy VAD + 1.5s/0.6s windows + CAM++ + spectral.

use crate::config::{DIARIZATION_CAMP_FILE, DIARIZATION_SAMPLE_RATE};
use crate::diarization_engine::align::SpeakerTurn;
use crate::diarization_engine::clustering::{energy_vad, senko_cluster, windows_to_turns};
use crate::diarization_engine::embedding::{EmbeddingModel, EMB_DIM};
use anyhow::Result;
use ndarray::Array2;
use std::path::Path;

pub const WINDOW_SEC: f64 = 1.5;
pub const STEP_SEC: f64 = 0.6;
pub const BATCH_SIZE: usize = 32;
pub const FRAME_SHIFT_MS: f64 = 10.0;

pub struct DiarizationConfig {
    pub num_speakers: Option<usize>,
    pub num_threads: usize,
    pub mer_cos: f64,
}

impl Default for DiarizationConfig {
    fn default() -> Self {
        Self {
            num_speakers: None,
            num_threads: 4,
            mer_cos: 0.875,
        }
    }
}

pub struct DiarizationEngine {
    emb: EmbeddingModel,
    cfg: DiarizationConfig,
}

impl DiarizationEngine {
    pub fn load(model_dir: &Path, cfg: DiarizationConfig) -> Result<Self> {
        let onnx = model_dir.join(DIARIZATION_CAMP_FILE);
        let emb = EmbeddingModel::load(&onnx, cfg.num_threads)?;
        Ok(Self { emb, cfg })
    }

    pub fn diarize(&mut self, samples: &[f32]) -> Result<Vec<SpeakerTurn>> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        let duration = samples.len() as f64 / DIARIZATION_SAMPLE_RATE as f64;
        if duration < 0.5 {
            return Ok(Vec::new());
        }

        let mut speech = energy_vad(samples, DIARIZATION_SAMPLE_RATE, 0.1, 0.3, 0.3);
        if speech.is_empty() {
            speech = vec![(0.0, duration)];
        }

        let window_frames = (WINDOW_SEC * 1000.0 / FRAME_SHIFT_MS) as usize; // 150
        let step_frames = (STEP_SEC * 1000.0 / FRAME_SHIFT_MS) as usize; // 60
        let sr = DIARIZATION_SAMPLE_RATE as usize;

        let mut slices: Vec<(Vec<Vec<f32>>, f64, f64)> = Vec::new();
        for (region_start, region_end) in speech {
            let start_sample = (region_start * sr as f64) as usize;
            let end_sample = ((region_end * sr as f64) as usize).min(samples.len());
            if end_sample <= start_sample + sr / 40 {
                continue;
            }
            let region_audio = &samples[start_sample..end_sample];
            let region_fbank =
                EmbeddingModel::compute_fbank(region_audio, DIARIZATION_SAMPLE_RATE as f32)?;
            let n_frames = region_fbank.len();
            if n_frames < 10 {
                continue;
            }
            if n_frames < window_frames {
                slices.push((region_fbank, region_start, region_end));
            } else {
                let mut pos = 0usize;
                while pos + window_frames < n_frames {
                    let fb = region_fbank[pos..pos + window_frames].to_vec();
                    let ws = region_start + pos as f64 * FRAME_SHIFT_MS / 1000.0;
                    slices.push((fb, ws, ws + WINDOW_SEC));
                    pos += step_frames;
                }
                let tail = n_frames.saturating_sub(window_frames);
                let fb = region_fbank[tail..tail + window_frames].to_vec();
                let ws = region_start + tail as f64 * FRAME_SHIFT_MS / 1000.0;
                slices.push((fb, ws, ws + WINDOW_SEC));
            }
        }

        if slices.is_empty() {
            return Ok(Vec::new());
        }

        let mut embeddings: Vec<[f32; EMB_DIM]> = Vec::new();
        let mut times: Vec<(f64, f64)> = Vec::new();
        for batch in slices.chunks(BATCH_SIZE) {
            let fbs: Vec<Vec<Vec<f32>>> = batch.iter().map(|(f, _, _)| f.clone()).collect();
            let embs = self.emb.embed_batch(&fbs)?;
            for (i, e) in embs.into_iter().enumerate() {
                embeddings.push(e);
                times.push((batch[i].1, batch[i].2));
            }
        }

        let n = embeddings.len();
        if n == 0 {
            return Ok(Vec::new());
        }
        let mut mat = Array2::<f64>::zeros((n, EMB_DIM));
        for (i, row) in embeddings.iter().enumerate() {
            for (j, &v) in row.iter().enumerate() {
                mat[[i, j]] = v as f64;
            }
        }

        let (min_spk, max_spk, oracle) = match self.cfg.num_speakers {
            Some(k) if (1..=20).contains(&k) => (k, k, Some(k)),
            _ => (1usize, 15usize, None),
        };

        let labels = if n <= 2 {
            vec![0usize; n]
        } else {
            senko_cluster(
                &mat,
                self.cfg.mer_cos,
                4,
                min_spk,
                max_spk,
                0.012,
                oracle,
            )
        };

        Ok(windows_to_turns(&times, &labels, 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn model_dir() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("MEETINGONE_DIARIZATION_MODEL_DIR") {
            let dir = PathBuf::from(p);
            if dir.join(DIARIZATION_CAMP_FILE).exists() {
                return Some(dir);
            }
        }
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("diarization-fixture");
        if fixture.join(DIARIZATION_CAMP_FILE).exists() {
            return Some(fixture);
        }
        None
    }

    #[test]
    #[ignore]
    fn engine_load_optional() {
        let Some(dir) = model_dir() else {
            return;
        };
        let mut eng = DiarizationEngine::load(&dir, DiarizationConfig::default()).expect("load");
        let audio = vec![0.0f32; DIARIZATION_SAMPLE_RATE as usize * 3];
        let _ = eng.diarize(&audio);
    }
}
