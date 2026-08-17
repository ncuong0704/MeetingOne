//! Microphone quality check (DNSMOS + VAD + ASR-Proxy) — ported from test ASR
//! `core/audio_analyzer.py` / `quality_result_dialog.py` / `tab_live.test_microphone_quality`.

use crate::config::{
    DNSMOS_INPUT_SAMPLES, DNSMOS_MODEL_FILE, DNSMOS_SHA256, DNSMOS_SIZE_BYTES, DNSMOS_SUBDIR,
    DNSMOS_URL, MIC_QUALITY_RECORD_SECS,
};
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Sample, SampleFormat, SampleRate, StreamConfig};
use futures_util::StreamExt;
use log::{error, info, warn};
use ort::session::Session;
use ort::value::TensorRef;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::io::AsyncWriteExt;

const SAMPLE_RATE: u32 = 16_000;
/// Must be ≥ silero `post_speech_pad` (hardcoded 400ms in `ContinuousVadProcessor`).
/// 300ms (test ASR min-silence) makes silero-rs slice `speech_end+400ms` past the
/// session buffer and panic (`range end index … out of range`).
const VAD_REDEMPTION_MS: u32 = 400;
const MIN_SPEECH_SAMPLES: usize = SAMPLE_RATE as usize / 2; // 0.5s
const MIN_DNSMOS_SAMPLES: usize = (SAMPLE_RATE as f32 * 0.3) as usize;
const ASR_READY_THRESHOLD: f32 = 0.60;
const DNSMOS_READY_THRESHOLD: f32 = 2.5;

static CANCEL: AtomicBool = AtomicBool::new(false);
static IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static DNSMOS_SESSION: Mutex<Option<DnsmosSession>> = Mutex::new(None);

struct DnsmosSession {
    session: Session,
    input_name: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct QualityMetrics {
    pub dnsmos_sig: f32,
    pub dnsmos_bak: f32,
    pub dnsmos_ovrl: f32,
    pub asr_confidence: f32,
    pub sample_text: String,
    pub duration_analyzed: f32,
    pub num_segments: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AnalysisResult {
    pub metrics: QualityMetrics,
    pub suggestions: Vec<String>,
    pub is_ready: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MicQualityProgress {
    pub phase: String,
    pub percent: u32,
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

pub fn poly_eval(a: f64, b: f64, c: f64, x: f64) -> f64 {
    a * x * x + b * x + c
}

/// Microsoft DNSMOS polynomial map, then clip to MOS 1–5.
pub fn map_dnsmos_scores(raw_sig: f32, raw_bak: f32, raw_ovr: f32) -> (f32, f32, f32) {
    let sig = poly_eval(-0.08397278, 1.22083953, 0.0052439, raw_sig as f64);
    let bak = poly_eval(-0.13166888, 1.60915514, -0.39604546, raw_bak as f64);
    let ovr = poly_eval(-0.06766283, 1.11546468, 0.04602535, raw_ovr as f64);
    (
        sig.clamp(1.0, 5.0) as f32,
        bak.clamp(1.0, 5.0) as f32,
        ovr.clamp(1.0, 5.0) as f32,
    )
}

pub fn pad_or_trim_dnsmos(audio: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0f32; DNSMOS_INPUT_SAMPLES];
    let n = audio.len().min(DNSMOS_INPUT_SAMPLES);
    out[..n].copy_from_slice(&audio[..n]);
    out
}

pub fn dnsmos_window_starts(len: usize) -> Vec<usize> {
    if len <= DNSMOS_INPUT_SAMPLES {
        return vec![0];
    }
    let step = DNSMOS_INPUT_SAMPLES / 2;
    let last = len - DNSMOS_INPUT_SAMPLES;
    (0..=last).step_by(step).collect()
}

pub fn asr_confidence_from_log_probs(log_probs: &[f32]) -> Option<f32> {
    if log_probs.is_empty() {
        return None;
    }
    let mean = log_probs.iter().sum::<f32>() / log_probs.len() as f32;
    Some(mean.exp())
}

pub fn generate_suggestions(metrics: &QualityMetrics, has_dnsmos: bool, has_asr: bool) -> Vec<String> {
    let mut suggestions = Vec::new();
    if has_dnsmos {
        if metrics.dnsmos_bak < 2.5 {
            suggestions.push(
                "🔴 Nhiễu nền cao: Tắt quạt, điều hòa, hoặc chuyển nơi yên tĩnh hơn".into(),
            );
        } else if metrics.dnsmos_bak < 3.5 {
            suggestions.push("🟡 Có nhiễu nền: Cố gắng giảm âm thanh xung quanh".into());
        }
        if metrics.dnsmos_sig < 2.5 {
            suggestions.push("🔴 Giọng nói kém: Đưa microphone gần miệng hơn (15-20cm)".into());
        } else if metrics.dnsmos_sig < 3.5 {
            suggestions.push("🟡 Chất lượng giọng nói trung bình: Điều chỉnh vị trí microphone".into());
        }
        if metrics.dnsmos_ovrl < 2.5 {
            suggestions.push(
                "🔴 Chất lượng tổng thể kém: Kiểm tra lại thiết bị và môi trường".into(),
            );
        }
    }
    if has_asr {
        if metrics.asr_confidence < 0.60 {
            suggestions.push("🔴 ASR khó nhận diện: Nói chậm rãi, phát âm rõ ràng từng từ".into());
        } else if metrics.asr_confidence < 0.75 {
            suggestions.push("🟡 ASR có thể sai sót: Kiểm tra kết quả sau khi nhận dạng".into());
        }
    }
    if suggestions.is_empty() {
        suggestions.push("✅ Chất lượng tốt! Sẵn sàng cho nhận dạng.".into());
    }
    suggestions
}

pub fn is_ready(has_dnsmos: bool, ovrl: f32, asr: Option<f32>) -> bool {
    match (has_dnsmos, asr) {
        (true, Some(conf)) => conf >= ASR_READY_THRESHOLD && ovrl >= DNSMOS_READY_THRESHOLD,
        (true, None) => ovrl >= DNSMOS_READY_THRESHOLD,
        (false, Some(conf)) => conf >= ASR_READY_THRESHOLD,
        (false, None) => false,
    }
}

pub fn dnsmos_label(score: f32) -> &'static str {
    if score >= 4.0 {
        "Tốt"
    } else if score >= 3.0 {
        "Khá"
    } else if score >= 2.0 {
        "Trung bình"
    } else {
        "Kém"
    }
}

pub fn confidence_label(confidence: f32) -> &'static str {
    if confidence >= 0.85 {
        "Xuất sắc"
    } else if confidence >= 0.75 {
        "Tốt"
    } else if confidence >= 0.60 {
        "Trung bình"
    } else {
        "Kém"
    }
}

fn resolve_dnsmos_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|d| d.join("models").join(DNSMOS_SUBDIR))
        .map_err(|e| e.to_string())
}

fn dnsmos_model_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(resolve_dnsmos_dir(app)?.join(DNSMOS_MODEL_FILE))
}

pub fn model_file_is_valid(path: &std::path::Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    sha256_hex(&bytes) == DNSMOS_SHA256
}

fn load_dnsmos_session(path: &std::path::Path) -> Result<DnsmosSession> {
    let session = Session::builder()
        .map_err(|e| anyhow!("DNSMOS session builder: {e}"))?
        .with_intra_threads(1)
        .map_err(|e| anyhow!("DNSMOS intra threads: {e}"))?
        .commit_from_file(path)
        .map_err(|e| anyhow!("load DNSMOS {}: {e}", path.display()))?;
    let input_name = session
        .inputs
        .first()
        .map(|i| i.name.clone())
        .ok_or_else(|| anyhow!("DNSMOS model has no inputs"))?;
    Ok(DnsmosSession {
        session,
        input_name,
    })
}

fn compute_dnsmos_one(audio: &[f32], state: &mut DnsmosSession) -> Result<(f32, f32, f32)> {
    let padded = pad_or_trim_dnsmos(audio);
    let tensor = TensorRef::from_array_view(([1usize, DNSMOS_INPUT_SAMPLES], padded.as_slice()))
        .map_err(|e| anyhow!("DNSMOS tensor: {e}"))?;
    let name = state.input_name.clone();
    let outputs = state
        .session
        .run(ort::inputs![name.as_str() => tensor])
        .map_err(|e| anyhow!("DNSMOS inference: {e}"))?;
    let (_out_name, val) = outputs
        .iter()
        .next()
        .ok_or_else(|| anyhow!("DNSMOS has no outputs"))?;
    let (_shape, data) = val
        .try_extract_tensor::<f32>()
        .map_err(|e| anyhow!("DNSMOS extract: {e}"))?;
    if data.len() < 3 {
        return Err(anyhow!("DNSMOS output len {}", data.len()));
    }
    Ok(map_dnsmos_scores(data[0], data[1], data[2]))
}

fn compute_dnsmos_average(audio: &[f32], state: &mut DnsmosSession) -> Result<(f32, f32, f32)> {
    let starts = dnsmos_window_starts(audio.len());
    let mut sig = 0.0;
    let mut bak = 0.0;
    let mut ovr = 0.0;
    let mut n = 0u32;
    for start in starts {
        let end = (start + DNSMOS_INPUT_SAMPLES).min(audio.len());
        if end <= start {
            continue;
        }
        let (s, b, o) = compute_dnsmos_one(&audio[start..end], state)?;
        sig += s;
        bak += b;
        ovr += o;
        n += 1;
    }
    if n == 0 {
        return Err(anyhow!("no DNSMOS windows"));
    }
    let n = n as f32;
    Ok((sig / n, bak / n, ovr / n))
}

fn vad_segments(audio: &[f32]) -> Result<Vec<Vec<f32>>> {
    let chunks = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::audio::vad::get_speech_chunks(audio, VAD_REDEMPTION_MS)
    }));
    match chunks {
        Ok(Ok(segs)) => Ok(segs.into_iter().map(|c| c.samples).collect()),
        Ok(Err(e)) => Err(e),
        Err(_) => {
            warn!("VAD panicked (silero buffer slice); using raw microphone audio");
            Ok(vec![audio.to_vec()])
        }
    }
}

fn analyze_pcm_dnsmos_vad(
    audio: &[f32],
    model_path: &std::path::Path,
) -> Result<(Vec<Vec<f32>>, Option<(f32, f32, f32)>, bool)> {
    let mut segments = vad_segments(audio).unwrap_or_default();
    let mut vad_found = !segments.is_empty();
    if segments.is_empty() {
        warn!("Mic VAD found no speech, using raw audio as fallback");
        vad_found = false;
        if audio.len() >= MIN_SPEECH_SAMPLES {
            segments = vec![audio.to_vec()];
        } else {
            return Ok((Vec::new(), None, vad_found));
        }
    }
    let total: usize = segments.iter().map(|s| s.len()).sum();
    if total < MIN_SPEECH_SAMPLES {
        return Ok((Vec::new(), None, vad_found));
    }

    let mut guard = DNSMOS_SESSION.lock().unwrap();
    if guard.is_none() {
        *guard = Some(load_dnsmos_session(model_path)?);
    }
    let state = guard.as_mut().unwrap();
    let mut scores = Vec::new();
    for seg in &segments {
        if seg.len() >= MIN_DNSMOS_SAMPLES {
            match compute_dnsmos_average(seg, state) {
                Ok(s) => scores.push(s),
                Err(e) => warn!("DNSMOS segment skipped: {e}"),
            }
        }
    }
    let dnsmos = if scores.is_empty() {
        None
    } else {
        let n = scores.len() as f32;
        Some((
            scores.iter().map(|s| s.0).sum::<f32>() / n,
            scores.iter().map(|s| s.1).sum::<f32>() / n,
            scores.iter().map(|s| s.2).sum::<f32>() / n,
        ))
    };
    Ok((segments, dnsmos, vad_found))
}

async fn transcribe_streaming(
    engine: std::sync::Arc<crate::asr_engine::streaming::StreamingEngine>,
    audio: &[f32],
) -> String {
    let hotwords = engine.hotwords().await;
    let rec_guard = engine.recognizer().read().await;
    let Some(recognizer) = rec_guard.as_ref() else {
        return String::new();
    };
    tokio::task::block_in_place(|| {
        let stream = if hotwords.is_empty() {
            recognizer.create_stream()
        } else {
            recognizer.create_stream_with_hotwords(&hotwords)
        };
        let chunk = (SAMPLE_RATE as f32 * 0.1) as usize;
        for part in audio.chunks(chunk.max(1)) {
            stream.accept_waveform(SAMPLE_RATE as i32, part);
            while recognizer.is_ready(&stream) {
                recognizer.decode(&stream);
            }
        }
        stream.input_finished();
        while recognizer.is_ready(&stream) {
            recognizer.decode(&stream);
        }
        recognizer
            .get_result(&stream)
            .map(|r| r.text)
            .unwrap_or_default()
    })
}

/// Load the saved live ASR model if needed (same path as starting a meeting), then
/// transcribe the test clip. Missing model files skip ASR; DNSMOS still returns.
async fn asr_proxy<R: Runtime>(app: &AppHandle<R>, audio: &[f32]) -> (Option<f32>, String) {
    if audio.is_empty() {
        return (None, String::new());
    }
    if let Err(e) = crate::asr_engine::commands::asr_validate_model_ready(
        app.clone(),
        None,
        None,
        None,
        None,
    )
    .await
    {
        warn!("Mic quality ASR skipped: {e}");
        return (None, String::new());
    }

    let raw = if let Some(engine) = crate::asr_engine::streaming::streaming_engine_if_init() {
        if engine.is_loaded().await {
            transcribe_streaming(engine, audio).await
        } else {
            transcribe_offline(audio).await
        }
    } else {
        transcribe_offline(audio).await
    };

    let mut capu_trailing = Vec::new();
    let text = crate::audio::post_asr::process_asr_text(&raw, &mut capu_trailing);
    (None, text.trim().to_string())
}

async fn transcribe_offline(audio: &[f32]) -> String {
    match crate::asr_engine::commands::get_engine_arc() {
        Ok(engine) => match engine.transcribe_audio(audio.to_vec()).await {
            Ok(text) => text,
            Err(e) => {
                warn!("ASR-Proxy offline failed: {e}");
                String::new()
            }
        },
        Err(_) => String::new(),
    }
}

fn finish_metrics(
    segments: &[Vec<f32>],
    dnsmos: Option<(f32, f32, f32)>,
    asr_conf: Option<f32>,
    text: String,
) -> AnalysisResult {
    let total: usize = segments.iter().map(|s| s.len()).sum();
    let mut metrics = QualityMetrics {
        sample_text: text,
        duration_analyzed: total as f32 / SAMPLE_RATE as f32,
        num_segments: segments.len() as u32,
        asr_confidence: asr_conf.unwrap_or(0.0),
        ..Default::default()
    };
    let has_dnsmos = dnsmos.is_some();
    if let Some((sig, bak, ovrl)) = dnsmos {
        metrics.dnsmos_sig = sig;
        metrics.dnsmos_bak = bak;
        metrics.dnsmos_ovrl = ovrl;
    }
    let has_asr = asr_conf.is_some();
    let suggestions = generate_suggestions(&metrics, has_dnsmos, has_asr);
    let ready = is_ready(has_dnsmos, metrics.dnsmos_ovrl, asr_conf);
    AnalysisResult {
        metrics,
        suggestions,
        is_ready: ready,
        error_message: None,
    }
}

fn resolve_input_device(device_name: Option<&str>) -> Result<crate::audio::AudioDevice> {
    match device_name {
        Some(n) if !n.is_empty() && n != "default" => crate::audio::parse_audio_device(n).or_else(|_| {
            Ok(crate::audio::AudioDevice::new(
                n.trim().to_string(),
                crate::audio::DeviceType::Input,
            ))
        }),
        _ => crate::audio::default_input_device(),
    }
}

fn record_blocking<R: Runtime>(
    device: cpal::Device,
    supported: cpal::SupportedStreamConfig,
    app: &AppHandle<R>,
) -> Result<Vec<f32>> {
    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let stream_config = StreamConfig {
        channels,
        sample_rate: SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };
    info!(
        "Mic quality capture: {}Hz, {} ch, {:?}",
        sample_rate, channels, sample_format
    );

    let buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let buf_cb = buf.clone();

    let err_fn = |e| error!("Mic quality stream error: {e}");

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut g) = buf_cb.lock() {
                    g.extend_from_slice(data);
                }
            },
            err_fn,
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                if let Ok(mut g) = buf_cb.lock() {
                    g.extend(data.iter().map(|s| s.to_sample::<f32>()));
                }
            },
            err_fn,
            None,
        )?,
        SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                if let Ok(mut g) = buf_cb.lock() {
                    g.extend(data.iter().map(|s| s.to_sample::<f32>()));
                }
            },
            err_fn,
            None,
        )?,
        other => return Err(anyhow!("Unsupported sample format: {:?}", other)),
    };

    stream.play()?;
    let ticks = MIC_QUALITY_RECORD_SECS * 10;
    for i in 0..ticks {
        if CANCEL.load(Ordering::SeqCst) {
            drop(stream);
            return Err(anyhow!("Đã hủy"));
        }
        std::thread::sleep(Duration::from_millis(100));
        let percent = ((i + 1) * 100) / ticks;
        let _ = app.emit(
            "mic-quality-progress",
            MicQualityProgress {
                phase: "recording".into(),
                percent,
            },
        );
    }
    drop(stream);

    let interleaved = buf.lock().unwrap().clone();
    if interleaved.is_empty() {
        return Err(anyhow!("Không ghi được dữ liệu"));
    }
    let mono = crate::audio::audio_processing::audio_to_mono(&interleaved, channels);
    crate::audio::audio_processing::resample(&mono, sample_rate, SAMPLE_RATE)
}

#[tauri::command]
pub async fn mic_quality_is_model_ready<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    let path = dnsmos_model_path(&app)?;
    Ok(model_file_is_valid(&path))
}

#[tauri::command]
pub async fn mic_quality_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn mic_quality_download_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = resolve_dnsmos_dir(&app)?;
    let dest = dir.join(DNSMOS_MODEL_FILE);
    if model_file_is_valid(&dest) {
        return Ok(());
    }
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        match download_dnsmos(&dir, &app_clone).await {
            Ok(()) => {
                info!("DNSMOS model download complete");
                *DNSMOS_SESSION.lock().unwrap() = None;
                let _ = app_clone.emit("mic-quality-download-complete", ());
            }
            Err(e) => {
                error!("DNSMOS download failed: {e}");
                let _ = app_clone.emit(
                    "mic-quality-download-error",
                    serde_json::json!({ "error": e.to_string() }),
                );
            }
        }
    });
    Ok(())
}

async fn download_dnsmos<R: Runtime>(dir: &PathBuf, app: &AppHandle<R>) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    let dest = dir.join(DNSMOS_MODEL_FILE);
    let tmp = dir.join(format!("{}.tmp", DNSMOS_MODEL_FILE));
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(300))
        .build()?;
    let response = client.get(DNSMOS_URL).send().await?;
    if !response.status().is_success() {
        anyhow::bail!("HTTP {} for DNSMOS", response.status());
    }
    let total = response.content_length().unwrap_or(DNSMOS_SIZE_BYTES);
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last: u8 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        let percent = if total > 0 {
            ((downloaded * 100) / total).min(100) as u8
        } else {
            0
        };
        if percent >= last.saturating_add(5) || percent == 100 {
            last = percent;
            let _ = app.emit(
                "mic-quality-progress",
                MicQualityProgress {
                    phase: "download".into(),
                    percent: percent as u32,
                },
            );
        }
    }
    file.flush().await?;
    drop(file);
    let hex: String = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
    if hex != DNSMOS_SHA256 {
        let _ = tokio::fs::remove_file(&tmp).await;
        anyhow::bail!("SHA-256 mismatch — file bị hỏng hoặc bị thay đổi");
    }
    tokio::fs::rename(&tmp, &dest).await?;
    Ok(())
}

#[tauri::command]
pub async fn mic_quality_analyze<R: Runtime>(
    app: AppHandle<R>,
    device_name: Option<String>,
) -> Result<AnalysisResult, String> {
    if crate::audio::recording_commands::is_recording().await {
        return Err("Đang ghi cuộc họp. Hãy dừng ghi rồi đánh giá microphone.".into());
    }
    if IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return Err("Đang đánh giá microphone".into());
    }
    CANCEL.store(false, Ordering::SeqCst);
    let result = mic_quality_analyze_inner(app, device_name).await;
    IN_PROGRESS.store(false, Ordering::SeqCst);
    result
}

async fn mic_quality_analyze_inner<R: Runtime>(
    app: AppHandle<R>,
    device_name: Option<String>,
) -> Result<AnalysisResult, String> {
    let path = dnsmos_model_path(&app)?;
    if !model_file_is_valid(&path) {
        return Err("Cần tải model DNSMOS (~5MB) để phân tích.".into());
    }

    let audio_device =
        resolve_input_device(device_name.as_deref()).map_err(|e| e.to_string())?;
    let (device, config) = crate::audio::get_device_and_config(&audio_device)
        .await
        .map_err(|e| format!("Không mở được microphone: {e}"))?;

    let app_rec = app.clone();
    let pcm = tokio::task::spawn_blocking(move || record_blocking(device, config, &app_rec))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    if CANCEL.load(Ordering::SeqCst) {
        return Err("Đã hủy".into());
    }

    let _ = app.emit(
        "mic-quality-progress",
        MicQualityProgress {
            phase: "analyzing".into(),
            percent: 0,
        },
    );

    let model_path = path.clone();
    let pcm_for_vad = pcm.clone();
    let (segments, dnsmos, _vad_found) = tokio::task::spawn_blocking(move || {
        analyze_pcm_dnsmos_vad(&pcm_for_vad, &model_path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    if segments.is_empty() {
        return Ok(AnalysisResult {
            error_message: Some("Giọng nói quá ngắn. Vui lòng nói lâu hơn.".into()),
            ..Default::default()
        });
    }

    let concat: Vec<f32> = segments.iter().flatten().copied().collect();
    if CANCEL.load(Ordering::SeqCst) {
        return Err("Đã hủy".into());
    }
    let _ = app.emit(
        "mic-quality-progress",
        MicQualityProgress {
            phase: "transcribing".into(),
            percent: 60,
        },
    );
    let (asr_conf, text) = asr_proxy(&app, &concat).await;
    if CANCEL.load(Ordering::SeqCst) {
        return Err("Đã hủy".into());
    }
    let _ = app.emit(
        "mic-quality-progress",
        MicQualityProgress {
            phase: "transcribing".into(),
            percent: 100,
        },
    );
    if !text.is_empty() {
        info!("Mic quality transcript: {text}");
    }
    Ok(finish_metrics(&segments, dnsmos, asr_conf, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vad_redemption_covers_silero_post_speech_pad() {
        // 300ms redemption + 400ms pad overran the buffer by 70ms (1120 samples @ 16 kHz)
        // because SpeechEnd fires one 30ms frame after redemption (330ms silence vs 400ms pad).
        assert!(VAD_REDEMPTION_MS >= 400);
    }

    #[test]
    fn polynomial_maps_known_raw_score() {
        let (sig, bak, ovr) = map_dnsmos_scores(3.0, 3.0, 3.0);
        assert!((sig - 2.912_007_5).abs() < 1e-5, "sig={sig}");
        let bak_expect = poly_eval(-0.13166888, 1.60915514, -0.39604546, 3.0) as f32;
        let ovr_expect = poly_eval(-0.06766283, 1.11546468, 0.04602535, 3.0) as f32;
        assert!((bak - bak_expect).abs() < 1e-5, "bak={bak}");
        assert!((ovr - ovr_expect).abs() < 1e-5, "ovr={ovr}");
    }

    #[test]
    fn polynomial_clips_to_mos_range() {
        let (sig, bak, ovr) = map_dnsmos_scores(100.0, -100.0, 0.0);
        assert!(sig >= 1.0 && sig <= 5.0);
        assert!(bak >= 1.0 && bak <= 5.0);
        assert!(ovr >= 1.0 && ovr <= 5.0);
    }

    #[test]
    fn pad_short_audio_to_dnsmos_len() {
        let padded = pad_or_trim_dnsmos(&[0.5; 16]);
        assert_eq!(padded.len(), DNSMOS_INPUT_SAMPLES);
        assert_eq!(padded[0], 0.5);
        assert_eq!(padded[15], 0.5);
        assert_eq!(padded[16], 0.0);
    }

    #[test]
    fn trim_long_audio_to_dnsmos_len() {
        let padded = pad_or_trim_dnsmos(&vec![1.0; DNSMOS_INPUT_SAMPLES + 50]);
        assert_eq!(padded.len(), DNSMOS_INPUT_SAMPLES);
        assert!(padded.iter().all(|&x| x == 1.0));
    }

    #[test]
    fn sliding_windows_overlap_half() {
        let len = DNSMOS_INPUT_SAMPLES * 2;
        let starts = dnsmos_window_starts(len);
        assert_eq!(starts[0], 0);
        assert_eq!(starts[1], DNSMOS_INPUT_SAMPLES / 2);
        assert_eq!(*starts.last().unwrap(), DNSMOS_INPUT_SAMPLES);
    }

    #[test]
    fn short_clip_has_single_window() {
        assert_eq!(dnsmos_window_starts(8000), vec![0]);
        assert_eq!(dnsmos_window_starts(DNSMOS_INPUT_SAMPLES), vec![0]);
    }

    #[test]
    fn asr_confidence_exp_mean_log_prob() {
        let conf = asr_confidence_from_log_probs(&[-0.2, -0.2]).unwrap();
        assert!((conf - (-0.2f32).exp()).abs() < 1e-6);
        assert!(asr_confidence_from_log_probs(&[]).is_none());
    }

    #[test]
    fn suggestions_match_thresholds() {
        let metrics = QualityMetrics {
            dnsmos_sig: 2.0,
            dnsmos_bak: 2.0,
            dnsmos_ovrl: 2.0,
            asr_confidence: 0.5,
            ..Default::default()
        };
        let s = generate_suggestions(&metrics, true, true);
        assert!(s.iter().any(|x| x.contains("Nhiễu nền cao")));
        assert!(s.iter().any(|x| x.contains("Giọng nói kém")));
        assert!(s.iter().any(|x| x.contains("tổng thể kém")));
        assert!(s.iter().any(|x| x.contains("ASR khó nhận diện")));
    }

    #[test]
    fn suggestions_good_when_nothing_wrong() {
        let metrics = QualityMetrics {
            dnsmos_sig: 4.2,
            dnsmos_bak: 4.1,
            dnsmos_ovrl: 4.0,
            asr_confidence: 0.9,
            ..Default::default()
        };
        let s = generate_suggestions(&metrics, true, true);
        assert_eq!(s.len(), 1);
        assert!(s[0].contains("Chất lượng tốt"));
    }

    #[test]
    fn is_ready_matches_test_asr_when_both_present() {
        assert!(is_ready(true, 3.0, Some(0.70)));
        assert!(!is_ready(true, 2.0, Some(0.80)));
        assert!(!is_ready(true, 3.0, Some(0.50)));
    }

    #[test]
    fn is_ready_dnsmos_only_when_asr_unmeasured() {
        assert!(is_ready(true, 3.0, None));
        assert!(!is_ready(true, 2.0, None));
        assert!(is_ready(false, 0.0, Some(0.70)));
        assert!(!is_ready(false, 0.0, Some(0.50)));
        assert!(!is_ready(false, 0.0, None));
    }

    #[test]
    fn labels_match_test_asr_bands() {
        assert_eq!(dnsmos_label(4.1), "Tốt");
        assert_eq!(dnsmos_label(3.2), "Khá");
        assert_eq!(dnsmos_label(2.1), "Trung bình");
        assert_eq!(dnsmos_label(1.5), "Kém");
        assert_eq!(confidence_label(0.90), "Xuất sắc");
        assert_eq!(confidence_label(0.80), "Tốt");
        assert_eq!(confidence_label(0.65), "Trung bình");
        assert_eq!(confidence_label(0.40), "Kém");
    }
}
