# Meeting Reference Documents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users attach reference documents (PDF/DOCX/PPTX) to an existing meeting, so their extracted text is included alongside the transcript when the AI generates the meeting report — fixing the gap where slide/document content referenced verbally ("as shown on this slide") never reaches the LLM.

**Architecture:** New `meeting_documents` DB table stores extracted text only (no original file). A new `meeting_documents` Rust module exposes Tauri commands to attach/list/delete documents, reusing and extending the existing `document_import::extractors` module (adding PPTX support). `service.rs` fetches attached documents by `meeting_id` and threads their concatenated text through `generate_meeting_summary`, which injects it into the LLM user prompt as a new `<meeting_documents>` block — separate from `<transcript>` and `<user_context>`. Frontend gets a new hook + dialog, surfaced via a "Tài liệu" button in the existing summary toolbar.

**Tech Stack:** Rust (sqlx, existing `regex`/`docx-rs`/`pdf-extract` extractors, `zip` crate — currently only a build-dependency, promoted to a runtime dependency for PPTX extraction), React/TypeScript (Tauri `invoke`, existing dialog/hook patterns).

Spec: `docs/superpowers/specs/2026-07-29-meeting-reference-documents-design.md`

---

### Task 1: DB schema + `MeetingDocument` model + repository

**Files:**
- Create: `frontend/src-tauri/migrations/20260729010000_add_meeting_documents.sql`
- Modify: `frontend/src-tauri/src/database/models.rs`
- Create: `frontend/src-tauri/src/database/repositories/meeting_document.rs`
- Modify: `frontend/src-tauri/src/database/repositories/mod.rs`

- [ ] **Step 1: Create the migration**

Create `frontend/src-tauri/migrations/20260729010000_add_meeting_documents.sql`:

```sql
CREATE TABLE IF NOT EXISTS meeting_documents (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    extracted_text TEXT NOT NULL,
    char_count INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_meeting_documents_meeting_id ON meeting_documents(meeting_id);
```

- [ ] **Step 2: Add the `MeetingDocument` model**

In `frontend/src-tauri/src/database/models.rs`, add this struct after the existing `MeetingModel`/`DateTimeUtc` block (i.e. after the `impl From<NaiveDateTime> for DateTimeUtc` block, before the `Transcript` struct):

```rust
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MeetingDocument {
    pub id: String,
    pub meeting_id: String,
    pub filename: String,
    pub extracted_text: String,
    pub char_count: i64,
    pub created_at: DateTimeUtc,
}
```

- [ ] **Step 3: Add the repository**

Create `frontend/src-tauri/src/database/repositories/meeting_document.rs`:

```rust
use crate::database::models::{DateTimeUtc, MeetingDocument};
use chrono::Utc;
use sqlx::{Error as SqlxError, SqlitePool};
use tracing::info;
use uuid::Uuid;

pub struct MeetingDocumentsRepository;

impl MeetingDocumentsRepository {
    /// Extracts and stores a new reference document attached to a meeting.
    pub async fn create(
        pool: &SqlitePool,
        meeting_id: &str,
        filename: &str,
        extracted_text: &str,
    ) -> Result<MeetingDocument, SqlxError> {
        let id = format!("meeting-document-{}", Uuid::new_v4());
        let now = Utc::now();
        let char_count = extracted_text.chars().count() as i64;

        sqlx::query(
            "INSERT INTO meeting_documents (id, meeting_id, filename, extracted_text, char_count, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(meeting_id)
        .bind(filename)
        .bind(extracted_text)
        .bind(char_count)
        .bind(now)
        .execute(pool)
        .await?;

        info!("Attached document '{}' to meeting {}", filename, meeting_id);

        Ok(MeetingDocument {
            id,
            meeting_id: meeting_id.to_string(),
            filename: filename.to_string(),
            extracted_text: extracted_text.to_string(),
            char_count,
            created_at: DateTimeUtc(now),
        })
    }

    /// Lists all documents attached to a meeting, oldest first.
    pub async fn list_by_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingDocument>, SqlxError> {
        sqlx::query_as::<_, MeetingDocument>(
            "SELECT id, meeting_id, filename, extracted_text, char_count, created_at
             FROM meeting_documents WHERE meeting_id = ? ORDER BY created_at ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    /// Deletes a document by id. Returns true if a row was actually deleted.
    pub async fn delete(pool: &SqlitePool, document_id: &str) -> Result<bool, SqlxError> {
        let result = sqlx::query("DELETE FROM meeting_documents WHERE id = ?")
            .bind(document_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
```

- [ ] **Step 4: Register the repository module**

In `frontend/src-tauri/src/database/repositories/mod.rs`, change:

```rust
pub mod meeting;
pub mod setting;
```

to:

```rust
pub mod meeting;
pub mod meeting_document;
pub mod setting;
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check --manifest-path frontend/src-tauri/Cargo.toml`
Expected: no errors. `MeetingDocumentsRepository` is unused at this point, which will produce a `dead_code` **warning**, not an error — that's expected and resolved once Task 6 wires it in. Confirm there are no *new errors*.

Migrations run automatically against the app's SQLite DB on next launch (sqlx embeds and auto-applies migration files from the `migrations/` directory at startup) — no manual migration-runner step needed here, consistent with how every prior migration in this repo was added.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/migrations/20260729010000_add_meeting_documents.sql frontend/src-tauri/src/database/models.rs frontend/src-tauri/src/database/repositories/meeting_document.rs frontend/src-tauri/src/database/repositories/mod.rs
git commit -m "feat: add meeting_documents table, model, and repository"
```

---

### Task 2: PPTX text extraction

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`
- Modify: `frontend/src-tauri/src/document_import/extractors.rs`

- [ ] **Step 1: Promote `zip` to a runtime dependency**

In `frontend/src-tauri/Cargo.toml`, the `zip` crate currently exists only under `[build-dependencies]` (line ~28: `zip = "2.2"           # ZIP extraction (Windows, macOS)`). Find the `[dependencies]` section and locate these two lines:

```toml
docx-rs = "0.4.17"
pdf-extract = "0.12.0"
```

Change them to:

```toml
docx-rs = "0.4.17"
pdf-extract = "0.12.0"
zip = "2.2"
```

(This adds `zip` as a normal dependency too — it does not remove or change the existing `[build-dependencies]` entry.)

- [ ] **Step 2: Write the failing tests**

In `frontend/src-tauri/src/document_import/extractors.rs`, add these two tests inside the existing `#[cfg(test)] mod tests { ... }` block at the bottom of the file (add them after `test_extract_from_pdf_invalid_bytes_returns_err`, before `test_extract_text_dispatches_by_extension`):

```rust
    #[test]
    fn test_extract_from_pptx_reads_slide_text_in_order() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.pptx");

        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        // Written out of order on purpose — extraction must sort by slide number.
        zip.start_file("ppt/slides/slide2.xml", options).unwrap();
        zip.write_all(b"<p:sld><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Second slide</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>").unwrap();

        zip.start_file("ppt/slides/slide1.xml", options).unwrap();
        zip.write_all(b"<p:sld><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>First slide</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>").unwrap();

        zip.finish().unwrap();

        let text = extract_from_pptx(&path).unwrap();
        let first_pos = text.find("First slide").expect("First slide text missing");
        let second_pos = text.find("Second slide").expect("Second slide text missing");
        assert!(
            first_pos < second_pos,
            "slides should be ordered by slide number, got: {}",
            text
        );
    }

    #[test]
    fn test_extract_from_pptx_decodes_xml_entities() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entities.pptx");

        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file("ppt/slides/slide1.xml", options).unwrap();
        zip.write_all(b"<a:t>Q&amp;A session</a:t>").unwrap();
        zip.finish().unwrap();

        let text = extract_from_pptx(&path).unwrap();
        assert!(text.contains("Q&A session"), "got: {}", text);
    }
```

- [ ] **Step 3: Run tests to verify they fail to compile**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml test_extract_from_pptx`
Expected: compile error — `extract_from_pptx` not found in this scope.

- [ ] **Step 4: Implement `extract_from_pptx`**

In `frontend/src-tauri/src/document_import/extractors.rs`, add this function after `extract_from_pdf` (before `fn extract_text`):

```rust
fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// PPTX is a ZIP archive of per-slide XML files under `ppt/slides/slideN.xml`.
/// Extracts visible text runs (`<a:t>...</a:t>`) from each slide, in slide order.
fn extract_from_pptx(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Lỗi đọc file: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Lỗi đọc PPTX: {}", e))?;

    let slide_path_re =
        regex::Regex::new(r"^ppt/slides/slide(\d+)\.xml$").expect("static regex is valid");
    let text_run_re = regex::Regex::new(r"<a:t>(.*?)</a:t>").expect("static regex is valid");

    let mut slide_indices: Vec<(usize, usize)> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("Lỗi đọc PPTX: {}", e))?;
        if let Some(captures) = slide_path_re.captures(entry.name()) {
            let slide_num: usize = captures[1].parse().unwrap_or(0);
            slide_indices.push((slide_num, i));
        }
    }
    slide_indices.sort_by_key(|(num, _)| *num);

    let mut text = String::new();
    for (_, index) in slide_indices {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("Lỗi đọc PPTX: {}", e))?;
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut entry, &mut xml)
            .map_err(|e| format!("Lỗi đọc PPTX: {}", e))?;

        for cap in text_run_re.captures_iter(&xml) {
            text.push_str(&decode_xml_entities(&cap[1]));
            text.push(' ');
        }
        text.push('\n');
    }

    Ok(text)
}
```

- [ ] **Step 5: Wire PPTX into the extension dispatch and supported list**

In `frontend/src-tauri/src/document_import/extractors.rs`, change:

```rust
pub const SUPPORTED_EXTENSIONS: &[&str] = &["pdf", "docx", "txt", "srt", "vtt"];
```

to:

```rust
pub const SUPPORTED_EXTENSIONS: &[&str] = &["pdf", "docx", "pptx", "txt", "srt", "vtt"];
```

Change `extract_text`:

```rust
    match extension.as_str() {
        "pdf" => extract_from_pdf(path),
        "docx" => extract_from_docx(path),
        "srt" | "vtt" => extract_from_subtitle(path),
        "txt" => extract_from_plain_text(path),
        other => Err(format!("Định dạng .{} không được hỗ trợ", other)),
    }
```

to:

```rust
    match extension.as_str() {
        "pdf" => extract_from_pdf(path),
        "docx" => extract_from_docx(path),
        "pptx" => extract_from_pptx(path),
        "srt" | "vtt" => extract_from_subtitle(path),
        "txt" => extract_from_plain_text(path),
        other => Err(format!("Định dạng .{} không được hỗ trợ", other)),
    }
```

Change `parse_file_segments`:

```rust
    let text = match extension.as_str() {
        "pdf" => extract_from_pdf(path)?,
        "docx" => extract_from_docx(path)?,
        "txt" | "srt" | "vtt" => {
            std::fs::read_to_string(path).map_err(|e| format!("Lỗi đọc file: {}", e))?
        }
        other => return Err(format!("Định dạng .{} không được hỗ trợ", other)),
    };
```

to:

```rust
    let text = match extension.as_str() {
        "pdf" => extract_from_pdf(path)?,
        "docx" => extract_from_docx(path)?,
        "pptx" => extract_from_pptx(path)?,
        "txt" | "srt" | "vtt" => {
            std::fs::read_to_string(path).map_err(|e| format!("Lỗi đọc file: {}", e))?
        }
        other => return Err(format!("Định dạng .{} không được hỗ trợ", other)),
    };
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml test_extract_from_pptx`
Expected: both tests PASS.

Then run the full extractors test suite to confirm nothing else broke:
Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml document_import::extractors::tests`
Expected: all tests PASS (the 2 new ones plus the pre-existing ones for txt/srt/vtt/docx/pdf).

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/Cargo.toml frontend/src-tauri/src/document_import/extractors.rs
git commit -m "feat: add PPTX text extraction support"
```

---

### Task 3: `build_final_user_prompt` helper + `documents_context` parameter

**Files:**
- Modify: `frontend/src-tauri/src/summary/processor.rs`

- [ ] **Step 1: Write the failing tests**

Add these three tests to the `#[cfg(test)] mod tests { ... }` block at the bottom of `frontend/src-tauri/src/summary/processor.rs` (after the existing `build_final_system_prompt_replaces_all_placeholders` test):

```rust
    #[test]
    fn build_final_user_prompt_includes_transcript() {
        let result = build_final_user_prompt("hello transcript", None, "");
        assert!(result.contains("<transcript>\nhello transcript\n</transcript>"));
    }

    #[test]
    fn build_final_user_prompt_includes_meeting_documents_when_present() {
        let result = build_final_user_prompt("t", Some("slide content here"), "");
        assert!(result.contains("<meeting_documents>\nslide content here\n</meeting_documents>"));
    }

    #[test]
    fn build_final_user_prompt_omits_meeting_documents_when_absent_or_empty() {
        let without = build_final_user_prompt("t", None, "");
        assert!(!without.contains("<meeting_documents>"));

        let empty = build_final_user_prompt("t", Some(""), "");
        assert!(!empty.contains("<meeting_documents>"));
    }

    #[test]
    fn build_final_user_prompt_orders_transcript_then_documents_then_user_context() {
        let result = build_final_user_prompt("t", Some("doc"), "please focus on X");

        let transcript_pos = result.find("<transcript>").expect("transcript missing");
        let documents_pos = result.find("<meeting_documents>").expect("meeting_documents missing");
        let context_pos = result.find("<user_context>").expect("user_context missing");

        assert!(transcript_pos < documents_pos, "transcript should come before meeting_documents");
        assert!(documents_pos < context_pos, "meeting_documents should come before user_context");
    }
```

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml build_final_user_prompt`
Expected: compile error — `build_final_user_prompt` not found in this scope.

- [ ] **Step 3: Extract the pure `build_final_user_prompt` helper**

In `frontend/src-tauri/src/summary/processor.rs`, add this function right after `build_final_system_prompt` (before the doc comment `/// Generates a complete meeting summary...`):

```rust
/// Builds the final user prompt: the transcript, optionally followed by a
/// `<meeting_documents>` block (reference materials attached to the meeting —
/// slides, docs — extracted as plain text) and/or a `<user_context>` block
/// (free-form instructions the user typed in). Both extra blocks are omitted
/// when their content is absent/empty.
fn build_final_user_prompt(
    text: &str,
    documents_context: Option<&str>,
    custom_prompt: &str,
) -> String {
    let mut final_user_prompt = format!(
        r#"
<transcript>
{}
</transcript>
"#,
        text
    );

    if let Some(docs) = documents_context {
        if !docs.is_empty() {
            final_user_prompt.push_str("\n\n<meeting_documents>\n");
            final_user_prompt.push_str(docs);
            final_user_prompt.push_str("\n</meeting_documents>");
        }
    }

    if !custom_prompt.is_empty() {
        final_user_prompt.push_str("\n\nUser Provided Context:\n\n<user_context>\n");
        final_user_prompt.push_str(custom_prompt);
        final_user_prompt.push_str("\n</user_context>");
    }

    final_user_prompt
}
```

- [ ] **Step 4: Use the helper in `generate_meeting_summary`, and add the `documents_context` parameter**

Change the signature (currently ending with `meeting_created_at: DateTime<Utc>,`):

```rust
    prompt_config: &PromptConfig,
    meeting_created_at: DateTime<Utc>,
) -> Result<(String, i64), String> {
```

to:

```rust
    prompt_config: &PromptConfig,
    meeting_created_at: DateTime<Utc>,
    documents_context: Option<String>,
) -> Result<(String, i64), String> {
```

Then replace this block:

```rust
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
```

with:

```rust
    let final_user_prompt = build_final_user_prompt(text, documents_context.as_deref(), custom_prompt);
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml build_final_user_prompt`
Expected: all 4 tests PASS.

Run: `cargo check --manifest-path frontend/src-tauri/Cargo.toml`
Expected: FAIL — the call to `generate_meeting_summary` in `service.rs` is now missing the new `documents_context` argument. **This is expected**; Task 5 fixes that call site. Confirm the only error is the missing-argument one at `service.rs`, with nothing wrong originating from `processor.rs` itself.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/summary/processor.rs
git commit -m "feat: inject meeting_documents block into the summary user prompt"
```

---

### Task 4: Explain `<meeting_documents>` in the system prompt

**Files:**
- Modify: `frontend/src-tauri/src/summary/prompts.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests { ... }` block at the bottom of `frontend/src-tauri/src/summary/prompts.rs` (after the existing `system_prompt_final_template_contains_time_placeholders` test):

```rust
    #[test]
    fn system_prompt_final_template_explains_meeting_documents_block() {
        assert!(SYSTEM_PROMPT_FINAL_TEMPLATE.contains("<meeting_documents>"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml system_prompt_final_template_explains_meeting_documents_block`
Expected: FAIL — the template doesn't mention `<meeting_documents>` yet.

- [ ] **Step 3: Add rule 6 to the template**

In `frontend/src-tauri/src/summary/prompts.rs`, change:

```rust
5. Xử lý dữ liệu thiếu: Nếu một thông tin bị thiếu một phần (ví dụ: có việc nhưng không có người làm, hoặc không có deadline), bắt buộc phải ghi rõ từ "(không rõ)" ngay tại vị trí đó. Chỉ ghi "Không có thông tin trong transcript" nếu mục đó hoàn toàn không được nhắc đến.

**HƯỚNG DẪN THEO TỪNG MỤC:**
```

to:

```rust
5. Xử lý dữ liệu thiếu: Nếu một thông tin bị thiếu một phần (ví dụ: có việc nhưng không có người làm, hoặc không có deadline), bắt buộc phải ghi rõ từ "(không rõ)" ngay tại vị trí đó. Chỉ ghi "Không có thông tin trong transcript" nếu mục đó hoàn toàn không được nhắc đến.
6. Tài liệu tham khảo: Nếu tin nhắn của người dùng có khối `<meeting_documents>`, đó là nội dung trích xuất từ tài liệu tham khảo (slide, văn bản...) được đính kèm cuộc họp — KHÔNG phải lời thoại. Dùng nội dung này để đối chiếu số liệu, thuật ngữ, tên riêng khi tóm tắt, nhưng không trích dẫn nó như một phát biểu của người tham dự.

**HƯỚNG DẪN THEO TỪNG MỤC:**
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml system_prompt_final_template_explains_meeting_documents_block`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/summary/prompts.rs
git commit -m "feat: explain the meeting_documents block in the report system prompt"
```

---

### Task 5: Fetch attached documents in `service.rs` and pass them through

**Files:**
- Modify: `frontend/src-tauri/src/summary/service.rs`

- [ ] **Step 1: Import the repository**

In `frontend/src-tauri/src/summary/service.rs`, change:

```rust
use crate::database::repositories::{
    meeting::MeetingsRepository, setting::SettingsRepository, summary::SummaryProcessesRepository,
};
```

to:

```rust
use crate::database::repositories::{
    meeting::MeetingsRepository, meeting_document::MeetingDocumentsRepository,
    setting::SettingsRepository, summary::SummaryProcessesRepository,
};
```

- [ ] **Step 2: Fetch and concatenate attached documents**

In `process_transcript_background`, immediately after the existing `meeting_created_at` fetch block (right after its closing `};`, before `let client = reqwest::Client::new();`), insert:

```rust
        let documents_context = match MeetingDocumentsRepository::list_by_meeting(&pool, &meeting_id).await {
            Ok(docs) if !docs.is_empty() => Some(
                docs.iter()
                    .map(|d| format!("--- Tài liệu: {} ---\n{}", d.filename, d.extracted_text))
                    .collect::<Vec<_>>()
                    .join("\n\n"),
            ),
            Ok(_) => None,
            Err(e) => {
                warn!(
                    "Failed to fetch meeting documents for prompt context: {}. Continuing without them.",
                    e
                );
                None
            }
        };
```

So the region reads, in order: the `meeting_created_at` match block, then this new `documents_context` match block, then `let client = reqwest::Client::new();`.

- [ ] **Step 3: Pass `documents_context` into the `generate_meeting_summary` call**

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
                meeting_created_at,
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
                documents_context.clone(),
            )
            .await;
```

(`.clone()` is needed because this call sits inside the `for (idx, current_model) in models_to_try.iter().enumerate()` fallback loop — `generate_meeting_summary` takes `documents_context` by value, and the loop may call it more than once across fallback attempts, so the outer `documents_context` binding must stay owned by `process_transcript_background` and be cloned per attempt, the same reason `&prompt_config` etc. are passed by reference rather than moved.)

- [ ] **Step 4: Verify the whole crate builds and all prior tests still pass**

Run: `cargo check --manifest-path frontend/src-tauri/Cargo.toml`
Expected: 0 errors.

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml summary::`
Expected: all tests pass, including the new ones from Tasks 3 and 4.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/summary/service.rs
git commit -m "feat: thread attached meeting documents through to the summary prompt"
```

---

### Task 6: Tauri commands for attach/list/delete + registration

**Files:**
- Create: `frontend/src-tauri/src/meeting_documents/mod.rs`
- Create: `frontend/src-tauri/src/meeting_documents/commands.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Create the module**

Create `frontend/src-tauri/src/meeting_documents/mod.rs`:

```rust
// Meeting documents module - lets users attach reference documents (PDF/DOCX/PPTX)
// to an existing meeting so their extracted text can be included when generating
// the AI summary, alongside the transcript.

pub mod commands;
```

- [ ] **Step 2: Create the commands**

Create `frontend/src-tauri/src/meeting_documents/commands.rs`:

```rust
use crate::database::models::MeetingDocument;
use crate::database::repositories::meeting_document::MeetingDocumentsRepository;
use crate::document_import::extractors;
use crate::state::AppState;
use log::info;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;

/// Metadata about an attached document, as shown to the frontend.
/// Deliberately excludes `extracted_text` — the UI only needs to list documents,
/// not display their full content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingDocumentInfo {
    pub id: String,
    pub filename: String,
    pub char_count: i64,
    pub created_at: String,
}

impl From<MeetingDocument> for MeetingDocumentInfo {
    fn from(doc: MeetingDocument) -> Self {
        Self {
            id: doc.id,
            filename: doc.filename,
            char_count: doc.char_count,
            created_at: doc.created_at.0.to_rfc3339(),
        }
    }
}

/// Open a native multi-file picker filtered to supported document formats.
/// Returns an empty vec if the user cancels.
#[tauri::command]
pub async fn api_select_meeting_document_files<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<String>, String> {
    info!("Opening file dialog for meeting document attachment");

    let app_clone = app.clone();
    let file_paths = tokio::task::spawn_blocking(move || {
        app_clone
            .dialog()
            .file()
            .add_filter("Tài liệu", extractors::SUPPORTED_EXTENSIONS)
            .blocking_pick_files()
    })
    .await
    .map_err(|e| format!("Lỗi mở hộp thoại chọn file: {}", e))?;

    match file_paths {
        Some(paths) => Ok(paths.into_iter().map(|p| p.to_string()).collect()),
        None => Ok(Vec::new()),
    }
}

/// Extracts text from `path` and attaches it to `meeting_id` as a reference document.
#[tauri::command]
pub async fn api_attach_meeting_document<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    path: String,
) -> Result<MeetingDocumentInfo, String> {
    let path_buf = std::path::PathBuf::from(&path);
    let filename = path_buf
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&path)
        .to_string();

    info!(
        "Attaching document '{}' to meeting_id: {}",
        filename, meeting_id
    );

    let extracted_text =
        tokio::task::spawn_blocking(move || extractors::extract_text_validated(&path_buf))
            .await
            .map_err(|e| format!("Lỗi xử lý file: {}", e))??;

    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Không thể truy cập trạng thái ứng dụng".to_string())?;

    let document = MeetingDocumentsRepository::create(
        app_state.db_manager.pool(),
        &meeting_id,
        &filename,
        &extracted_text,
    )
    .await
    .map_err(|e| format!("Lỗi lưu tài liệu: {}", e))?;

    info!(
        "Document '{}' attached to meeting_id: {} ({} chars)",
        filename, meeting_id, document.char_count
    );

    Ok(document.into())
}

/// Lists all documents attached to a meeting.
#[tauri::command]
pub async fn api_list_meeting_documents<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
) -> Result<Vec<MeetingDocumentInfo>, String> {
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Không thể truy cập trạng thái ứng dụng".to_string())?;

    let documents =
        MeetingDocumentsRepository::list_by_meeting(app_state.db_manager.pool(), &meeting_id)
            .await
            .map_err(|e| format!("Lỗi tải danh sách tài liệu: {}", e))?;

    Ok(documents.into_iter().map(MeetingDocumentInfo::from).collect())
}

/// Deletes an attached document by id.
#[tauri::command]
pub async fn api_delete_meeting_document<R: Runtime>(
    app: AppHandle<R>,
    document_id: String,
) -> Result<(), String> {
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Không thể truy cập trạng thái ứng dụng".to_string())?;

    MeetingDocumentsRepository::delete(app_state.db_manager.pool(), &document_id)
        .await
        .map_err(|e| format!("Lỗi xóa tài liệu: {}", e))?;

    Ok(())
}
```

- [ ] **Step 3: Register the module and commands**

In `frontend/src-tauri/src/lib.rs`, change:

```rust
pub mod document_import;
```

to:

```rust
pub mod document_import;
pub mod meeting_documents;
```

Then find the invoke_handler registration block:

```rust
            // Import document commands
            document_import::commands::api_select_document_files,
            document_import::commands::api_import_documents,
        ])
```

and change it to:

```rust
            // Import document commands
            document_import::commands::api_select_document_files,
            document_import::commands::api_import_documents,
            // Meeting reference document commands
            meeting_documents::commands::api_select_meeting_document_files,
            meeting_documents::commands::api_attach_meeting_document,
            meeting_documents::commands::api_list_meeting_documents,
            meeting_documents::commands::api_delete_meeting_document,
        ])
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check --manifest-path frontend/src-tauri/Cargo.toml`
Expected: 0 errors, and the `dead_code` warning on `MeetingDocumentsRepository` from Task 1 is now gone (it's used here).

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml`
Expected: all tests still pass (no test added in this task — this is Tauri-command/DB glue code with no existing DB test infrastructure in this repo, consistent with how `service.rs`'s DB-touching code was left untested in the prior timestamp feature).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/meeting_documents/mod.rs frontend/src-tauri/src/meeting_documents/commands.rs frontend/src-tauri/src/lib.rs
git commit -m "feat: add Tauri commands to attach/list/delete meeting reference documents"
```

---

### Task 7: Frontend hook `useMeetingDocuments`

**Files:**
- Create: `frontend/src/hooks/useMeetingDocuments.ts`

- [ ] **Step 1: Create the hook**

Create `frontend/src/hooks/useMeetingDocuments.ts`:

```typescript
import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface MeetingDocumentInfo {
  id: string;
  filename: string;
  char_count: number;
  created_at: string;
}

export type MeetingDocumentsStatus = 'idle' | 'loading' | 'attaching' | 'error';

export interface UseMeetingDocumentsReturn {
  documents: MeetingDocumentInfo[];
  status: MeetingDocumentsStatus;
  error: string | null;
  isBusy: boolean;
  refetch: (meetingId: string) => Promise<void>;
  selectAndAttach: (meetingId: string) => Promise<void>;
  remove: (documentId: string) => Promise<void>;
}

function extractErrorMessage(err: unknown, fallback: string): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  return fallback;
}

export function useMeetingDocuments(): UseMeetingDocumentsReturn {
  const [documents, setDocuments] = useState<MeetingDocumentInfo[]>([]);
  const [status, setStatus] = useState<MeetingDocumentsStatus>('idle');
  const [error, setError] = useState<string | null>(null);

  const refetch = useCallback(async (meetingId: string): Promise<void> => {
    setStatus('loading');
    setError(null);

    try {
      const result = await invoke<MeetingDocumentInfo[]>('api_list_meeting_documents', {
        meetingId,
      });
      setDocuments(result);
      setStatus('idle');
    } catch (err) {
      const errorMsg = extractErrorMessage(err, 'Không tải được danh sách tài liệu');
      setStatus('error');
      setError(errorMsg);
    }
  }, []);

  const selectAndAttach = useCallback(async (meetingId: string): Promise<void> => {
    setStatus('attaching');
    setError(null);

    try {
      const paths = await invoke<string[]>('api_select_meeting_document_files');
      if (paths.length === 0) {
        setStatus('idle');
        return;
      }

      for (const path of paths) {
        const doc = await invoke<MeetingDocumentInfo>('api_attach_meeting_document', {
          meetingId,
          path,
        });
        setDocuments((prev) => [...prev, doc]);
      }
      setStatus('idle');
    } catch (err) {
      const errorMsg = extractErrorMessage(err, 'Không đính kèm được tài liệu');
      setStatus('error');
      setError(errorMsg);
    }
  }, []);

  const remove = useCallback(async (documentId: string): Promise<void> => {
    setError(null);

    try {
      await invoke('api_delete_meeting_document', { documentId });
      setDocuments((prev) => prev.filter((d) => d.id !== documentId));
    } catch (err) {
      const errorMsg = extractErrorMessage(err, 'Không xóa được tài liệu');
      setStatus('error');
      setError(errorMsg);
    }
  }, []);

  return {
    documents,
    status,
    error,
    isBusy: status === 'loading' || status === 'attaching',
    refetch,
    selectAndAttach,
    remove,
  };
}
```

- [ ] **Step 2: Verify it lints cleanly**

Run: `pnpm --dir frontend run lint`
Expected: no new errors/warnings attributable to `useMeetingDocuments.ts`. (Pre-existing warnings elsewhere in the codebase are not your concern.)

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/useMeetingDocuments.ts
git commit -m "feat: add useMeetingDocuments hook for attaching reference documents"
```

---

### Task 8: `MeetingDocumentsDialog` component

**Files:**
- Create: `frontend/src/components/MeetingDetails/MeetingDocumentsDialog.tsx`

- [ ] **Step 1: Create the dialog**

Create `frontend/src/components/MeetingDetails/MeetingDocumentsDialog.tsx`:

```tsx
'use client';

import React, { useEffect, useRef } from 'react';
import { Paperclip, Loader2, FileText, Trash2, Upload } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { toast } from 'sonner';
import { useMeetingDocuments } from '@/hooks/useMeetingDocuments';

interface MeetingDocumentsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  meetingId: string;
  onDocumentsChanged?: (count: number) => void;
}

export function MeetingDocumentsDialog({
  open,
  onOpenChange,
  meetingId,
  onDocumentsChanged,
}: MeetingDocumentsDialogProps) {
  const { documents, status, error, isBusy, refetch, selectAndAttach, remove } =
    useMeetingDocuments();
  const prevOpenRef = useRef(false);

  useEffect(() => {
    const wasOpen = prevOpenRef.current;
    prevOpenRef.current = open;

    if (open && !wasOpen) {
      refetch(meetingId);
    }
  }, [open, meetingId, refetch]);

  useEffect(() => {
    if (error) {
      toast.error('Lỗi tài liệu tham khảo', { description: error });
    }
  }, [error]);

  useEffect(() => {
    onDocumentsChanged?.(documents.length);
  }, [documents.length, onDocumentsChanged]);

  const handleAttach = async () => {
    await selectAndAttach(meetingId);
  };

  const handleRemove = async (documentId: string) => {
    await remove(documentId);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Paperclip className="h-5 w-5 text-blue-600" />
            Tài liệu tham khảo
          </DialogTitle>
          <DialogDescription>
            Đính kèm slide, văn bản (PDF, DOCX, PPTX) được dùng trong cuộc họp để AI tham khảo khi
            tạo báo cáo.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3 py-2">
          {status === 'loading' ? (
            <div className="flex items-center justify-center py-8 text-gray-400 text-sm">
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              Đang tải...
            </div>
          ) : documents.length > 0 ? (
            <ul className="space-y-1 max-h-64 overflow-y-auto">
              {documents.map((doc) => (
                <li
                  key={doc.id}
                  className="flex items-center gap-2 text-sm text-gray-700 bg-gray-50 rounded-lg px-3 py-2"
                >
                  <FileText className="h-4 w-4 text-blue-600 shrink-0" />
                  <span className="truncate flex-1">{doc.filename}</span>
                  <span className="text-xs text-gray-400 shrink-0">
                    {doc.char_count.toLocaleString()} ký tự
                  </span>
                  <button
                    onClick={() => handleRemove(doc.id)}
                    disabled={isBusy}
                    className="text-gray-400 hover:text-red-600 disabled:opacity-40 shrink-0"
                    title="Xóa tài liệu"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <div className="border-2 border-dashed border-gray-300 rounded-lg p-6 text-center text-sm text-gray-500">
              Chưa có tài liệu nào được đính kèm
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Đóng
          </Button>
          <Button
            onClick={handleAttach}
            disabled={isBusy}
            className="bg-[#16478e] hover:bg-[#1a55ab]"
          >
            {status === 'attaching' ? (
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            ) : (
              <Upload className="h-4 w-4 mr-2" />
            )}
            Thêm tài liệu
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Verify it lints cleanly**

Run: `pnpm --dir frontend run lint`
Expected: no new errors/warnings attributable to `MeetingDocumentsDialog.tsx`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/MeetingDetails/MeetingDocumentsDialog.tsx
git commit -m "feat: add MeetingDocumentsDialog for managing attached reference documents"
```

---

### Task 9: Toolbar integration

**Files:**
- Modify: `frontend/src/components/MeetingDetails/SummaryGeneratorButtonGroup.tsx`
- Modify: `frontend/src/components/MeetingDetails/SummaryPanel.tsx`

- [ ] **Step 1: Add the "Tài liệu" button to `SummaryGeneratorButtonGroup`**

In `frontend/src/components/MeetingDetails/SummaryGeneratorButtonGroup.tsx`, change the imports:

```tsx
import { Sparkles, Settings, Loader2, FileText, Check, Square } from 'lucide-react';
import Analytics from '@/lib/analytics';
import { useState, useEffect } from 'react';
```

to:

```tsx
import { Sparkles, Settings, Loader2, FileText, Check, Square, Paperclip } from 'lucide-react';
import Analytics from '@/lib/analytics';
import { useState, useEffect, useCallback } from 'react';
import { useMeetingDocuments } from '@/hooks/useMeetingDocuments';
import { MeetingDocumentsDialog } from './MeetingDocumentsDialog';
```

Change the props interface:

```tsx
interface SummaryGeneratorButtonGroupProps {
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSaveModelConfig: (config?: ModelConfig) => Promise<void>;
  onGenerateSummary: (customPrompt: string) => Promise<void>;
  onStopGeneration: () => void;
  customPrompt: string;
  summaryStatus: 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';
  availableTemplates: Array<{ id: string, name: string, description: string }>;
  selectedTemplate: string;
  onTemplateSelect: (templateId: string, templateName: string) => void;
  hasTranscripts?: boolean;
  isModelConfigLoading?: boolean;
  onOpenModelSettings?: (openFn: () => void) => void;
}
```

to:

```tsx
interface SummaryGeneratorButtonGroupProps {
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSaveModelConfig: (config?: ModelConfig) => Promise<void>;
  onGenerateSummary: (customPrompt: string) => Promise<void>;
  onStopGeneration: () => void;
  customPrompt: string;
  summaryStatus: 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';
  availableTemplates: Array<{ id: string, name: string, description: string }>;
  selectedTemplate: string;
  onTemplateSelect: (templateId: string, templateName: string) => void;
  hasTranscripts?: boolean;
  isModelConfigLoading?: boolean;
  onOpenModelSettings?: (openFn: () => void) => void;
  meetingId: string;
}
```

Change the function signature to destructure the new prop:

```tsx
export function SummaryGeneratorButtonGroup({
  modelConfig,
  setModelConfig,
  onSaveModelConfig,
  onGenerateSummary,
  onStopGeneration,
  customPrompt,
  summaryStatus,
  availableTemplates,
  selectedTemplate,
  onTemplateSelect,
  hasTranscripts = true,
  isModelConfigLoading = false,
  onOpenModelSettings
}: SummaryGeneratorButtonGroupProps) {
  const [settingsDialogOpen, setSettingsDialogOpen] = useState(false);
```

to:

```tsx
export function SummaryGeneratorButtonGroup({
  modelConfig,
  setModelConfig,
  onSaveModelConfig,
  onGenerateSummary,
  onStopGeneration,
  customPrompt,
  summaryStatus,
  availableTemplates,
  selectedTemplate,
  onTemplateSelect,
  hasTranscripts = true,
  isModelConfigLoading = false,
  onOpenModelSettings,
  meetingId,
}: SummaryGeneratorButtonGroupProps) {
  const [settingsDialogOpen, setSettingsDialogOpen] = useState(false);
  const [documentsDialogOpen, setDocumentsDialogOpen] = useState(false);
  const { documents, refetch: refetchDocuments } = useMeetingDocuments();

  useEffect(() => {
    refetchDocuments(meetingId);
  }, [meetingId, refetchDocuments]);

  const handleDocumentsDialogChange = useCallback(
    (open: boolean) => {
      setDocumentsDialogOpen(open);
      if (!open) {
        refetchDocuments(meetingId);
      }
    },
    [meetingId, refetchDocuments]
  );
```

Insert the new button right after the closing `)}` of the Generate/Stop button block and before the `{/* Settings button */}` comment:

```tsx
      {/* Settings button */}
      <Dialog open={settingsDialogOpen} onOpenChange={setSettingsDialogOpen}>
```

change to:

```tsx
      {/* Reference documents button */}
      <Button
        variant="outline"
        size="sm"
        className="relative"
        onClick={() => setDocumentsDialogOpen(true)}
        title="Tài liệu tham khảo"
      >
        <Paperclip />
        <span className="hidden lg:inline">Tài liệu</span>
        {documents.length > 0 && (
          <span className="absolute -top-1.5 -right-1.5 flex h-4 w-4 items-center justify-center rounded-full bg-[#16478e] text-[10px] font-medium text-white">
            {documents.length}
          </span>
        )}
      </Button>
      <MeetingDocumentsDialog
        open={documentsDialogOpen}
        onOpenChange={handleDocumentsDialogChange}
        meetingId={meetingId}
      />

      {/* Settings button */}
      <Dialog open={settingsDialogOpen} onOpenChange={setSettingsDialogOpen}>
```

- [ ] **Step 2: Pass `meetingId` from `SummaryPanel`**

In `frontend/src/components/MeetingDetails/SummaryPanel.tsx`, change:

```tsx
  const sharedGeneratorProps = {
    modelConfig,
    setModelConfig,
    onSaveModelConfig,
    onGenerateSummary,
    onStopGeneration,
    customPrompt,
    summaryStatus,
    availableTemplates,
    selectedTemplate,
    onTemplateSelect,
    hasTranscripts: transcripts.length > 0,
    isModelConfigLoading,
    onOpenModelSettings,
  };
```

to:

```tsx
  const sharedGeneratorProps = {
    modelConfig,
    setModelConfig,
    onSaveModelConfig,
    onGenerateSummary,
    onStopGeneration,
    customPrompt,
    summaryStatus,
    availableTemplates,
    selectedTemplate,
    onTemplateSelect,
    hasTranscripts: transcripts.length > 0,
    isModelConfigLoading,
    onOpenModelSettings,
    meetingId: meeting.id,
  };
```

- [ ] **Step 3: Verify it lints cleanly**

Run: `pnpm --dir frontend run lint`
Expected: no new errors/warnings attributable to either modified file. In particular, confirm there is no "missing prop" TypeScript error for `meetingId` at the `<SummaryGeneratorButtonGroup {...sharedGeneratorProps} />` call site in `SummaryPanel.tsx` (it's satisfied via the spread now that `meetingId` is in `sharedGeneratorProps`).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/MeetingDetails/SummaryGeneratorButtonGroup.tsx frontend/src/components/MeetingDetails/SummaryPanel.tsx
git commit -m "feat: surface reference-document attachment in the summary toolbar"
```

---

### Task 10: Final verification

- [ ] **Step 1: Full Rust crate check**

Run: `cargo check --manifest-path frontend/src-tauri/Cargo.toml`
Expected: no errors, no new warnings (in particular, no leftover `dead_code` warning on `MeetingDocumentsRepository` or `extract_from_pptx`).

- [ ] **Step 2: Full Rust test run for the affected modules**

Run: `cargo test --manifest-path frontend/src-tauri/Cargo.toml summary:: document_import::`
Expected: all tests pass — the pre-existing timestamp-feature tests, the 2 new PPTX extractor tests, the 4 new `build_final_user_prompt` tests, and the new `system_prompt_final_template_explains_meeting_documents_block` test.

- [ ] **Step 3: Full frontend build (type-checks all new/changed TSX)**

Run: `pnpm --dir frontend run build`
Expected: build succeeds with no TypeScript errors in `useMeetingDocuments.ts`, `MeetingDocumentsDialog.tsx`, `SummaryGeneratorButtonGroup.tsx`, or `SummaryPanel.tsx`.
