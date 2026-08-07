use crate::rnnt_decoder::features::FBANK_DIM;
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::{Path, PathBuf};

pub struct RnntSessions {
    encoder: Session,
    decoder: Session,
    joiner: Session,
    #[cfg(feature = "cuda")]
    fallback: CudaFallbackState,
}

/// Tracks whether each session is still CUDA-backed and what it takes to reload it
/// CPU-only, so a CUDA execution failure during a real `.run()` call (which
/// `commit_from_file` cannot catch — see `load_session`) can be recovered from in place
/// instead of crashing the whole decode.
#[cfg(feature = "cuda")]
struct CudaFallbackState {
    threads: usize,
    encoder_path: PathBuf,
    decoder_path: PathBuf,
    joiner_path: PathBuf,
    encoder_is_cuda: bool,
    decoder_is_cuda: bool,
    joiner_is_cuda: bool,
}

fn build_cpu_session(threads: usize, label: &str) -> Result<ort::session::builder::SessionBuilder> {
    Session::builder()
        .map_err(|e| anyhow!("Failed to create {} session builder: {}", label, e))?
        .with_intra_threads(threads.max(1))
        .map_err(|e| anyhow!("Failed to set {} intra-op threads: {}", label, e))
}

#[cfg(feature = "cuda")]
fn reload_cpu_only(path: &Path, label: &str, threads: usize) -> Result<Session> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("Non-UTF8 {} path: {:?}", label, path))?;
    build_cpu_session(threads, label)?
        .commit_from_file(path_str)
        .map_err(|e| anyhow!("Failed to reload {} {:?} CPU-only: {}", label, path, e))
}

/// Returns the loaded session plus whether it ended up CUDA-backed (always `false` when
/// the crate-level `cuda` feature is off).
#[cfg(feature = "cuda")]
fn load_session(path: &Path, label: &str, threads: usize) -> Result<(Session, bool)> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("Non-UTF8 {} path: {:?}", label, path))?;

    // `with_execution_providers` registers CUDA as a *preference*, not a requirement: if
    // the CUDA/cuDNN native libraries aren't loadable (see `ort::execution_providers::cuda`
    // for the exact DLL/so list ONNX Runtime needs), `ort` logs a warning and falls back to
    // CPU on its own — that path never fails the load. CUDA can also register successfully
    // and still fail at actual execution time, which `commit_from_file` cannot catch: on a
    // 4GB Quadro P600, this encoder's subsampling Conv reliably fails with
    // `CUDNN_BACKEND_API_FAILED` in cuDNN 9's Frontend graph `execute()` on the first real
    // `.run()` call, for both int8 and fp32 weights, regardless of conv algorithm search
    // mode or workspace/arena limits — Pascal-class GPUs have documented practical
    // incompatibilities with recent onnxruntime+cuDNN9 CUDA EP builds (see e.g.
    // https://github.com/blakeblackshear/frigate/discussions/21803 for the same pattern on
    // a GTX 1070). That failure surfaces per-call, not at load time, so `run_encoder` /
    // `run_decoder` / `run_joiner` each retry once with a CPU-reloaded session on error.
    let cuda_builder = build_cpu_session(threads, label)?
        .with_execution_providers([ort::execution_providers::CUDAExecutionProvider::default()
            .with_conv_algorithm_search(ort::execution_providers::cuda::CuDNNConvAlgorithmSearch::Heuristic)
            .with_conv_max_workspace(false)
            .with_arena_extend_strategy(ort::execution_providers::ArenaExtendStrategy::SameAsRequested)
            .build()])
        .map_err(|e| anyhow!("Failed to configure CUDA execution provider for {}: {}", label, e))?;

    match cuda_builder.commit_from_file(path_str) {
        Ok(session) => Ok((session, true)),
        Err(e) => {
            log::warn!(
                "CUDA execution provider failed loading {} model ({:?}): {}. Retrying CPU-only.",
                label,
                path,
                e
            );
            let session = build_cpu_session(threads, label)?
                .commit_from_file(path_str)
                .map_err(|e2| anyhow!("Failed to load {} {:?} (CPU fallback after CUDA failure): {}", label, path, e2))?;
            Ok((session, false))
        }
    }
}

#[cfg(not(feature = "cuda"))]
fn load_session(path: &Path, label: &str, threads: usize) -> Result<(Session, bool)> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("Non-UTF8 {} path: {:?}", label, path))?;
    let session = build_cpu_session(threads, label)?
        .commit_from_file(path_str)
        .map_err(|e| anyhow!("Failed to load {} {:?}: {}", label, path, e))?;
    Ok((session, false))
}

/// Runs the encoder once and fully extracts its output into owned data. Kept as a plain
/// function taking `&mut Session` (not a method taking `&mut self`) so its return type
/// carries no lifetime borrowed from the session — that lets `run_encoder` below reassign
/// `self.encoder` in the error-handling branch without the borrow checker treating the
/// (already-failed) first attempt's `SessionOutputs` as still live.
fn run_and_extract_encoder(session: &mut Session, x_flat: &[f32], x_lens: &[i64], t: usize) -> Result<Vec<Vec<f32>>> {
    let x_tensor = TensorRef::from_array_view(([1usize, t, FBANK_DIM], x_flat))
        .map_err(|e| anyhow!("Failed to build encoder input x: {}", e))?;
    let x_lens_tensor = TensorRef::from_array_view(([1usize], x_lens))
        .map_err(|e| anyhow!("Failed to build encoder input x_lens: {}", e))?;

    let outputs = session
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

/// See `run_and_extract_encoder` for why this is a plain function over `&mut Session`.
fn run_and_extract_decoder(session: &mut Session, y_flat: &[i64], b: usize) -> Result<Vec<Vec<f32>>> {
    let y_tensor = TensorRef::from_array_view(([b, 2usize], y_flat))
        .map_err(|e| anyhow!("Failed to build decoder input y: {}", e))?;

    let outputs = session
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

/// See `run_and_extract_encoder` for why this is a plain function over `&mut Session`.
fn run_and_extract_joiner(
    session: &mut Session,
    enc_flat: &[f32],
    dec_flat: &[f32],
    b: usize,
    enc_dim: usize,
    dec_dim: usize,
) -> Result<Vec<Vec<f32>>> {
    let enc_tensor = TensorRef::from_array_view(([b, enc_dim], enc_flat))
        .map_err(|e| anyhow!("Failed to build joiner input encoder_out: {}", e))?;
    let dec_tensor = TensorRef::from_array_view(([b, dec_dim], dec_flat))
        .map_err(|e| anyhow!("Failed to build joiner input decoder_out: {}", e))?;

    let outputs = session
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

impl RnntSessions {
    pub fn load(encoder_path: &Path, decoder_path: &Path, joiner_path: &Path, threads: usize) -> Result<Self> {
        let (encoder, _encoder_is_cuda) = load_session(encoder_path, "encoder", threads)?;
        let (decoder, _decoder_is_cuda) = load_session(decoder_path, "decoder", threads)?;
        let (joiner, _joiner_is_cuda) = load_session(joiner_path, "joiner", threads)?;

        Ok(Self {
            encoder,
            decoder,
            joiner,
            #[cfg(feature = "cuda")]
            fallback: CudaFallbackState {
                threads,
                encoder_path: encoder_path.to_path_buf(),
                decoder_path: decoder_path.to_path_buf(),
                joiner_path: joiner_path.to_path_buf(),
                encoder_is_cuda: _encoder_is_cuda,
                decoder_is_cuda: _decoder_is_cuda,
                joiner_is_cuda: _joiner_is_cuda,
            },
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
        let x_lens: Vec<i64> = vec![t as i64];

        match run_and_extract_encoder(&mut self.encoder, &x_flat, &x_lens, t) {
            Ok(frames) => Ok(frames),
            Err(e) => {
                #[cfg(feature = "cuda")]
                if self.fallback.encoder_is_cuda {
                    log::warn!(
                        "CUDA execution failed for encoder during inference ({:?}): {}. Reloading CPU-only.",
                        self.fallback.encoder_path,
                        e
                    );
                    self.encoder = reload_cpu_only(&self.fallback.encoder_path, "encoder", self.fallback.threads)?;
                    self.fallback.encoder_is_cuda = false;
                    run_and_extract_encoder(&mut self.encoder, &x_flat, &x_lens, t)
                        .map_err(|e2| anyhow!("Encoder inference failed even after CPU fallback: {}", e2))
                } else {
                    Err(e)
                }
                #[cfg(not(feature = "cuda"))]
                Err(e)
            }
        }
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

        match run_and_extract_decoder(&mut self.decoder, &y_flat, b) {
            Ok(out) => Ok(out),
            Err(e) => {
                #[cfg(feature = "cuda")]
                if self.fallback.decoder_is_cuda {
                    log::warn!(
                        "CUDA execution failed for decoder during inference ({:?}): {}. Reloading CPU-only.",
                        self.fallback.decoder_path,
                        e
                    );
                    self.decoder = reload_cpu_only(&self.fallback.decoder_path, "decoder", self.fallback.threads)?;
                    self.fallback.decoder_is_cuda = false;
                    run_and_extract_decoder(&mut self.decoder, &y_flat, b)
                        .map_err(|e2| anyhow!("Decoder inference failed even after CPU fallback: {}", e2))
                } else {
                    Err(e)
                }
                #[cfg(not(feature = "cuda"))]
                Err(e)
            }
        }
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

        match run_and_extract_joiner(&mut self.joiner, &enc_flat, &dec_flat, b, enc_dim, dec_dim) {
            Ok(out) => Ok(out),
            Err(e) => {
                #[cfg(feature = "cuda")]
                if self.fallback.joiner_is_cuda {
                    log::warn!(
                        "CUDA execution failed for joiner during inference ({:?}): {}. Reloading CPU-only.",
                        self.fallback.joiner_path,
                        e
                    );
                    self.joiner = reload_cpu_only(&self.fallback.joiner_path, "joiner", self.fallback.threads)?;
                    self.fallback.joiner_is_cuda = false;
                    run_and_extract_joiner(&mut self.joiner, &enc_flat, &dec_flat, b, enc_dim, dec_dim)
                        .map_err(|e2| anyhow!("Joiner inference failed even after CPU fallback: {}", e2))
                } else {
                    Err(e)
                }
                #[cfg(not(feature = "cuda"))]
                Err(e)
            }
        }
    }
}
