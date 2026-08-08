use crate::rnnt_decoder::beam_search::{modified_beam_search, CONTEXT_SIZE};
use crate::rnnt_decoder::confidence::{compute_token_confidence, word_confidence};
use crate::rnnt_decoder::features::compute_fbank;
use crate::rnnt_decoder::sessions::RnntSessions;
use crate::rnnt_decoder::vocab::{pieces_to_words, PieceToken, Vocab};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone)]
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

/// Every model family this app uses (ZipFormer, Gipformer, the 2025 sherpa export) is a
/// non-streaming Zipformer2 encoder, which subsamples its input 4x relative to the 10ms
/// fbank frame shift (verified empirically: feeding a 3000-fbank-frame input to each of
/// the 3 encoders returns `encoder_out_lens` around 748, i.e. a ~4.01 ratio — the small
/// excess over exactly 4.0 is fixed conv padding overhead that shrinks toward 0 as the
/// input grows). So one *encoder output* frame — what beam search's `emitted_frames`
/// indexes into — spans 40ms of real audio, not 10ms. Using 10ms here (as this code did
/// before) understated every word's timestamp by ~4x, an error that starts small (a
/// fraction of a second for early words) and grows without bound as more frames
/// accumulate — exactly the "transcript highlight drifts further ahead of the audio the
/// longer playback runs" symptom this constant fixes.
const ENCODER_FRAME_SHIFT_MS: f32 = 40.0;

pub struct RnntDecoder {
    sessions: RnntSessions,
    vocab: Vocab,
    frame_shift_ms: f32,
    beam_size: usize,
    /// Decoder-network outputs keyed by 2-token context, reused across every `decode`
    /// call this instance makes (i.e. across a whole file's worth of chunks for a given
    /// worker, not just within one chunk) — the decoder is a stateless, pure function of
    /// its context, so a hit here is always correct and skips a real ONNX call. See
    /// `beam_search::missing_contexts`.
    decoder_cache: HashMap<[i64; CONTEXT_SIZE], Vec<f32>>,
}

impl RnntDecoder {
    /// Meetily deliberately uses 4 here, trading a little search width for speed — the
    /// reference app's equivalent (`max_active_paths`) is 8, but that isn't a
    /// requirement, just a different point on the same quality/speed tradeoff. Pass a
    /// different value to tune quality vs. speed.
    pub fn load(
        encoder_path: &Path,
        decoder_path: &Path,
        joiner_path: &Path,
        tokens_path: &Path,
        beam_size: usize,
        threads: usize,
    ) -> Result<Self> {
        let sessions = RnntSessions::load(encoder_path, decoder_path, joiner_path, threads)?;
        let vocab = Vocab::from_tokens_file(tokens_path)?;
        Ok(Self {
            sessions,
            vocab,
            frame_shift_ms: ENCODER_FRAME_SHIFT_MS,
            beam_size,
            decoder_cache: HashMap::new(),
        })
    }

    pub fn decode(&mut self, samples: &[f32], sample_rate: f32) -> Result<DecodeResult> {
        let fbank = compute_fbank(samples, sample_rate)?;
        self.decode_with_fbank(&fbank)
    }

    /// Same as `decode`, but for callers that already have fbank features computed
    /// (ROVER decodes A and B from the same audio, so the caller shares one fbank pass
    /// between both instead of recomputing it per model).
    pub fn decode_with_fbank(&mut self, fbank: &[Vec<f32>]) -> Result<DecodeResult> {
        let encoder_frames = self.sessions.run_encoder(fbank)?;

        let result = modified_beam_search(
            &mut self.sessions,
            &encoder_frames,
            self.beam_size,
            self.vocab.vocab_size(),
            &mut self.decoder_cache,
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

        Ok(DecodeResult {
            text,
            words: word_results,
        })
    }
}

#[cfg(test)]
mod manual_smoke_tests {
    use super::*;
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

    /// End-to-end decode with confidence. Set RNNT_MODEL_DIR and RNNT_WAV_PATH.
    /// Optionally override RNNT_ENCODER_FILE/RNNT_DECODER_FILE/RNNT_JOINER_FILE to point
    /// at a different family's filenames (defaults are ZipFormer 30M's).
    #[test]
    #[ignore]
    fn rnnt_decoder_decode_on_real_audio() {
        let model_dir = PathBuf::from(std::env::var("RNNT_MODEL_DIR").expect("set RNNT_MODEL_DIR"));
        let wav_path = std::env::var("RNNT_WAV_PATH").expect("set RNNT_WAV_PATH");
        let encoder_file = std::env::var("RNNT_ENCODER_FILE")
            .unwrap_or_else(|_| "encoder-epoch-20-avg-10.int8.onnx".to_string());
        let decoder_file = std::env::var("RNNT_DECODER_FILE")
            .unwrap_or_else(|_| "decoder-epoch-20-avg-10.int8.onnx".to_string());
        let joiner_file = std::env::var("RNNT_JOINER_FILE")
            .unwrap_or_else(|_| "joiner-epoch-20-avg-10.int8.onnx".to_string());

        let mut decoder = RnntDecoder::load(
            &model_dir.join(&encoder_file),
            &model_dir.join(&decoder_file),
            &model_dir.join(&joiner_file),
            &resolve_tokens_path(&model_dir),
            4,
            2,
        )
        .expect("load decoder");

        let (samples, sample_rate) = load_audio(&wav_path);
        let result = decoder.decode(&samples, sample_rate as f32).expect("decode");
        println!("Text: {}", result.text);
        for w in &result.words {
            println!(
                "  {} [{:.2}-{:.2}s] conf={:.3} margin_min={:.3} tsallis_max={:.3}",
                w.text, w.start, w.end, w.confidence, w.margin_min, w.tsallis_max
            );
        }
        assert!(!result.text.is_empty());
        for w in &result.words {
            assert!(w.confidence >= 0.0 && w.confidence <= 1.0);
            assert!(!w.confidence.is_nan());
        }
    }
}
