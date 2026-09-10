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

/// Returns absolute path to the first audio file in the meeting folder (same rules as retranscription).
#[tauri::command]
pub fn resolve_meeting_audio_file_path(folder_path: String) -> Result<String, String> {
    let trimmed = folder_path.trim_end_matches(|c| c == '/' || c == '\\');
    let folder = Path::new(trimmed);
    find_audio_file(folder)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

/// Bytes of the meeting playback file. Used for imported 16 kHz WAV: WebView2's
/// `<audio>` element often rejects `convertFileSrc` for PCM WAV even when the
/// file exists (MEDIA_ERR_SRC_NOT_SUPPORTED), which the UI used to show as
/// "no recording saved".
#[tauri::command]
pub fn read_meeting_audio_file(folder_path: String) -> Result<Vec<u8>, String> {
    let trimmed = folder_path.trim_end_matches(|c| c == '/' || c == '\\');
    let path = find_audio_file(Path::new(trimmed)).map_err(|e| e.to_string())?;
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

        let bytes = read_meeting_audio_file(dir.path().to_string_lossy().to_string())
            .expect("read wav");
        assert_eq!(bytes, b"RIFF-playback");
    }
}
