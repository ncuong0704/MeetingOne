// Retranscription module - re-processes stored audio with the ZipFormer Vietnamese ASR engine.

use crate::audio::audio_processing::{HighPassFilter, LoudnessNormalizer};
use crate::audio::decoder::decode_audio_file;
use crate::audio::vad::get_speech_chunks_with_progress;
use super::common::{create_transcript_segments, expand_segments_at_silence, write_transcripts_json};
use super::constants::AUDIO_EXTENSIONS;
use crate::state::AppState;
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
        "audio.mp4", "audio.m4a", "audio.wav", "audio.mp3",
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
    let decoded = tokio::task::spawn_blocking(move || decode_audio_file(&path_for_decode))
        .await
        .map_err(|e| anyhow!("Decode task panicked: {}", e))??;
    let duration_seconds = decoded.duration_seconds;

    info!("Decoded audio: {:.2}s, {}Hz, {} channels", duration_seconds, decoded.sample_rate, decoded.channels);

    emit_progress(&app, &meeting_id, "decoding", 15, "Converting audio format...");

    if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Retranscription cancelled"));
    }

    let audio_samples = tokio::task::spawn_blocking(move || decoded.to_whisper_format())
        .await
        .map_err(|e| anyhow!("Resample task panicked: {}", e))?;
    info!("Converted to 16kHz mono: {} samples", audio_samples.len());

    // Noise reduction pipeline at 16kHz (RNNoise requires 48kHz so not applicable here)
    let audio_samples = {
        let mut hpf = HighPassFilter::new(16000, 80.0);
        let filtered = hpf.process(&audio_samples);
        match LoudnessNormalizer::new(1, 16000) {
            Ok(mut normalizer) => normalizer.normalize_loudness(&filtered),
            Err(e) => {
                warn!("Failed to create loudness normalizer for retranscription: {}, skipping normalization", e);
                filtered
            }
        }
    };
    info!("Noise reduction applied: high-pass filter (80Hz) + EBU R128 normalization");

    emit_progress(&app, &meeting_id, "vad", 20, "Detecting speech segments...");

    if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Retranscription cancelled"));
    }

    let app_for_vad = app.clone();
    let meeting_id_for_vad = meeting_id.clone();

    let speech_segments = tokio::task::spawn_blocking(move || {
        get_speech_chunks_with_progress(
            &audio_samples,
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

    emit_progress(&app, &meeting_id, "transcribing", 25, "Loading Vietnamese ASR...");

    // Ensure ASR engine is ready (ROVER or single-model, per file path config)
    let file_cfg = {
        let app_state = app
            .try_state::<AppState>()
            .ok_or_else(|| anyhow!("App state not available"))?;
        crate::database::repositories::setting::SettingsRepository::get_path_asr_config(
            app_state.db_manager.pool(),
            crate::asr_engine::config::AsrPath::File,
        )
        .await
    };

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

    let max_segment_seconds = file_cfg.max_segment_seconds;

    let processable_segments = expand_segments_at_silence(speech_segments, max_segment_seconds);

    let processable_count = processable_segments.len();
    info!("Processing {} segments", processable_count);

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
    let segments = crate::audio::batch_transcribe::batch_transcribe(
        &app,
        processable_segments,
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
    .await?;

    info!("Transcription complete: {} segments", segments.len());

    if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Retranscription cancelled"));
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

    for segment in &segments {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&segment.id)
        .bind(&meeting_id)
        .bind(&segment.text)
        .bind(&segment.timestamp)
        .bind(segment.audio_start_time)
        .bind(segment.audio_end_time)
        .bind(segment.duration)
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

/// Returns absolute path to the first audio file in the meeting folder (same rules as retranscription).
#[tauri::command]
pub fn resolve_meeting_audio_file_path(folder_path: String) -> Result<String, String> {
    let trimmed = folder_path.trim_end_matches(|c| c == '/' || c == '\\');
    let folder = Path::new(trimmed);
    find_audio_file(folder)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
