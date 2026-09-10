use super::gemini_key::vocabulary_from_hotwords;
use super::gemini_parse::{extract_interaction_text, segments_from_file_output};
use crate::api::TranscriptSegment;
use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;
use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use reqwest::Client;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

const MAX_DURATION_SECS: f64 = 3600.0;
const PRIMARY_MODEL: &str = "gemini-3.5-transcribe";
const PREVIEW_MODEL: &str = "gemini-3.5-transcribe-preview";
const UPLOAD_START_URL: &str = "https://generativelanguage.googleapis.com/upload/v1beta/files";
const INTERACTIONS_URL: &str = "https://generativelanguage.googleapis.com/v1beta/interactions";
const API_REVISION: &str = "2026-05-20";
const POLL_MAX: usize = 10;
const POLL_SLEEP: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq)]
enum InteractionPoll {
    Ready,
    Pending,
    Failed(String),
}

pub fn reject_if_too_long(duration_seconds: f64) -> Result<()> {
    if duration_seconds > MAX_DURATION_SECS {
        return Err(anyhow!(
            "File dài hơn 1 giờ (tối đa {MAX_DURATION_SECS:.0} giây). Gemini Transcribe không hỗ trợ file dài hơn."
        ));
    }
    Ok(())
}

pub async fn vocabulary_from_app<R: Runtime>(app: &AppHandle<R>) -> Vec<String> {
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

fn throw_if_cancelled(is_cancelled: impl Fn() -> bool, cancel_msg: &str) -> Result<()> {
    if is_cancelled() {
        Err(anyhow!("{cancel_msg}"))
    } else {
        Ok(())
    }
}

pub async fn transcribe_file(
    api_key: &str,
    audio_path: &Path,
    duration_seconds: f64,
    vocabulary: &[String],
    is_cancelled: impl Fn() -> bool,
    cancel_msg: &str,
) -> Result<Vec<TranscriptSegment>> {
    reject_if_too_long(duration_seconds)?;
    throw_if_cancelled(&is_cancelled, cancel_msg)?;
    let mime = mime_for_path(audio_path);
    let bytes = tokio::fs::read(audio_path)
        .await
        .with_context(|| format!("Failed to read audio file {}", audio_path.display()))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(1800))
        .build()
        .context("Failed to create HTTP client")?;

    info!(
        "Gemini file STT: uploading {} bytes ({mime}) duration={duration_seconds:.1}s",
        bytes.len()
    );
    let uri = upload_file(&client, api_key, &bytes, mime, &is_cancelled, cancel_msg).await?;
    throw_if_cancelled(&is_cancelled, cancel_msg)?;
    let body = create_interaction(
        &client,
        api_key,
        PRIMARY_MODEL,
        &uri,
        mime,
        vocabulary,
        &is_cancelled,
        cancel_msg,
    )
    .await?;
    throw_if_cancelled(&is_cancelled, cancel_msg)?;
    let text = extract_interaction_text(&body);
    let timed = timed_from_interaction(&body);
    require_nonempty_transcript(&text, &timed)?;
    Ok(segments_from_file_output(&text, &timed, duration_seconds))
}

async fn upload_file(
    client: &Client,
    api_key: &str,
    bytes: &[u8],
    mime: &str,
    is_cancelled: &impl Fn() -> bool,
    cancel_msg: &str,
) -> Result<String> {
    throw_if_cancelled(is_cancelled, cancel_msg)?;
    let start = client
        .post(UPLOAD_START_URL)
        .header("x-goog-api-key", api_key)
        .header("X-Goog-Upload-Protocol", "resumable")
        .header("X-Goog-Upload-Command", "start")
        .header("X-Goog-Upload-Header-Content-Length", bytes.len().to_string())
        .header("X-Goog-Upload-Header-Content-Type", mime)
        .header("Content-Type", "application/json")
        .json(&json!({"file": {"display_name": "meeting"}}))
        .send()
        .await
        .context("Gemini file upload start failed")?;

    if !start.status().is_success() {
        return Err(http_error("upload start", start).await);
    }

    let upload_url = start
        .headers()
        .get("x-goog-upload-url")
        .ok_or_else(|| anyhow!("Gemini upload did not return x-goog-upload-url"))?
        .to_str()
        .context("Invalid x-goog-upload-url header")?
        .to_string();

    throw_if_cancelled(is_cancelled, cancel_msg)?;
    let uploaded = client
        .post(&upload_url)
        .header("Content-Length", bytes.len().to_string())
        .header("X-Goog-Upload-Offset", "0")
        .header("X-Goog-Upload-Command", "upload, finalize")
        .body(bytes.to_vec())
        .send()
        .await
        .context("Gemini file upload finalize failed")?;

    if !uploaded.status().is_success() {
        return Err(http_error("upload finalize", uploaded).await);
    }

    throw_if_cancelled(is_cancelled, cancel_msg)?;
    let body: Value = uploaded
        .json()
        .await
        .context("Gemini upload response was not JSON")?;
    body.pointer("/file/uri")
        .or_else(|| body.get("uri"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("Gemini upload response missing file.uri"))
}

async fn create_interaction(
    client: &Client,
    api_key: &str,
    model: &str,
    uri: &str,
    mime: &str,
    vocabulary: &[String],
    is_cancelled: &impl Fn() -> bool,
    cancel_msg: &str,
) -> Result<Value> {
    let transcription_config = build_transcription_config(vocabulary);
    let mut model = model;
    loop {
        throw_if_cancelled(is_cancelled, cancel_msg)?;
        let payload = json!({
            "model": model,
            "input": [{
                "type": "audio",
                "uri": uri,
                "mime_type": mime
            }],
            "generation_config": {
                "transcription_config": transcription_config.clone()
            }
        });

        let resp = client
            .post(INTERACTIONS_URL)
            .header("x-goog-api-key", api_key)
            .header("Content-Type", "application/json")
            .header("Api-Revision", API_REVISION)
            .json(&payload)
            .send()
            .await
            .context("Gemini interactions.create failed")?;

        let status = resp.status();
        if status.as_u16() == 404 && model == PRIMARY_MODEL {
            warn!("Gemini file model {PRIMARY_MODEL} not found; retrying {PREVIEW_MODEL}");
            model = PREVIEW_MODEL;
            continue;
        }
        if !status.is_success() {
            return Err(http_error("interactions.create", resp).await);
        }

        let body: Value = resp
            .json()
            .await
            .context("Gemini interaction response was not JSON")?;
        if let Some(err) = body.get("error") {
            return Err(anyhow!(
                "Gemini Transcribe lỗi: {}",
                err.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error")
            ));
        }
        return settle_interaction(client, api_key, body, is_cancelled, cancel_msg).await;
    }
}

fn build_transcription_config(vocabulary: &[String]) -> Value {
    let mut cfg = json!({ "mode": { "type": "smart" } });
    if !vocabulary.is_empty() {
        cfg["custom_vocabulary"] = json!(vocabulary);
    }
    cfg
}

fn classify_interaction(body: &Value) -> InteractionPoll {
    match body.get("status").and_then(|v| v.as_str()) {
        Some(status @ ("failed" | "incomplete" | "budget_exceeded")) => {
            InteractionPoll::Failed(format!(
                "Gemini Transcribe không hoàn tất (status={status})."
            ))
        }
        Some("in_progress" | "queued") => InteractionPoll::Pending,
        _ => InteractionPoll::Ready,
    }
}

fn interaction_id(body: &Value) -> Option<String> {
    if let Some(id) = body
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return Some(id.to_string());
    }
    body.get("name")
        .and_then(|v| v.as_str())
        .and_then(|name| name.rsplit('/').next())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn require_nonempty_transcript(text: &str, timed: &[(String, f64, f64)]) -> Result<()> {
    if text.trim().is_empty() && timed.is_empty() {
        Err(anyhow!(
            "Gemini Transcribe trả về transcript rỗng. Không lưu kết quả trống."
        ))
    } else {
        Ok(())
    }
}

async fn settle_interaction(
    client: &Client,
    api_key: &str,
    mut body: Value,
    is_cancelled: &impl Fn() -> bool,
    cancel_msg: &str,
) -> Result<Value> {
    for attempt in 0..POLL_MAX {
        throw_if_cancelled(is_cancelled, cancel_msg)?;
        match classify_interaction(&body) {
            InteractionPoll::Failed(msg) => return Err(anyhow!("{msg}")),
            InteractionPoll::Ready => return Ok(body),
            InteractionPoll::Pending => {
                if attempt + 1 == POLL_MAX {
                    break;
                }
                let id = interaction_id(&body).ok_or_else(|| {
                    anyhow!("Gemini Transcribe đang xử lý nhưng thiếu interaction id.")
                })?;
                tokio::time::sleep(POLL_SLEEP).await;
                throw_if_cancelled(is_cancelled, cancel_msg)?;
                body = get_interaction(client, api_key, &id).await?;
            }
        }
    }
    Err(anyhow!(
        "Gemini Transcribe vẫn đang xử lý sau khi chờ. Thử lại sau."
    ))
}

async fn get_interaction(client: &Client, api_key: &str, id: &str) -> Result<Value> {
    let url = format!("{INTERACTIONS_URL}/{id}");
    let resp = client
        .get(&url)
        .header("x-goog-api-key", api_key)
        .header("Api-Revision", API_REVISION)
        .send()
        .await
        .context("Gemini interactions.get failed")?;
    if !resp.status().is_success() {
        return Err(http_error("interactions.get", resp).await);
    }
    let body: Value = resp
        .json()
        .await
        .context("Gemini interaction GET was not JSON")?;
    if let Some(err) = body.get("error") {
        return Err(anyhow!(
            "Gemini Transcribe lỗi: {}",
            err.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
        ));
    }
    Ok(body)
}

fn timed_from_interaction(body: &Value) -> Vec<(String, f64, f64)> {
    let arr = body
        .get("utterances")
        .or_else(|| body.pointer("/outputs/0/utterances"))
        .or_else(|| body.pointer("/outputs/0/content/utterances"))
        .and_then(|v| v.as_array());
    let Some(arr) = arr else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|u| {
            let text = u.get("text").and_then(|t| t.as_str())?.trim();
            if text.is_empty() {
                return None;
            }
            let start = json_secs(u.get("start").or_else(|| u.get("start_time")).or_else(|| u.get("startTime"))?)?;
            let end = json_secs(u.get("end").or_else(|| u.get("end_time")).or_else(|| u.get("endTime"))?)?;
            Some((text.to_string(), start, end))
        })
        .collect()
}

fn json_secs(v: &Value) -> Option<f64> {
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    let s = v.as_str()?.trim();
    s.strip_suffix('s').unwrap_or(s).parse().ok()
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("m4a") | Some("mp4") => "audio/mp4",
        Some("flac") => "audio/flac",
        Some("ogg") => "audio/ogg",
        Some("webm") => "audio/webm",
        _ => "audio/wav",
    }
}

async fn http_error(op: &str, resp: reqwest::Response) -> anyhow::Error {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let snippet: String = body.chars().take(300).collect();
    match status.as_u16() {
        401 | 403 => anyhow!(
            "API key Gemini không hợp lệ hoặc bị từ chối. Kiểm tra key trong Cài đặt → Nhận dạng."
        ),
        429 => anyhow!("Hết hạn mức Gemini Transcribe. Kiểm tra quota API rồi thử lại."),
        404 => anyhow!("Model Gemini Transcribe không khả dụng. Thử lại sau hoặc đổi nhà cung cấp STT."),
        _ => anyhow!("Gemini {op} failed ({status}): {snippet}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_over_one_hour() {
        assert!(reject_if_too_long(3600.0).is_ok());
        assert!(reject_if_too_long(3599.9).is_ok());
        let err = reject_if_too_long(3600.1).unwrap_err().to_string();
        assert!(err.contains("3600"));
        assert!(reject_if_too_long(7200.0).is_err());
    }

    #[test]
    fn timed_from_interaction_reads_utterances_or_empty() {
        let empty = json!({"output_text": "hello"});
        assert!(timed_from_interaction(&empty).is_empty());

        let body = json!({
            "utterances": [
                {"text": "A", "start": 0.0, "end": 1.2},
                {"text": "B", "start_time": 1.2, "end_time": 3.0}
            ]
        });
        let timed = timed_from_interaction(&body);
        assert_eq!(timed.len(), 2);
        assert_eq!(timed[0], ("A".to_string(), 0.0, 1.2));
        assert_eq!(timed[1], ("B".to_string(), 1.2, 3.0));
    }

    #[test]
    fn cancel_check_uses_caller_message() {
        assert!(throw_if_cancelled(|| false, "Import cancelled").is_ok());
        assert_eq!(
            throw_if_cancelled(|| true, "Import cancelled")
                .unwrap_err()
                .to_string(),
            "Import cancelled"
        );
        assert_eq!(
            throw_if_cancelled(|| true, "Retranscription cancelled")
                .unwrap_err()
                .to_string(),
            "Retranscription cancelled"
        );
    }

    #[test]
    fn transcription_config_mode_is_discriminated_union() {
        let cfg = build_transcription_config(&[]);
        assert_eq!(cfg["mode"], json!({ "type": "smart" }));
        assert!(cfg.get("custom_vocabulary").is_none());

        let with_vocab = build_transcription_config(&["Meetily".into()]);
        assert_eq!(with_vocab["mode"]["type"], "smart");
        assert_eq!(with_vocab["custom_vocabulary"], json!(["Meetily"]));
    }

    #[test]
    fn classify_failed_pending_and_sdk_missing_status() {
        for status in ["failed", "incomplete", "budget_exceeded"] {
            match classify_interaction(&json!({ "status": status })) {
                InteractionPoll::Failed(msg) => assert!(msg.contains(status)),
                other => panic!("expected Failed for {status}, got {other:?}"),
            }
        }
        assert_eq!(
            classify_interaction(&json!({ "status": "in_progress" })),
            InteractionPoll::Pending
        );
        assert_eq!(
            classify_interaction(&json!({ "status": "queued" })),
            InteractionPoll::Pending
        );
        assert_eq!(
            classify_interaction(&json!({ "status": "completed" })),
            InteractionPoll::Ready
        );
        assert_eq!(
            classify_interaction(&json!({ "output_text": "SDK transcript" })),
            InteractionPoll::Ready
        );
    }

    #[test]
    fn empty_transcript_is_err_text_or_timed_is_ok() {
        let err = require_nonempty_transcript("", &[]).unwrap_err().to_string();
        assert!(err.contains("rỗng"));
        assert!(require_nonempty_transcript("   ", &[]).is_err());
        assert!(require_nonempty_transcript("Xin chào.", &[]).is_ok());
        assert!(require_nonempty_transcript("", &[("A".into(), 0.0, 1.0)]).is_ok());
    }

    #[test]
    fn interaction_id_from_id_or_name() {
        assert_eq!(
            interaction_id(&json!({ "id": "abc" })).as_deref(),
            Some("abc")
        );
        assert_eq!(
            interaction_id(&json!({ "name": "interactions/xyz" })).as_deref(),
            Some("xyz")
        );
    }
}
