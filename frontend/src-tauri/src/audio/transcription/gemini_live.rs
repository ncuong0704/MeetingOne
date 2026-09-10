use super::gemini_key::{f32_to_pcm16_le, resolve_stt_api_key, vocabulary_from_hotwords};
use super::gemini_parse::{parse_live_message, LiveTranscriptEvent};
use super::worker::{reset_speech_detected_flag, TranscriptUpdate};
use crate::audio::AudioChunk;
use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use serde_json::{json, Value};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

const PRIMARY_MODEL: &str = "models/gemini-3.5-transcribe-live";
const PREVIEW_MODEL: &str = "models/gemini-3.5-transcribe-live-preview";
const LIVE_WS_BASE: &str =
    "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
const SAMPLE_RATE: f64 = 16_000.0;
const MAX_RECONNECTS: u32 = 5;
const SETUP_TIMEOUT: Duration = Duration::from_secs(20);

const KEY_MISSING_MSG: &str =
    "Chưa có API key Gemini. Nhập key ở Cài đặt → Nhận dạng, hoặc key LLM (custom-openai / Gemini).";
const AUTH_MSG: &str =
    "API key Gemini không hợp lệ hoặc bị từ chối. Kiểm tra key trong Cài đặt → Nhận dạng.";
const QUOTA_MSG: &str =
    "Hết hạn mức Gemini Transcribe Live. Kiểm tra quota API rồi thử lại.";
const DISCONNECT_MSG: &str = "Mất kết nối Gemini Transcribe Live. Kiểm tra mạng rồi ghi lại.";
const MODEL_MSG: &str = "Model Gemini Transcribe Live không khả dụng. Thử lại sau hoặc đổi nhà cung cấp STT.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionEnd {
    ReceiverClosed,
    Auth,
    Quota,
    ModelNotFound,
    NeedReconnect,
}

struct EmitState {
    sequence_id: u64,
    sample_clock: f64,
    utterance_start: f64,
    speech_emitted: bool,
}

impl EmitState {
    fn note_samples(&mut self, n: usize) {
        self.sample_clock += n as f64 / SAMPLE_RATE;
    }
}

pub fn start_gemini_live_task<R: Runtime>(
    app: AppHandle<R>,
    mut transcription_receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting Gemini Transcribe Live task");
        reset_speech_detected_flag();

        let Some(state) = app.try_state::<AppState>() else {
            emit_error(&app, "App state not available", KEY_MISSING_MSG);
            return;
        };
        let api_key = match resolve_stt_api_key(state.db_manager.pool()).await {
            Ok(k) => k,
            Err(msg) => {
                emit_error(&app, &msg, &msg);
                return;
            }
        };

        let vocab = load_vocabulary(&app).await;
        let url = live_ws_url(&api_key);
        let mut model = PRIMARY_MODEL;
        let mut resumption: Option<String> = None;
        let mut reconnects = 0u32;
        let mut emit = EmitState {
            sequence_id: 0,
            sample_clock: 0.0,
            utterance_start: 0.0,
            speech_emitted: false,
        };

        loop {
            match run_session(
                &app,
                &url,
                model,
                &vocab,
                &mut resumption,
                &mut transcription_receiver,
                &mut emit,
                &mut reconnects,
            )
            .await
            {
                SessionEnd::ReceiverClosed => {
                    info!("Gemini live task finished (recording stopped)");
                    return;
                }
                SessionEnd::Auth => {
                    emit_error(&app, "Gemini STT authentication failed", AUTH_MSG);
                    return;
                }
                SessionEnd::Quota => {
                    emit_error(&app, "Gemini STT quota exhausted", QUOTA_MSG);
                    return;
                }
                SessionEnd::ModelNotFound => {
                    if model == PRIMARY_MODEL {
                        warn!("Gemini live model not found; retrying with preview");
                        model = PREVIEW_MODEL;
                        continue;
                    }
                    emit_error(&app, "Gemini transcribe live model not found", MODEL_MSG);
                    return;
                }
                SessionEnd::NeedReconnect => {
                    if reconnects >= MAX_RECONNECTS {
                        emit_error(&app, "Gemini live reconnects exhausted", DISCONNECT_MSG);
                        return;
                    }
                    let secs = reconnect_backoff_secs(reconnects);
                    warn!("Gemini live reconnect {}/{} in {}s", reconnects + 1, MAX_RECONNECTS, secs);
                    tokio::time::sleep(Duration::from_secs(secs)).await;
                    reconnects += 1;
                }
            }
        }
    })
}

fn live_ws_url(api_key: &str) -> String {
    let mut url = url::Url::parse(LIVE_WS_BASE).expect("static Gemini live URL");
    url.query_pairs_mut().append_pair("key", api_key);
    url.to_string()
}

fn reconnect_backoff_secs(reconnect_index: u32) -> u64 {
    1u64 << reconnect_index.min(4)
}

fn build_setup(model: &str, vocab: &[String], resumption: Option<&str>) -> Value {
    let mut input = json!({ "mode": "smart" });
    if !vocab.is_empty() {
        input["customVocabulary"] = json!(vocab);
    }
    let session_resumption = match resumption {
        Some(handle) if !handle.is_empty() => json!({ "handle": handle }),
        _ => json!({}),
    };
    json!({
        "setup": {
            "model": model,
            "generationConfig": { "responseModalities": ["TEXT"] },
            "inputAudioTranscription": input,
            "sessionResumption": session_resumption
        }
    })
}

fn classify_http_status(status: u16) -> Option<SessionEnd> {
    match status {
        401 | 403 => Some(SessionEnd::Auth),
        429 => Some(SessionEnd::Quota),
        _ => None,
    }
}

fn classify_ws_error(err: &WsError) -> Option<SessionEnd> {
    match err {
        WsError::Http(resp) => classify_http_status(resp.status().as_u16()),
        _ => classify_error_text(&err.to_string()),
    }
}

fn classify_error_text(text: &str) -> Option<SessionEnd> {
    let lower = text.to_lowercase();
    if lower.contains("401") || lower.contains("403") {
        return Some(SessionEnd::Auth);
    }
    if lower.contains("429")
        || lower.contains("quota")
        || lower.contains("resource exhausted")
        || lower.contains("resource_exhausted")
    {
        return Some(SessionEnd::Quota);
    }
    None
}

fn classify_json_error(value: &Value) -> Option<SessionEnd> {
    let err = value.get("error")?;
    let code = err.get("code").and_then(|c| c.as_u64()).unwrap_or(0);
    let status = err.get("status").and_then(|s| s.as_str()).unwrap_or("");
    let msg = err.get("message").and_then(|s| s.as_str()).unwrap_or("");
    if let Some(end) = classify_http_status(code as u16) {
        return Some(end);
    }
    if status == "UNAUTHENTICATED" || status == "PERMISSION_DENIED" {
        return Some(SessionEnd::Auth);
    }
    if status == "RESOURCE_EXHAUSTED" {
        return Some(SessionEnd::Quota);
    }
    if let Some(end) = classify_error_text(msg) {
        return Some(end);
    }
    if code == 404 || status == "NOT_FOUND" || msg.to_lowercase().contains("not found") {
        return Some(SessionEnd::ModelNotFound);
    }
    None
}

fn is_model_not_found_text(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("not found") || lower.contains("not_found")
}

fn emit_error<R: Runtime>(app: &AppHandle<R>, error: &str, user_message: &str) {
    error!("{error}: {user_message}");
    let _ = app.emit(
        "transcription-error",
        json!({
            "error": error,
            "userMessage": user_message,
            "actionable": true
        }),
    );
}

async fn load_vocabulary<R: Runtime>(app: &AppHandle<R>) -> Vec<String> {
    let Some(state) = app.try_state::<AppState>() else {
        return Vec::new();
    };
    let stored = SettingsRepository::get_transcript_config(state.db_manager.pool())
        .await
        .ok()
        .flatten()
        .and_then(|c| c.hotwords);
    let bundled = crate::asr_engine::commands::load_bundled_hotwords_raw(app);
    let text = crate::asr_engine::hotwords::effective_hotwords_text(
        stored.as_deref(),
        bundled.as_deref(),
    );
    vocabulary_from_hotwords(&text)
}

fn audio_message(pcm16: &[u8]) -> Message {
    let payload = json!({
        "realtimeInput": {
            "audio": {
                "data": base64::engine::general_purpose::STANDARD.encode(pcm16),
                "mimeType": "audio/pcm;rate=16000"
            }
        }
    });
    Message::Text(payload.to_string().into())
}

fn emit_transcript<R: Runtime>(app: &AppHandle<R>, emit: &mut EmitState, text: String, is_partial: bool) {
    if text.trim().is_empty() {
        return;
    }
    if !emit.speech_emitted {
        emit.speech_emitted = true;
        let _ = app.emit(
            "speech-detected",
            json!({ "message": "Speech activity detected" }),
        );
    }
    let audio_end = emit.sample_clock;
    let audio_start = emit.utterance_start;
    let speaker_name = super::live_speaker::stamp();
    let speaker_color = speaker_name
        .as_deref()
        .map(super::live_speaker::color_for_name);
    let update = TranscriptUpdate {
        text,
        timestamp: super::worker::format_current_timestamp(),
        source: "Audio".to_string(),
        sequence_id: emit.sequence_id,
        chunk_start_time: audio_start,
        is_partial,
        confidence: 0.85,
        audio_start_time: audio_start,
        audio_end_time: audio_end,
        duration: (audio_end - audio_start).max(0.0),
        speaker_name,
        speaker_color,
    };
    if let Err(e) = app.emit("transcript-update", &update) {
        error!("Failed to emit Gemini transcript update: {e}");
    }
    if !is_partial {
        emit.sequence_id += 1;
        emit.utterance_start = emit.sample_clock;
        if let Some(name) = super::live_speaker::apply_pending() {
            super::live_speaker::emit_speaker_committed(app, &name);
        }
    }
}

fn handle_server_json<R: Runtime>(
    app: &AppHandle<R>,
    emit: &mut EmitState,
    resumption: &mut Option<String>,
    value: &Value,
) -> Option<SessionEnd> {
    if let Some(end) = classify_json_error(value) {
        return Some(end);
    }
    match parse_live_message(value) {
        LiveTranscriptEvent::SetupComplete => None,
        LiveTranscriptEvent::Interim { text } => {
            emit_transcript(app, emit, text, true);
            None
        }
        LiveTranscriptEvent::Final { text } => {
            emit_transcript(app, emit, text, false);
            None
        }
        LiveTranscriptEvent::GoAway => Some(SessionEnd::NeedReconnect),
        LiveTranscriptEvent::Resumption { handle } => {
            *resumption = Some(handle);
            None
        }
        LiveTranscriptEvent::Ignored => None,
    }
}

async fn run_session<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    model: &str,
    vocab: &[String],
    resumption: &mut Option<String>,
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<AudioChunk>,
    emit: &mut EmitState,
    reconnects: &mut u32,
) -> SessionEnd {
    let (ws, _) = match tokio_tungstenite::connect_async(url).await {
        Ok(pair) => pair,
        Err(e) => {
            warn!("Gemini live connect failed: {e}");
            if let Some(end) = classify_ws_error(&e) {
                return end;
            }
            return SessionEnd::NeedReconnect;
        }
    };
    let (mut sink, mut stream) = ws.split();

    let setup = build_setup(model, vocab, resumption.as_deref());
    if let Err(e) = sink
        .send(Message::Text(setup.to_string().into()))
        .await
    {
        warn!("Gemini live setup send failed: {e}");
        return SessionEnd::NeedReconnect;
    }

    let mut pending: Vec<Vec<f32>> = Vec::new();
    let mut setup_done = false;
    let deadline = tokio::time::Instant::now() + SETUP_TIMEOUT;
    while !setup_done {
        let remain = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remain.is_zero() {
            warn!("Gemini live setup timed out");
            return SessionEnd::NeedReconnect;
        }
        tokio::select! {
            _ = tokio::time::sleep(remain) => {
                warn!("Gemini live setup timed out");
                return SessionEnd::NeedReconnect;
            }
            chunk = receiver.recv() => {
                match chunk {
                    None => return SessionEnd::ReceiverClosed,
                    Some(c) => {
                        if c.data.is_empty() || c.chunk_id >= u64::MAX - 10 {
                            continue;
                        }
                        emit.note_samples(c.data.len());
                        pending.push(c.data);
                    }
                }
            }
            msg = stream.next() => {
                match msg {
                    None => return SessionEnd::NeedReconnect,
                    Some(Err(e)) => {
                        warn!("Gemini live setup read failed: {e}");
                        if let Some(end) = classify_ws_error(&e) {
                            return end;
                        }
                        return SessionEnd::NeedReconnect;
                    }
                    Some(Ok(Message::Close(frame))) => {
                        let reason = frame.as_ref().map(|f| f.reason.to_string()).unwrap_or_default();
                        if is_model_not_found_text(&reason) {
                            return SessionEnd::ModelNotFound;
                        }
                        if let Some(end) = classify_error_text(&reason) {
                            return end;
                        }
                        return SessionEnd::NeedReconnect;
                    }
                    Some(Ok(m)) => {
                        let Some(value) = message_json(&m) else { continue };
                        if let Some(end) = handle_server_json(app, emit, resumption, &value) {
                            return end;
                        }
                        if parse_live_message(&value) == LiveTranscriptEvent::SetupComplete {
                            setup_done = true;
                            *reconnects = 0;
                        }
                    }
                }
            }
        }
    }

    for samples in pending {
        let pcm = f32_to_pcm16_le(&samples);
        if sink.send(audio_message(&pcm)).await.is_err() {
            return SessionEnd::NeedReconnect;
        }
    }

    loop {
        tokio::select! {
            chunk = receiver.recv() => {
                match chunk {
                    None => return SessionEnd::ReceiverClosed,
                    Some(c) => {
                        if c.data.is_empty() || c.chunk_id >= u64::MAX - 10 {
                            continue;
                        }
                        emit.note_samples(c.data.len());
                        let pcm = f32_to_pcm16_le(&c.data);
                        if sink.send(audio_message(&pcm)).await.is_err() {
                            return SessionEnd::NeedReconnect;
                        }
                    }
                }
            }
            msg = stream.next() => {
                match msg {
                    None => return SessionEnd::NeedReconnect,
                    Some(Err(e)) => {
                        warn!("Gemini live read failed: {e}");
                        if let Some(end) = classify_ws_error(&e) {
                            return end;
                        }
                        return SessionEnd::NeedReconnect;
                    }
                    Some(Ok(Message::Close(frame))) => {
                        let reason = frame.as_ref().map(|f| f.reason.to_string()).unwrap_or_default();
                        if is_model_not_found_text(&reason) {
                            return SessionEnd::ModelNotFound;
                        }
                        if let Some(end) = classify_error_text(&reason) {
                            return end;
                        }
                        return SessionEnd::NeedReconnect;
                    }
                    Some(Ok(m)) => {
                        let Some(value) = message_json(&m) else { continue };
                        if let Some(end) = handle_server_json(app, emit, resumption, &value) {
                            return end;
                        }
                    }
                }
            }
        }
    }
}

fn message_json(msg: &Message) -> Option<Value> {
    let text = msg.to_text().ok()?;
    serde_json::from_str(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_backoff_is_1_2_4_8_16() {
        assert_eq!(
            (0..5).map(reconnect_backoff_secs).collect::<Vec<_>>(),
            vec![1, 2, 4, 8, 16]
        );
    }

    #[test]
    fn gemini_setup_json_matches_live_fields() {
        let v = build_setup(PRIMARY_MODEL, &[], None);
        assert_eq!(v["setup"]["model"], PRIMARY_MODEL);
        assert_eq!(v["setup"]["generationConfig"]["responseModalities"][0], "TEXT");
        assert_eq!(v["setup"]["inputAudioTranscription"]["mode"], "smart");
        assert!(v["setup"]["inputAudioTranscription"]["customVocabulary"].is_null());
        assert_eq!(v["setup"]["sessionResumption"], json!({}));
    }

    #[test]
    fn gemini_setup_attaches_documented_custom_vocabulary() {
        let vocab = vec!["ACT".to_string(), "Meetily".to_string()];
        let v = build_setup(PREVIEW_MODEL, &vocab, Some("handle-1"));
        assert_eq!(v["setup"]["model"], PREVIEW_MODEL);
        assert_eq!(
            v["setup"]["inputAudioTranscription"]["customVocabulary"],
            json!(["ACT", "Meetily"])
        );
        assert_eq!(v["setup"]["sessionResumption"]["handle"], "handle-1");
    }

    #[test]
    fn gemini_classifies_model_not_found() {
        let v = json!({
            "error": { "code": 404, "status": "NOT_FOUND", "message": "model is not found" }
        });
        assert_eq!(classify_json_error(&v), Some(SessionEnd::ModelNotFound));
        assert!(is_model_not_found_text("models/x is not found"));
    }

    #[test]
    fn gemini_classifies_auth_errors() {
        let v = json!({
            "error": { "code": 403, "status": "PERMISSION_DENIED", "message": "denied" }
        });
        assert_eq!(classify_json_error(&v), Some(SessionEnd::Auth));
    }

    #[test]
    fn gemini_classifies_quota_as_fatal() {
        let by_code = json!({ "error": { "code": 429, "message": "rate limit" } });
        assert_eq!(classify_json_error(&by_code), Some(SessionEnd::Quota));

        let by_status = json!({
            "error": { "status": "RESOURCE_EXHAUSTED", "message": "quota exceeded" }
        });
        assert_eq!(classify_json_error(&by_status), Some(SessionEnd::Quota));
        assert_eq!(classify_http_status(429), Some(SessionEnd::Quota));
        assert_eq!(classify_error_text("429 Too Many Requests"), Some(SessionEnd::Quota));
        assert_eq!(classify_http_status(401), Some(SessionEnd::Auth));
    }
}
