use anyhow::Result;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;

use super::audio_processing::create_meeting_folder;
use super::incremental_saver::IncrementalAudioSaver;
use super::recording_state::AudioChunk;

/// Structured transcript segment for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub text: String,
    pub audio_start_time: f64, // Seconds from recording start
    pub audio_end_time: f64,   // Seconds from recording start
    pub duration: f64,         // Segment duration in seconds
    pub display_time: String,  // Formatted time for display like "[02:15]"
    pub confidence: f32,
    pub sequence_id: u64,
    /// When true, STT updates must not replace `text` (user corrected this segment).
    #[serde(default)]
    pub user_edited: bool,
    #[serde(default)]
    pub is_partial: bool,
    /// Live hotkey-assigned speaker name (not file-import diarization).
    #[serde(default)]
    pub speaker_name: Option<String>,
}

/// Meeting metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingMetadata {
    pub version: String,
    pub meeting_id: Option<String>,
    pub meeting_name: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub duration_seconds: Option<f64>,
    pub devices: DeviceInfo,
    pub audio_file: String,
    pub transcript_file: String,
    pub sample_rate: u32,
    pub status: String, // "recording", "completed", "error"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub microphone: Option<String>,
    pub system_audio: Option<String>,
}

/** Session-owned persistence shared with ASR workers, independent of UI events. */
#[derive(Clone)]
pub struct TranscriptWriter {
    segments: Arc<Mutex<Vec<TranscriptSegment>>>,
    folder: Option<PathBuf>,
    snapshot_state: Arc<Mutex<Option<Instant>>>,
}

impl TranscriptWriter {
    pub fn record_update(&self, update: &super::transcription::TranscriptUpdate) -> Result<()> {
        self.record_segment(TranscriptSegment {
            id: format!("seg_{}", update.sequence_id),
            text: update.text.clone(),
            audio_start_time: update.audio_start_time,
            audio_end_time: update.audio_end_time,
            duration: update.duration,
            display_time: update.timestamp.clone(),
            confidence: update.confidence,
            sequence_id: update.sequence_id,
            user_edited: false,
            is_partial: update.is_partial,
            speaker_name: update.speaker_name.clone(),
        })
    }

    fn record_segment(&self, mut segment: TranscriptSegment) -> Result<()> {
        {
            let mut segments = self
                .segments
                .lock()
                .map_err(|_| anyhow::anyhow!("Transcript lock poisoned"))?;
            if let Some(existing) = segments
                .iter_mut()
                .find(|s| s.sequence_id == segment.sequence_id)
            {
                if !existing.is_partial && segment.is_partial {
                    return Ok(());
                }
                if existing.user_edited {
                    segment.text = existing.text.clone();
                    segment.user_edited = true;
                }
                *existing = segment;
            } else {
                segments.push(segment);
            }
        }
        if let Some(folder) = &self.folder {
            self.write_snapshot(folder, false)?;
        }
        Ok(())
    }

    fn write_snapshot(&self, folder: &PathBuf, force: bool) -> Result<()> {
        // Serialize snapshot writers and cap full-history JSON rewrites to 1/s.
        // Finalization and user edits always flush immediately.
        let mut last_write = self
            .snapshot_state
            .lock()
            .map_err(|_| anyhow::anyhow!("Snapshot lock poisoned"))?;
        if !force && last_write.is_some_and(|last| last.elapsed() < Duration::from_secs(1)) {
            return Ok(());
        }
        self.write_json(folder)?;
        *last_write = Some(Instant::now());
        Ok(())
    }

    fn write_json(&self, folder: &PathBuf) -> Result<()> {
        // Clone segments to avoid holding lock during I/O
        let segments_clone = if let Ok(segments) = self.segments.lock() {
            segments.clone()
        } else {
            error!("Failed to lock transcript segments for writing");
            return Err(anyhow::anyhow!("Failed to lock transcript segments"));
        };

        let transcript_path = folder.join("transcripts.json");
        let temp_path = folder.join(".transcripts.json.tmp");

        // Create JSON structure
        let json = serde_json::json!({
            "version": "1.0",
            "segments": segments_clone,
            "last_updated": chrono::Utc::now().to_rfc3339(),
            "total_segments": segments_clone.len()
        });

        // Serialize to pretty JSON string
        let json_string = serde_json::to_string_pretty(&json).map_err(|e| {
            error!("Failed to serialize transcripts to JSON: {}", e);
            anyhow::anyhow!("JSON serialization failed: {}", e)
        })?;

        // Write to temp file with error handling
        std::fs::write(&temp_path, &json_string).map_err(|e| {
            error!(
                "Failed to write transcript temp file to {}: {}",
                temp_path.display(),
                e
            );
            anyhow::anyhow!("Failed to write temp file: {}", e)
        })?;

        // Verify temp file was written correctly
        if !temp_path.exists() {
            error!(
                "Temp transcript file does not exist after write: {}",
                temp_path.display()
            );
            return Err(anyhow::anyhow!("Temp file verification failed"));
        }

        // Atomic rename
        std::fs::rename(&temp_path, &transcript_path).map_err(|e| {
            error!(
                "Failed to rename transcript file from {} to {}: {}",
                temp_path.display(),
                transcript_path.display(),
                e
            );
            anyhow::anyhow!("Failed to rename transcript file: {}", e)
        })?;

        Ok(())
    }
}

/// New recording saver using incremental saving strategy
pub struct RecordingSaver {
    incremental_saver: Option<Arc<AsyncMutex<IncrementalAudioSaver>>>,
    meeting_folder: Option<PathBuf>,
    meeting_name: Option<String>,
    base_folder: Option<PathBuf>,
    metadata: Option<MeetingMetadata>,
    transcript_segments: Arc<Mutex<Vec<TranscriptSegment>>>,
    accumulation_task: Option<JoinHandle<Result<()>>>,
    snapshot_state: Arc<Mutex<Option<Instant>>>,
    session_id: String,
}

impl RecordingSaver {
    pub fn new() -> Self {
        Self {
            incremental_saver: None,
            meeting_folder: None,
            meeting_name: None,
            base_folder: None,
            metadata: None,
            transcript_segments: Arc::new(Mutex::new(Vec::new())),
            accumulation_task: None,
            snapshot_state: Arc::new(Mutex::new(None)),
            session_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Set the meeting name for this recording session
    pub fn set_meeting_name(&mut self, name: Option<String>) {
        self.meeting_name = name;
    }

    /// Set the base folder for saving meeting recordings and transcripts
    pub fn set_save_folder(&mut self, folder: PathBuf) {
        self.base_folder = Some(folder);
    }

    /// Set device information in metadata
    pub fn set_device_info(&mut self, mic_name: Option<String>, sys_name: Option<String>) {
        if let Some(ref mut metadata) = self.metadata {
            metadata.devices.microphone = mic_name;
            metadata.devices.system_audio = sys_name;

            // Write updated metadata to disk if folder exists
            if let Some(folder) = &self.meeting_folder {
                let metadata_clone = metadata.clone();
                if let Err(e) = self.write_metadata(folder, &metadata_clone) {
                    warn!("Failed to update metadata with device info: {}", e);
                }
            }
        }
    }

    /// Add or update a structured transcript segment (upserts based on sequence_id)
    /// Also saves incrementally to disk
    pub fn add_transcript_segment(&self, segment: TranscriptSegment) {
        if let Err(e) = self.transcript_writer().record_segment(segment) {
            error!("Failed to persist transcript segment: {}", e);
        }
    }

    /// Replaces one or more stored segments (matched by `source_ids`) with a single
    /// finalized segment — used when the CAPU background stage finishes punctuating a
    /// batch of raw ASR segments. The replacement is inserted at the position of the
    /// first matched segment, preserving chronological order (by `sequence_id`).
    ///
    /// No-op (with a warning log) if none of `source_ids` are found. Also a no-op if any
    /// matched segment has `user_edited = true` — mirrors `add_transcript_segment`'s own
    /// rule that a user's manual correction is never silently overwritten.
    pub fn replace_transcript_segments(
        &self,
        source_ids: &[u64],
        finalized_text: String,
        audio_start_time: f64,
        audio_end_time: f64,
    ) {
        let (replaced_count, sequence_id) = {
            let mut segments = match self.transcript_segments.lock() {
                Ok(s) => s,
                Err(_) => {
                    error!("Failed to lock transcript segments for replace");
                    return;
                }
            };

            let matched: Vec<usize> = segments
                .iter()
                .enumerate()
                .filter(|(_, s)| source_ids.contains(&s.sequence_id))
                .map(|(i, _)| i)
                .collect();

            if matched.is_empty() {
                warn!(
                    "replace_transcript_segments: none of {:?} found in stored segments",
                    source_ids
                );
                return;
            }

            if matched.iter().any(|&i| segments[i].user_edited) {
                info!(
                    "replace_transcript_segments: skipping batch {:?} — contains a user-edited segment",
                    source_ids
                );
                return;
            }

            let sequence_id = segments[matched[0]].sequence_id;
            let display_time = segments[matched[0]].display_time.clone();
            let confidence = segments[matched[0]].confidence;
            let speaker_name = segments[matched[0]].speaker_name.clone();

            let replacement = TranscriptSegment {
                id: format!("seg_{}_finalized", sequence_id),
                text: finalized_text,
                audio_start_time,
                audio_end_time,
                duration: audio_end_time - audio_start_time,
                display_time,
                confidence,
                sequence_id,
                user_edited: false,
                is_partial: false,
                speaker_name,
            };

            segments.retain(|s| !source_ids.contains(&s.sequence_id));
            let insert_at = segments.partition_point(|s| s.sequence_id < sequence_id);
            segments.insert(insert_at, replacement);

            (matched.len(), sequence_id)
        };

        info!(
            "Replaced {} raw segment(s) with 1 finalized segment (sequence_id={})",
            replaced_count, sequence_id
        );

        if let Some(folder) = &self.meeting_folder {
            if let Err(e) = self.write_transcripts_json(folder) {
                warn!("Failed to write transcripts.json after replace: {}", e);
            }
        }
    }

    /// Rebuild unedited live transcripts from CAPU sentence spans.
    /// Keeps `user_edited` rows; one batch may expand into many sentences.
    pub fn apply_live_capu_results(
        &self,
        finalized: &[crate::capu_engine::batch::FinalizedSegment],
    ) {
        {
            let mut segments = match self.transcript_segments.lock() {
                Ok(s) => s,
                Err(_) => {
                    error!("Failed to lock transcript segments for live CAPU apply");
                    return;
                }
            };

            let mut speaker_by_id: HashMap<u64, Option<String>> = HashMap::new();
            let mut confidence_by_id: HashMap<u64, f32> = HashMap::new();
            let mut display_by_id: HashMap<u64, String> = HashMap::new();
            for s in segments.iter() {
                speaker_by_id.insert(s.sequence_id, s.speaker_name.clone());
                confidence_by_id.insert(s.sequence_id, s.confidence);
                display_by_id.insert(s.sequence_id, s.display_time.clone());
            }

            let user_edited: Vec<TranscriptSegment> =
                segments.iter().filter(|s| s.user_edited).cloned().collect();
            let edited_ids: HashSet<u64> = user_edited.iter().map(|s| s.sequence_id).collect();

            let mut new_list: Vec<TranscriptSegment> = Vec::new();
            for (i, f) in finalized.iter().enumerate() {
                if f.source_ids.iter().any(|id| edited_ids.contains(id)) {
                    continue;
                }
                if f.text.trim().is_empty() {
                    continue;
                }
                let first_id = f.source_ids.first().copied();
                let speaker_name = first_id
                    .and_then(|id| speaker_by_id.get(&id).cloned())
                    .flatten();
                let confidence = first_id
                    .and_then(|id| confidence_by_id.get(&id).copied())
                    .unwrap_or(0.9);
                let display_time = first_id
                    .and_then(|id| display_by_id.get(&id).cloned())
                    .unwrap_or_else(|| format_mmss(f.audio_start_time));
                new_list.push(TranscriptSegment {
                    id: format!("seg_{}_finalized", i),
                    text: f.text.clone(),
                    audio_start_time: f.audio_start_time,
                    audio_end_time: f.audio_end_time,
                    duration: (f.audio_end_time - f.audio_start_time).max(0.0),
                    display_time,
                    confidence,
                    sequence_id: i as u64,
                    user_edited: false,
                    is_partial: false,
                    speaker_name,
                });
            }
            new_list.extend(user_edited);
            new_list.sort_by(|a, b| {
                a.audio_start_time
                    .partial_cmp(&b.audio_start_time)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for (i, s) in new_list.iter_mut().enumerate() {
                s.sequence_id = i as u64;
            }

            info!(
                "Applied live CAPU: {} sentence(s) + kept user-edited segments",
                new_list.iter().filter(|s| !s.user_edited).count()
            );
            *segments = new_list;
        }

        if let Some(folder) = &self.meeting_folder {
            if let Err(e) = self.write_transcripts_json(folder) {
                warn!("Failed to write transcripts.json after live CAPU: {}", e);
            }
        }
    }

    /// Legacy method for backward compatibility - converts text to basic segment
    pub fn add_transcript_chunk(&self, text: String) {
        let segment = TranscriptSegment {
            id: format!("seg_{}", chrono::Utc::now().timestamp_millis()),
            text,
            audio_start_time: 0.0,
            audio_end_time: 0.0,
            duration: 0.0,
            display_time: "[00:00]".to_string(),
            confidence: 1.0,
            sequence_id: 0,
            user_edited: false,
            is_partial: false,
            speaker_name: None,
        };
        self.add_transcript_segment(segment);
    }

    /// Start accumulation with optional incremental saving
    ///
    /// # Arguments
    /// * `auto_save` - If true, creates checkpoints and enables saving. If false, audio chunks are discarded.
    pub fn start_accumulation(&mut self, auto_save: bool) -> mpsc::Sender<AudioChunk> {
        if auto_save {
            info!("Initializing incremental audio saver for recording (auto-save ENABLED)");
        } else {
            info!(
                "Starting recording without audio saving (auto-save DISABLED - transcripts only)"
            );
        }

        // Create channel for receiving audio chunks
        let (sender, mut receiver) =
            mpsc::channel::<AudioChunk>(super::constants::RECORDING_QUEUE_CAPACITY);

        // Initialize meeting folder and incremental saver ONLY if auto_save is enabled
        if auto_save {
            if let Some(name) = self.meeting_name.clone() {
                match self.initialize_meeting_folder(&name, true) {
                    Ok(()) => info!("Successfully initialized meeting folder with checkpoints"),
                    Err(e) => {
                        error!("Failed to initialize meeting folder: {}", e);
                        // Continue anyway - will use fallback flat structure
                    }
                }
            }
        } else {
            // When auto_save is false, still create meeting folder for transcripts/metadata
            // but skip .checkpoints directory
            if let Some(name) = self.meeting_name.clone() {
                match self.initialize_meeting_folder(&name, false) {
                    Ok(()) => info!("Successfully initialized meeting folder (transcripts only)"),
                    Err(e) => {
                        error!("Failed to initialize meeting folder: {}", e);
                    }
                }
            }
        }

        let saver = self.incremental_saver.clone();
        self.accumulation_task = Some(tokio::task::spawn_blocking(move || {
            // Drain until every pipeline sender is dropped. Encoding runs on a
            // blocking worker, so FFmpeg cannot occupy a Tokio runtime thread.
            let mut first_error = None;
            while let Some(chunk) = receiver.blocking_recv() {
                if auto_save {
                    if let Some(saver) = &saver {
                        if first_error.is_some() {
                            saver.blocking_lock().buffer_chunk(chunk);
                            continue;
                        }
                        if let Err(error) = saver.blocking_lock().add_chunk(chunk) {
                            receiver.close();
                            error!("Audio checkpoint failed: {}", error);
                            if first_error.is_none() {
                                first_error = Some(error);
                            }
                        }
                    } else if first_error.is_none() {
                        receiver.close();
                        first_error = Some(anyhow::anyhow!("Audio saver unavailable"));
                    }
                }
            }
            match first_error {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }));

        sender
    }

    /// Initialize meeting folder structure and metadata
    ///
    /// # Arguments
    /// * `meeting_name` - Name of the meeting
    /// * `create_checkpoints` - Whether to create .checkpoints/ directory and IncrementalAudioSaver
    fn initialize_meeting_folder(
        &mut self,
        meeting_name: &str,
        create_checkpoints: bool,
    ) -> Result<()> {
        // Load preferences to get base recordings folder
        let base_folder = self
            .base_folder
            .clone()
            .unwrap_or_else(super::recording_preferences::get_default_recordings_folder);

        // Create meeting folder structure (with or without .checkpoints/ subdirectory)
        let meeting_folder = create_meeting_folder(&base_folder, meeting_name, create_checkpoints)?;

        // Only initialize incremental saver if checkpoints are needed (auto_save is true)
        if create_checkpoints {
            let incremental_saver = IncrementalAudioSaver::new(meeting_folder.clone(), 48000)?;
            self.incremental_saver = Some(Arc::new(AsyncMutex::new(incremental_saver)));
            info!(
                "✅ Incremental audio saver initialized for meeting: {}",
                meeting_name
            );
        } else {
            info!("⚠️  Skipped incremental audio saver (auto-save disabled)");
        }

        // Create initial metadata
        let metadata = MeetingMetadata {
            version: "1.0".to_string(),
            meeting_id: Some(self.session_id.clone()),
            meeting_name: Some(meeting_name.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            duration_seconds: None,
            devices: DeviceInfo {
                microphone: None, // Could be enhanced to store actual device names
                system_audio: None,
            },
            audio_file: if create_checkpoints {
                "audio.mp4".to_string()
            } else {
                "".to_string()
            },
            transcript_file: "transcripts.json".to_string(),
            sample_rate: 48000,
            status: "recording".to_string(),
        };

        // Write initial metadata.json
        self.write_metadata(&meeting_folder, &metadata)?;

        self.meeting_folder = Some(meeting_folder);
        self.metadata = Some(metadata);

        Ok(())
    }

    /// Write metadata.json to disk (atomic write with temp file)
    fn write_metadata(&self, folder: &PathBuf, metadata: &MeetingMetadata) -> Result<()> {
        let metadata_path = folder.join("metadata.json");
        let temp_path = folder.join(".metadata.json.tmp");

        let json_string = serde_json::to_string_pretty(metadata)?;
        std::fs::write(&temp_path, json_string)?;
        std::fs::rename(&temp_path, &metadata_path)?; // Atomic

        Ok(())
    }

    /// Write transcripts.json to disk (atomic write with temp file and validation)
    fn write_transcripts_json(&self, folder: &PathBuf) -> Result<()> {
        self.transcript_writer().write_snapshot(folder, true)
    }

    pub fn transcript_writer(&self) -> TranscriptWriter {
        TranscriptWriter {
            segments: self.transcript_segments.clone(),
            folder: self.meeting_folder.clone(),
            snapshot_state: self.snapshot_state.clone(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    // in frontend/src-tauri/src/audio/recording_saver.rs
    pub fn get_stats(&self) -> (usize, u32) {
        if let Some(ref saver) = self.incremental_saver {
            if let Ok(guard) = saver.try_lock() {
                (guard.get_checkpoint_count() as usize, 48000)
            } else {
                (0, 48000)
            }
        } else {
            (0, 48000)
        }
    }

    /// Stop and save using incremental saving approach
    ///
    /// # Arguments
    /// * `app` - Tauri app handle for emitting events
    /// * `recording_duration` - Actual recording duration in seconds (from RecordingState)
    pub async fn stop_and_save<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        recording_duration: Option<f64>,
    ) -> Result<Option<String>, String> {
        info!("Stopping recording saver");

        if let Some(task) = self.accumulation_task.take() {
            task.await
                .map_err(|e| format!("Audio saver worker failed: {}", e))?
                .map_err(|e| format!("Audio checkpoint failed: {}", e))?;
        }

        // Check if incremental saver exists (indicates auto_save was enabled)
        let should_save_audio = self.incremental_saver.is_some();

        if !should_save_audio {
            info!("⚠️  No audio saver initialized (auto-save was disabled) - skipping audio finalization");
            if let Some(folder) = &self.meeting_folder {
                self.write_transcripts_json(folder)
                    .map_err(|e| e.to_string())?;
            }
            return Ok(None);
        }

        // Finalize incremental saver (merge checkpoints into final audio.mp4)
        let final_audio_path = if let Some(saver_arc) = &self.incremental_saver {
            let mut saver = saver_arc.lock().await;
            match saver.finalize().await {
                Ok(path) => {
                    info!("✅ Successfully finalized audio: {}", path.display());
                    path
                }
                Err(e) => {
                    error!("❌ Failed to finalize incremental saver: {}", e);
                    return Err(format!("Failed to finalize audio: {}", e));
                }
            }
        } else {
            error!("No incremental saver initialized - cannot save recording");
            return Err("No incremental saver initialized".to_string());
        };

        // Save final transcripts.json with validation
        if let Some(folder) = &self.meeting_folder {
            if let Err(e) = self.write_transcripts_json(folder) {
                error!("❌ Failed to write final transcripts: {}", e);
                return Err(format!("Failed to save transcripts: {}", e));
            }

            // Verify transcripts were written correctly
            let transcript_path = folder.join("transcripts.json");
            if !transcript_path.exists() {
                error!(
                    "❌ Transcript file was not created at: {}",
                    transcript_path.display()
                );
                return Err("Transcript file verification failed".to_string());
            }
            info!(
                "✅ Transcripts saved and verified at: {}",
                transcript_path.display()
            );
        }

        // Update metadata to completed status with actual recording duration
        if let (Some(folder), Some(mut metadata)) = (&self.meeting_folder, self.metadata.clone()) {
            metadata.status = "completed".to_string();
            metadata.completed_at = Some(chrono::Utc::now().to_rfc3339());

            // Use actual recording duration from RecordingState (more accurate than transcript segments)
            // Falls back to last transcript segment if duration not provided
            metadata.duration_seconds = recording_duration.or_else(|| {
                if let Ok(segments) = self.transcript_segments.lock() {
                    segments.last().map(|seg| seg.audio_end_time)
                } else {
                    None
                }
            });

            if let Err(e) = self.write_metadata(folder, &metadata) {
                error!("❌ Failed to update metadata to completed: {}", e);
                return Err(format!("Failed to update metadata: {}", e));
            }

            info!(
                "✅ Metadata updated with duration: {:?}s",
                metadata.duration_seconds
            );
        }

        // Emit save event with audio and transcript paths
        let save_event = serde_json::json!({
            "audio_file": final_audio_path.to_string_lossy(),
            "transcript_file": self.meeting_folder.as_ref()
                .map(|f| f.join("transcripts.json").to_string_lossy().to_string()),
            "meeting_name": self.meeting_name,
            "meeting_folder": self.meeting_folder.as_ref()
                .map(|f| f.to_string_lossy().to_string())
        });

        if let Err(e) = app.emit("recording-saved", &save_event) {
            warn!("Failed to emit recording-saved event: {}", e);
        }

        Ok(Some(final_audio_path.to_string_lossy().to_string()))
    }

    /// Get the meeting folder path (for passing to backend)
    pub fn get_meeting_folder(&self) -> Option<&PathBuf> {
        self.meeting_folder.as_ref()
    }

    /// Get accumulated transcript segments (for reload sync)
    pub fn get_transcript_segments(&self) -> Vec<TranscriptSegment> {
        if let Ok(segments) = self.transcript_segments.lock() {
            segments.clone()
        } else {
            Vec::new()
        }
    }

    /// Apply a user text edit during recording; marks segment as user-edited and rewrites transcripts.json.
    pub fn update_live_transcript_text(
        &self,
        sequence_id: u64,
        new_text: String,
    ) -> Result<(), String> {
        let trimmed = new_text.trim().to_string();
        if trimmed.is_empty() {
            return Err("Nội dung không được để trống".to_string());
        }
        {
            let mut segments = self
                .transcript_segments
                .lock()
                .map_err(|_| "Khóa transcript bị lỗi".to_string())?;
            let seg = segments
                .iter_mut()
                .find(|s| s.sequence_id == sequence_id)
                .ok_or_else(|| format!("Không có đoạn sequence_id={}", sequence_id))?;
            seg.text = trimmed;
            seg.user_edited = true;
        }
        if let Some(folder) = &self.meeting_folder {
            self.write_transcripts_json(folder)
                .map_err(|e| format!("Ghi transcripts.json thất bại: {}", e))?;
        }
        Ok(())
    }

    /// Get meeting name (for reload sync)
    pub fn get_meeting_name(&self) -> Option<String> {
        self.meeting_name.clone()
    }
}

fn format_mmss(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("[{:02}:{:02}]", total / 60, total % 60)
}

impl Default for RecordingSaver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn audio_saver_drains_queued_chunks_before_finishing() {
        let mut saver = RecordingSaver::new();
        let temp = tempfile::tempdir().unwrap();
        saver.set_save_folder(temp.path().to_path_buf());
        saver.set_meeting_name(Some("Drain test".into()));
        let sender = saver.start_accumulation(true);
        for i in 0..100 {
            sender
                .send(AudioChunk {
                    data: vec![0.0; 960],
                    sample_rate: 48000,
                    timestamp: i as f64 * 0.02,
                    chunk_id: i,
                    device_type: super::super::recording_state::DeviceType::Microphone,
                })
                .await
                .unwrap();
        }
        drop(sender);
        saver
            .accumulation_task
            .take()
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            saver
                .incremental_saver
                .as_ref()
                .unwrap()
                .lock()
                .await
                .buffered_samples(),
            100 * 960
        );
    }

    #[test]
    fn transcript_writer_remains_valid_when_manager_is_taken_for_shutdown() {
        let saver = RecordingSaver::new();
        let writer = saver.transcript_writer();
        writer.record_segment(seg(1, "before stop")).unwrap();
        writer.record_segment(seg(2, "last ASR result")).unwrap();
        let segments = saver.get_transcript_segments();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[1].text, "last ASR result");
    }

    #[test]
    fn final_snapshot_preserves_final_text_and_user_corrections() {
        let temp = tempfile::tempdir().unwrap();
        let mut saver = RecordingSaver::new();
        saver.set_save_folder(temp.path().to_path_buf());
        saver.initialize_meeting_folder("Snapshot", false).unwrap();
        let writer = saver.transcript_writer();
        let mut partial = seg(1, "partial");
        partial.is_partial = true;
        writer.record_segment(partial.clone()).unwrap();
        writer.record_segment(seg(1, "final")).unwrap();
        writer.record_segment(partial).unwrap();
        assert_eq!(saver.get_transcript_segments()[0].text, "final");
        saver
            .update_live_transcript_text(1, "user correction".into())
            .unwrap();
        writer.record_segment(seg(1, "ASR overwrite")).unwrap();
        let folder = saver.meeting_folder.as_ref().unwrap();
        writer.write_snapshot(folder, true).unwrap();
        let snapshot: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.join("transcripts.json")).unwrap())
                .unwrap();
        assert_eq!(snapshot["segments"][0]["text"], "user correction");
        assert_eq!(snapshot["segments"][0]["is_partial"], false);
    }

    fn seg(sequence_id: u64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            id: format!("seg_{}", sequence_id),
            text: text.to_string(),
            audio_start_time: sequence_id as f64,
            audio_end_time: sequence_id as f64 + 1.0,
            duration: 1.0,
            display_time: "[00:00]".to_string(),
            confidence: 0.9,
            sequence_id,
            user_edited: false,
            is_partial: false,
            speaker_name: None,
        }
    }

    #[test]
    fn replace_merges_matched_segments_into_one_in_order() {
        let saver = RecordingSaver::new();
        saver.add_transcript_segment(seg(0, "xin"));
        saver.add_transcript_segment(seg(1, "chao"));
        saver.add_transcript_segment(seg(2, "ban"));

        saver.replace_transcript_segments(&[0, 1], "Xin chào.".to_string(), 0.0, 2.0);

        let segments = saver.get_transcript_segments();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Xin chào.");
        assert_eq!(segments[0].sequence_id, 0);
        assert_eq!(segments[0].audio_start_time, 0.0);
        assert_eq!(segments[0].audio_end_time, 2.0);
        assert_eq!(segments[1].text, "ban");
    }

    #[test]
    fn replace_is_noop_when_no_source_ids_match() {
        let saver = RecordingSaver::new();
        saver.add_transcript_segment(seg(0, "xin"));

        saver.replace_transcript_segments(&[99], "ignored".to_string(), 0.0, 1.0);

        let segments = saver.get_transcript_segments();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "xin");
    }

    #[test]
    fn replace_skips_batch_containing_a_user_edited_segment() {
        let saver = RecordingSaver::new();
        saver.add_transcript_segment(seg(0, "xin"));
        saver.add_transcript_segment(seg(1, "chao"));
        saver
            .update_live_transcript_text(1, "Chào (đã sửa)".to_string())
            .unwrap();

        saver.replace_transcript_segments(&[0, 1], "Xin chào.".to_string(), 0.0, 2.0);

        let segments = saver.get_transcript_segments();
        assert_eq!(
            segments.len(),
            2,
            "user-edited segment must not be clobbered"
        );
        assert_eq!(segments[1].text, "Chào (đã sửa)");
    }

    #[test]
    fn apply_live_capu_results_expands_sentences_and_keeps_user_edited() {
        use crate::capu_engine::batch::FinalizedSegment;

        let saver = RecordingSaver::new();
        saver.add_transcript_segment(seg(0, "xin"));
        saver.add_transcript_segment(seg(1, "chao"));
        saver.add_transcript_segment(seg(2, "keep"));
        saver
            .update_live_transcript_text(2, "Giữ nguyên".to_string())
            .unwrap();

        saver.apply_live_capu_results(&[
            FinalizedSegment {
                text: "Xin chào.".to_string(),
                audio_start_time: 0.0,
                audio_end_time: 1.0,
                source_ids: vec![0, 1],
            },
            FinalizedSegment {
                text: "Các bạn.".to_string(),
                audio_start_time: 1.0,
                audio_end_time: 2.0,
                source_ids: vec![0, 1],
            },
        ]);

        let segments = saver.get_transcript_segments();
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "Xin chào.");
        assert_eq!(segments[1].text, "Các bạn.");
        assert!(segments
            .iter()
            .any(|s| s.user_edited && s.text == "Giữ nguyên"));
        assert!(segments[0].audio_start_time <= segments[1].audio_start_time);
    }
}
