use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::info;

const REQUEST_TIMEOUT_DURATION: Duration = Duration::from_secs(300);
/// Gemini 3.x thinking tokens count against this cap; 4096 cuts a minutes document mid-sentence.
const CUSTOM_OPENAI_DEFAULT_MAX_TOKENS: u32 = 32768;

// Generic structure for OpenAI-compatible API chat messages
#[derive(Debug, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// Generic structure for OpenAI-compatible API chat requests
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

// Claude-specific request structure
#[derive(Debug, Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: String,
    pub messages: Vec<ChatMessage>,
}

// Claude-specific response structure
#[derive(Deserialize, Debug)]
pub struct ClaudeChatResponse {
    pub content: Vec<ClaudeChatContent>,
}

#[derive(Deserialize, Debug)]
pub struct ClaudeChatContent {
    pub text: String,
}

/// LLM Provider enumeration for multi-provider support
#[derive(Debug, Clone, PartialEq)]
pub enum LLMProvider {
    OpenAI,
    Claude,
    OpenRouter,
    CustomOpenAI,
}

impl LLMProvider {
    /// Parse provider from string (case-insensitive)
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(Self::OpenAI),
            "claude" => Ok(Self::Claude),
            "openrouter" => Ok(Self::OpenRouter),
            "custom-openai" => Ok(Self::CustomOpenAI),
            "ollama" | "groq" => Err(
                "Nhà cung cấp này không còn được hỗ trợ. Vui lòng chọn nhà cung cấp khác trong Cài đặt."
                    .to_string(),
            ),
            _ => Err(format!("Unsupported LLM provider: {}", s)),
        }
    }
}

/// Generates a summary using the specified LLM provider
///
/// # Arguments
/// * `client` - Reqwest HTTP client (reused for performance)
/// * `provider` - The LLM provider to use
/// * `model_name` - The specific model to use (e.g., "gpt-4", "claude-3-opus")
/// * `api_key` - API key for the provider (not needed for Ollama)
/// * `system_prompt` - System instructions for the LLM
/// * `user_prompt` - User query/content to process
/// * `app_data_dir` - Reserved for future local model paths
/// * `custom_openai_endpoint` - Optional custom OpenAI-compatible endpoint
/// * `max_tokens` - Optional max tokens (for CustomOpenAI provider)
/// * `temperature` - Optional temperature (for CustomOpenAI provider)
/// * `top_p` - Optional top_p (for CustomOpenAI provider)
/// * `app_data_dir` - Unused, kept for API compatibility
/// * `cancellation_token` - Optional token to cancel the request
///
/// # Returns
/// The generated summary text or an error message
pub async fn generate_summary(
    client: &Client,
    provider: &LLMProvider,
    model_name: &str,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    custom_openai_endpoint: Option<&str>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    app_data_dir: Option<&PathBuf>,
    cancellation_token: Option<&CancellationToken>,
) -> Result<String, String> {
    // Check if cancelled before starting
    if let Some(token) = cancellation_token {
        if token.is_cancelled() {
            return Err("Summary generation was cancelled".to_string());
        }
    }

    let (api_url, mut headers) = match provider {
        LLMProvider::OpenAI => (
            "https://api.openai.com/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::OpenRouter => (
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::CustomOpenAI => {
            let endpoint = custom_openai_endpoint
                .ok_or_else(|| "Custom OpenAI endpoint not configured".to_string())?;
            (chat_completions_url(endpoint), header::HeaderMap::new())
        }
        LLMProvider::Claude => {
            let mut header_map = header::HeaderMap::new();
            header_map.insert(
                "x-api-key",
                api_key
                    .parse()
                    .map_err(|_| "Invalid API key format".to_string())?,
            );
            header_map.insert(
                "anthropic-version",
                "2023-06-01"
                    .parse()
                    .map_err(|_| "Invalid anthropic version".to_string())?,
            );
            ("https://api.anthropic.com/v1/messages".to_string(), header_map)
        }
    };

    // Add authorization header for non-Claude providers
    if provider != &LLMProvider::Claude {
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", api_key)
                .parse()
                .map_err(|_| "Invalid authorization header".to_string())?,
        );
    }
    headers.insert(
        header::CONTENT_TYPE,
        "application/json"
            .parse()
            .map_err(|_| "Invalid content type".to_string())?,
    );

    // Build request body based on provider
    let request_body = if provider != &LLMProvider::Claude {
        // For CustomOpenAI, apply optional parameters if provided.
        // If not provided, use defaults commonly expected for OpenAI-compatible servers.
        let (max_tokens_val, temperature_val, top_p_val, reasoning_effort) =
            if provider == &LLMProvider::CustomOpenAI
        {
            let reasoning = if api_url.contains("generativelanguage.googleapis.com") {
                Some("low".to_string())
            } else {
                None
            };
            (
                max_tokens.or(Some(CUSTOM_OPENAI_DEFAULT_MAX_TOKENS)),
                temperature.or(Some(0.2)),
                top_p.or(Some(0.9)),
                reasoning,
            )
        } else {
            (None, None, None, None)
        };

        serde_json::json!(ChatRequest {
            model: model_name.to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt.to_string(),
                }
            ],
            max_tokens: max_tokens_val,
            temperature: temperature_val,
            top_p: top_p_val,
            reasoning_effort,
        })
    } else {
        serde_json::json!(ClaudeRequest {
            system: system_prompt.to_string(),
            model: model_name.to_string(),
            max_tokens: 2048,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            }]
        })
    };

    info!("🐞 LLM Request to {}: model={}", provider_name(provider), model_name);

    // Send request with timeout and cancellation support
    let request_future = client
        .post(api_url)
        .headers(headers)
        .json(&request_body)
        .timeout(REQUEST_TIMEOUT_DURATION)
        .send();

    // Use tokio::select to race between cancellation and request completion
    let response = if let Some(token) = cancellation_token {
        tokio::select! {
            result = request_future => {
                result.map_err(|e| {
                    if e.is_timeout() {
                        format!("LLM request timed out after 300 seconds")
                    } else {
                        format!("Failed to send request to LLM: {}", e)
                    }
                })?
            }
            _ = token.cancelled() => {
                return Err("Summary generation was cancelled".to_string());
            }
        }
    } else {
        request_future.await.map_err(|e| {
            if e.is_timeout() {
                format!("LLM request timed out after 300 seconds")
            } else {
                format!("Failed to send request to LLM: {}", e)
            }
        })?
    };

    if !response.status().is_success() {
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        // Phát hiện lỗi giới hạn token hàng ngày (TPD) — cần thông báo thân thiện kèm thời gian chờ
        let error_lower = error_body.to_lowercase();
        if error_lower.contains("tokens per day")
            || error_lower.contains("tokens_per_day")
            || (error_lower.contains("tpd") && error_lower.contains("limit"))
            || error_lower.contains("daily limit")
            || error_lower.contains("daily token")
        {
            let wait_msg = extract_wait_time(&error_body)
                .map(|t| format!(" Vui lòng thử lại sau {}.", t))
                .unwrap_or_else(|| " Vui lòng thử lại vào ngày mai hoặc đổi nhà cung cấp.".to_string());
            return Err(format!(
                "Đã vượt giới hạn token hàng ngày của nhà cung cấp.{} (Chi tiết: {})",
                wait_msg, error_body
            ));
        }

        return Err(format!("LLM API request failed: {}", error_body));
    }

    // Parse response based on provider
    if provider == &LLMProvider::Claude {
        let chat_response = response
            .json::<ClaudeChatResponse>()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from Claude");

        let content = chat_response
            .content
            .get(0)
            .ok_or("No content in LLM response")?
            .text
            .trim();
        Ok(content.to_string())
    } else {
        let chat_response = response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from {}", provider_name(provider));

        let content = extract_completion_text(&chat_response)
            .ok_or("No content in LLM response")?;
        Ok(content.trim().to_string())
    }
}

/// Build `{base}/chat/completions`, rewriting Gemini native REST to the OpenAI-compat base.
pub fn chat_completions_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let without_completions = trimmed
        .strip_suffix("/chat/completions")
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    format!("{}/chat/completions", rewrite_gemini_native_base(without_completions))
}

fn rewrite_gemini_native_base(base: &str) -> String {
    let lower = base.to_lowercase();
    if lower.contains("generativelanguage.googleapis.com") && !lower.contains("/openai") {
        return "https://generativelanguage.googleapis.com/v1beta/openai".to_string();
    }
    base.to_string()
}

/// Pull assistant text from OpenAI chat completions or Gemini native `candidates`.
pub fn extract_completion_text(json: &serde_json::Value) -> Option<String> {
    if let Some(text) = extract_choices_text(json) {
        return Some(text);
    }
    extract_candidates_text(json)
}

/// True when the body is an OpenAI chat envelope, even if thinking used all tokens.
pub fn looks_openai_compatible(json: &serde_json::Value) -> bool {
    if extract_completion_text(json).is_some() {
        return true;
    }
    if json.get("object").and_then(|v| v.as_str()) == Some("chat.completion") {
        return true;
    }
    let Some(choices) = json.get("choices").and_then(|c| c.as_array()) else {
        return false;
    };
    choices
        .iter()
        .any(|c| c.get("message").is_some() || c.get("delta").is_some())
        || json.get("model").is_some()
        || json.get("usage").is_some()
}

fn extract_choices_text(json: &serde_json::Value) -> Option<String> {
    let choice = json.get("choices")?.as_array()?.first()?;
    let message = choice.get("message").or_else(|| choice.get("delta"))?;
    text_from_content_field(message.get("content"))
        .or_else(|| text_from_content_field(message.get("reasoning_content")))
        .or_else(|| text_from_content_field(message.get("reasoning")))
}

fn extract_candidates_text(json: &serde_json::Value) -> Option<String> {
    let parts = json
        .get("candidates")?
        .as_array()?
        .first()?
        .get("content")?
        .get("parts")?
        .as_array()?;
    let joined: String = parts
        .iter()
        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
        .collect();
    nonempty_text(joined)
}

fn text_from_content_field(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(s) => nonempty_text(s.clone()),
        serde_json::Value::Array(parts) => {
            let joined: String = parts
                .iter()
                .filter_map(|p| {
                    p.get("text")
                        .and_then(|t| t.as_str())
                        .or_else(|| p.as_str())
                })
                .collect();
            nonempty_text(joined)
        }
        _ => None,
    }
}

fn nonempty_text(s: String) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Trích xuất thời gian chờ từ error body, ví dụ "Please try again in 2h18m2.304s"
pub fn extract_wait_time(body: &str) -> Option<String> {
    let patterns = [
        "please try again in ",
        "retry after ",
    ];
    let body_lower = body.to_lowercase();
    for pat in &patterns {
        if let Some(idx) = body_lower.find(pat) {
            let rest = &body[idx + pat.len()..];
            let end = rest
                .find(|c: char| c == '.' || c == ',' || c == '"' || c == '\n')
                .unwrap_or(rest.len().min(40));
            let time_str = rest[..end].trim().to_string();
            if !time_str.is_empty() {
                return Some(time_str);
            }
        }
    }
    None
}

/// Helper function to get provider name for logging
fn provider_name(provider: &LLMProvider) -> &str {
    match provider {
        LLMProvider::OpenAI => "OpenAI",
        LLMProvider::Claude => "Claude",
        LLMProvider::OpenRouter => "OpenRouter",
        LLMProvider::CustomOpenAI => "Custom OpenAI",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rewrites_gemini_native_rest_to_openai_compat() {
        assert_eq!(
            chat_completions_url("https://generativelanguage.googleapis.com/v1beta"),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
        assert_eq!(
            chat_completions_url(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite:generateContent"
            ),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
    }

    #[test]
    fn keeps_openai_compat_and_strips_duplicate_path() {
        assert_eq!(
            chat_completions_url("https://generativelanguage.googleapis.com/v1beta/openai"),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
        assert_eq!(
            chat_completions_url(
                "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
            ),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
    }

    #[test]
    fn extracts_openai_and_gemini_payloads() {
        let openai = json!({"choices":[{"message":{"content":"Xin chào"}}]});
        assert_eq!(extract_completion_text(&openai).as_deref(), Some("Xin chào"));

        let reasoning = json!({"choices":[{"message":{"reasoning_content":"ok"}}]});
        assert_eq!(extract_completion_text(&reasoning).as_deref(), Some("ok"));

        let parts = json!({"choices":[{"message":{"content":[{"type":"text","text":"Hi"}]}}]});
        assert_eq!(extract_completion_text(&parts).as_deref(), Some("Hi"));

        let native = json!({"candidates":[{"content":{"parts":[{"text":"Native"}]}}]});
        assert_eq!(extract_completion_text(&native).as_deref(), Some("Native"));
    }

    #[test]
    fn gemini_request_includes_low_reasoning_and_large_cap() {
        let body = serde_json::to_value(ChatRequest {
            model: "gemini-3.6-flash".into(),
            messages: vec![],
            max_tokens: Some(CUSTOM_OPENAI_DEFAULT_MAX_TOKENS),
            temperature: Some(0.2),
            top_p: Some(0.9),
            reasoning_effort: Some("low".into()),
        })
        .unwrap();
        assert_eq!(body["max_tokens"], 32768);
        assert_eq!(body["reasoning_effort"], "low");
    }

    #[test]
    fn thinking_envelope_without_text_still_looks_compatible() {
        let empty_think = json!({
            "object": "chat.completion",
            "model": "gemini-3.1-flash-lite",
            "choices": [{"message": {"role": "assistant"}, "finish_reason": "length"}],
            "usage": {"completion_tokens": 5}
        });
        assert!(looks_openai_compatible(&empty_think));
        assert!(extract_completion_text(&empty_think).is_none());
    }
}
