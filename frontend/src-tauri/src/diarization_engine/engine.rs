//! Diarization orchestrator — Community-1 seg + ResNet34-LM emb + PLDA/VBx.
//!
//! v1 uses chunk-level dominant-speaker embeddings + VBx (same models as test ASR).
//! Full pyannote frame-level reconstruct can be tightened later without changing the
//! public `DiarizationEngine::diarize` API.

use crate::config::{
    DIARIZATION_DEFAULT_FA, DIARIZATION_DEFAULT_FB, DIARIZATION_DEFAULT_THRESHOLD,
    DIARIZATION_SAMPLE_RATE,
};
use crate::diarization_engine::align::SpeakerTurn;
use crate::diarization_engine::embedding::{EmbeddingModel, EMB_DIM};
use crate::diarization_engine::plda::{
    load_plda, plda_transform, vbx_cluster, vbx_hard_labels, xvec_transform, PldaData,
};
use crate::diarization_engine::segmentation::{
    powerset_binarize, SegmentationModel, CHUNK_SAMPLES, MAX_SPEAKERS_PER_CHUNK, NUM_SEG_FRAMES,
    SAMPLE_RATE, STEP_SAMPLES,
};
use anyhow::{anyhow, Result};
use ndarray::Array2;
use std::path::Path;

pub struct DiarizationConfig {
    pub num_speakers: Option<usize>,
    pub num_threads: usize,
    pub threshold: f64,
    pub fa: f64,
    pub fb: f64,
}

impl Default for DiarizationConfig {
    fn default() -> Self {
        Self {
            num_speakers: None,
            num_threads: 4,
            threshold: DIARIZATION_DEFAULT_THRESHOLD,
            fa: DIARIZATION_DEFAULT_FA,
            fb: DIARIZATION_DEFAULT_FB,
        }
    }
}

pub struct DiarizationEngine {
    seg: SegmentationModel,
    emb: EmbeddingModel,
    plda: PldaData,
    cfg: DiarizationConfig,
}

impl DiarizationEngine {
    /// `model_dir` layout:
    /// - `segmentation-community-1.onnx`
    /// - `embedding_encoder.onnx`, `resnet_seg_1_weight.npy`, `resnet_seg_1_bias.npy`
    /// - `plda/plda_prepared.npz`
    pub fn load(model_dir: &Path, cfg: DiarizationConfig) -> Result<Self> {
        let seg_path = model_dir.join("segmentation-community-1.onnx");
        let seg = SegmentationModel::load(&seg_path, cfg.num_threads, 8)?;
        let emb = EmbeddingModel::load(model_dir, cfg.num_threads)?;
        let plda = load_plda(model_dir)?;
        Ok(Self {
            seg,
            emb,
            plda,
            cfg,
        })
    }

    /// Mono f32 @ 16 kHz → speaker turns.
    pub fn diarize(&mut self, samples: &[f32]) -> Result<Vec<SpeakerTurn>> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        if DIARIZATION_SAMPLE_RATE != SAMPLE_RATE {
            return Err(anyhow!("sample rate mismatch"));
        }

        let seg_out = self.seg.segment(samples)?;
        let binarized = powerset_binarize(&seg_out.logits, seg_out.num_chunks);

        // One embedding per chunk from the dominant local speaker (or skip silent chunks).
        let mut emb_rows: Vec<[f32; EMB_DIM]> = Vec::new();
        let mut emb_chunk_idx: Vec<usize> = Vec::new();

        for c in 0..seg_out.num_chunks {
            let mut best_spk = None;
            let mut best_sum = 0.0f32;
            for s in 0..MAX_SPEAKERS_PER_CHUNK {
                let mut sum = 0.0f32;
                for f in 0..NUM_SEG_FRAMES {
                    let i = (c * NUM_SEG_FRAMES + f) * MAX_SPEAKERS_PER_CHUNK + s;
                    sum += binarized[i];
                }
                if sum > best_sum {
                    best_sum = sum;
                    best_spk = Some(s);
                }
            }
            // Need meaningful activity (~>5% of frames)
            if best_spk.is_none() || best_sum < 0.05 * NUM_SEG_FRAMES as f32 {
                continue;
            }

            let start = seg_out.chunk_starts[c];
            let end = (start + CHUNK_SAMPLES).min(samples.len());
            if end <= start + SAMPLE_RATE as usize / 10 {
                continue; // <100ms
            }
            let chunk = &samples[start..end];
            match self.emb.embed_audio_chunk(chunk, SAMPLE_RATE as f32) {
                Ok(e) => {
                    emb_rows.push(e);
                    emb_chunk_idx.push(c);
                }
                Err(e) => {
                    log::debug!("skip chunk {c} embedding: {e}");
                }
            }
        }

        if emb_rows.is_empty() {
            return Ok(Vec::new());
        }

        let n = emb_rows.len();
        let mut emb_mat = Array2::<f64>::zeros((n, EMB_DIM));
        for (i, row) in emb_rows.iter().enumerate() {
            for (j, &v) in row.iter().enumerate() {
                emb_mat[[i, j]] = v as f64;
            }
        }

        let xt = xvec_transform(&emb_mat, &self.plda);
        let pt = plda_transform(&xt, &self.plda, 128);

        // AHC init: if num_speakers fixed, assign round-robin-ish by cosine to random seeds;
        // else binary split by first principal-ish heuristic (sign of dim0 after centering).
        let ahc = initial_ahc_labels(&pt, self.cfg.num_speakers);
        let psi = self.plda.plda_psi.slice(ndarray::s![..128]).to_owned();
        let (gamma, _) = vbx_cluster(
            &pt,
            &psi,
            &ahc,
            self.cfg.fa,
            self.cfg.fb,
            20,
        );
        let labels = vbx_hard_labels(&gamma);

        // Map chunk → cluster, then emit contiguous turns
        let mut chunk_cluster = vec![None; seg_out.num_chunks];
        for (i, &c) in emb_chunk_idx.iter().enumerate() {
            chunk_cluster[c] = Some(labels[i]);
        }

        let duration = samples.len() as f64 / SAMPLE_RATE as f64;
        Ok(chunks_to_turns(
            &chunk_cluster,
            &seg_out.chunk_starts,
            duration,
        ))
    }
}

fn initial_ahc_labels(fea: &Array2<f64>, num_speakers: Option<usize>) -> Vec<usize> {
    let n = fea.nrows();
    if n == 0 {
        return Vec::new();
    }
    if let Some(k) = num_speakers {
        let k = k.max(1);
        // Simple: sort by first dim and cut into k equal bins
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| fea[[a, 0]].partial_cmp(&fea[[b, 0]]).unwrap());
        let mut labels = vec![0usize; n];
        for (rank, &i) in order.iter().enumerate() {
            labels[i] = (rank * k / n).min(k - 1);
        }
        return labels;
    }
    // Auto: 2-way split on dim0 median
    let mut vals: Vec<f64> = (0..n).map(|i| fea[[i, 0]]).collect();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = vals[n / 2];
    (0..n)
        .map(|i| if fea[[i, 0]] >= mid { 1 } else { 0 })
        .collect()
}

fn chunks_to_turns(
    chunk_cluster: &[Option<usize>],
    chunk_starts: &[usize],
    duration: f64,
) -> Vec<SpeakerTurn> {
    let mut turns = Vec::new();
    let mut i = 0;
    while i < chunk_cluster.len() {
        let Some(ci) = chunk_cluster[i] else {
            i += 1;
            continue;
        };
        let start_sec = chunk_starts[i] as f64 / SAMPLE_RATE as f64;
        let mut j = i + 1;
        while j < chunk_cluster.len() && chunk_cluster[j] == Some(ci) {
            // allow small gaps of silent chunks
            let mut k = j;
            while k < chunk_cluster.len() && chunk_cluster[k].is_none() {
                k += 1;
            }
            if k < chunk_cluster.len() && chunk_cluster[k] == Some(ci) && k - j <= 2 {
                j = k + 1;
            } else if chunk_cluster[j] == Some(ci) {
                j += 1;
            } else {
                break;
            }
        }
        let end_sample = if j < chunk_starts.len() {
            chunk_starts[j]
        } else {
            (duration * SAMPLE_RATE as f64) as usize
        };
        // each active chunk covers up to CHUNK_SAMPLES but steps by STEP
        let end_sec = (end_sample as f64 / SAMPLE_RATE as f64)
            .max(start_sec + STEP_SAMPLES as f64 / SAMPLE_RATE as f64)
            .min(duration);
        turns.push(SpeakerTurn {
            start_sec,
            end_sec,
            cluster_index: ci,
        });
        i = j.max(i + 1);
    }
    // Merge adjacent same cluster
    let mut merged: Vec<SpeakerTurn> = Vec::new();
    for t in turns {
        if let Some(last) = merged.last_mut() {
            if last.cluster_index == t.cluster_index && t.start_sec <= last.end_sec + 0.5 {
                last.end_sec = last.end_sec.max(t.end_sec);
                continue;
            }
        }
        merged.push(t);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn model_dir() -> Option<PathBuf> {
        // Combined layout: copy onnx files + plda under one dir for tests we point to
        // test ASR's two folders by synthesizing via env, or skip.
        if let Ok(p) = std::env::var("MEETINGONE_DIARIZATION_MODEL_DIR") {
            let dir = PathBuf::from(p);
            if dir.join("segmentation-community-1.onnx").exists()
                && dir.join("plda/plda_prepared.npz").exists()
            {
                return Some(dir);
            }
        }
        None
    }

    #[test]
    fn chunks_to_turns_merges_same_speaker() {
        let clusters = vec![Some(0), Some(0), None, Some(1)];
        let starts = vec![0, 16000, 32000, 48000];
        let turns = chunks_to_turns(&clusters, &starts, 5.0);
        assert!(turns.len() >= 2);
        assert_eq!(turns[0].cluster_index, 0);
    }

    #[test]
    fn engine_load_optional() {
        let Some(dir) = model_dir() else {
            eprintln!("skip: set MEETINGONE_DIARIZATION_MODEL_DIR with combined Community-1 files");
            return;
        };
        let mut eng = DiarizationEngine::load(&dir, DiarizationConfig::default()).expect("load");
        let audio = vec![0.0f32; SAMPLE_RATE as usize * 3];
        let _ = eng.diarize(&audio); // silence may yield empty — should not panic
    }
}
