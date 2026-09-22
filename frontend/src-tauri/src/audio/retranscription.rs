// Retranscription module - re-processes stored audio with the ZipFormer Vietnamese ASR engine.

use crate::audio::audio_processing::create_meeting_folder;
use crate::audio::decoder::load_audio_for_file_pipeline;
use crate::audio::transcription::gemini_file::{
    reject_if_too_long, transcribe_file, vocabulary_from_app,
};
use crate::audio::transcription::gemini_key::{resolve_stt_api_key, SttProvider};
use crate::audio::vad::get_speech_chunks_with_progress;
use super::common::write_transcripts_json;
use super::file_batch_prepare::{boost_audio_for_vad, prepare_file_asr_segments};
use super::constants::AUDIO_EXTENSIONS;
use crate::state::AppState;
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager, Runtime};

static RETRANSCRIPTION_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static RETRANSCRIPTION_CANCELLED: AtomicBool = AtomicBool::new(false);

struct RetranscriptionGuard;

impl RetranscriptionGuard {
    fn acquire() -> Result<Self, String> {
        RETRANSCRIPTION_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "Retranscription already in progress".to_string())?;
        Ok(RetranscriptionGuard)
    }
}

impl Drop for RetranscriptionGuard {
    fn drop(&mut self) {
        RETRANSCRIPTION_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

const VAD_REDEMPTION_TIME_MS: u32 = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetranscriptionProgress {
    pub meeting_id: String,
    pub stage: String,
    pub progress_percentage: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetranscriptionResult {
    pub meeting_id: String,
    pub segments_count: usize,
    pub duration_seconds: f64,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetranscriptionError {
    pub meeting_id: String,
    pub error: String,
}

pub fn is_retranscription_in_progress() -> bool {
    RETRANSCRIPTION_IN_PROGRESS.load(Ordering::SeqCst)
}

pub fn cancel_retranscription() {
    RETRANSCRIPTION_CANCELLED.store(true, Ordering::SeqCst);
}

pub async fn start_retranscription<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    meeting_folder_path: String,
    _language: Option<String>,
    _model: Option<String>,
    _provider: Option<String>,
) -> Result<RetranscriptionResult> {
    let _guard = RetranscriptionGuard::acquire().map_err(|e| anyhow!(e))?;
    RETRANSCRIPTION_CANCELLED.store(false, Ordering::SeqCst);

    let result = run_retranscription(app.clone(), meeting_id.clone(), meeting_folder_path).await;

    super::common::unload_engine_after_batch().await;

    match &result {
        Ok(res) => {
            let _ = app.emit(
                "retranscription-complete",
                serde_json::json!({
                    "meeting_id": res.meeting_id,
                    "segments_count": res.segments_count,
                    "duration_seconds": res.duration_seconds,
                    "language": "vi"
                }),
            );
        }
        Err(e) => {
            let _ = app.emit(
                "retranscription-error",
                RetranscriptionError {
                    meeting_id: meeting_id.clone(),
                    error: e.to_string(),
                },
            );
        }
    }

    result
}

fn find_audio_file(folder: &Path) -> Result<PathBuf> {
    let candidates = [
        // Legacy imports wrote this alongside a copy of the original source.
        "audio_decoded.wav",
        // Current imports persist only this 16 kHz playback WAV.
        "audio.wav",
        "audio.mp4", "audio.m4a", "audio.mp3",
        "audio.flac", "audio.ogg", "recording.mp4",
        "audio.mkv", "audio.webm", "audio.wma",
    ];

    for name in candidates {
        let path = folder.join(name);
        if path.exists() {
            return Ok(path);
        }
    }

    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                    return Ok(path);
                }
            }
        }
    }

    Err(anyhow!("No audio file found in: {}", folder.display()))
}

async fn run_retranscription<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    meeting_folder_path: String,
) -> Result<RetranscriptionResult> {
    let folder_path = PathBuf::from(&meeting_folder_path);
    let audio_path = find_audio_file(&folder_path)?;

    info!("Starting retranscription for meeting {}", meeting_id);

    emit_progress(&app, &meeting_id, "decoding", 5, "Decoding audio file...");

    if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Retranscription cancelled"));
    }

    let path_for_decode = audio_path.clone();
    let (audio_samples, duration_seconds) = tokio::task::spawn_blocking(move || {
        load_audio_for_file_pipeline(&path_for_decode, None)
    })
    .await
    .map_err(|e| anyhow!("Decode task panicked: {}", e))??;

    info!(
        "Loaded audio for retranscription: {:.2}s, {} samples @ 16kHz mono",
        duration_seconds,
        audio_samples.len()
    );

    let file_provider = {
        let app_state = app
            .try_state::<AppState>()
            .ok_or_else(|| anyhow!("App state not available"))?;
        crate::database::repositories::setting::SettingsRepository::get_stt_provider(
            app_state.db_manager.pool(),
            crate::asr_engine::config::AsrPath::File,
        )
        .await
    };

    let mut segments = if file_provider == SttProvider::Gemini {
        if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
            return Err(anyhow!("Retranscription cancelled"));
        }
        reject_if_too_long(duration_seconds)?;
        emit_progress(
            &app,
            &meeting_id,
            "transcribing",
            25,
            "Đang nhận dạng bằng Gemini...",
        );
        let app_state = app
            .try_state::<AppState>()
            .ok_or_else(|| anyhow!("App state not available"))?;
        let api_key = resolve_stt_api_key(app_state.db_manager.pool())
            .await
            .map_err(|e| anyhow!(e))?;
        let vocab = vocabulary_from_app(&app).await;
        if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
            return Err(anyhow!("Retranscription cancelled"));
        }
        transcribe_file(
            &api_key,
            &audio_path,
            duration_seconds,
            &vocab,
            || RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst),
            "Retranscription cancelled",
        )
        .await?
    } else {
    emit_progress(&app, &meeting_id, "vad", 15, "Detecting speech segments...");

    if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Retranscription cancelled"));
    }

    let app_for_vad = app.clone();
    let meeting_id_for_vad = meeting_id.clone();
    let audio_for_vad = boost_audio_for_vad(&audio_samples);

    let speech_segments = tokio::task::spawn_blocking(move || {
        get_speech_chunks_with_progress(
            &audio_for_vad,
            VAD_REDEMPTION_TIME_MS,
            |vad_progress, segments_found| {
                let overall_progress = 20 + (vad_progress as f32 * 0.05) as u32;
                emit_progress(
                    &app_for_vad,
                    &meeting_id_for_vad,
                    "vad",
                    overall_progress,
                    &format!("Detecting speech... {}% ({} found)", vad_progress, segments_found),
                );
                !RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst)
            },
        )
    })
    .await
    .map_err(|e| anyhow!("VAD task panicked: {}", e))?
    .map_err(|e| anyhow!("VAD processing failed: {}", e))?;

    let total_segments = speech_segments.len();
    info!("VAD detected {} speech segments", total_segments);

    if total_segments == 0 {
        return Err(anyhow!("No speech detected in audio file"));
    }

    let file_cfg_preview = {
        let app_state = app
            .try_state::<AppState>()
            .ok_or_else(|| anyhow!("App state not available"))?;
        crate::database::repositories::setting::SettingsRepository::get_path_asr_config(
            app_state.db_manager.pool(),
            crate::asr_engine::config::AsrPath::File,
        )
        .await
    };

    let (processable_segments, leading_context_samples, prepare_stats) = {
        let audio_for_prepare = audio_samples.clone();
        let vad_for_prepare = speech_segments.clone();
        let chunk_sec = file_cfg_preview.max_segment_seconds;
        tokio::task::spawn_blocking(move || {
            prepare_file_asr_segments(&audio_for_prepare, vad_for_prepare, chunk_sec)
        })
        .await
        .map_err(|e| anyhow!("Chunk prepare task panicked: {}", e))?
    };

    let processable_count = processable_segments.len();
    info!(
        "Prepared {} ASR chunks for retranscription (from {} VAD segments, {:.1}s speech, preprocess {:.3}s)",
        processable_count,
        prepare_stats.vad_segments_in,
        prepare_stats.concat_speech_sec,
        prepare_stats.preprocess_sec
    );

    emit_progress(&app, &meeting_id, "transcribing", 25, "Loading Vietnamese ASR...");

    // Ensure ASR engine is ready (ROVER or single-model, per file path config)
    let file_cfg = file_cfg_preview;

    let (engine, rover): (
        Option<std::sync::Arc<crate::asr_engine::engine::AsrEngine>>,
        Option<std::sync::Arc<tokio::sync::Mutex<crate::rover_engine::engine::RoverDecoder>>>,
    ) = if file_cfg.rover_enabled {
        crate::rover_engine::commands::rover_init().await
            .map_err(|e| anyhow!("Failed to init ROVER: {}", e))?;
        crate::rover_engine::commands::rover_validate_model_ready(app.clone())
            .await
            .map_err(|e| anyhow!("{}", e))?;
        let rover = crate::rover_engine::commands::get_engine_arc()
            .map_err(|e| anyhow!("{}", e))?;
        (None, Some(rover))
    } else {
        crate::asr_engine::commands::asr_init().await
            .map_err(|e| anyhow!("Failed to init ASR: {}", e))?;
        crate::asr_engine::commands::asr_validate_model_ready(
            app.clone(),
            Some(file_cfg.family_id.clone()),
            Some(file_cfg.variant.as_str().to_string()),
            Some(file_cfg.decoding_method.clone()),
            Some(file_cfg.num_active_paths),
        )
        .await
        .map_err(|e| anyhow!("{}", e))?;
        let engine = crate::asr_engine::commands::get_engine_arc()
            .map_err(|e| anyhow!("{}", e))?;
        (Some(engine), None)
    };

    // Best-effort CAPU init before retranscription
    if crate::capu_engine::commands::capu_is_model_downloaded(app.clone())
        .await
        .unwrap_or(false)
    {
        let _ = crate::capu_engine::commands::capu_init(app.clone()).await;
    }

    let primary = if let Some(rover) = rover {
        crate::audio::batch_transcribe::PrimaryEngine::Rover(rover)
    } else {
        crate::audio::batch_transcribe::PrimaryEngine::Single(
            engine.expect("engine must be Some when rover is None"),
        )
    };

    let app_for_progress = app.clone();
    let meeting_id_for_progress = meeting_id.clone();
    crate::audio::batch_transcribe::batch_transcribe(
        &app,
        processable_segments,
        leading_context_samples,
        primary,
        move |done, total| {
            let progress = 25 + ((done as f32 / total.max(1) as f32) * 55.0) as u32;
            emit_progress(
                &app_for_progress,
                &meeting_id_for_progress,
                "transcribing",
                progress,
                &format!("Transcribing segment {} of {}...", done, total),
            );
        },
        || RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst),
    )
    .await?
    };

    info!("Transcription complete: {} segments", segments.len());

    if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Retranscription cancelled"));
    }

    // Fail-open diarization (last choice from the import dialog)
    {
        let app_state = app
            .try_state::<AppState>()
            .ok_or_else(|| anyhow!("App state not available"))?;
        let (enabled, num_speakers) =
            match crate::database::repositories::setting::SettingsRepository::get_transcript_config(
                app_state.db_manager.pool(),
            )
            .await
            {
                Ok(Some(cfg)) => crate::audio::import::resolve_diarization_options(
                    Some(cfg.diarization_enabled),
                    cfg.diarization_num_speakers,
                ),
                _ => (false, None),
            };
        crate::audio::import::maybe_apply_diarization(
            &app,
            &audio_samples,
            &mut segments,
            enabled,
            num_speakers,
        )
        .await;
    }

    emit_progress(&app, &meeting_id, "saving", 80, "Saving transcripts...");

    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;

    let pool = app_state.db_manager.pool();
    let mut conn = pool.acquire().await.map_err(|e| anyhow!("DB error: {}", e))?;
    let mut tx = sqlx::Connection::begin(&mut *conn)
        .await
        .map_err(|e| anyhow!("Failed to start transaction: {}", e))?;

    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(&meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("Failed to delete existing transcripts: {}", e))?;

    sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ?")
        .bind(&meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("Failed to delete existing speakers: {}", e))?;

    let mut cluster_to_speaker_id: std::collections::HashMap<usize, String> =
        std::collections::HashMap::new();
    let mut unique_clusters: Vec<usize> = segments
        .iter()
        .filter_map(|s| s.speaker_cluster)
        .collect();
    unique_clusters.sort_unstable();
    unique_clusters.dedup();

    for cluster_index in unique_clusters {
        let speaker_id = format!("speaker-{}", uuid::Uuid::new_v4());
        let display_name = format!("Người nói {}", cluster_index + 1);
        let color = crate::diarization_engine::speaker_color_for_index(cluster_index).to_string();
        sqlx::query(
            "INSERT INTO meeting_speakers (id, meeting_id, cluster_index, display_name, color)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&speaker_id)
        .bind(&meeting_id)
        .bind(cluster_index as i32)
        .bind(&display_name)
        .bind(&color)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("Failed to insert meeting_speaker: {}", e))?;
        cluster_to_speaker_id.insert(cluster_index, speaker_id);
    }

    for segment in &segments {
        let speaker_id = segment
            .speaker_cluster
            .and_then(|c| cluster_to_speaker_id.get(&c).cloned());
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&segment.id)
        .bind(&meeting_id)
        .bind(&segment.text)
        .bind(&segment.timestamp)
        .bind(segment.audio_start_time)
        .bind(segment.audio_end_time)
        .bind(segment.duration)
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("Failed to insert transcript: {}", e))?;
    }

    tx.commit().await
        .map_err(|e| anyhow!("Failed to commit transaction: {}", e))?;

    info!("Updated {} transcripts for meeting {}", segments.len(), meeting_id);

    emit_progress(&app, &meeting_id, "saving", 90, "Writing transcript files...");

    if let Err(e) = write_transcripts_json(&folder_path, &segments) {
        warn!("Failed to write transcripts.json: {}", e);
    }

    let audio_filename = audio_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio.mp4")
        .to_string();

    if let Err(e) = write_retranscription_metadata(&folder_path, &meeting_id, duration_seconds, &audio_filename) {
        warn!("Failed to update metadata.json: {}", e);
    }

    emit_progress(&app, &meeting_id, "complete", 100, "Retranscription complete");

    Ok(RetranscriptionResult {
        meeting_id,
        segments_count: segments.len(),
        duration_seconds,
        language: Some("vi".to_string()),
    })
}

fn emit_progress<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, stage: &str, progress: u32, message: &str) {
    let _ = app.emit(
        "retranscription-progress",
        RetranscriptionProgress {
            meeting_id: meeting_id.to_string(),
            stage: stage.to_string(),
            progress_percentage: progress,
            message: message.to_string(),
        },
    );
}

fn write_retranscription_metadata(folder: &Path, meeting_id: &str, duration_seconds: f64, audio_filename: &str) -> Result<()> {
    let metadata_path = folder.join("metadata.json");
    let temp_path = folder.join(".metadata.json.tmp");
    let now = chrono::Utc::now().to_rfc3339();

    let json = if metadata_path.exists() {
        let existing = std::fs::read_to_string(&metadata_path)?;
        let mut value: serde_json::Value = serde_json::from_str(&existing)?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("retranscribed_at".to_string(), serde_json::json!(now));
            obj.insert("status".to_string(), serde_json::json!("completed"));
            obj.insert("transcript_file".to_string(), serde_json::json!("transcripts.json"));
        }
        value
    } else {
        serde_json::json!({
            "version": "1.0",
            "meeting_id": meeting_id,
            "created_at": now,
            "completed_at": now,
            "retranscribed_at": now,
            "duration_seconds": duration_seconds,
            "audio_file": audio_filename,
            "transcript_file": "transcripts.json",
            "status": "completed",
            "source": "retranscription"
        })
    };

    let json_string = serde_json::to_string_pretty(&json)?;
    std::fs::write(&temp_path, &json_string)?;
    std::fs::rename(&temp_path, &metadata_path)?;
    info!("Wrote metadata.json to {}", metadata_path.display());
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetranscriptionStarted {
    pub meeting_id: String,
    pub message: String,
}

#[tauri::command]
pub async fn start_retranscription_command<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    meeting_folder_path: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<RetranscriptionStarted, String> {
    if RETRANSCRIPTION_IN_PROGRESS.load(Ordering::SeqCst) {
        return Err("Retranscription already in progress".to_string());
    }

    let meeting_id_clone = meeting_id.clone();

    tauri::async_runtime::spawn(async move {
        let result = start_retranscription(app, meeting_id_clone, meeting_folder_path, language, model, provider).await;
        if let Err(e) = result {
            error!("Retranscription failed: {}", e);
        }
    });

    Ok(RetranscriptionStarted {
        meeting_id,
        message: "Retranscription started".to_string(),
    })
}

#[tauri::command]
pub async fn cancel_retranscription_command() -> Result<(), String> {
    if !is_retranscription_in_progress() {
        return Err("No retranscription in progress".to_string());
    }
    cancel_retranscription();
    Ok(())
}

#[tauri::command]
pub async fn is_retranscription_in_progress_command() -> bool {
    is_retranscription_in_progress()
}

/// WAV larger than this is not sent through JS IPC (a 3-hour import freezes WebView2).
const MAX_BLOB_BYTES: u64 = 32 * 1024 * 1024;

/// At most one ffmpeg transcode runs at any moment; queued prepares wait here.
/// Keeps CPU load predictable on weak machines instead of stacking one encoder
/// per opened meeting on top of ASR/UI work.
static TRANSCODE_GATE: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

fn playback_mp4_sidecar(wav: &Path) -> PathBuf {
    wav.with_file_name("audio_playback.mp4")
}

fn playback_file_needs_mp4_transcode(path: &Path, size: u64) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        && size > MAX_BLOB_BYTES
}

fn sidecar_is_fresh(wav: &Path, sidecar: &Path) -> bool {
    let Ok(side_meta) = std::fs::metadata(sidecar) else {
        return false;
    };
    if side_meta.len() == 0 {
        return false;
    }
    let Ok(wav_meta) = std::fs::metadata(wav) else {
        return false;
    };
    match (side_meta.modified(), wav_meta.modified()) {
        (Ok(side_t), Ok(wav_t)) => side_t >= wav_t,
        _ => false,
    }
}

/// Hard ceiling on one transcode. AAC-encoding speech is far faster than realtime,
/// so even a 3-hour recording finishes well under this; it exists to fail loudly
/// instead of leaving a worker stuck on a wedged ffmpeg.
const FFMPEG_TRANSCODE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);

fn transcode_tmp_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

/// PCM WAV duration from the RIFF header (data size / byte rate), used to turn
/// ffmpeg's `out_time` into a percentage for the UI. None for odd headers —
/// callers then just skip progress events.
fn wav_duration_seconds(path: &Path) -> Option<f64> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; 4096];
    let n = file.read(&mut head).ok()?;
    head.truncate(n);
    if head.len() < 12 || &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return None;
    }
    let u32le = |off: usize| -> u32 {
        u32::from_le_bytes([head[off], head[off + 1], head[off + 2], head[off + 3]])
    };
    let mut byte_rate = None;
    let mut data_size = None;
    let mut off = 12;
    while off + 8 <= head.len() {
        let id = &head[off..off + 4];
        let size = u32le(off + 4) as usize;
        let body = off + 8;
        if id == b"fmt " && body + 12 <= head.len() {
            byte_rate = Some(u32le(body + 8));
        } else if id == b"data" {
            data_size = Some(size as u64);
        }
        if byte_rate.is_some() && data_size.is_some() {
            break;
        }
        off = body + size + (size & 1); // RIFF chunks are word-aligned
    }
    let rate = byte_rate.filter(|r| *r > 0)?;
    Some(data_size? as f64 / rate as f64)
}

fn transcode_wav_to_aac_mp4(
    wav: &Path,
    dest: &Path,
    mut on_progress: impl FnMut(f64),
) -> Result<(), String> {
    let ffmpeg_path = super::ffmpeg::find_ffmpeg_path().ok_or_else(|| {
        "FFmpeg not found. Cannot prepare a long recording for playback.".to_string()
    })?;
    let wav_str = wav
        .to_str()
        .ok_or_else(|| format!("WAV path is not valid UTF-8: {}", wav.display()))?;
    // Encode into a temp file and rename on success, so a crash/kill can never
    // leave a half-written sidecar that `sidecar_is_fresh` would trust later.
    let tmp = transcode_tmp_path(dest);
    let tmp_str = tmp
        .to_str()
        .ok_or_else(|| format!("Playback path is not valid UTF-8: {}", dest.display()))?;

    let mut command = std::process::Command::new(ffmpeg_path);
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostats",
        "-progress",
        "pipe:1",
        "-y",
        "-i",
        wav_str,
        // CPU-polite: cap encoder threads so weak machines stay responsive while
        // the app's UI/ASR work continues; speech AAC still encodes far faster
        // than realtime on two threads.
        "-threads",
        "2",
        "-c:a",
        "aac",
        // 16 kHz mono speech: 96k is transparent for AAC-LC at this rate and
        // keeps the sidecar (and the disk I/O to write it) half the size.
        "-b:a",
        "96k",
        "-profile:a",
        "aac_low",
        "-movflags",
        "+faststart",
        "-f",
        "mp4",
        tmp_str,
    ]);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x4000_0000;
        command.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to run FFmpeg: {e}"))?;

    // Drain both pipes concurrently: stdout carries `-progress` key=value lines
    // (latest out_time_us lands in the atomic), stderr is kept for diagnostics.
    // Without readers a chatty child deadlocks on a full pipe.
    let out_time_us = std::sync::Arc::new(AtomicU64::new(0));
    if let Some(stdout) = child.stdout.take() {
        let sink = std::sync::Arc::clone(&out_time_us);
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(value) = line.strip_prefix("out_time_us=") {
                    if let Ok(us) = value.trim().parse::<u64>() {
                        sink.store(us, Ordering::Relaxed);
                    }
                }
            }
        });
    }
    let stderr_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    if let Some(mut stderr) = child.stderr.take() {
        let buf = std::sync::Arc::clone(&stderr_buf);
        std::thread::spawn(move || {
            use std::io::Read;
            let mut collected = String::new();
            let _ = stderr.read_to_string(&mut collected);
            if let Ok(mut guard) = buf.lock() {
                *guard = collected;
            }
        });
    }

    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > FFMPEG_TRANSCODE_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&tmp);
                    return Err("FFmpeg transcode timed out".to_string());
                }
                on_progress(out_time_us.load(Ordering::Relaxed) as f64 / 1_000_000.0);
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("Failed to poll FFmpeg: {e}"));
            }
        }
    };

    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        let stderr = stderr_buf.lock().map(|g| g.clone()).unwrap_or_default();
        return Err(format!("FFmpeg transcode failed: {stderr}"));
    }
    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Failed to finalize playback MP4: {e}")
    })?;
    if std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0) == 0 {
        let _ = std::fs::remove_file(dest);
        return Err("FFmpeg did not write a playback MP4".to_string());
    }
    Ok(())
}

/// What the player should do with a meeting folder right now. Serialized to the
/// frontend as `{"status":"ready","path":...}` or `{"status":"preparing"}`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum MeetingAudioResolution {
    /// A playable file is on disk; `path` is absolute.
    Ready { path: String },
    /// A background transcode was just kicked off. The frontend receives
    /// `meeting-audio-progress` (`MeetingAudioPrepareProgress`) while it runs and
    /// `meeting-audio-status` (`MeetingAudioStatus`) when it is playable.
    Preparing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingAudioStatus {
    pub folder_path: String,
    pub ready: bool,
    pub path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeetingAudioPrepareProgress {
    folder_path: String,
    percent: u32,
}

enum ResolvedPlayback {
    Ready(PathBuf),
    NeedsPrepare(PathBuf),
}

/// Fire-and-forget: make `wav` playable entirely off the user's path. Serialized
/// through `TRANSCODE_GATE` (one ffmpeg at a time, CPU-capped inside), re-checks
/// sidecar freshness under the gate so queued duplicates do no work, and notifies
/// the UI via `meeting-audio-status` in every outcome — including errors, so the
/// frontend never waits on an event that will not come.
pub(crate) fn spawn_playback_prepare<R: Runtime>(
    app: AppHandle<R>,
    folder_path: String,
    wav: PathBuf,
) {
    tauri::async_runtime::spawn(async move {
        let _gate = TRANSCODE_GATE.lock().await;

        let status_error = |app: &AppHandle<R>, folder_path: &str, error: String| {
            let _ = app.emit(
                "meeting-audio-status",
                MeetingAudioStatus {
                    folder_path: folder_path.to_string(),
                    ready: false,
                    path: None,
                    error: Some(error),
                },
            );
        };

        let size = match std::fs::metadata(&wav) {
            Ok(meta) => meta.len(),
            Err(e) => {
                status_error(&app, &folder_path, format!("Cannot read audio file: {e}"));
                return;
            }
        };
        if !playback_file_needs_mp4_transcode(&wav, size) {
            // Small enough not to need a sidecar after all (e.g. replaced file).
            let _ = app.emit(
                "meeting-audio-status",
                MeetingAudioStatus {
                    folder_path,
                    ready: true,
                    path: Some(wav.to_string_lossy().into_owned()),
                    error: None,
                },
            );
            return;
        }
        let sidecar = playback_mp4_sidecar(&wav);
        if sidecar_is_fresh(&wav, &sidecar) {
            let _ = app.emit(
                "meeting-audio-status",
                MeetingAudioStatus {
                    folder_path,
                    ready: true,
                    path: Some(sidecar.to_string_lossy().into_owned()),
                    error: None,
                },
            );
            return;
        }

        info!(
            "Preparing playback MP4 sidecar in background for {}",
            wav.display()
        );
        let wav_for_task = wav.clone();
        let sidecar_for_task = sidecar.clone();
        let folder_for_progress = folder_path.clone();
        let app_for_progress = app.clone();
        let duration = wav_duration_seconds(&wav);
        let mut last_percent = u32::MAX;
        let result = tokio::task::spawn_blocking(move || {
            transcode_wav_to_aac_mp4(&wav_for_task, &sidecar_for_task, |seconds| {
                let total = duration.unwrap_or(0.0);
                if total <= 0.0 {
                    return;
                }
                let percent = ((seconds / total).clamp(0.0, 1.0) * 100.0) as u32;
                if percent != last_percent {
                    last_percent = percent;
                    let _ = app_for_progress.emit(
                        "meeting-audio-progress",
                        MeetingAudioPrepareProgress {
                            folder_path: folder_for_progress.clone(),
                            percent,
                        },
                    );
                }
            })
        })
        .await
        .map_err(|e| format!("Transcode task panicked: {e}"))
        .and_then(|r| r);

        match result {
            Ok(()) => {
                let _ = app.emit(
                    "meeting-audio-status",
                    MeetingAudioStatus {
                        folder_path,
                        ready: true,
                        path: Some(sidecar.to_string_lossy().into_owned()),
                        error: None,
                    },
                );
            }
            Err(e) => {
                warn!("Playback sidecar prepare failed for {}: {}", wav.display(), e);
                status_error(&app, &folder_path, e);
            }
        }
    });
}

fn resolve_playback_sync(folder_path: String) -> Result<ResolvedPlayback, String> {
    let trimmed = folder_path.trim_end_matches(|c| c == '/' || c == '\\');
    let path = find_audio_file(Path::new(trimmed)).map_err(|e| e.to_string())?;
    let size = std::fs::metadata(&path)
        .map_err(|e| e.to_string())?
        .len();
    if !playback_file_needs_mp4_transcode(&path, size) {
        return Ok(ResolvedPlayback::Ready(path));
    }
    let sidecar = playback_mp4_sidecar(&path);
    if sidecar_is_fresh(&path, &sidecar) {
        return Ok(ResolvedPlayback::Ready(sidecar));
    }
    // Never transcode inline: this must answer in milliseconds. The caller kicks
    // off `spawn_playback_prepare` and reports `Preparing` to the UI instead.
    Ok(ResolvedPlayback::NeedsPrepare(path))
}

/// Returns the playback-ready path, or `Preparing` while a background transcode
/// runs. The file-existence/freshness checks run on the blocking-task pool — a
/// plain (non-`async`) `#[tauri::command]` executes on the main/event-loop thread,
/// which previously froze the whole WebView for the duration of the work.
#[tauri::command]
pub async fn resolve_meeting_audio_file_path<R: Runtime>(
    app: AppHandle<R>,
    folder_path: String,
) -> Result<MeetingAudioResolution, String> {
    let folder_for_sync = folder_path.clone();
    let outcome = tokio::task::spawn_blocking(move || resolve_playback_sync(folder_for_sync))
        .await
        .map_err(|e| format!("Audio resolve task panicked: {e}"))??;
    match outcome {
        ResolvedPlayback::Ready(path) => Ok(MeetingAudioResolution::Ready {
            path: path.to_string_lossy().into_owned(),
        }),
        ResolvedPlayback::NeedsPrepare(wav) => {
            spawn_playback_prepare(app, folder_path, wav);
            Ok(MeetingAudioResolution::Preparing)
        }
    }
}

/// Bytes of the meeting playback file. Used for imported 16 kHz WAV: WebView2's
/// `<audio>` element often rejects `convertFileSrc` for PCM WAV even when the
/// file exists (MEDIA_ERR_SRC_NOT_SUPPORTED), which the UI used to show as
/// "no recording saved".
///
/// Returns a raw IPC response (an ArrayBuffer on the JS side): a `Vec<u8>` result
/// would be JSON-serialized as an array of numbers — roughly 3x the byte count —
/// and WebView2's `JSON.parse` of a ~30 MB file's array froze the UI for seconds.
#[tauri::command]
pub async fn read_meeting_audio_file(
    folder_path: String,
) -> Result<tauri::ipc::Response, String> {
    let bytes = tokio::task::spawn_blocking(move || read_meeting_audio_file_sync(folder_path))
        .await
        .map_err(|e| format!("Audio read task panicked: {e}"))??;
    Ok(tauri::ipc::Response::new(bytes))
}

fn read_meeting_audio_file_sync(folder_path: String) -> Result<Vec<u8>, String> {
    let trimmed = folder_path.trim_end_matches(|c| c == '/' || c == '\\');
    let path = find_audio_file(Path::new(trimmed)).map_err(|e| e.to_string())?;
    let len = std::fs::metadata(&path)
        .map_err(|e| e.to_string())?
        .len();
    if len > MAX_BLOB_BYTES {
        return Err("FILE_TOO_LARGE_FOR_BLOB".to_string());
    }
    std::fs::read(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::common::create_transcript_segments;

    #[test]
    fn test_create_transcript_segments_empty() {
        let transcripts: Vec<(String, f64, f64)> = vec![];
        let segments = create_transcript_segments(&transcripts);
        assert!(segments.is_empty());
    }

    #[test]
    fn test_cancellation_flag() {
        RETRANSCRIPTION_CANCELLED.store(false, Ordering::SeqCst);
        RETRANSCRIPTION_IN_PROGRESS.store(false, Ordering::SeqCst);
        assert!(!is_retranscription_in_progress());
        cancel_retranscription();
        assert!(RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst));
        RETRANSCRIPTION_CANCELLED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn test_vad_redemption_time_constant() {
        assert_eq!(VAD_REDEMPTION_TIME_MS, 2000);
    }

    #[test]
    fn find_audio_file_prefers_legacy_decoded_wav_when_both_exist() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("audio.mp3"), b"original").unwrap();
        std::fs::write(dir.path().join("audio_decoded.wav"), b"decoded").unwrap();

        let found = find_audio_file(dir.path()).expect("audio file");
        assert_eq!(found.file_name().unwrap(), "audio_decoded.wav");
    }

    #[test]
    fn find_audio_file_uses_audio_wav_when_that_is_the_only_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("audio.wav"), b"playback").unwrap();

        let found = find_audio_file(dir.path()).expect("audio file");
        assert_eq!(found.file_name().unwrap(), "audio.wav");
    }

    #[test]
    fn read_meeting_audio_file_returns_wav_bytes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("audio.wav"), b"RIFF-playback").unwrap();

        let bytes = read_meeting_audio_file_sync(dir.path().to_string_lossy().to_string())
            .expect("read wav");
        assert_eq!(bytes, b"RIFF-playback");
    }

    #[test]
    fn read_meeting_audio_file_rejects_huge_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio.wav");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(33 * 1024 * 1024).unwrap();

        let err = read_meeting_audio_file_sync(dir.path().to_string_lossy().to_string())
            .expect_err("huge wav");
        assert!(err.contains("FILE_TOO_LARGE_FOR_BLOB"), "{err}");
    }

    #[test]
    fn long_wav_needs_mp4_transcode_small_wav_and_mp4_do_not() {
        assert!(playback_file_needs_mp4_transcode(
            Path::new("audio.wav"),
            MAX_BLOB_BYTES + 1
        ));
        assert!(playback_file_needs_mp4_transcode(
            Path::new("AUDIO.WAV"),
            MAX_BLOB_BYTES + 1
        ));
        assert!(!playback_file_needs_mp4_transcode(
            Path::new("audio.wav"),
            MAX_BLOB_BYTES
        ));
        assert!(!playback_file_needs_mp4_transcode(
            Path::new("audio.mp4"),
            MAX_BLOB_BYTES + 1
        ));
    }

    #[test]
    fn fresh_sidecar_mp4_is_reused_without_reencoding() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("audio.wav");
        std::fs::write(&wav, b"RIFF").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let sidecar = playback_mp4_sidecar(&wav);
        std::fs::write(&sidecar, b"fake-mp4").unwrap();

        assert!(sidecar_is_fresh(&wav, &sidecar));
    }

    #[test]
    fn stale_sidecar_mp4_is_not_reused() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("audio.wav");
        let sidecar = playback_mp4_sidecar(&wav);
        std::fs::write(&sidecar, b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&wav, b"RIFF-new").unwrap();

        assert!(!sidecar_is_fresh(&wav, &sidecar));
    }

    #[test]
    fn resolve_small_wav_does_not_create_playback_mp4() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("audio.wav"), b"RIFF-playback").unwrap();

        match resolve_playback_sync(dir.path().to_string_lossy().to_string())
            .expect("resolve wav")
        {
            ResolvedPlayback::Ready(path) => {
                assert!(path.ends_with("audio.wav"), "{path:?}")
            }
            ResolvedPlayback::NeedsPrepare(_) => panic!("small wav must resolve Ready"),
        }
        assert!(!dir.path().join("audio_playback.mp4").exists());
    }

    #[test]
    fn large_wav_without_sidecar_needs_prepare_without_running_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("audio.wav");
        let file = std::fs::File::create(&wav).unwrap();
        file.set_len(MAX_BLOB_BYTES + 1).unwrap();
        drop(file);

        match resolve_playback_sync(dir.path().to_string_lossy().to_string())
            .expect("resolve large wav")
        {
            ResolvedPlayback::NeedsPrepare(path) => assert_eq!(path, wav),
            ResolvedPlayback::Ready(_) => panic!("large wav without fresh sidecar must not block"),
        }
        // Critically: no synchronous ffmpeg ran, no partial sidecar left behind.
        assert!(!dir.path().join("audio_playback.mp4").exists());
        assert!(!dir.path().join("audio_playback.mp4.tmp").exists());
    }

    #[test]
    fn large_wav_reuses_fresh_sidecar_instead_of_blob_or_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("audio.wav");
        let file = std::fs::File::create(&wav).unwrap();
        file.set_len(MAX_BLOB_BYTES + 1).unwrap();
        drop(file);
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.path().join("audio_playback.mp4"), b"cached-mp4").unwrap();

        match resolve_playback_sync(dir.path().to_string_lossy().to_string())
            .expect("resolve sidecar")
        {
            ResolvedPlayback::Ready(path) => {
                assert!(path.ends_with("audio_playback.mp4"), "{path:?}")
            }
            ResolvedPlayback::NeedsPrepare(_) => panic!("fresh sidecar must resolve Ready"),
        }
    }

    #[test]
    fn wav_duration_reads_byte_rate_and_data_size() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("audio.wav");
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&36u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&16_000u32.to_le_bytes()); // sample rate
        bytes.extend_from_slice(&32_000u32.to_le_bytes()); // byte rate
        bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
        bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&3_200_000u32.to_le_bytes()); // 100 s @ 32 kB/s
        std::fs::write(&wav, &bytes).unwrap();

        assert_eq!(wav_duration_seconds(&wav), Some(100.0));
    }

    #[test]
    fn wav_duration_walks_chunks_and_pads_odd_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("audio.wav");
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"LIST");
        bytes.extend_from_slice(&3u32.to_le_bytes()); // odd size -> needs pad byte
        bytes.extend_from_slice(b"abc");
        bytes.extend_from_slice(&[0u8]); // padding
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8_000u32.to_le_bytes());
        bytes.extend_from_slice(&16_000u32.to_le_bytes()); // 16 kB/s
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&1_600_000u32.to_le_bytes()); // 100 s
        std::fs::write(&wav, &bytes).unwrap();

        assert_eq!(wav_duration_seconds(&wav), Some(100.0));
    }

    #[test]
    fn wav_duration_returns_none_for_non_wav_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("audio.wav");
        std::fs::write(&wav, b"not-a-wav-at-all").unwrap();

        assert_eq!(wav_duration_seconds(&wav), None);
    }

    #[test]
    fn find_audio_file_prefers_wav_over_playback_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("audio.wav"), b"asr-source").unwrap();
        std::fs::write(dir.path().join("audio_playback.mp4"), b"player-only").unwrap();

        let found = find_audio_file(dir.path()).expect("audio file");
        assert_eq!(found.file_name().unwrap(), "audio.wav");
    }
}
