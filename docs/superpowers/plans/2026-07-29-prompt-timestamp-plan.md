# Prompt Timestamp Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the LLM that generates the final meeting report an explicit "now" and "when the meeting happened" so it can correctly resolve relative/partial dates in the transcript instead of guessing.

**Architecture:** Add two new placeholder tokens (`{current_datetime}`, `{meeting_datetime}`) to `SYSTEM_PROMPT_FINAL_TEMPLATE`. A new pure helper function in `processor.rs` fills them in alongside the existing `{section_instructions}`/`{template_markdown}` substitution. `service.rs` fetches the meeting's `created_at` from the DB (already-existing `MeetingsRepository::get_meeting_metadata`) and threads it through.

**Tech Stack:** Rust (`chrono` — already a dependency with default "clock" feature), no new crates.

Spec: `docs/superpowers/specs/2026-07-29-prompt-timestamp-design.md`

---

### Task 1: Add `build_final_system_prompt` helper + wire into `generate_meeting_summary`

**Files:**
- Modify: `frontend/src-tauri/src/summary/processor.rs`

- [ ] **Step 1: Write the failing test**

Add to the bottom of `frontend/src-tauri/src/summary/processor.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails to compile**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml build_final_system_prompt_replaces_all_placeholders`
Expected: compile error — `build_final_system_prompt` not found in this scope, and `DateTime`/`Local` not in scope.

- [ ] **Step 3: Add the `chrono` import**

At the top of `frontend/src-tauri/src/summary/processor.rs`, change:

```rust
use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::prompt_config::PromptConfig;
use crate::summary::templates;
use once_cell::sync::Lazy;
```

to:

```rust
use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::prompt_config::PromptConfig;
use crate::summary::templates;
use chrono::{DateTime, Local, Utc};
use once_cell::sync::Lazy;
```

- [ ] **Step 4: Add the `build_final_system_prompt` helper**

Insert this function right after `extract_meeting_name_from_markdown` (after its closing `}`, before the `/// Generates a complete meeting summary...` doc comment):

```rust
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
```

- [ ] **Step 5: Add `meeting_created_at` parameter to `generate_meeting_summary` and use the helper**

Change the signature (currently ending with `prompt_config: &PromptConfig,`):

```rust
    app_data_dir: Option<&PathBuf>,
    cancellation_token: Option<&CancellationToken>,
    prompt_config: &PromptConfig,
) -> Result<(String, i64), String> {
```

to:

```rust
    app_data_dir: Option<&PathBuf>,
    cancellation_token: Option<&CancellationToken>,
    prompt_config: &PromptConfig,
    meeting_created_at: DateTime<Utc>,
) -> Result<(String, i64), String> {
```

Then replace the existing substitution block:

```rust
    let final_system_prompt = prompt_config
        .system_prompt_final_template
        .replace("{section_instructions}", &section_instructions)
        .replace("{template_markdown}", &clean_template_markdown);
```

with:

```rust
    let final_system_prompt = build_final_system_prompt(
        &prompt_config.system_prompt_final_template,
        &section_instructions,
        &clean_template_markdown,
        meeting_created_at,
        Local::now(),
    );
```

- [ ] **Step 6: Run test to verify it fails (compiles now, but call site in service.rs is broken)**

Run: `cargo check --manifest-path frontend/src-tauri/Cargo.toml`
Expected: FAIL — `generate_meeting_summary` call in `service.rs` is missing the new `meeting_created_at` argument. This is expected; it's fixed in Task 3.

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/summary/processor.rs
git commit -m "feat: substitute meeting/current datetime into final report system prompt"
```

(This commit will not compile standalone since `service.rs` isn't updated yet — that's fine, Task 3 fixes it in the same session before any push.)

---

### Task 2: Add time-info block and rewrite date rule in the prompt template

**Files:**
- Modify: `frontend/src-tauri/src/summary/prompts.rs`

- [ ] **Step 1: Write the failing test**

Add to the bottom of `frontend/src-tauri/src/summary/prompts.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_final_template_contains_time_placeholders() {
        assert!(SYSTEM_PROMPT_FINAL_TEMPLATE.contains("{meeting_datetime}"));
        assert!(SYSTEM_PROMPT_FINAL_TEMPLATE.contains("{current_datetime}"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml system_prompt_final_template_contains_time_placeholders`
Expected: FAIL — assertion fails, template doesn't contain `{meeting_datetime}` yet.

- [ ] **Step 3: Update the template constant**

Replace the full `SYSTEM_PROMPT_FINAL_TEMPLATE` constant body with:

```rust
pub const SYSTEM_PROMPT_FINAL_TEMPLATE: &str = r#"Bạn là Trợ lý Tóm tắt Cuộc họp AI cấp cao, có nhiệm vụ xử lý văn bản nguồn một cách chính xác, toàn vẹn và chi tiết ở mức tối đa. Hãy tạo báo cáo cuối cùng bằng cách điền vào mẫu Markdown dựa trên văn bản nguồn.

**NGÔN NGỮ:** Trả lời hoàn toàn bằng tiếng Việt.

**THÔNG TIN THỜI GIAN:**
- Thời điểm cuộc họp diễn ra: {meeting_datetime}
- Thời điểm tạo báo cáo này: {current_datetime}

**CÁC NGUYÊN TẮC CỐT LÕI:**
1. Nguyên tắc toàn vẹn: Chỉ sử dụng thông tin có sẵn trong transcript. Không tự ý thêm bớt, suy diễn hoặc nhận xét cá nhân.
2. Trích xuất toàn diện: Ghi lại TẤT CẢ các chi tiết thực tế bao gồm: con số (tài chính, %, số lượng), mốc thời gian, ngày tháng, tên người và chức danh.
3. Chuẩn hóa ngày tháng (BẮT BUỘC): Tất cả các mốc ngày tháng xuất hiện trong báo cáo phải được quy đổi và hiển thị đồng nhất theo định dạng `dd/mm/yyyy` (Ví dụ: "ngày 5 tháng 4 năm 2026" hoặc "4/5" phải được viết thành "05/04/2026"). Nếu không có năm trong transcript, sử dụng năm của thời điểm cuộc họp diễn ra ({meeting_datetime} ở trên).
4. Chống tóm tắt sơ sài:
   - Không gộp các ý kiến khác nhau thành một câu khái quát chung.
   - Liệt kê đầy đủ mọi khía cạnh/ý kiến của từng người phát biểu.
   - Không dùng các từ viết tắt đại khái như: "v.v...", "và các vấn đề khác", "như trên".
5. Xử lý dữ liệu thiếu: Nếu một thông tin bị thiếu một phần (ví dụ: có việc nhưng không có người làm, hoặc không có deadline), bắt buộc phải ghi rõ từ "(không rõ)" ngay tại vị trí đó. Chỉ ghi "Không có thông tin trong transcript" nếu mục đó hoàn toàn không được nhắc đến.

**HƯỚNG DẪN THEO TỪNG MỤC:**
{section_instructions}

<template>
{template_markdown}
</template>
"#;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml system_prompt_final_template_contains_time_placeholders`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/summary/prompts.rs
git commit -m "feat: add time-info block to final report system prompt template"
```

---

### Task 3: Fetch meeting's `created_at` in `service.rs` and pass it through

**Files:**
- Modify: `frontend/src-tauri/src/summary/service.rs`

- [ ] **Step 1: Add the `chrono` import**

At the top of `frontend/src-tauri/src/summary/service.rs`, change:

```rust
use crate::database::repositories::{
    meeting::MeetingsRepository, setting::SettingsRepository, summary::SummaryProcessesRepository,
};
use crate::summary::llm_client::LLMProvider;
use crate::summary::processor::{extract_meeting_name_from_markdown, generate_meeting_summary};
use sqlx::SqlitePool;
```

to:

```rust
use crate::database::repositories::{
    meeting::MeetingsRepository, setting::SettingsRepository, summary::SummaryProcessesRepository,
};
use crate::summary::llm_client::LLMProvider;
use crate::summary::processor::{extract_meeting_name_from_markdown, generate_meeting_summary};
use chrono::Utc;
use sqlx::SqlitePool;
```

- [ ] **Step 2: Fetch `meeting_created_at` before the model-fallback loop**

In `process_transcript_background`, immediately before `let client = reqwest::Client::new();` (right after the `prompt_config`/`fallback_models` setup block), insert:

```rust
        let meeting_created_at = match MeetingsRepository::get_meeting_metadata(&pool, &meeting_id).await {
            Ok(Some(meeting)) => meeting.created_at.0,
            Ok(None) => {
                warn!(
                    "Meeting {} not found when fetching created_at for prompt timestamp; using now()",
                    meeting_id
                );
                Utc::now()
            }
            Err(e) => {
                warn!(
                    "Failed to fetch meeting created_at for prompt timestamp: {}. Using now()",
                    e
                );
                Utc::now()
            }
        };

        let client = reqwest::Client::new();
```

(This replaces the standalone `let client = reqwest::Client::new();` line with the block above followed by that same line.)

- [ ] **Step 3: Pass `meeting_created_at` into the `generate_meeting_summary` call**

Change:

```rust
            let attempt = generate_meeting_summary(
                &client,
                &provider,
                current_model,
                &final_api_key,
                &text,
                &custom_prompt,
                &template_id,
                custom_openai_endpoint.as_deref(),
                custom_openai_max_tokens,
                custom_openai_temperature,
                custom_openai_top_p,
                app_data_dir.as_ref(),
                Some(&cancellation_token),
                &prompt_config,
            )
            .await;
```

to:

```rust
            let attempt = generate_meeting_summary(
                &client,
                &provider,
                current_model,
                &final_api_key,
                &text,
                &custom_prompt,
                &template_id,
                custom_openai_endpoint.as_deref(),
                custom_openai_max_tokens,
                custom_openai_temperature,
                custom_openai_top_p,
                app_data_dir.as_ref(),
                Some(&cancellation_token),
                &prompt_config,
                meeting_created_at,
            )
            .await;
```

- [ ] **Step 4: Verify the whole crate builds and both new tests pass**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml build_final_system_prompt_replaces_all_placeholders system_prompt_final_template_contains_time_placeholders`
Expected: both tests PASS, no compile errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/summary/service.rs
git commit -m "feat: thread meeting created_at through to the report system prompt"
```

---

### Task 4: Surface the new placeholders in the Prompt Settings UI

**Files:**
- Modify: `frontend/src/components/PromptSettings.tsx:26`

- [ ] **Step 1: Update the placeholders list**

Change:

```typescript
    placeholders: ['{section_instructions}', '{template_markdown}'],
```

to:

```typescript
    placeholders: ['{section_instructions}', '{template_markdown}', '{meeting_datetime}', '{current_datetime}'],
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/PromptSettings.tsx
git commit -m "feat: list new timestamp placeholders in prompt settings UI"
```

---

### Task 5: Final verification

- [ ] **Step 1: Full crate check**

Run: `cargo check --manifest-path frontend/src-tauri/Cargo.toml`
Expected: no errors, no warnings about unused imports (`DateTime`, `Local`, `Utc` all used).

- [ ] **Step 2: Full test run for the summary module**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml summary::`
Expected: all tests in `summary::processor` and `summary::prompts` PASS.
