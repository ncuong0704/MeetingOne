use crate::rnnt_decoder::engine::RnntDecoder;
use crate::rnnt_decoder::features::compute_fbank;
use crate::rover_engine::merge::{rover_merge_words, MergedWord};
use anyhow::{anyhow, Result};
use std::path::Path;

pub struct RoverDecodeResult {
    pub text: String,
    pub words: Vec<MergedWord>,
}

pub struct RoverDecoder {
    decoder_a: RnntDecoder,
    decoder_b: RnntDecoder,
}

impl RoverDecoder {
    /// Each `(encoder, decoder, joiner, tokens)` tuple identifies one family's model
    /// files, exactly as `RnntDecoder::load` already takes them — `rover_engine` does
    /// not know about `ModelFamily`; that mapping is Phase C's job.
    pub fn load(
        family_a: (&Path, &Path, &Path, &Path),
        family_b: (&Path, &Path, &Path, &Path),
        beam_size: usize,
        threads_per_decoder: usize,
    ) -> Result<Self> {
        let decoder_a = RnntDecoder::load(
            family_a.0, family_a.1, family_a.2, family_a.3, beam_size, threads_per_decoder,
        )?;
        let decoder_b = RnntDecoder::load(
            family_b.0, family_b.1, family_b.2, family_b.3, beam_size, threads_per_decoder,
        )?;
        Ok(Self { decoder_a, decoder_b })
    }

    /// Computes fbank once and shares it between both concurrently-running models —
    /// they decode the same audio, so a second fbank pass over identical samples is
    /// pure waste. Concurrency (A and B on separate OS threads) is unchanged from
    /// before; only the redundant feature computation is removed.
    pub fn decode(&mut self, samples: &[f32], sample_rate: f32) -> Result<RoverDecodeResult> {
        let fbank = compute_fbank(samples, sample_rate)?;
        let (result_a, result_b) = std::thread::scope(|scope| {
            let decoder_a = &mut self.decoder_a;
            let decoder_b = &mut self.decoder_b;
            let fbank_ref = &fbank;
            let handle_a = scope.spawn(move || decoder_a.decode_with_fbank(fbank_ref));
            let handle_b = scope.spawn(move || decoder_b.decode_with_fbank(fbank_ref));
            let result_a = handle_a.join().map_err(|_| anyhow!("Decoder A thread panicked"));
            let result_b = handle_b.join().map_err(|_| anyhow!("Decoder B thread panicked"));
            (result_a, result_b)
        });

        let decode_a = result_a??;
        let decode_b = result_b??;

        let merged = rover_merge_words(&decode_a.words, &decode_b.words);
        let text = merged
            .iter()
            .map(|m| m.word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        Ok(RoverDecodeResult { text, words: merged })
    }
}

#[cfg(test)]
mod manual_smoke_tests {
    use super::*;
    use std::path::PathBuf;

    fn resolve_tokens_path(model_dir: &PathBuf) -> PathBuf {
        let tokens = model_dir.join("tokens.txt");
        if tokens.exists() {
            tokens
        } else {
            model_dir.join("config.json")
        }
    }

    /// Runs ROVER over ZipFormer 30M int8 + Gipformer 65M int8 on real audio.
    /// Set ROVER_A_DIR, ROVER_B_DIR, ROVER_WAV_PATH. Filenames follow each
    /// family's own convention — adjust the join()s below if pointing at a
    /// different pair than ZipFormer/Gipformer int8.
    #[test]
    #[ignore]
    fn rover_decode_on_real_audio() {
        let dir_a = PathBuf::from(std::env::var("ROVER_A_DIR").expect("set ROVER_A_DIR"));
        let dir_b = PathBuf::from(std::env::var("ROVER_B_DIR").expect("set ROVER_B_DIR"));
        let wav_path = std::env::var("ROVER_WAV_PATH").expect("set ROVER_WAV_PATH");

        let enc_a = dir_a.join("encoder-epoch-20-avg-10.int8.onnx");
        let dec_a = dir_a.join("decoder-epoch-20-avg-10.int8.onnx");
        let joi_a = dir_a.join("joiner-epoch-20-avg-10.int8.onnx");
        let tok_a = resolve_tokens_path(&dir_a);

        let enc_b = dir_b.join("encoder-epoch-35-avg-6.int8.onnx");
        let dec_b = dir_b.join("decoder-epoch-35-avg-6.int8.onnx");
        let joi_b = dir_b.join("joiner-epoch-35-avg-6.int8.onnx");
        let tok_b = resolve_tokens_path(&dir_b);

        let mut rover = RoverDecoder::load(
            (&enc_a, &dec_a, &joi_a, &tok_a),
            (&enc_b, &dec_b, &joi_b, &tok_b),
            4,
            2,
        )
        .expect("load RoverDecoder");

        let decoded = crate::audio::decoder::decode_audio_file(PathBuf::from(&wav_path).as_path())
            .expect("decode audio file");

        let result = rover
            .decode(&decoded.samples, decoded.sample_rate as f32)
            .expect("rover decode");

        println!("Merged text: {}", result.text);
        let disagreements = result.words.iter().filter(|w| w.disagree).count();
        println!(
            "{} / {} words came from a disagreement (B overrode A, or B-only supplement)",
            disagreements,
            result.words.len()
        );
        for w in &result.words {
            let marker = if w.disagree { "*" } else { " " };
            println!(
                "  {}{} [{:.2}-{:.2}s] conf={:.3}",
                marker,
                w.word.text,
                w.word.start,
                w.word.end,
                w.word.confidence
            );
        }

        assert!(!result.text.is_empty());
    }
}
