use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::prompt_config::PromptConfig;
use crate::summary::templates;
use chrono::{DateTime, Local, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Client;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
use tracing::info;

static THINKING_TAG_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<think(?:ing)?>.*?</think(?:ing)?>").unwrap()
});

/// Cleans markdown output from LLM by removing thinking tags and code fences
pub fn clean_llm_markdown_output(markdown: &str) -> String {
    let without_thinking = THINKING_TAG_REGEX.replace_all(markdown, "");
    let trimmed = without_thinking.trim();

    const PREFIXES: &[&str] = &["```markdown\n", "```\n"];
    const SUFFIX: &str = "```";

    for prefix in PREFIXES {
        if trimmed.starts_with(prefix) && trimmed.ends_with(SUFFIX) {
            let content = &trimmed[prefix.len()..trimmed.len() - SUFFIX.len()];
            return content.trim().to_string();
        }
    }

    trimmed.to_string()
}

/// Extracts meeting name from the first heading in markdown
pub fn extract_meeting_name_from_markdown(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").trim().to_string())
}

/// Builds the final system prompt by substituting all placeholder tokens,
/// including the current time and the meeting's own start time so the LLM
/// can resolve relative/partial dates found in the transcript.
fn build_final_system_prompt(
    template: &str,
    section_instructions: &str,
    template_markdown: &str,
    meeting_created_at: DateTime<Utc>,
    now: DateTime<Local>,
) -> String {
    let meeting_datetime = meeting_created_at
        .with_timezone(&Local)
        .format("%H:%M ngày %d/%m/%Y")
        .to_string();
    let current_datetime = now.format("%H:%M ngày %d/%m/%Y").to_string();

    template
        .replace("{section_instructions}", section_instructions)
        .replace("{template_markdown}", template_markdown)
        .replace("{meeting_datetime}", &meeting_datetime)
        .replace("{current_datetime}", &current_datetime)
}

/// Generates a complete meeting summary from the full transcript in a single pass.
pub async fn generate_meeting_summary(
    client: &Client,
    provider: &LLMProvider,
    model_name: &str,
    api_key: &str,
    text: &str,
    custom_prompt: &str,
    template_id: &str,
    custom_openai_endpoint: Option<&str>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    app_data_dir: Option<&PathBuf>,
    cancellation_token: Option<&CancellationToken>,
    prompt_config: &PromptConfig,
    meeting_created_at: DateTime<Utc>,
) -> Result<(String, i64), String> {
    if let Some(token) = cancellation_token {
        if token.is_cancelled() {
            return Err("Summary generation was cancelled".to_string());
        }
    }

    info!(
        "Starting full-transcript summary with provider: {:?}, model: {}",
        provider, model_name
    );

    info!("Generating final markdown report with template: {}", template_id);

    let template = templates::get_template(template_id)
        .map_err(|e| format!("Failed to load template '{}': {}", template_id, e))?;

    let clean_template_markdown = template.to_markdown_structure();
    let section_instructions = template.to_section_instructions();

    let final_system_prompt = build_final_system_prompt(
        &prompt_config.system_prompt_final_template,
        &section_instructions,
        &clean_template_markdown,
        meeting_created_at,
        Local::now(),
    );

    let mut final_user_prompt = format!(
        r#"
<transcript>
{}
</transcript>
"#,
        text
    );

    if !custom_prompt.is_empty() {
        final_user_prompt.push_str("\n\nUser Provided Context:\n\n<user_context>\n");
        final_user_prompt.push_str(custom_prompt);
        final_user_prompt.push_str("\n</user_context>");
    }

    if let Some(token) = cancellation_token {
        if token.is_cancelled() {
            info!("Summary generation cancelled before final summary");
            return Err("Summary generation was cancelled".to_string());
        }
    }

    let raw_markdown = generate_summary(
        client,
        provider,
        model_name,
        api_key,
        &final_system_prompt,
        &final_user_prompt,
        custom_openai_endpoint,
        max_tokens,
        temperature,
        top_p,
        app_data_dir,
        cancellation_token,
    )
    .await?;

    let final_markdown = clean_llm_markdown_output(&raw_markdown);

    info!("Summary generation completed successfully");
    Ok((final_markdown, 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn build_final_system_prompt_replaces_all_placeholders() {
        let template = "A:{section_instructions} B:{template_markdown} M:{meeting_datetime} N:{current_datetime}";
        let meeting_created_at = Utc.with_ymd_and_hms(2026, 3, 5, 2, 30, 0).unwrap();
        let now = Local.with_ymd_and_hms(2026, 7, 29, 10, 15, 0).unwrap();

        let result = build_final_system_prompt(template, "SEC", "TPL", meeting_created_at, now);

        assert!(result.contains("A:SEC"));
        assert!(result.contains("B:TPL"));
        assert!(result.contains("N:10:15 ngày 29/07/2026"));
        assert!(!result.contains('{'), "leftover placeholder in: {result}");

        let datetime_pattern = Regex::new(r"M:\d{2}:\d{2} ngày \d{2}/\d{2}/\d{4}").unwrap();
        assert!(
            datetime_pattern.is_match(&result),
            "meeting_datetime not formatted correctly: {result}"
        );
    }
}
