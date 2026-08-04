use crate::rnnt_decoder::features::FBANK_DIM;
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

pub struct RnntSessions {
    encoder: Session,
    decoder: Session,
    joiner: Session,
}

fn load_session(path: &Path, label: &str, threads: usize) -> Result<Session> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("Non-UTF8 {} path: {:?}", label, path))?;
    Session::builder()
        .map_err(|e| anyhow!("Failed to create {} session builder: {}", label, e))?
        .with_intra_threads(threads.max(1))
        .map_err(|e| anyhow!("Failed to set {} intra-op threads: {}", label, e))?
        .commit_from_file(path_str)
        .map_err(|e| anyhow!("Failed to load {} {:?}: {}", label, path, e))
}

impl RnntSessions {
    pub fn load(encoder_path: &Path, decoder_path: &Path, joiner_path: &Path, threads: usize) -> Result<Self> {
        Ok(Self {
            encoder: load_session(encoder_path, "encoder", threads)?,
            decoder: load_session(decoder_path, "decoder", threads)?,
            joiner: load_session(joiner_path, "joiner", threads)?,
        })
    }

    /// Runs the encoder over the full fbank feature matrix and returns one Vec<f32> per
    /// output frame. `encoder_out_lens` (not just the raw output shape) determines how
    /// many frames are valid, since some export configurations pad the output.
    pub fn run_encoder(&mut self, fbank: &[Vec<f32>]) -> Result<Vec<Vec<f32>>> {
        let t = fbank.len();
        let mut x_flat: Vec<f32> = Vec::with_capacity(t * FBANK_DIM);
        for frame in fbank {
            x_flat.extend_from_slice(frame);
        }
        let x_tensor = TensorRef::from_array_view(([1usize, t, FBANK_DIM], &*x_flat))
            .map_err(|e| anyhow!("Failed to build encoder input x: {}", e))?;
        let x_lens: Vec<i64> = vec![t as i64];
        let x_lens_tensor = TensorRef::from_array_view(([1usize], &*x_lens))
            .map_err(|e| anyhow!("Failed to build encoder input x_lens: {}", e))?;

        let outputs = self
            .encoder
            .run(ort::inputs!["x" => x_tensor, "x_lens" => x_lens_tensor])
            .map_err(|e| anyhow!("Encoder inference failed: {}", e))?;

        let (enc_shape, enc_data) = outputs["encoder_out"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("Failed to read encoder_out: {}", e))?;
        let (_, lens_data) = outputs["encoder_out_lens"]
            .try_extract_tensor::<i64>()
            .map_err(|e| anyhow!("Failed to read encoder_out_lens: {}", e))?;

        let out_t = lens_data[0] as usize;
        let out_dim = enc_shape[2] as usize;
        let mut frames = Vec::with_capacity(out_t);
        for i in 0..out_t {
            let start = i * out_dim;
            frames.push(enc_data[start..start + out_dim].to_vec());
        }
        Ok(frames)
    }

    /// Runs the stateless, context-size-2 decoder for a batch of context tuples. Each
    /// row is `[y_{t-2}, y_{t-1}]`; the caller may pass `-1` for a not-yet-emitted slot
    /// (icefall convention for the initial context) — clamped to `0` here since the
    /// decoder's embedding table has no negative index.
    pub fn run_decoder(&mut self, contexts: &[[i64; 2]]) -> Result<Vec<Vec<f32>>> {
        let b = contexts.len();
        let mut y_flat: Vec<i64> = Vec::with_capacity(b * 2);
        for ctx in contexts {
            y_flat.push(ctx[0].max(0));
            y_flat.push(ctx[1].max(0));
        }
        let y_tensor = TensorRef::from_array_view(([b, 2usize], &*y_flat))
            .map_err(|e| anyhow!("Failed to build decoder input y: {}", e))?;

        let outputs = self
            .decoder
            .run(ort::inputs!["y" => y_tensor])
            .map_err(|e| anyhow!("Decoder inference failed: {}", e))?;

        let (dec_shape, dec_data) = outputs["decoder_out"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("Failed to read decoder_out: {}", e))?;
        let dim = dec_shape[1] as usize;
        let mut out = Vec::with_capacity(b);
        for i in 0..b {
            let start = i * dim;
            out.push(dec_data[start..start + dim].to_vec());
        }
        Ok(out)
    }

    /// Runs the joiner for a batch of (encoder_out, decoder_out) pairs, returning raw
    /// (pre-softmax) logits per row. Callers must not treat these as probabilities —
    /// `confidence.rs` and `beam_search.rs` both apply their own softmax.
    pub fn run_joiner(
        &mut self,
        encoder_outs: &[Vec<f32>],
        decoder_outs: &[Vec<f32>],
    ) -> Result<Vec<Vec<f32>>> {
        let b = encoder_outs.len();
        let enc_dim = encoder_outs[0].len();
        let dec_dim = decoder_outs[0].len();
        let mut enc_flat: Vec<f32> = Vec::with_capacity(b * enc_dim);
        let mut dec_flat: Vec<f32> = Vec::with_capacity(b * dec_dim);
        for row in encoder_outs {
            enc_flat.extend_from_slice(row);
        }
        for row in decoder_outs {
            dec_flat.extend_from_slice(row);
        }
        let enc_tensor = TensorRef::from_array_view(([b, enc_dim], &*enc_flat))
            .map_err(|e| anyhow!("Failed to build joiner input encoder_out: {}", e))?;
        let dec_tensor = TensorRef::from_array_view(([b, dec_dim], &*dec_flat))
            .map_err(|e| anyhow!("Failed to build joiner input decoder_out: {}", e))?;

        let outputs = self
            .joiner
            .run(ort::inputs!["encoder_out" => enc_tensor, "decoder_out" => dec_tensor])
            .map_err(|e| anyhow!("Joiner inference failed: {}", e))?;

        let (logits_shape, logits_data) = outputs["logit"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("Failed to read joiner logits: {}", e))?;
        let v = logits_shape[1] as usize;
        let mut out = Vec::with_capacity(b);
        for i in 0..b {
            let start = i * v;
            out.push(logits_data[start..start + v].to_vec());
        }
        Ok(out)
    }
}
