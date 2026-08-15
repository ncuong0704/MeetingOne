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
    mut transcription_receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting live streaming ASR task (OnlineRecognizer, no VAD)");
        reset_speech_detected_flag();
        reset_streaming_speech_flag();

        let engine = get_or_init_streaming_engine();
        if !engine.is_loaded().await {
            error!("Streaming ASR model is not loaded");
            let _ = app.emit(
                "transcription-error",
                serde_json::json!({
                    "error": "Streaming ASR model not loaded",
                    "userMessage": "Model streaming chưa sẵn sàng. Vui lòng tải trong Cài đặt → Transcription.",
                    "actionable": true
                }),
            );
            return;
        }

        let hotwords = engine.hotwords().await;
        let mut session = StreamingSession::new(
            16000,
            crate::config::ZIPFORMER_STREAMING_MAX_UTTERANCE_SECS,
        );

        {
            let rec_guard = engine.recognizer().read().await;
            let Some(recognizer) = rec_guard.as_ref() else {
                error!("Streaming recognizer disappeared before stream create");
                return;
            };

            let stream = if hotwords.is_empty() {
                recognizer.create_stream()
            } else {
                recognizer.create_stream_with_hotwords(&hotwords)
            };

            drop(rec_guard);

            while let Some(chunk) = transcription_receiver.recv().await {
                if chunk.chunk_id >= u64::MAX - 10 {
                    continue;
                }
                if chunk.data.is_empty() {
                    continue;
                }

                session.note_samples(chunk.data.len());

                let rec_guard = engine.recognizer().read().await;
                let Some(recognizer) = rec_guard.as_ref() else {
                    warn!("Streaming recognizer unloaded mid-recording");
                    break;
                };

                stream.accept_waveform(16000, &chunk.data);
                while recognizer.is_ready(&stream) {
                    recognizer.decode(&stream);
                }

                let hyp = recognizer
                    .get_result(&stream)
                    .map(|r| r.text)
                    .unwrap_or_default();
                let is_endpoint = recognizer.is_endpoint(&stream);
                let emits = session.on_hypothesis(&hyp, is_endpoint);
                if session.take_pending_reset() {
                    recognizer.reset(&stream);
                }
                drop(rec_guard);

                emit_updates(&app, emits);
            }

            let rec_guard = engine.recognizer().read().await;
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
                if session.take_pending_reset() {
                    recognizer.reset(&stream);
                }
                drop(rec_guard);
                emit_updates(&app, emits);
            }
        }

        info!("Live streaming ASR task finished");
    })
}

fn emit_updates<R: Runtime>(
    app: &AppHandle<R>,
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
        };

        if let Err(e) = app.emit("transcript-update", &update) {
            error!("Failed to emit streaming transcript update: {}", e);
        }
    }
}
