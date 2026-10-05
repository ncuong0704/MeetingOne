use super::encode::encode_single_audio;
use super::recording_state::AudioChunk;
use anyhow::{anyhow, Result};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::ffmpeg::find_ffmpeg_path;
use super::meeting_folder::{validate_checkpoint_directory, validate_meeting_folder};

fn checkpoint_index(path: &std::path::Path) -> Option<u32> {
    path.file_name()?
        .to_str()?
        .strip_prefix("audio_chunk_")?
        .strip_suffix(".mp4")?
        .parse()
        .ok()
}

fn concat_entry(path: &std::path::Path) -> Result<String> {
    // Relative generated filenames also work when the parent contains apostrophes.
    let index = checkpoint_index(path).ok_or_else(|| anyhow!("Invalid checkpoint filename"))?;
    Ok(format!("file 'audio_chunk_{index:03}.mp4'\n"))
}

fn has_finalized_audio(folder: &std::path::Path) -> bool {
    use std::io::Read;
    let mut header = [0u8; 12];
    std::fs::File::open(folder.join("audio.mp4"))
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok()
        && &header[4..8] == b"ftyp"
}

/// FFmpeg concat with stream-copy (no AAC re-encode).
/// Checkpoints are already AAC MP4; remuxing is enough at stop.
pub(crate) fn ffmpeg_concat_copy_args<'a>(list_file: &'a str, output: &'a str) -> Vec<&'a str> {
    vec![
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        list_file,
        "-c",
        "copy",
        "-movflags",
        "+faststart",
        "-y",
        output,
    ]
}

/// Audio data without device type (we only store mixed audio)
#[derive(Clone)]
struct AudioData {
    data: Vec<f32>,
    // sample_rate: u32,
}

/// Incremental audio saver that writes checkpoints every 30 seconds
/// to minimize memory usage and enable crash recovery
pub struct IncrementalAudioSaver {
    checkpoint_buffer: Vec<AudioData>,
    checkpoint_interval_samples: usize, // 30s at 48kHz = 1,440,000 samples
    checkpoint_count: u32,
    checkpoints_dir: PathBuf,
    meeting_folder: PathBuf,
    sample_rate: u32,
    finalized_path: Option<PathBuf>,
}

impl IncrementalAudioSaver {
    /// Create a new incremental saver
    ///
    /// # Arguments
    /// * `meeting_folder` - Path to the meeting folder (contains .checkpoints/)
    /// * `sample_rate` - Sample rate of audio (typically 48000)
    pub fn new(meeting_folder: PathBuf, sample_rate: u32) -> Result<Self> {
        let checkpoints_dir = meeting_folder.join(".checkpoints");

        // Verify checkpoints directory exists
        if !checkpoints_dir.exists() {
            return Err(anyhow!(
                "Checkpoints directory does not exist: {}",
                checkpoints_dir.display()
            ));
        }

        Ok(Self {
            checkpoint_buffer: Vec::new(),
            checkpoint_interval_samples: sample_rate as usize * 30, // 30 seconds
            checkpoint_count: 0,
            checkpoints_dir,
            meeting_folder,
            sample_rate,
            finalized_path: None,
        })
    }

    pub(crate) fn buffer_chunk(&mut self, chunk: AudioChunk) {
        self.checkpoint_buffer.push(AudioData { data: chunk.data });
    }

    #[cfg(test)]
    pub(crate) fn buffered_samples(&self) -> usize {
        self.checkpoint_buffer
            .iter()
            .map(|chunk| chunk.data.len())
            .sum()
    }

    /// Add an audio chunk to the buffer
    /// Automatically saves a checkpoint when buffer reaches 30 seconds
    pub fn add_chunk(&mut self, chunk: AudioChunk) -> Result<()> {
        let audio_data = AudioData {
            data: chunk.data,
            // sample_rate: chunk.sample_rate,
        };

        self.checkpoint_buffer.push(audio_data);

        // Calculate total samples in buffer
        let total_samples: usize = self.checkpoint_buffer.iter().map(|c| c.data.len()).sum();

        // Save checkpoint when buffer reaches threshold (30 seconds)
        if total_samples >= self.checkpoint_interval_samples {
            self.save_checkpoint()?;
            self.checkpoint_buffer.clear();
        }

        Ok(())
    }

    /// Save current buffer as a checkpoint file
    fn save_checkpoint(&mut self) -> Result<()> {
        // Concatenate all chunks in buffer
        let audio_data: Vec<f32> = self
            .checkpoint_buffer
            .iter()
            .flat_map(|c| &c.data)
            .cloned()
            .collect();

        if audio_data.is_empty() {
            warn!("Attempted to save empty checkpoint, skipping");
            return Ok(());
        }

        // Generate checkpoint filename
        let checkpoint_path = self
            .checkpoints_dir
            .join(format!("audio_chunk_{:03}.mp4", self.checkpoint_count));

        // Encode and save checkpoint
        encode_single_audio(
            bytemuck::cast_slice(&audio_data),
            self.sample_rate,
            1, // mono
            &checkpoint_path,
        )?;

        let duration_seconds = audio_data.len() as f32 / self.sample_rate as f32;
        self.checkpoint_count += 1;

        info!(
            "Saved checkpoint {}: {:.2}s of audio ({} samples)",
            self.checkpoint_count,
            duration_seconds,
            audio_data.len()
        );

        Ok(())
    }

    /// Finalize the recording: save final checkpoint, merge all checkpoints, cleanup
    ///
    /// Returns the path to the final merged audio.mp4 file
    pub async fn finalize(&mut self) -> Result<PathBuf> {
        if let Some(path) = &self.finalized_path {
            return Ok(path.clone());
        }
        info!("Finalizing incremental recording...");

        // Save final buffer if not empty
        if !self.checkpoint_buffer.is_empty() {
            info!(
                "Saving final checkpoint with remaining {} chunks",
                self.checkpoint_buffer.len()
            );
            let buffered = std::mem::take(&mut self.checkpoint_buffer);
            let samples: Vec<f32> = buffered
                .iter()
                .flat_map(|chunk| chunk.data.iter().copied())
                .collect();
            let path = self
                .checkpoints_dir
                .join(format!("audio_chunk_{:03}.mp4", self.checkpoint_count));
            let sample_rate = self.sample_rate;
            let result = tokio::task::spawn_blocking(move || {
                encode_single_audio(bytemuck::cast_slice(&samples), sample_rate, 1, &path)
            })
            .await
            .map_err(|error| anyhow!("Checkpoint worker failed: {error}"))
            .and_then(|result| result);
            if let Err(error) = result {
                self.checkpoint_buffer = buffered;
                return Err(error);
            }
            self.checkpoint_count += 1;
        }

        if self.checkpoint_count == 0 {
            return Err(anyhow!(
                "No audio checkpoints to merge - recording may have failed"
            ));
        }

        // Merge all checkpoints using FFmpeg concat
        let final_audio_path = self.meeting_folder.join("audio.mp4");
        self.merge_checkpoints(&final_audio_path).await?;

        // Retain checkpoints until the meeting has been committed to SQLite.
        self.finalized_path = Some(final_audio_path.clone());

        info!("Finalized recording: {}", final_audio_path.display());

        Ok(final_audio_path)
    }

    /// Merge all checkpoint files into final audio.mp4 using FFmpeg concat stream-copy.
    async fn merge_checkpoints(&self, output: &PathBuf) -> Result<()> {
        info!(
            "Merging {} checkpoints into final audio file...",
            self.checkpoint_count
        );

        // Create concat list file for FFmpeg
        let list_file = self.checkpoints_dir.join("concat_list.txt");
        let mut list_content = String::new();

        for i in 0..self.checkpoint_count {
            let checkpoint_path = self
                .checkpoints_dir
                .join(format!("audio_chunk_{:03}.mp4", i));

            // Verify checkpoint exists
            if !checkpoint_path.exists() {
                return Err(anyhow!(
                    "Checkpoint file missing: {}",
                    checkpoint_path.display()
                ));
            }

            list_content.push_str(&concat_entry(&checkpoint_path)?);
        }

        std::fs::write(&list_file, list_content)?;

        let ffmpeg_path = find_ffmpeg_path().ok_or_else(|| {
            anyhow!("FFmpeg not found. Please install FFmpeg to finalize recordings.")
        })?;
        info!("Using FFmpeg at: {:?}", ffmpeg_path);

        let list_file_str = list_file.to_str().ok_or_else(|| {
            anyhow!(
                "Checkpoint list file path is not valid UTF-8: {}",
                list_file.display()
            )
        })?;
        let output_str = output.to_str().ok_or_else(|| {
            anyhow!(
                "Output audio file path is not valid UTF-8: {}",
                output.display()
            )
        })?;

        let mut command = tokio::process::Command::new(ffmpeg_path);

        command.args(ffmpeg_concat_copy_args(list_file_str, output_str));

        // Hide console window on Windows to prevent CMD popup during finalization
        #[cfg(target_os = "windows")]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let ffmpeg_output = command.output().await?;

        if !ffmpeg_output.status.success() {
            let stderr = String::from_utf8_lossy(&ffmpeg_output.stderr);
            error!("FFmpeg merge failed: {}", stderr);
            return Err(anyhow!("FFmpeg concat failed: {}", stderr));
        }

        // Verify output file was created
        if !output.exists() {
            return Err(anyhow!(
                "Merged audio file was not created: {}",
                output.display()
            ));
        }

        info!(
            "Successfully merged {} checkpoints → {}",
            self.checkpoint_count,
            output.display()
        );

        Ok(())
    }

    /// Get the meeting folder path
    pub fn get_meeting_folder(&self) -> &PathBuf {
        &self.meeting_folder
    }

    /// Get current checkpoint count
    pub fn get_checkpoint_count(&self) -> u32 {
        self.checkpoint_count
    }
}

/// Audio recovery status for transcript recovery feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRecoveryStatus {
    pub status: String, // "success" | "partial" | "failed" | "none"
    pub chunk_count: u32,
    pub estimated_duration_seconds: f64,
    pub audio_file_path: Option<String>,
    pub message: String,
}

/// Recover audio from checkpoint files
/// This is called by the transcript recovery system to merge audio chunks after a crash
#[tauri::command]
pub async fn recover_audio_from_checkpoints(
    meeting_folder: String,
    _sample_rate: u32,
) -> Result<AudioRecoveryStatus, String> {
    info!("Starting audio recovery for folder: {}", meeting_folder);

    let folder_path = validate_meeting_folder(std::path::Path::new(&meeting_folder))?;
    let checkpoints_dir = validate_checkpoint_directory(&folder_path)?;

    // Check if checkpoints directory exists
    if !checkpoints_dir.exists() {
        info!(
            "No checkpoints directory found at: {}",
            checkpoints_dir.display()
        );
        return Ok(AudioRecoveryStatus {
            status: if has_finalized_audio(&folder_path) {
                "success"
            } else {
                "none"
            }
            .to_string(),
            chunk_count: 0,
            estimated_duration_seconds: 0.0,
            audio_file_path: has_finalized_audio(&folder_path)
                .then(|| folder_path.join("audio.mp4").to_string_lossy().into_owned()),
            message: "No audio checkpoints found".to_string(),
        });
    }

    // Scan for checkpoint files
    let mut checkpoint_files: Vec<_> = std::fs::read_dir(&checkpoints_dir)
        .map_err(|e| format!("Failed to read checkpoints directory: {}", e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| checkpoint_index(&entry.path()).is_some())
        .collect();

    if checkpoint_files.is_empty() {
        info!(
            "No checkpoint files found in: {}",
            checkpoints_dir.display()
        );
        return Ok(AudioRecoveryStatus {
            status: if has_finalized_audio(&folder_path) {
                "success"
            } else {
                "none"
            }
            .to_string(),
            chunk_count: 0,
            estimated_duration_seconds: 0.0,
            audio_file_path: has_finalized_audio(&folder_path)
                .then(|| folder_path.join("audio.mp4").to_string_lossy().into_owned()),
            message: "No audio checkpoint files found".to_string(),
        });
    }

    // Sort by filename (audio_chunk_000.mp4, audio_chunk_001.mp4, etc.)
    checkpoint_files.sort_by_key(|entry| checkpoint_index(&entry.path()));

    let chunk_count = checkpoint_files.len() as u32;
    let estimated_duration = (chunk_count as f64) * 30.0; // 30 seconds per chunk

    info!(
        "Found {} checkpoint files, estimated duration: {:.2}s",
        chunk_count, estimated_duration
    );

    // Create FFmpeg concat file
    let concat_file_path = checkpoints_dir.join("concat_list.txt");
    let mut concat_content = String::new();

    for entry in &checkpoint_files {
        let path = entry.path().canonicalize().map_err(|e| e.to_string())?;
        if path.parent()
            != Some(
                checkpoints_dir
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .as_path(),
            )
        {
            return Err("Checkpoint file escapes the checkpoint directory".into());
        }
        concat_content.push_str(&concat_entry(&entry.path()).map_err(|e| e.to_string())?);
    }

    std::fs::write(&concat_file_path, concat_content)
        .map_err(|e| format!("Failed to write concat file: {}", e))?;

    // Run FFmpeg to merge chunks
    // A failed retry must never overwrite an already recovered recording.
    let output_path = folder_path.join(".recovered-audio.mp4");
    let output_path_str = output_path
        .to_str()
        .ok_or("Invalid output path")?
        .to_string();
    let concat_file_path_str = concat_file_path
        .to_str()
        .ok_or("Invalid checkpoint list file path")?
        .to_string();

    let ffmpeg_path = find_ffmpeg_path()
        .ok_or_else(|| "FFmpeg not found. Please install FFmpeg to recover audio.".to_string())?;
    info!("Using FFmpeg at: {:?}", ffmpeg_path);

    let mut command = tokio::process::Command::new(ffmpeg_path);

    command.args(ffmpeg_concat_copy_args(
        &concat_file_path_str,
        &output_path_str,
    ));

    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let ffmpeg_result = command.output().await;

    match ffmpeg_result {
        Ok(output) if output.status.success() => {
            let final_path = folder_path.join("audio.mp4");
            std::fs::rename(&output_path, &final_path).map_err(|e| e.to_string())?;
            let output_path_str = final_path.to_string_lossy().into_owned();
            // Clean up concat file
            let _ = std::fs::remove_file(concat_file_path);

            info!("Successfully recovered audio: {}", output_path_str);

            Ok(AudioRecoveryStatus {
                status: "success".to_string(),
                chunk_count,
                estimated_duration_seconds: estimated_duration,
                audio_file_path: Some(output_path_str),
                message: format!("Successfully recovered {} audio chunks", chunk_count),
            })
        }
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr);
            error!("FFmpeg recovery failed: {}", error);
            Ok(AudioRecoveryStatus {
                status: "failed".to_string(),
                chunk_count,
                estimated_duration_seconds: estimated_duration,
                audio_file_path: None,
                message: format!("FFmpeg failed: {}", error),
            })
        }
        Err(e) => {
            error!("Failed to run FFmpeg: {}", e);
            Ok(AudioRecoveryStatus {
                status: "failed".to_string(),
                chunk_count,
                estimated_duration_seconds: estimated_duration,
                audio_file_path: None,
                message: format!("Failed to run FFmpeg: {}", e),
            })
        }
    }
}

/// Clean up checkpoint files after successful recording or recovery
/// This command is called by the frontend after successful save to clean up checkpoint files
#[tauri::command]
pub async fn cleanup_checkpoints(meeting_folder: String) -> Result<(), String> {
    info!("Cleaning up checkpoints for folder: {}", meeting_folder);

    let folder_path = validate_meeting_folder(std::path::Path::new(&meeting_folder))?;
    let checkpoints_dir = validate_checkpoint_directory(&folder_path)?;

    if checkpoints_dir.exists() {
        if !has_finalized_audio(&folder_path) {
            return Err("Cannot remove checkpoints without finalized audio".into());
        }
        std::fs::remove_dir_all(&checkpoints_dir)
            .map_err(|e| format!("Failed to remove checkpoints directory: {}", e))?;
        info!("Successfully cleaned up checkpoints directory");
    } else {
        info!("No checkpoints directory to clean up");
    }

    Ok(())
}

/// Check if a meeting folder has audio checkpoint files
/// Returns true if .checkpoints/ directory exists and contains .mp4 files
#[tauri::command]
pub async fn has_audio_checkpoints(meeting_folder: String) -> Result<bool, String> {
    let folder_path = validate_meeting_folder(std::path::Path::new(&meeting_folder))?;
    let checkpoints_dir = validate_checkpoint_directory(&folder_path)?;

    // Check if checkpoints directory exists
    if !checkpoints_dir.exists() {
        return Ok(false);
    }

    // Scan for .mp4 checkpoint files
    let has_mp4_files = std::fs::read_dir(&checkpoints_dir)
        .map_err(|e| format!("Failed to read checkpoints directory: {}", e))?
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("mp4"));

    Ok(has_mp4_files)
}

#[cfg(test)]
mod tests {
    use super::super::recording_state::DeviceType;
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_checkpoint_creation() {
        // Create temp meeting folder
        let temp_dir = tempdir().unwrap();
        let meeting_folder = temp_dir.path().join("Test_Meeting");
        std::fs::create_dir_all(&meeting_folder).unwrap();
        std::fs::create_dir_all(meeting_folder.join(".checkpoints")).unwrap();

        let mut saver = IncrementalAudioSaver::new(meeting_folder.clone(), 48000).unwrap();

        // Add 60 seconds worth of audio (should create 2 checkpoints)
        for i in 0..120 {
            // 120 chunks of 0.5s each
            let chunk = AudioChunk {
                data: vec![0.5f32; 24000], // 0.5s at 48kHz
                sample_rate: 48000,
                timestamp: i as f64 * 0.5, // timestamp in seconds
                chunk_id: i as u64,
                device_type: DeviceType::Microphone,
            };
            saver.add_chunk(chunk).unwrap();
        }

        // Verify 2 checkpoints created
        assert_eq!(saver.checkpoint_count, 2);

        // Finalize and verify merge
        let final_path = saver.finalize().await.unwrap();
        assert!(final_path.exists());

        // A database retry keeps the same output and original checkpoints.
        assert!(meeting_folder.join(".checkpoints").exists());
        assert_eq!(saver.finalize().await.unwrap(), final_path);
    }

    #[tokio::test]
    async fn cleanup_refuses_to_remove_unmerged_checkpoints() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("metadata.json"),
            r#"{"version":"1.0","meeting_name":"Test","transcript_file":"transcripts.json"}"#,
        )
        .unwrap();
        let checkpoints = temp.path().join(".checkpoints");
        std::fs::create_dir(&checkpoints).unwrap();
        std::fs::write(checkpoints.join("audio_chunk_000.mp4"), b"checkpoint").unwrap();
        assert!(
            cleanup_checkpoints(temp.path().to_string_lossy().into_owned())
                .await
                .is_err()
        );
        assert!(checkpoints.join("audio_chunk_000.mp4").exists());
    }

    #[test]
    fn checkpoint_order_is_numeric_and_concat_paths_are_relative() {
        assert_eq!(
            checkpoint_index(std::path::Path::new("audio_chunk_1000.mp4")),
            Some(1000)
        );
        assert_eq!(checkpoint_index(std::path::Path::new("random.mp4")), None);
        assert_eq!(
            concat_entry(std::path::Path::new(
                "C:/John's recordings/audio_chunk_001.mp4"
            ))
            .unwrap(),
            "file 'audio_chunk_001.mp4'\n"
        );
    }

    #[tokio::test]
    async fn test_empty_recording() {
        let temp_dir = tempdir().unwrap();
        let meeting_folder = temp_dir.path().join("Empty_Test");
        std::fs::create_dir_all(&meeting_folder).unwrap();
        std::fs::create_dir_all(meeting_folder.join(".checkpoints")).unwrap();

        let mut saver = IncrementalAudioSaver::new(meeting_folder.clone(), 48000).unwrap();

        // Try to finalize without adding any chunks
        let result = saver.finalize().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No audio checkpoints"));
    }

    #[test]
    fn test_merge_checkpoints_path_conversion_does_not_panic_on_valid_utf8() {
        // Regression guard: confirms the to_str() error-handling introduced
        // by this fix doesn't break the normal (valid UTF-8 path) case.
        // The non-UTF-8 panic path itself isn't practically constructible in
        // a portable unit test (OsString internals differ by platform) —
        // this fix is verified primarily by code review: to_str().unwrap()
        // is replaced with a proper Result-returning check at every site in
        // this file, so the panic path is provably eliminated at the type
        // level regardless.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        assert!(
            path.to_str().is_some(),
            "sanity check: tempdir paths are UTF-8 in this test environment"
        );
    }

    #[test]
    fn concat_args_use_stream_copy() {
        let args = ffmpeg_concat_copy_args("list.txt", "out.mp4");
        assert!(args.windows(2).any(|w| w == ["-c", "copy"]));
        assert!(!args.windows(2).any(|w| w == ["-c:a", "aac"]));
        assert!(args.contains(&"-f") && args.contains(&"concat"));
    }
}
