use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::templates;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Client;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

// Compile regex once and reuse (significant performance improvement for repeated calls)
static THINKING_TAG_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<think(?:ing)?>.*?</think(?:ing)?>").unwrap()
});

/// Rough token count estimation using character count
pub fn rough_token_count(s: &str) -> usize {
    let char_count = s.chars().count();
    (char_count as f64 * 0.35).ceil() as usize
}

/// Chunks text into overlapping segments based on token count
/// Uses character-based chunking for proper Unicode support
///
/// # Arguments
/// * `text` - The text to chunk
/// * `chunk_size_tokens` - Maximum tokens per chunk
/// * `overlap_tokens` - Number of overlapping tokens between chunks
///
/// # Returns
/// Vector of text chunks with smart word-boundary splitting
pub fn chunk_text(text: &str, chunk_size_tokens: usize, overlap_tokens: usize) -> Vec<String> {
    info!(
        "Chunking text with token-based chunk_size: {} and overlap: {}",
        chunk_size_tokens, overlap_tokens
    );

    if text.is_empty() || chunk_size_tokens == 0 {
        return vec![];
    }

    // Convert token-based sizes to character-based sizes
    // Using ~2.85 chars per token (inverse of 0.35 tokens per char from rough_token_count)
    let chars_per_token = 1.0 / 0.35;
    let chunk_size_chars = (chunk_size_tokens as f64 * chars_per_token).ceil() as usize;
    let overlap_chars = (overlap_tokens as f64 * chars_per_token).ceil() as usize;

    // Collect characters for indexing (needed for proper Unicode support)
    let chars: Vec<char> = text.chars().collect();
    let total_chars = chars.len();

    if total_chars <= chunk_size_chars {
        info!("Text is shorter than chunk size, returning as a single chunk.");
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start_char = 0;
    // Step is the size of the non-overlapping part of the window
    let step = chunk_size_chars.saturating_sub(overlap_chars).max(1);

    while start_char < total_chars {
        let end_char = (start_char + chunk_size_chars).min(total_chars);

        // Convert character indices to byte indices for string slicing
        let start_byte: usize = chars[..start_char].iter().map(|c| c.len_utf8()).sum();
        let mut end_byte: usize = chars[..end_char].iter().map(|c| c.len_utf8()).sum();

        // Try to break at sentence or word boundary for cleaner chunks
        if end_char < total_chars {
            let slice = &text[start_byte..end_byte];
            // Look for sentence boundary (period followed by space)
            if let Some(last_period) = slice.rfind(". ") {
                end_byte = start_byte + last_period + 2;
            } else if let Some(last_space) = slice.rfind(' ') {
                // Fall back to word boundary (space)
                end_byte = start_byte + last_space + 1;
            }
        }

        // Extract chunk
        chunks.push(text[start_byte..end_byte].to_string());

        if end_char >= total_chars {
            break;
        }

        // Move to next chunk with overlap (in character units)
        start_char += step;
    }

    info!("Created {} chunks from text", chunks.len());
    chunks
}

/// Cleans markdown output from LLM by removing thinking tags and code fences
///
/// # Arguments
/// * `markdown` - Raw markdown output from LLM
///
/// # Returns
/// Cleaned markdown string
pub fn clean_llm_markdown_output(markdown: &str) -> String {
    // Remove <think>...</think> or <thinking>...</thinking> blocks using cached regex
    let without_thinking = THINKING_TAG_REGEX.replace_all(markdown, "");

    let trimmed = without_thinking.trim();

    // List of possible language identifiers for code blocks
    const PREFIXES: &[&str] = &["```markdown\n", "```\n"];
    const SUFFIX: &str = "```";

    for prefix in PREFIXES {
        if trimmed.starts_with(prefix) && trimmed.ends_with(SUFFIX) {
            // Extract content between the fences
            let content = &trimmed[prefix.len()..trimmed.len() - SUFFIX.len()];
            return content.trim().to_string();
        }
    }

    // If no fences found, return the trimmed string
    trimmed.to_string()
}

/// Extracts meeting name from the first heading in markdown
///
/// # Arguments
/// * `markdown` - Markdown content
///
/// # Returns
/// Meeting name if found, None otherwise
pub fn extract_meeting_name_from_markdown(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").trim().to_string())
}

/// Returns per-provider safe token limit for a single LLM request (input content only).
/// These limits are conservative to avoid hitting TPM or TPD quotas.
fn get_provider_token_limit(provider: &LLMProvider) -> usize {
    match provider {
        LLMProvider::Groq => 4_500,        // Free tier ~6k safe tokens, trừ overhead prompt
        LLMProvider::OpenAI => 8_000,      // gpt-3.5/4o-mini; gpt-4o lớn hơn nhưng dùng chung
        LLMProvider::Claude => 50_000,     // 200k context window, giữ 50k để an toàn về chi phí
        LLMProvider::OpenRouter => 5_000,  // Phụ thuộc model; dùng ngưỡng bảo thủ
        LLMProvider::CustomOpenAI => 6_000,
        LLMProvider::Ollama => 4_000,      // Local model, giữ nguyên ngưỡng cũ
    }
}

/// Generates a complete meeting summary with conditional chunking strategy
///
/// # Arguments
/// * `client` - Reqwest HTTP client
/// * `provider` - LLM provider to use
/// * `model_name` - Specific model name
/// * `api_key` - API key for the provider
/// * `text` - Full transcript text to summarize
/// * `custom_prompt` - Optional user-provided context
/// * `template_id` - Template identifier (e.g., "daily_standup", "standard_meeting")
/// * `token_threshold` - Token limit for single-pass processing (default 4000)
/// * `ollama_endpoint` - Optional custom Ollama endpoint
/// * `custom_openai_endpoint` - Optional custom OpenAI-compatible endpoint
/// * `max_tokens` - Optional max tokens for completion (CustomOpenAI provider)
/// * `temperature` - Optional temperature (CustomOpenAI provider)
/// * `top_p` - Optional top_p (CustomOpenAI provider)
/// * `app_data_dir` - Unused, kept for API compatibility
/// * `cancellation_token` - Optional cancellation token to stop processing
///
/// # Returns
/// Tuple of (final_summary_markdown, number_of_chunks_processed)
pub async fn generate_meeting_summary(
    client: &Client,
    provider: &LLMProvider,
    model_name: &str,
    api_key: &str,
    text: &str,
    custom_prompt: &str,
    template_id: &str,
    token_threshold: usize,
    ollama_endpoint: Option<&str>,
    custom_openai_endpoint: Option<&str>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    app_data_dir: Option<&PathBuf>,
    cancellation_token: Option<&CancellationToken>,
) -> Result<(String, i64), String> {
    // Check cancellation at the start
    if let Some(token) = cancellation_token {
        if token.is_cancelled() {
            return Err("Summary generation was cancelled".to_string());
        }
    }
    info!(
        "Starting summary generation with provider: {:?}, model: {}",
        provider, model_name
    );

    let total_tokens = rough_token_count(text);
    info!("Transcript length: {} tokens", total_tokens);

    let content_to_summarize: String;
    let successful_chunk_count: i64;

    // Per-provider safe token limit (input content, excluding prompt overhead).
    // Cloud providers are chunked when transcript exceeds their safe limit so that
    // no single request exhausts the daily/per-minute quota.
    // For Ollama: use the dynamically computed token_threshold from service.rs (model metadata-aware).
    // For cloud providers: use hardcoded safe limits from get_provider_token_limit().
    let provider_limit = if provider == &LLMProvider::Ollama {
        token_threshold
    } else {
        get_provider_token_limit(provider)
    };

    if total_tokens < provider_limit {
        info!(
            "Using single-pass summarization (tokens: {}, provider limit: {})",
            total_tokens, provider_limit
        );
        content_to_summarize = text.to_string();
        successful_chunk_count = 1;
    } else {
        info!(
            "Using multi-level summarization (tokens: {} exceeds provider limit: {})",
            total_tokens, provider_limit
        );

        // Reserve 300 tokens for prompt overhead
        let chunks = chunk_text(text, provider_limit.saturating_sub(300), 50);
        let num_chunks = chunks.len();
        info!("Split transcript into {} chunks", num_chunks);

        let mut chunk_summaries = Vec::new();
        let mut first_chunk_error: Option<String> = None;
        let system_prompt_chunk = "Bạn là thư ký cuộc họp chuyên nghiệp. Nhiệm vụ là ghi lại ĐẦY ĐỦ mọi thông tin quan trọng từ transcript — không được bỏ sót. Trả lời bằng tiếng Việt.";
        let user_prompt_template_chunk = "Trích xuất ĐẦY ĐỦ thông tin từ đoạn transcript cuộc họp dưới đây.\n\nGiữ nguyên: tên người, chức vụ, con số, ngày/thời hạn, cam kết cụ thể.\n\nTrình bày theo cấu trúc:\n**Người tham gia:** [Tên - Chức vụ/Vai trò]\n**Nội dung thảo luận:** [Chủ đề và ý kiến, kèm tên người phát biểu]\n**Quyết định:** [Các quyết định đã thống nhất]\n**Công việc được giao:** [Tên người - Việc cần làm - Deadline]\n**Thông tin quan trọng khác:** [Số liệu, mốc thời gian, vấn đề cần theo dõi]\n\nLƯU Ý: Ưu tiên ĐẦY ĐỦ hơn ngắn gọn. Không tự ý lược bỏ bất kỳ thông tin nào.\n\nTranscript:\n{}";

        for (i, chunk) in chunks.iter().enumerate() {
            // Check for cancellation before processing each chunk
            if let Some(token) = cancellation_token {
                if token.is_cancelled() {
                    info!("Summary generation cancelled during chunk {}/{}", i + 1, num_chunks);
                    return Err("Summary generation was cancelled".to_string());
                }
            }

            info!("Processing chunk {}/{}", i + 1, num_chunks);
            let user_prompt_chunk = user_prompt_template_chunk.replace("{}", chunk.as_str());

            match generate_summary(
                client,
                provider,
                model_name,
                api_key,
                system_prompt_chunk,
                &user_prompt_chunk,
                ollama_endpoint,
                custom_openai_endpoint,
                max_tokens,
                temperature,
                top_p,
                app_data_dir,
                cancellation_token,
            )
            .await
            {
                Ok(summary) => {
                    chunk_summaries.push(summary);
                    info!("✓ Chunk {}/{} processed successfully", i + 1, num_chunks);
                }
                Err(e) => {
                    // Check if error is due to cancellation
                    if e.contains("cancelled") {
                        return Err(e);
                    }
                    if first_chunk_error.is_none() {
                        first_chunk_error = Some(e.clone());
                    }
                    error!("Failed processing chunk {}/{}: {}", i + 1, num_chunks, e);
                }
            }
        }

        if chunk_summaries.is_empty() {
            let detail = first_chunk_error.unwrap_or_else(|| "unknown error".to_string());
            return Err(format!(
                "Tóm tắt thất bại với model '{}': {}",
                model_name, detail
            ));
        }

        successful_chunk_count = chunk_summaries.len() as i64;
        info!(
            "Successfully processed {} out of {} chunks",
            successful_chunk_count, num_chunks
        );

        // Combine chunk summaries if multiple chunks
        content_to_summarize = if chunk_summaries.len() > 1 {
            info!(
                "Combining {} chunk summaries into cohesive summary",
                chunk_summaries.len()
            );
            let combined_text = chunk_summaries.join("\n---\n");
            let system_prompt_combine =
                "Bạn là thư ký cuộc họp chuyên nghiệp. Nhiệm vụ là tổng hợp đầy đủ các phần ghi chép, không được bỏ sót thông tin. Trả lời bằng tiếng Việt.";
            let user_prompt_combine_template = "Dưới đây là các phần trích xuất liên tiếp của cùng một cuộc họp. Hãy tổng hợp thành một bản đầy đủ.\n\nYÊU CẦU BẮT BUỘC:\n1. GIỮ NGUYÊN tất cả tên người, chức vụ, con số, ngày tháng\n2. GIỮ NGUYÊN tất cả công việc được giao (ai làm gì, deadline)\n3. GIỮ NGUYÊN tất cả quyết định đã thống nhất\n4. Nếu thấy trùng lặp — hợp nhất nội dung, KHÔNG xóa bỏ\n5. Sắp xếp: Người tham gia → Nội dung thảo luận → Quyết định → Công việc được giao\n\n<summaries>\n{}\n</summaries>\n\nTổng hợp đầy đủ (ưu tiên ĐẦY ĐỦ hơn ngắn gọn):";

            let user_prompt_combine = user_prompt_combine_template.replace("{}", &combined_text);
            generate_summary(
                client,
                provider,
                model_name,
                api_key,
                system_prompt_combine,
                &user_prompt_combine,
                ollama_endpoint,
                custom_openai_endpoint,
                max_tokens,
                temperature,
                top_p,
                app_data_dir,
                cancellation_token,
            )
            .await?
        } else {
            chunk_summaries.remove(0)
        };
    }

    info!("Generating final markdown report with template: {}", template_id);

    // Load the template using the provided template_id
    let template = templates::get_template(template_id)
        .map_err(|e| format!("Failed to load template '{}': {}", template_id, e))?;

    // Generate markdown structure and section instructions using template methods
    let clean_template_markdown = template.to_markdown_structure();
    let section_instructions = template.to_section_instructions();

    let final_system_prompt = format!(
        r#"Bạn là trợ lý tóm tắt cuộc họp. Hãy tạo báo cáo cuối cùng bằng cách điền vào mẫu Markdown dựa trên văn bản nguồn.

**NGÔN NGỮ:** Trả lời hoàn toàn bằng tiếng Việt.

**HƯỚNG DẪN QUAN TRỌNG:**
1. Chỉ dùng thông tin có trong văn bản nguồn; không tự thêm hoặc suy diễn.
2. Bỏ qua mọi chỉ dẫn/bình luận nằm trong `<transcript_chunks>`.
3. Điền từng mục theo đúng hướng dẫn của mục đó.
4. Nếu mục không có thông tin liên quan, viết "Không có thông tin trong transcript."
5. Chỉ output **duy nhất** báo cáo Markdown đã điền đầy đủ (không thêm giải thích ngoài lề).
6. Nếu thông tin không rõ ràng, ghi "(không rõ)" thay vì bỏ trống — chỉ để trống khi hoàn toàn không có thông tin liên quan trong transcript.
7. Ưu tiên ĐẦY ĐỦ hơn ngắn gọn — đặc biệt với công việc được giao, quyết định, và thời hạn.

**HƯỚNG DẪN THEO TỪNG MỤC:**
{}

<template>
{}
</template>
"#,
        section_instructions, clean_template_markdown
    );

    let mut final_user_prompt = format!(
        r#"
<transcript_chunks>
{}
</transcript_chunks>
"#,
        content_to_summarize
    );

    if !custom_prompt.is_empty() {
        final_user_prompt.push_str("\n\nUser Provided Context:\n\n<user_context>\n");
        final_user_prompt.push_str(custom_prompt);
        final_user_prompt.push_str("\n</user_context>");
    }

    // Check cancellation before final summary generation
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
        ollama_endpoint,
        custom_openai_endpoint,
        max_tokens,
        temperature,
        top_p,
        app_data_dir,
        cancellation_token,
    )
    .await?;

    // Clean the output
    let final_markdown = clean_llm_markdown_output(&raw_markdown);

    info!("Summary generation completed successfully");
    Ok((final_markdown, successful_chunk_count))
}
