//! Community-1 segmentation ONNX — sliding 10s windows / 1s step + powerset decode.
//! Ported from test ASR `speaker_diarization_pure_ort.py` `_segment` / POWERSET_MAP.

use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

pub const SAMPLE_RATE: u32 = 16_000;
pub const CHUNK_SAMPLES: usize = 160_000; // 10s
pub const STEP_SAMPLES: usize = 16_000; // 1s
pub const NUM_SEG_FRAMES: usize = 589;
pub const MAX_SPEAKERS_PER_CHUNK: usize = 3;
pub const NUM_POWERSET_CLASSES: usize = 7;

/// Order: silence, single speakers, then pairs (pyannote Powerset, max_classes_per_frame=2)
pub const POWERSET_MAP: [[f32; 3]; 7] = [
    [0.0, 0.0, 0.0], // 0 silence
    [1.0, 0.0, 0.0], // 1 spk0
    [0.0, 1.0, 0.0], // 2 spk1
    [0.0, 0.0, 1.0], // 3 spk2
    [1.0, 1.0, 0.0], // 4 spk0+1
    [1.0, 0.0, 1.0], // 5 spk0+2
    [0.0, 1.0, 1.0], // 6 spk1+2
];

pub struct SegmentationModel {
    session: Session,
    batch_size: usize,
}

pub struct SegmentationOutput {
    /// Logits shape: [num_chunks, NUM_SEG_FRAMES, 7]
    pub logits: Vec<f32>,
    pub num_chunks: usize,
    /// Sample start index of each chunk
    pub chunk_starts: Vec<usize>,
}

impl SegmentationModel {
    pub fn load(model_path: &Path, threads: usize, batch_size: usize) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("seg session builder: {e}"))?
            .with_intra_threads(threads.max(1))
            .map_err(|e| anyhow!("seg intra threads: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("load segmentation model {}: {e}", model_path.display()))?;
        Ok(Self {
            session,
            batch_size: batch_size.max(1),
        })
    }

    /// Mono f32 @ 16 kHz → chunk logits + start sample indices.
    pub fn segment(&mut self, audio: &[f32]) -> Result<SegmentationOutput> {
        let total_samples = audio.len();
        let duration = total_samples as f64 / SAMPLE_RATE as f64;

        let mut starts = Vec::new();
        let mut s = 0usize;
        let mut has_last = false;
        loop {
            if has_last {
                break;
            }
            let chunk_end = (s + CHUNK_SAMPLES) as f64 / SAMPLE_RATE as f64;
            if chunk_end > duration {
                has_last = true;
            }
            starts.push(s);
            s += STEP_SAMPLES;
        }
        if starts.is_empty() {
            starts.push(0);
        }

        let mut all_logits = Vec::new();
        for b in (0..starts.len()).step_by(self.batch_size) {
            let be = (b + self.batch_size).min(starts.len());
            let batch_n = be - b;
            let mut batch = vec![0.0f32; batch_n * CHUNK_SAMPLES];
            for (i, idx) in (b..be).enumerate() {
                let start = starts[idx];
                let end = (start + CHUNK_SAMPLES).min(total_samples);
                let dst = &mut batch[i * CHUNK_SAMPLES..i * CHUNK_SAMPLES + (end - start)];
                dst.copy_from_slice(&audio[start..end]);
            }

            let shape = [batch_n, 1usize, CHUNK_SAMPLES];
            let tensor = TensorRef::from_array_view((shape, batch.as_slice()))
                .map_err(|e| anyhow!("seg tensor: {e}"))?;
            let outputs = self
                .session
                .run(ort::inputs!["input_values" => tensor])
                .map_err(|e| anyhow!("seg inference: {e}"))?;
            let (out_shape, data) = outputs["logits"]
                .try_extract_tensor::<f32>()
                .map_err(|e| anyhow!("seg logits: {e}"))?;
            // Expect [B, 589, 7]
            if out_shape.len() != 3
                || out_shape[0] as usize != batch_n
                || out_shape[1] as usize != NUM_SEG_FRAMES
                || out_shape[2] as usize != NUM_POWERSET_CLASSES
            {
                return Err(anyhow!(
                    "unexpected seg logits shape {:?}, expected [{batch_n}, {NUM_SEG_FRAMES}, 7]",
                    out_shape
                ));
            }
            all_logits.extend_from_slice(data);
        }

        Ok(SegmentationOutput {
            logits: all_logits,
            num_chunks: starts.len(),
            chunk_starts: starts,
        })
    }
}

/// Argmax over powerset classes → hard binary [chunks, frames, 3]
pub fn powerset_binarize(logits: &[f32], num_chunks: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; num_chunks * NUM_SEG_FRAMES * MAX_SPEAKERS_PER_CHUNK];
    for c in 0..num_chunks {
        for f in 0..NUM_SEG_FRAMES {
            let base = (c * NUM_SEG_FRAMES + f) * NUM_POWERSET_CLASSES;
            let mut best_i = 0usize;
            let mut best_v = f32::NEG_INFINITY;
            for k in 0..NUM_POWERSET_CLASSES {
                let v = logits[base + k];
                if v > best_v {
                    best_v = v;
                    best_i = k;
                }
            }
            let map = POWERSET_MAP[best_i];
            let ob = (c * NUM_SEG_FRAMES + f) * MAX_SPEAKERS_PER_CHUNK;
            out[ob] = map[0];
            out[ob + 1] = map[1];
            out[ob + 2] = map[2];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn seg_model_path() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("MEETINGONE_DIARIZATION_ONNX_DIR") {
            let path = PathBuf::from(p).join("segmentation-community-1.onnx");
            if path.exists() {
                return Some(path);
            }
        }
        let default = PathBuf::from(
            r"C:\Users\HP\Desktop\test ASR\models\pyannote-onnx\segmentation-community-1.onnx",
        );
        if default.exists() {
            Some(default)
        } else {
            None
        }
    }

    #[test]
    fn powerset_silence_is_zero() {
        // Class 0 logits highest → all zeros
        let mut logits = vec![0.0f32; 1 * NUM_SEG_FRAMES * 7];
        for f in 0..NUM_SEG_FRAMES {
            let base = f * 7;
            logits[base] = 10.0; // silence
        }
        let bin = powerset_binarize(&logits, 1);
        assert!(bin.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn segment_silence_smoke() {
        let Some(path) = seg_model_path() else {
            eprintln!("skip: segmentation model not found");
            return;
        };
        let mut model = SegmentationModel::load(&path, 2, 4).expect("load");
        // 1.5 seconds of silence → at least one padded chunk
        let audio = vec![0.0f32; SAMPLE_RATE as usize * 3 / 2];
        let out = model.segment(&audio).expect("segment");
        assert!(out.num_chunks >= 1);
        assert_eq!(
            out.logits.len(),
            out.num_chunks * NUM_SEG_FRAMES * NUM_POWERSET_CLASSES
        );
        let bin = powerset_binarize(&out.logits, out.num_chunks);
        assert_eq!(
            bin.len(),
            out.num_chunks * NUM_SEG_FRAMES * MAX_SPEAKERS_PER_CHUNK
        );
        // Finite outputs
        assert!(out.logits.iter().all(|v| v.is_finite()));
    }
}
