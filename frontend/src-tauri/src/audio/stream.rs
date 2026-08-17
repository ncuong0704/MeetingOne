use std::sync::Arc;
use anyhow::Result;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, Stream, SupportedStreamConfig};
use log::{error, info, warn};
use tokio::sync::mpsc;

use super::devices::{AudioDevice, get_device_and_config};
use super::pipeline::AudioCapture;
use super::recording_state::{RecordingState, DeviceType};
use super::capture::{AudioCaptureBackend, get_current_backend};

#[cfg(target_os = "macos")]
use super::capture::CoreAudioCapture;

/// Stream backend implementation
pub enum StreamBackend {
    /// CPAL-based stream (ScreenCaptureKit or default)
    Cpal(Stream),
    /// Core Audio direct implementation (macOS only)
    #[cfg(target_os = "macos")]
    CoreAudio {
        task: Option<tokio::task::JoinHandle<()>>,
    },
}

// SAFETY: While Stream doesn't implement Send, we ensure it's only accessed
// from the same thread context by using spawn_blocking for operations that cross thread boundaries
unsafe impl Send for StreamBackend {}

/// Wraps a `cpal::Stream` (which is `!Send` on most platforms due to real
/// thread-affinity requirements, e.g. WASAPI COM objects on Windows) so it
/// can be moved into a `tokio::task::spawn_blocking` closure.
///
/// SAFETY: sound only because every use of this type is a single, sequential
/// ownership handoff across exactly one `spawn_blocking` boundary — the
/// value is wrapped immediately before the call, unwrapped via `into_inner()`
/// as the very first line inside the closure, and never observed from more
/// than one thread at a time. `JoinHandle::await` establishes the
/// happens-before edge that makes this safe. This type is deliberately
/// NOT generic — do not repurpose it for any other type without
/// re-justifying this exact invariant for that type; a type with interior
/// mutability accessed concurrently (rather than handed off sequentially)
/// would NOT be safe to wrap this way.
///
/// This does NOT guarantee the same OS thread is used across separate
/// `spawn_blocking` calls (e.g. `play()` vs. the later `pause()`/`drop()`),
/// which is why this remains a partial mitigation for cpal's real
/// thread-affinity requirements rather than a complete fix.
struct AssertSend(Stream);
unsafe impl Send for AssertSend {}

impl AssertSend {
    /// Extract the wrapped value. Deliberately a method (rather than
    /// destructuring `AssertSend(x) = wrapped` at the call site) so that
    /// Rust 2021's disjoint closure capture can't see through to the inner
    /// field and capture it directly — a direct destructure inside a
    /// `spawn_blocking` closure body would make the closure capture just the
    /// (non-Send) inner field instead of this (Send) wrapper, defeating the
    /// whole point of the wrapper.
    fn into_inner(self) -> Stream {
        self.0
    }
}

/// Simplified audio stream wrapper with multi-backend support
pub struct AudioStream {
    device: Arc<AudioDevice>,
    backend: StreamBackend,
}

// SAFETY: AudioStream contains StreamBackend which we've marked as Send
unsafe impl Send for AudioStream {}

impl AudioStream {
    /// Create a new audio stream for the given device
    pub async fn create(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<Self> {
        // Get current backend from global config
        let backend_type = get_current_backend();
        Self::create_with_backend(device, state, device_type, recording_sender, backend_type).await
    }

    /// Create a new audio stream with explicit backend selection
    pub async fn create_with_backend(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
        backend_type: AudioCaptureBackend,
    ) -> Result<Self> {
        info!("🎵 Stream: Creating audio stream for device: {} with backend: {:?}, device_type: {:?}",
              device.name, backend_type, device_type);

        // For system audio devices, use the selected backend
        // For microphone devices, always use CPAL
        #[cfg(target_os = "macos")]
        let use_core_audio = device_type == DeviceType::System
            && backend_type == AudioCaptureBackend::CoreAudio;

        #[cfg(not(target_os = "macos"))]
        let use_core_audio = false;

        #[cfg(target_os = "macos")]
        info!("🎵 Stream: use_core_audio = {}, device_type == System: {}, backend == CoreAudio: {}",
              use_core_audio,
              device_type == DeviceType::System,
              backend_type == AudioCaptureBackend::CoreAudio);

        #[cfg(not(target_os = "macos"))]
        info!("🎵 Stream: use_core_audio = {}, device_type == System: {}",
              use_core_audio,
              device_type == DeviceType::System);

        #[cfg(target_os = "macos")]
        if use_core_audio {
            info!("🎵 Stream: Using Core Audio backend (cidre) for system audio");
            return Self::create_core_audio_stream(device, state, device_type, recording_sender).await;
        }

        // Default path: use CPAL
        #[cfg(target_os = "macos")]
        let backend_name = if backend_type == AudioCaptureBackend::ScreenCaptureKit {
            "ScreenCaptureKit"
        } else {
            "CPAL (default)"
        };

        #[cfg(not(target_os = "macos"))]
        let backend_name = "CPAL";

        info!("🎵 Stream: Using CPAL backend ({}) for device: {}", backend_name, device.name);
        Self::create_cpal_stream(device, state, device_type, recording_sender).await
    }

    /// Create a CPAL-based stream (ScreenCaptureKit on macOS)
    async fn create_cpal_stream(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<Self> {
        info!("Creating CPAL stream for device: {}", device.name);

        // Get the underlying cpal device and config
        let (cpal_device, config) = get_device_and_config(&device).await?;

        info!("Audio config - Sample rate: {}, Channels: {}, Format: {:?}",
              config.sample_rate().0, config.channels(), config.sample_format());

        // Create audio capture processor
        let capture = AudioCapture::new(
            device.clone(),
            state.clone(),
            config.sample_rate().0,
            config.channels(),
            device_type,
            recording_sender,
        );

        // Build the appropriate stream based on sample format
        let stream = Self::build_stream(&cpal_device, &config, capture.clone())?;

        // Start the stream on a blocking thread. cpal's Stream has real
        // thread-affinity requirements on some backends (e.g. WASAPI COM
        // objects on Windows) — spawn_blocking keeps this off the shared
        // tokio async worker pool. This doesn't guarantee the same OS thread
        // handles both play() and the later pause()/drop() in `stop()`
        // below — a full fix requires owning the stream on one dedicated
        // thread for its whole lifecycle, which is a larger follow-up.
        //
        // `Stream` itself is `!Send` (see `AssertSend` above), so it must be
        // wrapped before it can be moved into the spawn_blocking closure.
        let wrapped = AssertSend(stream);
        let stream = tokio::task::spawn_blocking(move || -> Result<AssertSend> {
            let stream = wrapped.into_inner();
            stream.play()?;
            Ok(AssertSend(stream))
        })
        .await
        .map_err(|e| anyhow::anyhow!("Stream play task panicked: {}", e))??
        .into_inner();
        info!("CPAL stream started for device: {}", device.name);

        Ok(Self {
            device,
            backend: StreamBackend::Cpal(stream),
        })
    }

    /// Create a Core Audio stream (macOS only)
    #[cfg(target_os = "macos")]
    async fn create_core_audio_stream(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<Self> {
        info!("🔊 Stream: Creating Core Audio stream for device: {}", device.name);

        // Create Core Audio capture
        info!("🔊 Stream: Calling CoreAudioCapture::new()...");
        let capture_impl = CoreAudioCapture::new()
            .map_err(|e| {
                error!("❌ Stream: CoreAudioCapture::new() failed: {}", e);
                anyhow::anyhow!("Failed to create Core Audio capture: {}", e)
            })?;

        info!("✅ Stream: CoreAudioCapture created, calling stream()...");
        let core_stream = capture_impl.stream()
            .map_err(|e| {
                error!("❌ Stream: capture_impl.stream() failed: {}", e);
                anyhow::anyhow!("Failed to create Core Audio stream: {}", e)
            })?;

        let sample_rate = core_stream.sample_rate();
        info!("✅ Stream: Core Audio stream created with sample rate: {} Hz", sample_rate);

        // Create audio capture processor for pipeline integration
        // CRITICAL: Core Audio tap is MONO (with_mono_global_tap_excluding_processes)
        let capture = AudioCapture::new(
            device.clone(),
            state.clone(),
            sample_rate,
            1, // Core Audio tap is MONO (not stereo!)
            device_type,
            recording_sender,
        );

        // Spawn task to process Core Audio stream samples
        // The stream needs to be polled continuously to produce samples
        let device_name = device.name.clone();
        info!("🔊 Stream: Spawning tokio task to poll Core Audio stream...");
        let task = tokio::spawn({
            let capture = capture.clone();
            let mut stream = core_stream;

            async move {
                use futures_util::StreamExt;

                let mut buffer = Vec::new();
                let mut frame_count = 0;
                let frames_per_chunk = 1024; // Process in chunks of 1024 samples

                info!("✅ Stream: Core Audio processing task started for {}", device_name);

                let mut _sample_count = 0u64;
                while let Some(sample) = stream.next().await {
                    _sample_count += 1;
                    // if _sample_count % 48000 == 0 {
                    //     info!("📊 Stream: Received {} samples from Core Audio stream", _sample_count);
                    // }

                    buffer.push(sample);
                    frame_count += 1;

                    // Process when we have enough samples
                    if frame_count >= frames_per_chunk {
                        capture.process_audio_data(&buffer);
                        buffer.clear();
                        frame_count = 0;
                    }
                }

                // Process any remaining samples
                if !buffer.is_empty() {
                    capture.process_audio_data(&buffer);
                }

                info!("⚠️ Stream: Core Audio processing task ended for {}", device_name);
            }
        });

        info!("✅ Stream: Core Audio stream fully initialized for device: {}", device.name);

        Ok(Self {
            device: device.clone(),
            backend: StreamBackend::CoreAudio {
                task: Some(task),
            },
        })
    }

    /// Build stream based on sample format
    fn build_stream(
        device: &Device,
        config: &SupportedStreamConfig,
        capture: AudioCapture,
    ) -> Result<Stream> {
        let config_copy = config.clone();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let capture_clone = capture.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        capture.process_audio_data(data);
                    },
                    move |err| {
                        capture_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let capture_clone = capture.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data.iter()
                            .map(|&sample| sample as f32 / i16::MAX as f32)
                            .collect();
                        capture.process_audio_data(&f32_data);
                    },
                    move |err| {
                        capture_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            cpal::SampleFormat::I32 => {
                let capture_clone = capture.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[i32], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data.iter()
                            .map(|&sample| sample as f32 / i32::MAX as f32)
                            .collect();
                        capture.process_audio_data(&f32_data);
                    },
                    move |err| {
                        capture_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            cpal::SampleFormat::I8 => {
                let capture_clone = capture.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[i8], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data.iter()
                            .map(|&sample| sample as f32 / i8::MAX as f32)
                            .collect();
                        capture.process_audio_data(&f32_data);
                    },
                    move |err| {
                        capture_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            _ => {
                return Err(anyhow::anyhow!("Unsupported sample format: {:?}", config.sample_format()));
            }
        };

        Ok(stream)
    }

    /// Get device info
    pub fn device(&self) -> &AudioDevice {
        &self.device
    }

    /// Stop the stream.
    ///
    /// This is `async` so the cpal `pause()` + `drop()` can be moved onto a
    /// `spawn_blocking` thread (see `create_cpal_stream` above for why —
    /// same thread-affinity rationale). Use this from any async context;
    /// `Drop` impls cannot call this (they can't `.await`) — see
    /// `stop_sync` below for that case.
    pub async fn stop(self) -> Result<()> {
        info!("Stopping audio stream for device: {}", self.device.name);

        match self.backend {
            StreamBackend::Cpal(stream) => {
                // CRITICAL: Pause the stream first to stop callbacks immediately
                // This ensures closures stop executing before we drop the stream,
                // allowing Arc references captured in callbacks to be released.
                // Done on a blocking thread for the same thread-affinity reason
                // as stream.play() in create_cpal_stream above. Note this does
                // NOT guarantee the same OS thread that called play() handles
                // this pause()/drop() — spawn_blocking doesn't pin to a
                // specific thread across separate calls. A full fix requires
                // owning the stream on one dedicated thread for its entire
                // lifecycle (tracked as a follow-up, not done here).
                //
                // `Stream` is `!Send` (see `AssertSend` above), so it must
                // be wrapped before it can be moved into the closure.
                let wrapped = AssertSend(stream);
                tokio::task::spawn_blocking(move || {
                    let stream = wrapped.into_inner();
                    if let Err(e) = stream.pause() {
                        warn!("Failed to pause stream before drop: {}", e);
                    }
                    info!("Stream paused, now dropping to release callbacks");
                    drop(stream);
                })
                .await
                .map_err(|e| anyhow::anyhow!("Stream stop task panicked: {}", e))?;
            }
            #[cfg(target_os = "macos")]
            StreamBackend::CoreAudio { task } => {
                // Abort the processing task and wait briefly for cleanup
                if let Some(task_handle) = task {
                    info!("Aborting Core Audio task...");
                    task_handle.abort();
                    // Give the runtime a moment to clean up the aborted task
                    // This helps ensure Arc references in the closure are dropped
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    info!("Core Audio task aborted");
                }
            }
        }

        // Explicitly drop self.device Arc reference
        drop(self.device);
        info!("Audio stream stopped and device reference dropped");
        Ok(())
    }

    /// Synchronous stop path used ONLY from `AudioStreamManager`'s `Drop`
    /// impl, where `.await` is unavailable. This performs the same
    /// pause()+drop() logic as `stop()` above but directly on whatever
    /// thread `Drop::drop` happens to run on, WITHOUT the `spawn_blocking`
    /// mitigation `stop()` uses — so it still carries the original
    /// cross-thread cpal thread-affinity risk in full.
    ///
    /// KNOWN GAP: this is not just a rare/expected emergency-cleanup path.
    /// `recording_commands.rs` currently has an open TOCTOU race on
    /// `start_recording` (`IS_RECORDING` is checked, then only set after a
    /// long `.await`-laden init, not atomically) — two near-simultaneous
    /// `start_recording` calls can cause a live `AudioStreamManager` (with
    /// actively running streams) to be replaced and dropped in place on a
    /// shared tokio worker thread, reaching this exact path with a stream
    /// that is genuinely live, not merely during unexpected teardown. A
    /// planned later fix for that race (see the crash-fix plan's TOCTOU
    /// task) should close this gap; re-verify this comment once that fix
    /// lands.
    fn stop_sync(self) -> Result<()> {
        info!("Stopping audio stream for device (sync/drop path): {}", self.device.name);

        match self.backend {
            StreamBackend::Cpal(stream) => {
                if let Err(e) = stream.pause() {
                    warn!("Failed to pause stream before drop: {}", e);
                }
                info!("Stream paused, now dropping to release callbacks");
                drop(stream);
            }
            #[cfg(target_os = "macos")]
            StreamBackend::CoreAudio { task } => {
                if let Some(task_handle) = task {
                    info!("Aborting Core Audio task...");
                    task_handle.abort();
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    info!("Core Audio task aborted");
                }
            }
        }

        drop(self.device);
        info!("Audio stream stopped and device reference dropped (sync path)");
        Ok(())
    }
}

/// Audio stream manager for handling multiple streams
pub struct AudioStreamManager {
    microphone_stream: Option<AudioStream>,
    system_stream: Option<AudioStream>,
    state: Arc<RecordingState>,
}

// SAFETY: AudioStreamManager contains AudioStream which we've marked as Send
unsafe impl Send for AudioStreamManager {}

impl AudioStreamManager {
    pub fn new(state: Arc<RecordingState>) -> Self {
        Self {
            microphone_stream: None,
            system_stream: None,
            state,
        }
    }

    /// Start audio streams for the given devices
    pub async fn start_streams(
        &mut self,
        microphone_device: Option<Arc<AudioDevice>>,
        system_device: Option<Arc<AudioDevice>>,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<()> {
        use super::capture::get_current_backend;
        let backend = get_current_backend();
        info!("🎙️ Starting audio streams with backend: {:?}", backend);

        // Start microphone stream
        if let Some(mic_device) = microphone_device {
            info!("🎤 Creating microphone stream: {} (always uses CPAL)", mic_device.name);
            match AudioStream::create(mic_device.clone(), self.state.clone(), DeviceType::Microphone, recording_sender.clone()).await {
                Ok(stream) => {
                    self.state.set_microphone_device(mic_device);
                    self.microphone_stream = Some(stream);
                    info!("✅ Microphone stream created successfully");
                }
                Err(e) => {
                    error!("❌ Failed to create microphone stream: {}", e);
                    return Err(e);
                }
            }
        } else {
            info!("ℹ️ No microphone device specified, skipping microphone stream");
        }

        // Start system audio stream
        if let Some(sys_device) = system_device {
            info!("🔊 Creating system audio stream: {} (backend: {:?})", sys_device.name, backend);
            match AudioStream::create(sys_device.clone(), self.state.clone(), DeviceType::System, recording_sender.clone()).await {
                Ok(stream) => {
                    self.state.set_system_device(sys_device);
                    self.system_stream = Some(stream);
                    info!("✅ System audio stream created with {:?} backend", backend);
                }
                Err(e) => {
                    warn!("⚠️ Failed to create system audio stream: {}", e);
                    // Don't fail if only system audio fails
                }
            }
        } else {
            info!("ℹ️ No system device specified, skipping system audio stream");
        }

        // Ensure at least one stream was created
        if self.microphone_stream.is_none() && self.system_stream.is_none() {
            return Err(anyhow::anyhow!("No audio streams could be created"));
        }

        Ok(())
    }

    /// Stop all audio streams.
    ///
    /// `async` because `AudioStream::stop` now runs the cpal pause/drop on a
    /// `spawn_blocking` thread. Use this from any async context (this is the
    /// normal path — e.g. `RecordingManager::stop_recording`). `Drop::drop`
    /// cannot call this since it can't `.await` — see `stop_streams_sync`.
    pub async fn stop_streams(&mut self) -> Result<()> {
        info!("Stopping all audio streams");

        let mut errors = Vec::new();

        // Stop microphone stream
        if let Some(mic_stream) = self.microphone_stream.take() {
            if let Err(e) = mic_stream.stop().await {
                error!("Failed to stop microphone stream: {}", e);
                errors.push(e);
            }
        }

        // Stop system stream
        if let Some(sys_stream) = self.system_stream.take() {
            if let Err(e) = sys_stream.stop().await {
                error!("Failed to stop system stream: {}", e);
                errors.push(e);
            }
        }

        if !errors.is_empty() {
            Err(anyhow::anyhow!("Failed to stop some streams: {:?}", errors))
        } else {
            info!("All audio streams stopped successfully");
            Ok(())
        }
    }

    /// Synchronous fallback used only by `Drop::drop` below, where `.await`
    /// is unavailable. Performs pause()+drop() directly on the calling
    /// thread via `AudioStream::stop_sync` instead of `spawn_blocking` — see
    /// `stop_sync`'s doc comment for why this path is reachable with
    /// genuinely live streams today (open `start_recording` TOCTOU race),
    /// not only during rare/unexpected cleanup.
    fn stop_streams_sync(&mut self) -> Result<()> {
        info!("Stopping all audio streams (sync/drop path)");

        let mut errors = Vec::new();

        if let Some(mic_stream) = self.microphone_stream.take() {
            if let Err(e) = mic_stream.stop_sync() {
                error!("Failed to stop microphone stream: {}", e);
                errors.push(e);
            }
        }

        if let Some(sys_stream) = self.system_stream.take() {
            if let Err(e) = sys_stream.stop_sync() {
                error!("Failed to stop system stream: {}", e);
                errors.push(e);
            }
        }

        if !errors.is_empty() {
            Err(anyhow::anyhow!("Failed to stop some streams: {:?}", errors))
        } else {
            info!("All audio streams stopped successfully (sync path)");
            Ok(())
        }
    }

    /// Get stream count
    pub fn active_stream_count(&self) -> usize {
        let mut count = 0;
        if self.microphone_stream.is_some() {
            count += 1;
        }
        if self.system_stream.is_some() {
            count += 1;
        }
        count
    }

    /// Check if any streams are active
    pub fn has_active_streams(&self) -> bool {
        self.microphone_stream.is_some() || self.system_stream.is_some()
    }
}

impl Drop for AudioStreamManager {
    fn drop(&mut self) {
        // Drop::drop cannot be async, so the spawn_blocking-based
        // `stop_streams` above (which requires `.await`) can't be used here.
        // Fall back to `stop_streams_sync`, which performs the pause()+drop()
        // directly on whatever thread `Drop::drop` happens to run on. This
        // carries the same thread-affinity risk `stop_streams`/`stop()`
        // mitigate via spawn_blocking.
        //
        // KNOWN GAP: normal shutdown goes through the async `stop_recording`
        // command path, which calls the async `stop_streams` before this
        // struct would be dropped — but that's not the only way this Drop
        // impl fires today. `recording_commands.rs` has an open TOCTOU race
        // on `start_recording` (checked-then-later-set `IS_RECORDING`, not
        // atomic): two near-simultaneous `start_recording` calls can replace
        // and drop a live `AudioStreamManager` in place on a shared tokio
        // worker thread, landing here with active streams rather than only
        // during rare/unexpected cleanup. A planned later fix for that race
        // should close this gap — see `stop_sync`'s doc comment for detail.
        if let Err(e) = self.stop_streams_sync() {
            error!("Error stopping streams during drop: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn start_streams_fails_when_both_devices_missing() {
        let mut manager = AudioStreamManager::new(RecordingState::new());
        let err = manager
            .start_streams(None, None, None)
            .await
            .expect_err("no streams");
        assert!(
            err.to_string().contains("No audio streams could be created"),
            "unexpected error: {err}"
        );
    }
}