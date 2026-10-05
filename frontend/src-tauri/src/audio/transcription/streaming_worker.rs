use super::worker::{reset_speech_detected_flag, TranscriptUpdate};
use crate::asr_engine::streaming::get_or_init_streaming_engine;
use crate::asr_engine::streaming_state::StreamingSession;
use crate::audio::AudioChunk;
use log::{error, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Runtime};

static STREAMING_SPEECH_EMITTED: AtomicBool = AtomicBool::new(false);

pub fn reset_streaming_speech_flag() {
    STREAMING_SPEECH_EMITTED.store(false, Ordering::SeqCst);
}

pub fn start_streaming_task<R: Runtime>(
    app: AppHandle<R>,
    mut transcription_receiver: tokio::sync::mpsc::Receiver<AudioChunk>,
    writer: crate::audio::recording_saver::TranscriptWriter,
) -> tokio::task::JoinHandle<Result<(), String>> {
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        info!("Starting live streaming ASR task (OnlineRecognizer, no VAD)");
        reset_speech_detected_flag();
        reset_streaming_speech_flag();

        let engine = get_or_init_streaming_engine();
        if !runtime.block_on(engine.is_loaded()) {
            error!("Streaming ASR model is not loaded");
            let _ = app.emit(
                "transcription-error",
                serde_json::json!({
                    "error": "Streaming ASR model not loaded",
                    "userMessage": "Model streaming chưa sẵn sàng. Vui lòng tải trong Cài đặt → Transcription.",
                    "actionable": true
                }),
            );
            return Err("Streaming ASR model is unavailable".into());
        }

        let hotwords = runtime.block_on(engine.hotwords());
        let mut session =
            StreamingSession::new(16000, crate::config::ZIPFORMER_STREAMING_MAX_UTTERANCE_SECS);

        {
            let rec_guard = engine.recognizer().blocking_read();
            let Some(recognizer) = rec_guard.as_ref() else {
                error!("Streaming recognizer disappeared before stream create");
                return Err("Streaming recognizer unavailable before decoding".into());
            };

            let stream = if hotwords.is_empty() {
                recognizer.create_stream()
            } else {
                recognizer.create_stream_with_hotwords(&hotwords)
            };

            drop(rec_guard);

            while let Some(chunk) = transcription_receiver.blocking_recv() {
                if chunk.chunk_id >= u64::MAX - 10 {
                    continue;
                }
                if chunk.data.is_empty() {
                    continue;
                }

                session.note_samples(chunk.data.len());

                let rec_guard = engine.recognizer().blocking_read();
                let Some(recognizer) = rec_guard.as_ref() else {
                    warn!("Streaming recognizer unloaded mid-recording");
                    return Err("Streaming recognizer unloaded mid-recording".into());
                };

                stream.accept_waveform(16000, &chunk.data);
                while recognizer.is_ready(&stream) {
                    recognizer.decode(&stream);
                }

                let hyp = recognizer
                    .get_result(&stream)
                    .map(|r| r.text)
                    .unwrap_or_default();
                let force_speaker = super::live_speaker::should_force_endpoint();
                let is_endpoint = recognizer.is_endpoint(&stream) || force_speaker;
                let emits = session.on_hypothesis(&hyp, is_endpoint);
                let did_reset = session.take_pending_reset();
                if did_reset {
                    recognizer.reset(&stream);
                }
                drop(rec_guard);

                emit_updates(&app, &writer, emits);
                if did_reset {
                    if let Some(name) = super::live_speaker::apply_pending() {
                        super::live_speaker::emit_speaker_committed(&app, &name);
                    }
                }
            }

            let rec_guard = engine.recognizer().blocking_read();
            if let Some(recognizer) = rec_guard.as_ref() {
                stream.input_finished();
                while recognizer.is_ready(&stream) {
                    recognizer.decode(&stream);
                }
                let hyp = recognizer
                    .get_result(&stream)
                    .map(|r| r.text)
                    .unwrap_or_default();
                let emits = session.on_hypothesis(&hyp, true);
                let did_reset = session.take_pending_reset();
                if did_reset {
                    recognizer.reset(&stream);
                }
                drop(rec_guard);
                emit_updates(&app, &writer, emits);
                if did_reset {
                    if let Some(name) = super::live_speaker::apply_pending() {
                        super::live_speaker::emit_speaker_committed(&app, &name);
                    }
                }
            }
        }

        info!("Live streaming ASR task finished");
        Ok(())
    })
}

fn emit_updates<R: Runtime>(
    app: &AppHandle<R>,
    writer: &crate::audio::recording_saver::TranscriptWriter,
    emits: Vec<crate::asr_engine::streaming_state::DecodeEmit>,
) {
    for emit in emits {
        let text = crate::audio::post_asr::normalize_asr_text(&emit.text);
        if text.trim().is_empty() {
            continue;
        }

        if !STREAMING_SPEECH_EMITTED.swap(true, Ordering::SeqCst) {
            let _ = app.emit(
                "speech-detected",
                serde_json::json!({ "message": "Speech activity detected" }),
            );
        }

        let duration = (emit.audio_end_time - emit.audio_start_time).max(0.0);
        let speaker_name = super::live_speaker::stamp();
        let speaker_color = speaker_name
            .as_deref()
            .map(super::live_speaker::color_for_name);
        let update = TranscriptUpdate {
            text,
            timestamp: super::worker::format_current_timestamp(),
            source: "Audio".to_string(),
            sequence_id: emit.sequence_id,
            chunk_start_time: emit.audio_start_time,
            is_partial: emit.is_partial,
            confidence: 0.85,
            audio_start_time: emit.audio_start_time,
            audio_end_time: emit.audio_end_time,
            duration,
            speaker_name,
            speaker_color,
        };

        if let Err(e) = writer.record_update(&update) {
            error!("Failed to persist streaming transcript: {}", e);
            let _ = app.emit(
                "recording-error",
                "Không lưu được bản ghi. Dữ liệu còn trong bộ nhớ; hãy dừng và thử lưu lại.",
            );
        }
        if let Err(e) = app.emit("transcript-update", &update) {
            error!("Failed to emit streaming transcript update: {}", e);
        }
    }
}
