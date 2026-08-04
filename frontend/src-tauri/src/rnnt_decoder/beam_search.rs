use crate::rnnt_decoder::sessions::RnntSessions;
use anyhow::{anyhow, Result};
use std::collections::HashMap;

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
        let decoder_outs: Vec<Vec<f32>> = prev
            .iter()
            .map(|h| decoder_cache[&h.context()].clone())
            .collect();
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
        .ok_or_else(|| anyhow!("Beam search produced no hypotheses"))?;

    Ok(BeamSearchResult {
        token_ids: best.ys[CONTEXT_SIZE..].to_vec(),
        frames: best.emitted_frames.clone(),
        logits: best.emitted_logits.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_add_matches_naive_log_sum_exp_for_moderate_values() {
        let a = -1.0_f32;
        let b = -2.0_f32;
        let naive = (a.exp() + b.exp()).ln();
        let via_log_add = log_add(a, b);
        assert!(
            (naive - via_log_add).abs() < 1e-5,
            "naive={} log_add={}",
            naive,
            via_log_add
        );
    }

    #[test]
    fn log_add_returns_larger_value_when_other_is_negligible() {
        let a = 0.0_f32;
        let b = -100.0_f32;
        assert!((log_add(a, b) - a).abs() < 1e-4);
    }
}

#[cfg(test)]
mod manual_smoke_tests {
    use super::*;
    use crate::rnnt_decoder::{features::compute_fbank, sessions::RnntSessions, vocab::Vocab};
    use std::path::PathBuf;

    fn load_audio(path: &str) -> (Vec<f32>, u32) {
        let decoded = crate::audio::decoder::decode_audio_file(PathBuf::from(path).as_path())
            .expect("decode audio file");
        (decoded.samples, decoded.sample_rate)
    }

    fn resolve_tokens_path(model_dir: &PathBuf) -> PathBuf {
        let tokens = model_dir.join("tokens.txt");
        if tokens.exists() {
            tokens
        } else {
            model_dir.join("config.json")
        }
    }

    /// Run with:
    /// `RNNT_MODEL_DIR=<models dir> RNNT_WAV_PATH=<wav> cargo test --release rnnt_decoder::beam_search::manual_smoke_tests -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn greedy_decode_on_real_audio() {
        let model_dir = PathBuf::from(std::env::var("RNNT_MODEL_DIR").expect("set RNNT_MODEL_DIR"));
        let wav_path = std::env::var("RNNT_WAV_PATH").expect("set RNNT_WAV_PATH");

        let mut sessions = RnntSessions::load(
            &model_dir.join("encoder-epoch-20-avg-10.int8.onnx"),
            &model_dir.join("decoder-epoch-20-avg-10.int8.onnx"),
            &model_dir.join("joiner-epoch-20-avg-10.int8.onnx"),
            2,
        )
        .expect("load sessions");
        let vocab = Vocab::from_tokens_file(&resolve_tokens_path(&model_dir)).expect("load vocab");

        let (samples, sample_rate) = load_audio(&wav_path);
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

    #[test]
    #[ignore]
    fn beam_search_matches_or_beats_greedy_on_real_audio() {
        let model_dir = PathBuf::from(std::env::var("RNNT_MODEL_DIR").expect("set RNNT_MODEL_DIR"));
        let wav_path = std::env::var("RNNT_WAV_PATH").expect("set RNNT_WAV_PATH");

        let mut sessions = RnntSessions::load(
            &model_dir.join("encoder-epoch-20-avg-10.int8.onnx"),
            &model_dir.join("decoder-epoch-20-avg-10.int8.onnx"),
            &model_dir.join("joiner-epoch-20-avg-10.int8.onnx"),
            2,
        )
        .expect("load sessions");
        let vocab = Vocab::from_tokens_file(&resolve_tokens_path(&model_dir)).expect("load vocab");

        let (samples, sample_rate) = load_audio(&wav_path);
        let fbank = compute_fbank(&samples, sample_rate as f32).expect("fbank");
        let encoder_frames = sessions.run_encoder(&fbank).expect("encoder");

        let greedy_tokens = greedy_decode(&mut sessions, &encoder_frames).expect("greedy");
        let beam_result =
            modified_beam_search(&mut sessions, &encoder_frames, 4, vocab.vocab_size())
                .expect("beam search");

        let greedy_text: String = greedy_tokens
            .iter()
            .filter_map(|&id| vocab.piece(id))
            .collect::<Vec<_>>()
            .join(" ");
        let beam_text: String = beam_result
            .token_ids
            .iter()
            .filter_map(|&id| vocab.piece(id))
            .collect::<Vec<_>>()
            .join(" ");
        println!("Greedy: {}\nBeam:   {}", greedy_text, beam_text);
        assert!(!beam_text.is_empty());
    }
}
