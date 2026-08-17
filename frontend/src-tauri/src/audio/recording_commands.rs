// audio/recording_commands.rs
//
// Slim Tauri command layer for recording functionality.
// Delegates to transcription and recording modules for actual implementation.

use anyhow::Result;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::task::JoinHandle;

use super::{
    parse_audio_device,
    default_input_device,   // Get default microphone
    default_output_device,  // Get default system audio
    RecordingManager,
    DeviceEvent,
    DeviceMonitorType
};

// Import transcription modules
use super::transcription::{
    self,
    reset_speech_detected_flag,
};
use super::recording_preferences::AudioCaptureSource;
use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;

// Re-export TranscriptUpdate for backward compatibility
pub use super::transcription::TranscriptUpdate;

async fn live_streaming_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    let cfg = SettingsRepository::get_path_asr_config(
        app.state::<AppState>().db_manager.pool(),
        crate::asr_engine::config::AsrPath::Live,
    )
    .await;
    crate::asr_engine::model_family::ModelFamily::from_id(&cfg.family_id).is_online_streaming()
}

fn spawn_live_asr_task<R: Runtime>(
    app: AppHandle<R>,
    receiver: tokio::sync::mpsc::UnboundedReceiver<crate::audio::AudioChunk>,
    streaming: bool,
) -> JoinHandle<()> {
    if streaming {
        transcription::start_streaming_task(app, receiver)
    } else {
        transcription::start_transcription_task(app, receiver)
    }
}

// ============================================================================
// GLOBAL STATE
// ============================================================================

// Simple recording state tracking
static IS_RECORDING: AtomicBool = AtomicBool::new(false);

// Set while attempt_device_reconnect legitimately owns (has taken) the
// RecordingManager out of RECORDING_MANAGER, so other commands (notably
// stop_recording) can tell "manager temporarily absent for reconnect" apart
// from "not recording," and avoid silently no-op'ing or letting a new
// recording start into the same slot while a reconnect is still in flight.
static RECONNECT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

// Set for the duration of a start_recording_*_inner call (from just after
// the IS_RECORDING claim until the manager is either fully installed or the
// attempt fails), so other commands (notably stop_recording) can tell "a
// start is still initializing its manager" apart from "genuinely not
// recording," the same way RECONNECT_IN_PROGRESS distinguishes an in-flight
// reconnect. Without this, a stop_recording call landing in that window
// would silently no-op while the manager finishes installing moments later,
// orphaning a live recording with no way to stop it.
static START_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

// Global recording manager and transcription task to keep them alive during recording
static RECORDING_MANAGER: Mutex<Option<RecordingManager>> = Mutex::new(None);
static TRANSCRIPTION_TASK: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

// Listener ID for proper cleanup - prevents microphone from staying active after recording stops
static TRANSCRIPT_LISTENER_ID: Mutex<Option<tauri::EventId>> = Mutex::new(None);
static TRANSCRIPT_FINALIZED_LISTENER_ID: Mutex<Option<tauri::EventId>> = Mutex::new(None);

// ============================================================================
// PUBLIC TYPES
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RecordingArgs {
    pub save_path: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct TranscriptionStatus {
    pub chunks_in_queue: usize,
    pub is_processing: bool,
    pub last_activity_ms: u64,
}

// ============================================================================
// RECORDING COMMANDS
// ============================================================================

fn resolve_microphone(preferred: Option<String>) -> Option<Arc<super::AudioDevice>> {
    if let Some(pref_name) = preferred {
        match parse_audio_device(&pref_name) {
            Ok(device) => {
                info!("✅ Using microphone: '{}'", device.name);
                return Some(Arc::new(device));
            }
            Err(e) => {
                warn!(
                    "⚠️ Preferred microphone '{}' not available: {}",
                    pref_name, e
                );
            }
        }
    }
    match default_input_device() {
        Ok(device) => {
            info!("✅ Using default microphone: '{}'", device.name);
            Some(Arc::new(device))
        }
        Err(e) => {
            warn!("⚠️ No microphone available: {}", e);
            None
        }
    }
}

fn resolve_system_audio(preferred: Option<String>) -> Option<Arc<super::AudioDevice>> {
    if let Some(pref_name) = preferred {
        match parse_audio_device(&pref_name) {
            Ok(device) => {
                info!("✅ Using system audio: '{}'", device.name);
                return Some(Arc::new(device));
            }
            Err(e) => {
                warn!(
                    "⚠️ Preferred system audio '{}' not available: {}",
                    pref_name, e
                );
            }
        }
    }
    match default_output_device() {
        Ok(device) => {
            info!("✅ Using default system audio: '{}'", device.name);
            Some(Arc::new(device))
        }
        Err(e) => {
            warn!("⚠️ No system audio available: {}", e);
            None
        }
    }
}

pub(crate) fn resolve_capture_devices(
    source: AudioCaptureSource,
    mic_name: Option<String>,
    system_name: Option<String>,
) -> Result<(Option<Arc<super::AudioDevice>>, Option<Arc<super::AudioDevice>>), String> {
    let mic = if source.wants_microphone() {
        resolve_microphone(mic_name)
    } else {
        info!("🎤 Audio source {:?} — skipping microphone", source);
        None
    };
    let system = if source.wants_system() {
        resolve_system_audio(system_name)
    } else {
        info!("🔊 Audio source {:?} — skipping system audio", source);
        None
    };

    accept_capture_devices(source, mic, system)
}

/// Accept already-resolved optional devices, or return the user-facing error.
pub(crate) fn accept_capture_devices(
    source: AudioCaptureSource,
    mic: Option<Arc<super::AudioDevice>>,
    system: Option<Arc<super::AudioDevice>>,
) -> Result<(Option<Arc<super::AudioDevice>>, Option<Arc<super::AudioDevice>>), String> {
    match source {
        AudioCaptureSource::Microphone if mic.is_none() => Err(
            "Không mở được microphone. Kiểm tra thiết bị và quyền truy cập.".into(),
        ),
        AudioCaptureSource::System if system.is_none() => {
            Err("Không mở được âm thanh hệ thống. Chọn thiết bị khác trong Cài đặt.".into())
        }
        _ if mic.is_none() && system.is_none() => {
            Err("Không có nguồn âm thanh nào khả dụng.".into())
        }
        _ => Ok((mic, system)),
    }
}

/// Start recording with default devices
pub async fn start_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let source = match super::recording_preferences::load_recording_preferences(&app).await {
        Ok(prefs) => prefs.audio_source,
        Err(_) => AudioCaptureSource::Both,
    };
    start_recording_with_meeting_name(app, None, source).await
}

/// Start recording with default devices and optional meeting name
pub async fn start_recording_with_meeting_name<R: Runtime>(
    app: AppHandle<R>,
    meeting_name: Option<String>,
    audio_source: AudioCaptureSource,
) -> Result<(), String> {
    // Atomically claim the "starting" state: only one caller can transition
    // IS_RECORDING from false to true here. Any concurrent caller sees the
    // swap fail (current value was already true) and is rejected immediately,
    // closing the race window where two near-simultaneous start calls could
    // both pass a plain load-then-later-store check and both open the same
    // audio devices.
    if IS_RECORDING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        info!("🔍 IS_RECORDING already true — rejecting concurrent start");
        return Err("Recording already in progress".to_string());
    }
    START_IN_PROGRESS.store(true, Ordering::SeqCst);

    let result = start_recording_with_meeting_name_inner(app, meeting_name, audio_source).await;

    START_IN_PROGRESS.store(false, Ordering::SeqCst);

    if result.is_err() {
        // Roll back the claim so a failed start doesn't permanently lock out
        // future attempts.
        IS_RECORDING.store(false, Ordering::SeqCst);
    }

    result
}

async fn start_recording_with_meeting_name_inner<R: Runtime>(
    app: AppHandle<R>,
    meeting_name: Option<String>,
    audio_source: AudioCaptureSource,
) -> Result<(), String> {
    info!(
        "Starting recording with default devices, meeting: {:?}, audio_source: {:?}",
        meeting_name, audio_source
    );

    // Validate that transcription models are available before starting recording
    info!("🔍 Validating transcription model availability before starting recording...");
    if let Err(validation_error) = transcription::validate_transcription_model_ready(&app).await {
        error!("Model validation failed: {}", validation_error);

        // Emit error event for frontend - actionable: false to show toast instead of modal
        // (download progress is already shown in top-right toast)
        let _ = app.emit("transcription-error", serde_json::json!({
            "error": validation_error,
            "userMessage": "Recording cannot start: Transcription model is still downloading. Please wait for the download to complete.",
            "actionable": false
        }));

        return Err(validation_error);
    }
    info!("✅ Transcription model validation passed");

    // Async-first approach - no more blocking operations!
    info!("🚀 Starting async recording initialization");

    // Create new recording manager
    let mut manager = RecordingManager::new();

    // Load recording preferences to get auto_save AND device preferences
    let (auto_save, preferred_mic_name, preferred_system_name, save_folder) =
        match super::recording_preferences::load_recording_preferences(&app).await {
            Ok(prefs) => {
                info!("📋 Loaded recording preferences: auto_save={}, save_folder={:?}, preferred_mic={:?}, preferred_system={:?}, audio_source={:?}",
                      prefs.auto_save, prefs.save_folder, prefs.preferred_mic_device, prefs.preferred_system_device, prefs.audio_source);
                (
                    prefs.auto_save,
                    prefs.preferred_mic_device,
                    prefs.preferred_system_device,
                    prefs.save_folder,
                )
            }
            Err(e) => {
                warn!("Failed to load recording preferences, using defaults: {}", e);
                (
                    true,
                    None,
                    None,
                    super::recording_preferences::get_default_recordings_folder(),
                )
            }
        };
    manager.set_save_folder(save_folder);

    let (microphone_device, system_device) =
        resolve_capture_devices(audio_source, preferred_mic_name, preferred_system_name)?;
    let effective_meeting_name = meeting_name.clone().unwrap_or_else(|| {
        // Example: Meeting 2025-10-03_08-25-23
        let now = chrono::Local::now();
        format!(
            "Meeting {}",
            now.format("%Y-%m-%d_%H-%M-%S")
        )
    });
    manager.set_meeting_name(Some(effective_meeting_name));

    // Set up error callback
    let app_for_error = app.clone();
    manager.set_error_callback(move |error| {
        let _ = app_for_error.emit("recording-error", error.user_message());
    });

    let max_segment_seconds = SettingsRepository::get_max_segment_seconds(
        app.state::<AppState>().db_manager.pool(),
    )
    .await;
    info!("Using max segment length: {}s for live transcription", max_segment_seconds);

    let streaming_asr = live_streaming_enabled(&app).await;
    if streaming_asr {
        info!("Live ASR path: OnlineRecognizer streaming (no VAD)");
    }

    // Start recording with resolved devices (replaces start_recording_with_defaults_and_auto_save call)
    let transcription_receiver = manager
        .start_recording(microphone_device, system_device, auto_save, max_segment_seconds, streaming_asr)
        .await
        .map_err(|e| format!("Failed to start recording: {}", e))?;

    // Store the manager globally to keep it alive
    {
        let mut global_manager = RECORDING_MANAGER.lock();
        *global_manager = Some(manager);
    }

    // Reset speech detection flag for new recording session
    // (IS_RECORDING was already set to true by the caller's atomic claim)
    info!("🔍 Resetting SPEECH_DETECTED_EMITTED for new recording session");
    reset_speech_detected_flag();
    crate::audio::transcription::live_speaker::reset_session();

    // Best-effort CAPU init before live transcription
    if crate::capu_engine::commands::capu_is_model_downloaded(app.clone())
        .await
        .unwrap_or(false)
    {
        let _ = crate::capu_engine::commands::capu_init(app.clone()).await;
    }

    // Start optimized parallel transcription task and store handle
    let task_handle = spawn_live_asr_task(app.clone(), transcription_receiver, streaming_asr);
    {
        let mut global_task = TRANSCRIPTION_TASK.lock();
        *global_task = Some(task_handle);
    }

    // CRITICAL: Listen for transcript-update events and save to recording manager
    // This enables transcript history persistence for page reload sync
    // Store listener ID for cleanup during stop_recording to ensure microphone is released
    {
        use tauri::Listener;
        let listener_id = app.listen("transcript-update", move |event: tauri::Event| {
            // Parse the transcript update from the event payload
            if let Ok(update) = serde_json::from_str::<TranscriptUpdate>(event.payload()) {
                // Create structured transcript segment
                let segment = crate::audio::recording_saver::TranscriptSegment {
                    id: format!("seg_{}", update.sequence_id),
                    text: update.text.clone(),
                    audio_start_time: update.audio_start_time,
                    audio_end_time: update.audio_end_time,
                    duration: update.duration,
                    display_time: update.timestamp.clone(), // Use wall-clock timestamp for display
                    confidence: update.confidence,
                    sequence_id: update.sequence_id,
                    user_edited: false,
                    speaker_name: update.speaker_name.clone(),
                };

                // Save to recording manager
                let manager_guard = RECORDING_MANAGER.lock();
                if let Some(manager) = manager_guard.as_ref() {
                    manager.add_transcript_segment(segment);
                }
            }
        });
        let mut global_listener = TRANSCRIPT_LISTENER_ID.lock();
        *global_listener = Some(listener_id);
        info!("✅ Transcript-update event listener registered for history persistence");
    }

    // Listen for transcript-finalized events (CAPU background stage) and merge the
    // finalized segment into the recording manager, replacing the raw segments it covers.
    {
        use tauri::Listener;
        let listener_id = app.listen("transcript-finalized", move |event: tauri::Event| {
            if let Ok(update) =
                serde_json::from_str::<crate::audio::transcription::TranscriptFinalized>(event.payload())
            {
                let manager_guard = RECORDING_MANAGER.lock();
                if let Some(manager) = manager_guard.as_ref() {
                    manager.replace_transcript_segments(
                        &update.source_sequence_ids,
                        update.text,
                        update.audio_start_time,
                        update.audio_end_time,
                    );
                }
            }
        });
        let mut global_listener = TRANSCRIPT_FINALIZED_LISTENER_ID.lock();
        *global_listener = Some(listener_id);
        info!("✅ Transcript-finalized event listener registered for CAPU background stage");
    }

    // Emit success event. Non-fatal: by this point the manager is already
    // live and installed (RECORDING_MANAGER, TRANSCRIPTION_TASK, and the
    // transcript listener are all set up above) — a failure to notify the
    // frontend must not be treated as a failed start, since the wrapper
    // would otherwise roll IS_RECORDING back to false while a real recording
    // keeps running, orphaning it.
    if let Err(e) = app.emit("recording-started", serde_json::json!({
        "message": "Recording started successfully with parallel processing",
        "devices": ["Default Microphone", "Default System Audio"],
        "workers": 3
    })) {
        warn!("Failed to emit recording-started event (recording itself started fine): {}", e);
    }

    // Update tray menu to reflect recording state
    crate::tray::update_tray_menu(&app);

    info!("✅ Recording started successfully with async-first approach");

    Ok(())
}

/// Start recording with specific devices
pub async fn start_recording_with_devices<R: Runtime>(
    app: AppHandle<R>,
    mic_device_name: Option<String>,
    system_device_name: Option<String>,
) -> Result<(), String> {
    start_recording_with_devices_and_meeting(
        app,
        mic_device_name,
        system_device_name,
        None,
        AudioCaptureSource::Both,
    )
    .await
}

/// Start recording with specific devices and optional meeting name
pub async fn start_recording_with_devices_and_meeting<R: Runtime>(
    app: AppHandle<R>,
    mic_device_name: Option<String>,
    system_device_name: Option<String>,
    meeting_name: Option<String>,
    audio_source: AudioCaptureSource,
) -> Result<(), String> {
    // Atomically claim the "starting" state: only one caller can transition
    // IS_RECORDING from false to true here. Any concurrent caller sees the
    // swap fail (current value was already true) and is rejected immediately,
    // closing the race window where two near-simultaneous start calls could
    // both pass a plain load-then-later-store check and both open the same
    // audio devices.
    if IS_RECORDING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        info!("🔍 IS_RECORDING already true — rejecting concurrent start");
        return Err("Recording already in progress".to_string());
    }
    START_IN_PROGRESS.store(true, Ordering::SeqCst);

    let result = start_recording_with_devices_and_meeting_inner(
        app,
        mic_device_name,
        system_device_name,
        meeting_name,
        audio_source,
    )
    .await;

    START_IN_PROGRESS.store(false, Ordering::SeqCst);

    if result.is_err() {
        // Roll back the claim so a failed start doesn't permanently lock out
        // future attempts.
        IS_RECORDING.store(false, Ordering::SeqCst);
    }

    result
}

async fn start_recording_with_devices_and_meeting_inner<R: Runtime>(
    app: AppHandle<R>,
    mic_device_name: Option<String>,
    system_device_name: Option<String>,
    meeting_name: Option<String>,
    audio_source: AudioCaptureSource,
) -> Result<(), String> {
    info!(
        "Starting recording with specific devices: mic={:?}, system={:?}, meeting={:?}, audio_source={:?}",
        mic_device_name, system_device_name, meeting_name, audio_source
    );

    // Validate that transcription models are available before starting recording
    info!("🔍 Validating transcription model availability before starting recording...");
    if let Err(validation_error) = transcription::validate_transcription_model_ready(&app).await {
        error!("Model validation failed: {}", validation_error);

        // Emit error event for frontend - actionable: false to show toast instead of modal
        // (download progress is already shown in top-right toast)
        let _ = app.emit("transcription-error", serde_json::json!({
            "error": validation_error,
            "userMessage": "Recording cannot start: Transcription model is still downloading. Please wait for the download to complete.",
            "actionable": false
        }));

        return Err(validation_error);
    }
    info!("✅ Transcription model validation passed");

    // Async-first approach for custom devices - no more blocking operations!
    info!("🚀 Starting async recording initialization with custom devices");

    // Create new recording manager
    let mut manager = RecordingManager::new();

    // Load recording preferences to check auto_save setting and fill missing device names
    let (auto_save, save_folder, pref_mic, pref_sys) =
        match super::recording_preferences::load_recording_preferences(&app).await {
            Ok(prefs) => {
                info!(
                    "📋 Loaded recording preferences: auto_save={}, save_folder={:?}, audio_source={:?}",
                    prefs.auto_save, prefs.save_folder, prefs.audio_source
                );
                (
                    prefs.auto_save,
                    prefs.save_folder,
                    prefs.preferred_mic_device,
                    prefs.preferred_system_device,
                )
            }
            Err(e) => {
                warn!(
                    "Failed to load recording preferences, defaulting to auto_save=true: {}",
                    e
                );
                (
                    true,
                    super::recording_preferences::get_default_recordings_folder(),
                    None,
                    None,
                )
            }
        };
    manager.set_save_folder(save_folder);

    let mic_name = mic_device_name.or(pref_mic);
    let system_name = system_device_name.or(pref_sys);
    let (mic_device, system_device) =
        resolve_capture_devices(audio_source, mic_name, system_name)?;

    // Always ensure a meeting name is set so incremental saver initializes
    let effective_meeting_name = meeting_name.clone().unwrap_or_else(|| {
        let now = chrono::Local::now();
        format!(
            "Meeting {}",
            now.format("%Y-%m-%d_%H-%M-%S")
        )
    });
    manager.set_meeting_name(Some(effective_meeting_name));

    // Set up error callback
    let app_for_error = app.clone();
    manager.set_error_callback(move |error| {
        let _ = app_for_error.emit("recording-error", error.user_message());
    });

    let max_segment_seconds = SettingsRepository::get_max_segment_seconds(
        app.state::<AppState>().db_manager.pool(),
    )
    .await;
    info!("Using max segment length: {}s for live transcription", max_segment_seconds);

    let streaming_asr = live_streaming_enabled(&app).await;
    if streaming_asr {
        info!("Live ASR path: OnlineRecognizer streaming (no VAD)");
    }

    let mic_label = mic_device
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "No Microphone".to_string());
    let system_label = system_device
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "No System Audio".to_string());

    // Start recording with specified devices and auto_save setting
    let transcription_receiver = manager
        .start_recording(mic_device, system_device, auto_save, max_segment_seconds, streaming_asr)
        .await
        .map_err(|e| format!("Failed to start recording: {}", e))?;

    // Store the manager globally to keep it alive
    {
        let mut global_manager = RECORDING_MANAGER.lock();
        *global_manager = Some(manager);
    }

    // Reset speech detection flag for new recording session
    // (IS_RECORDING was already set to true by the caller's atomic claim)
    info!("🔍 Resetting SPEECH_DETECTED_EMITTED for new recording session");
    reset_speech_detected_flag();
    crate::audio::transcription::live_speaker::reset_session();

    // Best-effort CAPU init before live transcription
    if crate::capu_engine::commands::capu_is_model_downloaded(app.clone())
        .await
        .unwrap_or(false)
    {
        let _ = crate::capu_engine::commands::capu_init(app.clone()).await;
    }

    // Start optimized parallel transcription task and store handle
    let task_handle = spawn_live_asr_task(app.clone(), transcription_receiver, streaming_asr);
    {
        let mut global_task = TRANSCRIPTION_TASK.lock();
        *global_task = Some(task_handle);
    }

    // CRITICAL: Listen for transcript-update events and save to recording manager
    // This enables transcript history persistence for page reload sync
    // Store listener ID for cleanup during stop_recording to ensure microphone is released
    {
        use tauri::Listener;
        let listener_id = app.listen("transcript-update", move |event: tauri::Event| {
            // Parse the transcript update from the event payload
            if let Ok(update) = serde_json::from_str::<TranscriptUpdate>(event.payload()) {
                // Create structured transcript segment
                let segment = crate::audio::recording_saver::TranscriptSegment {
                    id: format!("seg_{}", update.sequence_id),
                    text: update.text.clone(),
                    audio_start_time: update.audio_start_time,
                    audio_end_time: update.audio_end_time,
                    duration: update.duration,
                    display_time: update.timestamp.clone(), // Use wall-clock timestamp for display
                    confidence: update.confidence,
                    sequence_id: update.sequence_id,
                    user_edited: false,
                    speaker_name: update.speaker_name.clone(),
                };

                // Save to recording manager
                let manager_guard = RECORDING_MANAGER.lock();
                if let Some(manager) = manager_guard.as_ref() {
                    manager.add_transcript_segment(segment);
                }
            }
        });
        let mut global_listener = TRANSCRIPT_LISTENER_ID.lock();
        *global_listener = Some(listener_id);
        info!("✅ Transcript-update event listener registered for history persistence");
    }

    // Listen for transcript-finalized events (CAPU background stage) and merge the
    // finalized segment into the recording manager, replacing the raw segments it covers.
    {
        use tauri::Listener;
        let listener_id = app.listen("transcript-finalized", move |event: tauri::Event| {
            if let Ok(update) =
                serde_json::from_str::<crate::audio::transcription::TranscriptFinalized>(event.payload())
            {
                let manager_guard = RECORDING_MANAGER.lock();
                if let Some(manager) = manager_guard.as_ref() {
                    manager.replace_transcript_segments(
                        &update.source_sequence_ids,
                        update.text,
                        update.audio_start_time,
                        update.audio_end_time,
                    );
                }
            }
        });
        let mut global_listener = TRANSCRIPT_FINALIZED_LISTENER_ID.lock();
        *global_listener = Some(listener_id);
        info!("✅ Transcript-finalized event listener registered for CAPU background stage");
    }

    // Emit success event. Non-fatal: see the identical comment in
    // start_recording_with_meeting_name_inner above.
    if let Err(e) = app.emit("recording-started", serde_json::json!({
        "message": "Recording started with custom devices and parallel processing",
        "devices": [mic_label, system_label],
        "workers": 3
    })) {
        warn!("Failed to emit recording-started event (recording itself started fine): {}", e);
    }

    // Update tray menu to reflect recording state
    crate::tray::update_tray_menu(&app);

    info!("✅ Recording started with custom devices using async-first approach");

    Ok(())
}

/// Stop recording with optimized graceful shutdown ensuring NO transcript chunks are lost
pub async fn stop_recording<R: Runtime>(
    app: AppHandle<R>,
    _args: RecordingArgs,
) -> Result<(), String> {
    info!(
        "🛑 Starting optimized recording shutdown - ensuring ALL transcript chunks are preserved"
    );

    // Check if recording is active
    if !IS_RECORDING.load(Ordering::SeqCst) {
        info!("Recording was not active");
        return Ok(());
    }

    // A device reconnect currently owns the manager (RECORDING_MANAGER is
    // temporarily None while it does its I/O), OR a start is still
    // installing its manager. Proceeding here would hit the "no manager
    // found" no-op path below and silently discard the active session
    // without saving it — instead, ask the caller to retry shortly rather
    // than pretending the stop succeeded.
    if RECONNECT_IN_PROGRESS.load(Ordering::SeqCst) || START_IN_PROGRESS.load(Ordering::SeqCst) {
        return Err(
            "Recording is still starting up or reconnecting, please try stopping again in a moment".to_string(),
        );
    }

    // Emit shutdown progress to frontend
    let _ = app.emit(
        "recording-shutdown-progress",
        serde_json::json!({
            "stage": "stopping_audio",
            "message": "Stopping audio capture...",
            "progress": 20
        }),
    );

    // Step 1: Stop audio capture immediately (no more new chunks) with proper error handling
    let manager_for_cleanup = {
        let mut global_manager = RECORDING_MANAGER.lock();
        global_manager.take()
    };

    let stop_result = if let Some(mut manager) = manager_for_cleanup {
        // Use FORCE FLUSH to immediately process all accumulated audio - eliminates 30s delay!
        info!("🚀 Using FORCE FLUSH to eliminate pipeline accumulation delays");
        let result = manager.stop_streams_and_force_flush().await;
        // Store manager back for later cleanup
        let manager_for_cleanup = Some(manager);
        (result, manager_for_cleanup)
    } else {
        // Narrow residual race: a reconnect or an in-flight start may have
        // taken/not-yet-installed the manager in the brief window between
        // our checks above and this lock acquisition. Re-check before
        // treating this as "nothing to stop" — otherwise we'd silently flip
        // IS_RECORDING to false without ever saving the in-progress session.
        if RECONNECT_IN_PROGRESS.load(Ordering::SeqCst) || START_IN_PROGRESS.load(Ordering::SeqCst) {
            return Err(
                "Recording is still starting up or reconnecting, please try stopping again in a moment".to_string(),
            );
        }
        warn!("No recording manager found to stop");
        (Ok(()), None)
    };

    let (stop_result, manager_for_cleanup) = stop_result;

    match stop_result {
        Ok(_) => {
            info!("✅ Audio streams stopped successfully - no more chunks will be created");
        }
        Err(e) => {
            error!("❌ Failed to stop audio streams: {}", e);
            return Err(format!("Failed to stop audio streams: {}", e));
        }
    }

    // Step 1.5: Clean up transcript listener to release microphone
    // Unlisten transcript-update event to prevent lingering references
    {
        use tauri::Listener;
        if let Some(listener_id) = TRANSCRIPT_LISTENER_ID.lock().take() {
            app.unlisten(listener_id);
            info!("✅ Transcript-update listener removed");
        }
    }

    // NOTE: transcript-finalized listener stays active until after live CAPU finalize below.

    // Step 2: Signal transcription workers to finish processing ALL queued chunks
    let _ = app.emit(
        "recording-shutdown-progress",
        serde_json::json!({
            "stage": "processing_transcripts",
            "message": "Processing remaining transcript chunks...",
            "progress": 40
        }),
    );

    // Wait for transcription task with enhanced progress monitoring (NO TIMEOUT - we must process all chunks)
    let transcription_task = {
        let mut global_task = TRANSCRIPTION_TASK.lock();
        global_task.take()
    };

    if let Some(task_handle) = transcription_task {
        info!("⏳ Waiting for ALL transcription chunks to be processed (no timeout - preserving every chunk)");

        // Enhanced progress monitoring during shutdown
        let progress_app = app.clone();
        let progress_task = tokio::spawn(async move {
            let last_update = std::time::Instant::now();

            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                // Emit periodic progress updates during shutdown
                let elapsed = last_update.elapsed().as_secs();
                let _ = progress_app.emit(
                    "recording-shutdown-progress",
                    serde_json::json!({
                        "stage": "processing_transcripts",
                        "message": format!("Processing transcripts... ({}s elapsed)", elapsed),
                        "progress": 40,
                        "detailed": true,
                        "elapsed_seconds": elapsed
                    }),
                );
            }
        });

        // Wait up to 10 minutes for transcription completion to prevent indefinite hangs
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(600), // 10 minutes max
            task_handle
        ).await {
            Ok(Ok(())) => {
                info!("✅ ALL transcription chunks processed successfully - no data lost");
            }
            Ok(Err(e)) => {
                warn!("⚠️ Transcription task completed with error: {:?}", e);
                // Continue anyway - the worker may have processed most chunks
            }
            Err(_) => {
                warn!("⏱️ Transcription timeout (10 minutes) reached, continuing shutdown to prevent indefinite hang");
                // Continue shutdown even on timeout - better to lose some chunks than hang forever
            }
        }

        // Stop progress monitoring
        progress_task.abort();
    } else {
        info!("ℹ️ No transcription task found to wait for");
    }

    // Step 2.5: Apply CAPU once over the full live transcript (after ASR drain).
    let _ = app.emit(
        "recording-shutdown-progress",
        serde_json::json!({
            "stage": "applying_punctuation",
            "message": "Đang thêm dấu câu...",
            "progress": 55
        }),
    );

    // Ensure CAPU engine is loaded before finalize (startup init may still be in progress).
    if crate::capu_engine::commands::capu_is_model_downloaded(app.clone())
        .await
        .unwrap_or(false)
    {
        if let Err(e) = crate::capu_engine::commands::capu_init(app.clone()).await {
            warn!("CAPU init before live finalize failed: {}", e);
        }
    }

    // Drop the 1:1 replace listener before expanding batches into sentences,
    // otherwise a leftover transcript-finalized emit would collapse N sentences
    // back into one span.
    {
        use tauri::Listener;
        if let Some(listener_id) = TRANSCRIPT_FINALIZED_LISTENER_ID.lock().take() {
            app.unlisten(listener_id);
            info!("✅ Transcript-finalized listener removed before live CAPU apply");
        }
    }

    if let Some(ref manager) = manager_for_cleanup {
        let raw_segments = manager.get_transcript_segments();
        let finalized_batches =
            crate::capu_engine::live_finalize::finalize_live_with_capu(&raw_segments);
        let sentence_count = finalized_batches.len();
        manager.apply_live_capu_results(&finalized_batches);
        for finalized in finalized_batches {
            let payload = crate::audio::transcription::TranscriptFinalized {
                source_sequence_ids: finalized.source_ids,
                text: finalized.text,
                audio_start_time: finalized.audio_start_time,
                audio_end_time: finalized.audio_end_time,
            };
            if let Err(e) = app.emit("transcript-finalized", &payload) {
                warn!("Failed to emit transcript-finalized after live CAPU: {}", e);
            }
        }
        info!(
            "✅ Live CAPU finalize applied ({} sentence(s))",
            sentence_count
        );
    }

    // Step 3: Now safely unload Whisper model after ALL chunks are processed
    let _ = app.emit(
        "recording-shutdown-progress",
        serde_json::json!({
            "stage": "unloading_model",
            "message": "Unloading speech recognition model...",
            "progress": 70
        }),
    );

    info!("🧠 All transcript chunks processed. Now safely unloading transcription model...");

    // Determine which provider was used and unload the appropriate model (with timeout)
    let _config = match tokio::time::timeout(
        tokio::time::Duration::from_secs(30), // 30 seconds max for DB operation
        crate::api::api::api_get_transcript_config(
            app.clone(),
            app.clone().state(),
            None,
        )
    )
    .await
    {
        Ok(Ok(_config)) => Some("asr".to_string()),
        Ok(Err(e)) => {
            warn!("⚠️ Failed to get transcript config: {:?}", e);
            None
        }
        Err(_) => {
            warn!("⏱️ Transcript config timeout (30s), continuing shutdown");
            None
        }
    };

    // ZipFormer stays loaded between sessions — no unload needed.
    info!("🎤 ZipFormer engine stays loaded (no unload required)");

    // Step 3.5: Track meeting ended analytics with privacy-safe metadata
    // Extract all data from manager BEFORE any async operations to avoid Send issues
    let analytics_data = if let Some(ref manager) = manager_for_cleanup {
        let state = manager.get_state();
        let stats = state.get_stats();

        Some((
            manager.get_recording_duration(),
            manager.get_active_recording_duration().unwrap_or(0.0),
            manager.get_total_pause_duration(),
            manager.get_transcript_segments().len() as u64,
            state.has_fatal_error(),
            state.get_microphone_device().map(|d| d.name.clone()),
            state.get_system_device().map(|d| d.name.clone()),
            stats.chunks_processed,
        ))
    } else {
        None
    };

    // Now perform async analytics tracking without holding manager reference
    if let Some((total_duration, active_duration, pause_duration, transcript_segments_count, had_fatal_error, mic_device_name, sys_device_name, chunks_processed)) = analytics_data {
        info!("📊 Collecting analytics for meeting end");

        // Helper function to classify device type from device name (privacy-safe)
        fn classify_device_type(device_name: &str) -> &'static str {
            let name_lower = device_name.to_lowercase();
            // Check for Bluetooth keywords
            if name_lower.contains("bluetooth")
                || name_lower.contains("airpods")
                || name_lower.contains("beats")
                || name_lower.contains("headphones")
                || name_lower.contains("bt ")
                || name_lower.contains("wireless") {
                "Bluetooth"
            } else {
                "Wired"
            }
        }

        // Get transcription model info (already loaded above for model unload)
        let transcription_config = match crate::api::api::api_get_transcript_config(
            app.clone(),
            app.clone().state(),
            None,
        )
        .await
        {
            Ok(config) => Some(("asr".to_string(), config.live.model.clone())),
            _ => None,
        };

        let (transcription_provider, transcription_model) = transcription_config
            .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));

        // Get summary model info from API
        let summary_config = match crate::api::api::api_get_model_config(
            app.clone(),
            app.clone().state(),
            None,
        )
        .await
        {
            Ok(Some(config)) => Some((config.provider, config.model)),
            _ => None,
        };

        let (summary_provider, summary_model) = summary_config
            .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));

        // Classify device types (privacy-safe)
        let microphone_device_type = mic_device_name
            .as_ref()
            .map(|name| classify_device_type(name))
            .unwrap_or("Unknown");

        let system_audio_device_type = sys_device_name
            .as_ref()
            .map(|name| classify_device_type(name))
            .unwrap_or("Unknown");

        // Track meeting ended event with privacy-safe data
        match crate::analytics::commands::track_meeting_ended(
            transcription_provider.clone(),
            transcription_model.clone(),
            summary_provider.clone(),
            summary_model.clone(),
            total_duration,
            active_duration,
            pause_duration,
            microphone_device_type.to_string(),
            system_audio_device_type.to_string(),
            chunks_processed,
            transcript_segments_count,
            had_fatal_error,
        )
        .await
        {
            Ok(_) => info!("✅ Analytics tracked successfully for meeting end"),
            Err(e) => warn!("⚠️ Failed to track analytics: {}", e),
        }
    }

    // Step 4: Finalize recording state and cleanup resources safely
    let _ = app.emit(
        "recording-shutdown-progress",
        serde_json::json!({
            "stage": "finalizing",
            "message": "Finalizing recording and cleaning up resources...",
            "progress": 90
        }),
    );

    // Perform final cleanup with the manager if available
    let (meeting_folder, meeting_name) = if let Some(mut manager) = manager_for_cleanup {
        info!("🧹 Performing final cleanup and saving recording data");

        // Extract meeting info BEFORE async operations
        let meeting_folder = manager.get_meeting_folder();
        let meeting_name = manager.get_meeting_name();

        match tokio::time::timeout(
            tokio::time::Duration::from_secs(300), // 5 minutes max for file I/O
            manager.save_recording_only(&app)
        ).await {
            Ok(Ok(_)) => {
                info!("✅ Recording data saved successfully during cleanup");
            }
            Ok(Err(e)) => {
                warn!(
                    "⚠️ Error during recording cleanup (transcripts preserved): {}",
                    e
                );
                // Don't fail shutdown - transcripts are already preserved
            }
            Err(_) => {
                warn!("⏱️ File I/O timeout (5 minutes) reached during save, continuing shutdown");
                // Don't fail shutdown - transcripts are already preserved
            }
        }

        (meeting_folder, meeting_name)
    } else {
        info!("ℹ️ No recording manager available for cleanup");
        (None, None)
    };

    // Set recording flag to false
    info!("🔍 Setting IS_RECORDING to false");
    IS_RECORDING.store(false, Ordering::SeqCst);
    crate::audio::transcription::live_speaker::clear_turn_state();

    // Step 4.5: Prepare metadata for frontend (NO database save)
    // NOTE: We do NOT save to database here. The frontend will save after all transcripts are displayed.
    // This ensures the user sees all transcripts streaming in before the database save happens.
    let (folder_path_str, meeting_name_str) = match (&meeting_folder, &meeting_name) {
        (Some(path), Some(name)) => (
            Some(path.to_string_lossy().to_string()),
            Some(name.clone()),
        ),
        _ => (None, None),
    };

    info!("📤 Preparing recording metadata for frontend save");
    info!("   folder_path: {:?}", folder_path_str);
    info!("   meeting_name: {:?}", meeting_name_str);

    // Database save removed - frontend will handle this after receiving all transcripts
    info!("ℹ️ Skipping database save in Rust - frontend will save after all transcripts received");

    // Step 5: Complete shutdown
    let _ = app.emit(
        "recording-shutdown-progress",
        serde_json::json!({
            "stage": "complete",
            "message": "Recording stopped successfully",
            "progress": 100
        }),
    );

    // Emit final stop event with folder_path and meeting_name for frontend to save
    app.emit(
        "recording-stopped",
        serde_json::json!({
            "message": "Recording stopped - frontend will save after all transcripts received",
            "folder_path": folder_path_str,
            "meeting_name": meeting_name_str
        }),
    )
    .map_err(|e| e.to_string())?;

    // Update tray menu to reflect stopped state
    crate::tray::update_tray_menu(&app);

    info!("🎉 Recording stopped successfully with ZERO transcript chunks lost");
    Ok(())
}

/// Check if recording is active
pub async fn is_recording() -> bool {
    IS_RECORDING.load(Ordering::SeqCst)
}

/// Get recording statistics
pub async fn get_transcription_status() -> TranscriptionStatus {
    TranscriptionStatus {
        chunks_in_queue: 0,
        is_processing: IS_RECORDING.load(Ordering::SeqCst),
        last_activity_ms: 0,
    }
}

/// Pause the current recording
#[tauri::command]
pub async fn pause_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    info!("Pausing recording");

    // Check if currently recording
    if !IS_RECORDING.load(Ordering::SeqCst) {
        return Err("No recording is currently active".to_string());
    }

    // Access the recording manager and pause it
    let manager_guard = RECORDING_MANAGER.lock();
    if let Some(manager) = manager_guard.as_ref() {
        manager.pause_recording().map_err(|e| e.to_string())?;

        // Emit pause event to frontend
        app.emit(
            "recording-paused",
            serde_json::json!({
                "message": "Recording paused"
            }),
        )
        .map_err(|e| e.to_string())?;

        // Update tray menu to reflect paused state
        crate::tray::update_tray_menu(&app);

        info!("Recording paused successfully");
        Ok(())
    } else {
        Err("No recording manager found".to_string())
    }
}

/// Resume the current recording
#[tauri::command]
pub async fn resume_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    info!("Resuming recording");

    // Check if currently recording
    if !IS_RECORDING.load(Ordering::SeqCst) {
        return Err("No recording is currently active".to_string());
    }

    // Access the recording manager and resume it
    let manager_guard = RECORDING_MANAGER.lock();
    if let Some(manager) = manager_guard.as_ref() {
        manager.resume_recording().map_err(|e| e.to_string())?;

        // Emit resume event to frontend
        app.emit(
            "recording-resumed",
            serde_json::json!({
                "message": "Recording resumed"
            }),
        )
        .map_err(|e| e.to_string())?;

        // Update tray menu to reflect resumed state
        crate::tray::update_tray_menu(&app);

        info!("Recording resumed successfully");
        Ok(())
    } else {
        Err("No recording manager found".to_string())
    }
}

/// Check if recording is currently paused
#[tauri::command]
pub async fn is_recording_paused() -> bool {
    let manager_guard = RECORDING_MANAGER.lock();
    if let Some(manager) = manager_guard.as_ref() {
        manager.is_paused()
    } else {
        false
    }
}

/// Mute/unmute microphone audio during an active recording without stopping system audio.
#[tauri::command]
pub async fn set_recording_microphone_muted(muted: bool) -> Result<bool, String> {
    let manager_guard = RECORDING_MANAGER.lock();
    if let Some(manager) = manager_guard.as_ref() {
        manager.set_microphone_muted(muted);
        Ok(manager.is_microphone_muted())
    } else {
        Err("Recording not active".to_string())
    }
}

/// Get detailed recording state
#[tauri::command]
pub async fn get_recording_state() -> serde_json::Value {
    let is_recording = IS_RECORDING.load(Ordering::SeqCst);
    let manager_guard = RECORDING_MANAGER.lock();

    if let Some(manager) = manager_guard.as_ref() {
        serde_json::json!({
            "is_recording": is_recording,
            "is_paused": manager.is_paused(),
            "is_active": manager.is_active(),
            "recording_duration": manager.get_recording_duration(),
            "active_duration": manager.get_active_recording_duration(),
            "total_pause_duration": manager.get_total_pause_duration(),
            "current_pause_duration": manager.get_current_pause_duration()
        })
    } else {
        serde_json::json!({
            "is_recording": is_recording,
            "is_paused": false,
            "is_active": false,
            "recording_duration": null,
            "active_duration": null,
            "total_pause_duration": 0.0,
            "current_pause_duration": null
        })
    }
}

/// Get the meeting folder path for the current recording
/// Returns the path if a meeting name was set and folder structure initialized
#[tauri::command]
pub async fn get_meeting_folder_path() -> Result<Option<String>, String> {
    let manager_guard = RECORDING_MANAGER.lock();
    if let Some(manager) = manager_guard.as_ref() {
        Ok(manager.get_meeting_folder().map(|p| p.to_string_lossy().to_string()))
    } else {
        Ok(None)
    }
}

/// Get accumulated transcript segments from current recording session
/// Used for syncing frontend state after page reload during active recording
#[tauri::command]
pub async fn get_transcript_history() -> Result<Vec<crate::audio::recording_saver::TranscriptSegment>, String> {
    let manager_guard = RECORDING_MANAGER.lock();

    if let Some(manager) = manager_guard.as_ref() {
        Ok(manager.get_transcript_segments())
    } else {
        Ok(Vec::new()) // No recording active, return empty
    }
}

/// Load finalized transcript segments from a meeting folder's transcripts.json
/// (used after stop_recording when CAPU has updated the on-disk file).
#[tauri::command]
pub async fn load_transcripts_from_folder(
    folder_path: String,
) -> Result<Vec<crate::audio::recording_saver::TranscriptSegment>, String> {
    let path = std::path::PathBuf::from(&folder_path).join("transcripts.json");
    if !path.exists() {
        return Err(format!(
            "Không tìm thấy transcripts.json trong {}",
            folder_path
        ));
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let segments_value = json
        .get("segments")
        .cloned()
        .ok_or_else(|| "transcripts.json không có trường segments".to_string())?;
    let segments = serde_json::from_value::<Vec<crate::audio::recording_saver::TranscriptSegment>>(
        segments_value,
    )
    .map_err(|e| format!("Không đọc được segments từ transcripts.json: {}", e))?;
    Ok(segments)
}

/// Update transcript text for one segment during an active recording (persists to transcripts.json).
#[tauri::command]
pub async fn update_live_transcript_segment(
    sequence_id: u64,
    new_text: String,
) -> Result<(), String> {
    let manager_guard = RECORDING_MANAGER.lock();
    let manager = manager_guard
        .as_ref()
        .ok_or_else(|| "Không có phiên ghi âm đang hoạt động".to_string())?;
    manager.update_live_transcript_text(sequence_id, new_text)
}

/// Get meeting name from current recording session
/// Used for syncing frontend state after page reload during active recording
#[tauri::command]
pub async fn get_recording_meeting_name() -> Result<Option<String>, String> {
    let manager_guard = RECORDING_MANAGER.lock();

    if let Some(manager) = manager_guard.as_ref() {
        Ok(manager.get_meeting_name())
    } else {
        Ok(None)
    }
}

// ============================================================================
// DEVICE MONITORING COMMANDS (AirPods/Bluetooth disconnect/reconnect support)
// ============================================================================

/// Response structure for device events
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type")]
pub enum DeviceEventResponse {
    DeviceDisconnected {
        device_name: String,
        device_type: String,
    },
    DeviceReconnected {
        device_name: String,
        device_type: String,
    },
    DeviceListChanged,
}

impl From<DeviceEvent> for DeviceEventResponse {
    fn from(event: DeviceEvent) -> Self {
        match event {
            DeviceEvent::DeviceDisconnected { device_name, device_type } => {
                DeviceEventResponse::DeviceDisconnected {
                    device_name,
                    device_type: format!("{:?}", device_type),
                }
            }
            DeviceEvent::DeviceReconnected { device_name, device_type } => {
                DeviceEventResponse::DeviceReconnected {
                    device_name,
                    device_type: format!("{:?}", device_type),
                }
            }
            DeviceEvent::DeviceListChanged => DeviceEventResponse::DeviceListChanged,
        }
    }
}

/// Reconnection status information
#[derive(Debug, Serialize, Clone)]
pub struct ReconnectionStatus {
    pub is_reconnecting: bool,
    pub disconnected_device: Option<DisconnectedDeviceInfo>,
}

/// Information about a disconnected device
#[derive(Debug, Serialize, Clone)]
pub struct DisconnectedDeviceInfo {
    pub name: String,
    pub device_type: String,
}

/// Poll for audio device events (disconnect/reconnect)
/// Should be called periodically (every 1-2 seconds) by frontend during recording
#[tauri::command]
pub async fn poll_audio_device_events() -> Result<Option<DeviceEventResponse>, String> {
    let mut manager_guard = RECORDING_MANAGER.lock();

    if let Some(manager) = manager_guard.as_mut() {
        if let Some(event) = manager.poll_device_events() {
            info!("📱 Device event polled: {:?}", event);
            Ok(Some(event.into()))
        } else {
            Ok(None)
        }
    } else {
        // Not recording, no events
        Ok(None)
    }
}

/// Get current reconnection status
/// Returns whether the system is attempting to reconnect and which device
#[tauri::command]
pub async fn get_reconnection_status() -> Result<ReconnectionStatus, String> {
    let manager_guard = RECORDING_MANAGER.lock();

    if let Some(manager) = manager_guard.as_ref() {
        let state = manager.get_state();
        let disconnected_device = state.get_disconnected_device().map(|(device, device_type)| {
            DisconnectedDeviceInfo {
                name: device.name.clone(),
                device_type: format!("{:?}", device_type),
            }
        });

        Ok(ReconnectionStatus {
            is_reconnecting: manager.is_reconnecting(),
            disconnected_device,
        })
    } else {
        // Not recording, no reconnection in progress
        Ok(ReconnectionStatus {
            is_reconnecting: false,
            disconnected_device: None,
        })
    }
}

/// Get information about the active audio output device
/// Used to warn users about Bluetooth playback issues
#[tauri::command]
pub async fn get_active_audio_output() -> Result<super::playback_monitor::AudioOutputInfo, String> {
    super::playback_monitor::get_active_audio_output()
        .await
        .map_err(|e| format!("Failed to get audio output info: {}", e))
}

/// Manually trigger device reconnection attempt
/// Useful for UI "Retry" button
#[tauri::command]
pub async fn attempt_device_reconnect(
    device_name: String,
    device_type: String,
) -> Result<bool, String> {
    // Parse device type first
    let monitor_type = match device_type.as_str() {
        "Microphone" => DeviceMonitorType::Microphone,
        "SystemAudio" => DeviceMonitorType::SystemAudio,
        _ => return Err(format!("Invalid device type: {}", device_type)),
    };

    // Take the manager out from behind the global lock for the duration of
    // the reconnect attempt, instead of holding the lock the whole time.
    // This means other recording commands (notably poll_audio_device_events,
    // which the frontend calls every 1-2 seconds) don't block on this
    // command's unbounded-duration device re-enumeration + stream restart.
    let mut manager = {
        let mut manager_guard = RECORDING_MANAGER.lock();
        match manager_guard.take() {
            Some(m) => m,
            None => return Err("Recording not active".to_string()),
        }
    }; // Lock released here
    RECONNECT_IN_PROGRESS.store(true, Ordering::SeqCst);

    let result = manager.attempt_device_reconnect(&device_name, monitor_type).await;

    // Put the manager back, but only if the slot is still empty. If the user
    // stopped and started a new recording while this reconnect was in
    // flight, stop_recording's RECONNECT_IN_PROGRESS check below should have
    // prevented that — this is defense in depth in case that check is ever
    // bypassed: never silently clobber a live manager that's already there.
    {
        let mut manager_guard = RECORDING_MANAGER.lock();
        if manager_guard.is_none() {
            *manager_guard = Some(manager);
        } else {
            warn!(
                "⚠️ Reconnect finished but a new recording session is already active — \
                 discarding stale manager instead of overwriting it"
            );
        }
    }
    RECONNECT_IN_PROGRESS.store(false, Ordering::SeqCst);

    match result {
        Ok(success) => {
            if success {
                info!("✅ Manual reconnection successful");
            } else {
                warn!("❌ Manual reconnection failed - device not available");
            }
            Ok(success)
        }
        Err(e) => {
            error!("Manual reconnection error: {}", e);
            Err(e.to_string())
        }
    }
}

#[cfg(test)]
mod resolve_capture_source_tests {
    use super::*;
    use crate::audio::{AudioDevice, DeviceType};

    fn mic() -> Arc<AudioDevice> {
        Arc::new(AudioDevice::new("Mic".into(), DeviceType::Input))
    }

    fn speaker() -> Arc<AudioDevice> {
        Arc::new(AudioDevice::new("Speaker".into(), DeviceType::Output))
    }

    #[test]
    fn system_source_never_opens_microphone() {
        match resolve_capture_devices(AudioCaptureSource::System, None, None) {
            Ok((mic, _)) => assert!(
                mic.is_none(),
                "system-only capture must skip the microphone"
            ),
            Err(_) => {}
        }
    }

    #[test]
    fn microphone_source_never_opens_system_audio() {
        match resolve_capture_devices(AudioCaptureSource::Microphone, None, None) {
            Ok((_, system)) => assert!(
                system.is_none(),
                "microphone-only capture must skip system audio"
            ),
            Err(_) => {}
        }
    }

    #[test]
    fn both_ok_when_only_microphone_exists() {
        let (got_mic, got_sys) = accept_capture_devices(
            AudioCaptureSource::Both,
            Some(mic()),
            None,
        )
        .expect("mic-only machine can still record");
        assert!(got_mic.is_some());
        assert!(got_sys.is_none());
    }

    #[test]
    fn both_ok_when_only_speaker_exists() {
        let (got_mic, got_sys) = accept_capture_devices(
            AudioCaptureSource::Both,
            None,
            Some(speaker()),
        )
        .expect("speaker-only machine can still record system audio");
        assert!(got_mic.is_none());
        assert!(got_sys.is_some());
    }

    #[test]
    fn both_fail_when_neither_device_exists() {
        let err = accept_capture_devices(AudioCaptureSource::Both, None, None)
            .expect_err("no devices");
        assert_eq!(err, "Không có nguồn âm thanh nào khả dụng.");
    }

    #[test]
    fn microphone_only_fails_without_mic() {
        let err = accept_capture_devices(AudioCaptureSource::Microphone, None, Some(speaker()))
            .expect_err("forced mic-only");
        assert!(err.contains("microphone"));
    }

    #[test]
    fn system_only_fails_without_speaker() {
        let err = accept_capture_devices(AudioCaptureSource::System, Some(mic()), None)
            .expect_err("forced system-only");
        assert!(err.contains("âm thanh hệ thống"));
    }
}
