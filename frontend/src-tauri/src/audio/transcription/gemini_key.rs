#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttProvider {
    Asr,
    Gemini,
}

impl SttProvider {
    pub fn from_db(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("gemini") => Self::Gemini,
            _ => Self::Asr,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asr => "asr",
            Self::Gemini => "gemini",
        }
    }
}

pub async fn resolve_stt_api_key(pool: &sqlx::SqlitePool) -> Result<String, String> {
    use crate::database::repositories::setting::SettingsRepository;

    let transcript_override = SettingsRepository::get_transcript_api_key(pool, "gemini")
        .await
        .ok()
        .flatten();
    let custom_openai = SettingsRepository::get_api_key(pool, "custom-openai")
        .await
        .ok()
        .flatten();
    let llm_provider_key = match SettingsRepository::get_model_config(pool).await {
        Ok(Some(cfg)) if !cfg.provider.is_empty() && cfg.provider != "custom-openai" => {
            SettingsRepository::get_api_key(pool, &cfg.provider)
                .await
                .ok()
                .flatten()
        }
        _ => None,
    };

    pick_stt_api_key(
        transcript_override.as_deref(),
        custom_openai.as_deref(),
        llm_provider_key.as_deref(),
    )
    .ok_or_else(|| {
        "Chưa có API key Gemini. Nhập key ở Cài đặt → Nhận dạng, hoặc key LLM (custom-openai / Gemini)."
            .to_string()
    })
}

pub fn pick_stt_api_key(
    transcript_override: Option<&str>,
    custom_openai: Option<&str>,
    llm_provider_key: Option<&str>,
) -> Option<String> {
    for candidate in [transcript_override, custom_openai, llm_provider_key] {
        if let Some(key) = candidate.map(str::trim).filter(|s| !s.is_empty()) {
            return Some(key.to_string());
        }
    }
    None
}

pub fn f32_to_pcm16_le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

pub fn vocabulary_from_hotwords(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .take(100)
        .map(|s| s.to_string())
        .collect()
}

pub fn needs_local_asr(provider: SttProvider) -> bool {
    provider == SttProvider::Asr
}

pub fn is_stt_key_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("api key") || lower.contains("gemini") || lower.contains("key")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_defaults_to_asr() {
        assert_eq!(SttProvider::from_db(None), SttProvider::Asr);
        assert_eq!(SttProvider::from_db(Some("")), SttProvider::Asr);
        assert_eq!(SttProvider::from_db(Some("asr")), SttProvider::Asr);
        assert_eq!(SttProvider::from_db(Some("gemini")), SttProvider::Gemini);
    }

    #[test]
    fn key_prefers_override_then_custom_openai_then_llm() {
        assert_eq!(
            pick_stt_api_key(Some("  ov  "), Some("co"), Some("llm")).as_deref(),
            Some("ov")
        );
        assert_eq!(
            pick_stt_api_key(Some("  "), Some("co"), Some("llm")).as_deref(),
            Some("co")
        );
        assert_eq!(
            pick_stt_api_key(None, None, Some("llm")).as_deref(),
            Some("llm")
        );
        assert_eq!(pick_stt_api_key(None, Some(""), None), None);
    }

    #[test]
    fn pcm16_clips_and_is_little_endian() {
        let bytes = f32_to_pcm16_le(&[0.0, 1.5, -2.0]);
        assert_eq!(bytes.len(), 6);
        assert_eq!(&bytes[0..2], &[0, 0]);
        assert_eq!(i16::from_le_bytes([bytes[2], bytes[3]]), 32767);
        assert_eq!(i16::from_le_bytes([bytes[4], bytes[5]]), -32767);
    }

    #[test]
    fn gemini_skips_local_asr() {
        assert!(!needs_local_asr(SttProvider::Gemini));
        assert!(needs_local_asr(SttProvider::Asr));
    }

    #[test]
    fn gemini_key_error_matches_resolve_message() {
        assert!(is_stt_key_error(
            "Chưa có API key Gemini. Nhập key ở Cài đặt → Nhận dạng, hoặc key LLM (custom-openai / Gemini)."
        ));
        assert!(!is_stt_key_error(
            "Recording cannot start: Transcription model is still downloading. Please wait for the download to complete."
        ));
    }

    #[test]
    fn vocabulary_skips_comments_and_caps_at_100() {
        let mut text = String::from("# header\nACT\n\n");
        for i in 0..120 {
            text.push_str(&format!("w{i}\n"));
        }
        let v = vocabulary_from_hotwords(&text);
        assert_eq!(v[0], "ACT");
        assert_eq!(v.len(), 100);
    }
}
