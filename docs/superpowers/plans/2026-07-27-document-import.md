# Import Documents (DOCX/PDF/TXT/SRT/VTT) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user pick one or more DOCX/PDF/TXT/SRT/VTT files, combine their text into a single new meeting, and generate a report from it using the existing template + LLM summary pipeline unchanged.

**Architecture:** A new `document_import` Rust module extracts text per file format, concatenates it with per-file headers, and writes one `meetings` + one `transcripts` row directly via `sqlx` — mirroring the existing `audio::import` module's DB-write pattern but skipping ASR/audio entirely. A new frontend hook + dialog, wired the same way as the existing `ImportAudioDialog`, drives it from a new button under "Nhập file âm thanh" in the sidebar.

**Tech Stack:** Rust (Tauri 2, sqlx/SQLite, `docx-rs` for DOCX reading, new `pdf-extract` crate for PDF text extraction), Next.js/React/TypeScript frontend, no new frontend dependencies.

**Spec:** `docs/superpowers/specs/2026-07-27-document-import-design.md`

---

## Deviation from spec (found during planning — read before starting)

The spec proposes a `meetings.source_type` column so the frontend can hide audio-player UI for
document-imported meetings. Investigation of `frontend/src/components/MeetingDetails/AudioPlayer.tsx`,
`TranscriptPanel.tsx` (`frontend\src\components\MeetingDetails\TranscriptPanel.tsx:91` and `:119`) and
`TranscriptButtonGroup.tsx:86` shows every audio-related UI element is **already** conditionally
rendered on `meetingFolderPath` being truthy (`{meetingFolderPath && (...)}`), and
`useAudioPlayer.ts:14` (`if (!meetingFolderPath) return;`) already no-ops when there's no folder.
This existing null-safety was presumably built for other edge cases, but it means: if we simply
leave `meetings.folder_path = NULL` for document-imported meetings (there's no audio to store), the
audio player, its toggle button, and the retranscribe button **already stay hidden with zero
frontend changes**. Adding `source_type` would duplicate that mechanism for no behavioral gain, so
this plan **does not** add a migration or a `source_type` column. If a future need arises to visually
distinguish import sources in the meeting list, that's a separate, smaller follow-up.

---

## Task 1: Add the `pdf-extract` dependency

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml:98` (right after the `docx-rs = "0.4.17"` line)

- [ ] **Step 1: Add the dependency**

In `frontend/src-tauri/Cargo.toml`, right after line 98 (`docx-rs = "0.4.17"`), add:

```toml
pdf-extract = "0.12.0"
```

- [ ] **Step 2: Verify it resolves**

Run (from `frontend/src-tauri`):
```
cargo check -p meetingone --lib
```
Expected: compiles (may take a while the first time as it downloads/builds `pdf-extract` and its
dependencies). No error about `pdf-extract` not being found.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/Cargo.toml frontend/src-tauri/Cargo.lock
git commit -m "build: add pdf-extract dependency for PDF text extraction"
```

---

## Task 2: `document_import` module skeleton + TXT/SRT/VTT extraction

**Files:**
- Create: `frontend/src-tauri/src/document_import/mod.rs`
- Create: `frontend/src-tauri/src/document_import/extractors.rs`

- [ ] **Step 1: Create the module skeleton**

`frontend/src-tauri/src/document_import/mod.rs`:
```rust
// Document import module - extracts text from DOCX/PDF/TXT/SRT/VTT files
// and turns them into a new meeting, reusing the existing summary pipeline.

pub mod extractors;
pub mod commands;
```

(`commands.rs` doesn't exist yet — that's fine, it's created in Task 5. If you need `cargo check`
to pass before then, comment out `pub mod commands;` and uncomment it in Task 5 Step 1.)

- [ ] **Step 2: Write the failing tests for plain-text and subtitle extraction**

`frontend/src-tauri/src/document_import/extractors.rs`:
```rust
use std::path::Path;

/// File extensions this module knows how to extract text from.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["pdf", "docx", "txt", "srt", "vtt"];

/// Minimum number of characters (after trimming) a file must yield to be
/// considered "has real text content" rather than empty/scanned/corrupt.
const MIN_CONTENT_LENGTH: usize = 20;

fn extract_from_plain_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("Lỗi đọc file: {}", e))
}

fn extract_from_subtitle(path: &Path) -> Result<String, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("Lỗi đọc file: {}", e))?;

    let timestamp_re = regex::Regex::new(
        r"^\s*\d{2}:\d{2}:\d{2}[.,]\d{3}\s*-->\s*\d{2}:\d{2}:\d{2}[.,]\d{3}",
    )
    .expect("static regex is valid");
    let sequence_re = regex::Regex::new(r"^\s*\d+\s*$").expect("static regex is valid");

    let mut lines_out: Vec<String> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "WEBVTT" {
            continue;
        }
        if timestamp_re.is_match(trimmed) || sequence_re.is_match(trimmed) {
            continue;
        }
        lines_out.push(trimmed.to_string());
    }
    Ok(lines_out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_from_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "Nội dung cuộc họp mẫu").unwrap();

        let text = extract_from_plain_text(&path).unwrap();
        assert_eq!(text, "Nội dung cuộc họp mẫu");
    }

    #[test]
    fn test_extract_from_subtitle_strips_srt_markup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.srt");
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nXin chào các bạn\n\n2\n00:00:04,500 --> 00:00:07,000\nChúng ta bắt đầu cuộc họp\n";
        std::fs::write(&path, srt).unwrap();

        let text = extract_from_subtitle(&path).unwrap();
        assert_eq!(text, "Xin chào các bạn\nChúng ta bắt đầu cuộc họp");
    }

    #[test]
    fn test_extract_from_subtitle_strips_vtt_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.vtt");
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHello everyone\n";
        std::fs::write(&path, vtt).unwrap();

        let text = extract_from_subtitle(&path).unwrap();
        assert_eq!(text, "Hello everyone");
    }
}
```

- [ ] **Step 3: Run the tests to verify they pass**

Run (from `frontend/src-tauri`):
```
cargo test -p meetingone --lib document_import::extractors::tests
```
Expected: 3 tests pass (`test_extract_from_plain_text`, `test_extract_from_subtitle_strips_srt_markup`,
`test_extract_from_subtitle_strips_vtt_header`).

- [ ] **Step 4: Register the module in `lib.rs`**

In `frontend/src-tauri/src/lib.rs:40`, right after `pub mod database;`, add:
```rust
pub mod document_import;
```

- [ ] **Step 5: Verify the whole crate still compiles**

Run (from `frontend/src-tauri`):
```
cargo check -p meetingone --lib
```
Expected: compiles. `extract_from_plain_text` / `extract_from_subtitle` will report as unused
(dead_code warning, not an error) until Task 4 wires them into the public dispatcher — that's fine.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/document_import frontend/src-tauri/src/lib.rs
git commit -m "feat: add TXT/SRT/VTT text extraction for document import"
```

---

## Task 3: DOCX extraction

**Files:**
- Modify: `frontend/src-tauri/src/document_import/extractors.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `frontend/src-tauri/src/document_import/extractors.rs` (inside the
existing `mod tests { ... }` block, after the VTT test):

```rust
    #[test]
    fn test_extract_from_docx_reads_paragraph_text() {
        use docx_rs::{Docx, Paragraph, Run};
        use std::io::Cursor;

        let mut buf: Vec<u8> = Vec::new();
        Docx::new()
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Hello world")))
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Second paragraph")))
            .build()
            .pack(Cursor::new(&mut buf))
            .expect("failed to pack test docx");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.docx");
        std::fs::write(&path, &buf).unwrap();

        let text = extract_from_docx(&path).unwrap();
        assert!(text.contains("Hello world"), "got: {}", text);
        assert!(text.contains("Second paragraph"), "got: {}", text);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run (from `frontend/src-tauri`):
```
cargo test -p meetingone --lib document_import::extractors::tests::test_extract_from_docx_reads_paragraph_text
```
Expected: FAIL — `extract_from_docx` not found in this scope.

- [ ] **Step 3: Implement `extract_from_docx`**

Add this function to `frontend/src-tauri/src/document_import/extractors.rs` (above the `#[cfg(test)]`
block, next to `extract_from_subtitle`):

```rust
fn extract_from_docx(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Lỗi đọc file: {}", e))?;
    let docx = docx_rs::read_docx(&bytes).map_err(|e| format!("Lỗi đọc DOCX: {}", e))?;

    let mut text = String::new();
    for child in docx.document.children {
        if let docx_rs::DocumentChild::Paragraph(paragraph) = child {
            let mut paragraph_text = String::new();
            for pchild in paragraph.children {
                if let docx_rs::ParagraphChild::Run(run) = pchild {
                    for rchild in run.children {
                        if let docx_rs::RunChild::Text(t) = rchild {
                            paragraph_text.push_str(&t.text);
                        }
                    }
                }
            }
            if !paragraph_text.is_empty() {
                text.push_str(&paragraph_text);
                text.push('\n');
            }
        }
    }
    Ok(text)
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run (from `frontend/src-tauri`):
```
cargo test -p meetingone --lib document_import::extractors::tests::test_extract_from_docx_reads_paragraph_text
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/document_import/extractors.rs
git commit -m "feat: add DOCX text extraction for document import"
```

---

## Task 4: PDF extraction + public dispatcher + validated wrapper

**Files:**
- Modify: `frontend/src-tauri/src/document_import/extractors.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module (after the DOCX test):

```rust
    #[test]
    fn test_extract_from_pdf_invalid_bytes_returns_err() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.pdf");
        std::fs::write(&path, b"this is not a real pdf file").unwrap();

        let result = extract_from_pdf(&path);
        assert!(result.is_err(), "expected an error for a non-PDF file");
    }

    #[test]
    fn test_extract_text_dispatches_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "Nội dung mẫu").unwrap();

        let text = extract_text(&path).unwrap();
        assert_eq!(text, "Nội dung mẫu");
    }

    #[test]
    fn test_extract_text_unsupported_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.xyz");
        std::fs::write(&path, "some content").unwrap();

        let result = extract_text(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_text_validated_rejects_short_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("short.txt");
        std::fs::write(&path, "hi").unwrap();

        let result = extract_text_validated(&path);
        assert!(result.is_err(), "content shorter than MIN_CONTENT_LENGTH should be rejected");
    }

    #[test]
    fn test_extract_text_validated_accepts_real_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("real.txt");
        std::fs::write(&path, "Đây là nội dung cuộc họp có đủ độ dài để vượt qua ngưỡng kiểm tra").unwrap();

        let result = extract_text_validated(&path);
        assert!(result.is_ok());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run (from `frontend/src-tauri`):
```
cargo test -p meetingone --lib document_import::extractors::tests
```
Expected: FAIL to compile — `extract_from_pdf`, `extract_text`, `extract_text_validated` don't exist yet.

- [ ] **Step 3: Implement the PDF extractor, dispatcher, and validated wrapper**

Add these functions to `frontend/src-tauri/src/document_import/extractors.rs` (the dispatcher and
validated wrapper are `pub` since `commands.rs` will call `extract_text_validated`; `extract_from_pdf`
stays private next to the other format-specific extractors):

```rust
fn extract_from_pdf(path: &Path) -> Result<String, String> {
    pdf_extract::extract_text(path).map_err(|e| format!("Lỗi đọc PDF: {}", e))
}

fn extract_text(path: &Path) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "pdf" => extract_from_pdf(path),
        "docx" => extract_from_docx(path),
        "srt" | "vtt" => extract_from_subtitle(path),
        "txt" => extract_from_plain_text(path),
        other => Err(format!("Định dạng .{} không được hỗ trợ", other)),
    }
}

/// Extract text from `path` and validate it has real content.
/// Returns a trimmed, non-empty string, or an error describing why the file
/// was rejected (unsupported format, read/parse failure, or too little text —
/// e.g. a scanned/image-only PDF with no extractable text layer).
pub fn extract_text_validated(path: &Path) -> Result<String, String> {
    let text = extract_text(path)?;
    let trimmed = text.trim();
    if trimmed.chars().count() < MIN_CONTENT_LENGTH {
        return Err(
            "Không trích xuất được nội dung văn bản (có thể là file PDF dạng scan/ảnh, hoặc file rỗng)"
                .to_string(),
        );
    }
    Ok(trimmed.to_string())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run (from `frontend/src-tauri`):
```
cargo test -p meetingone --lib document_import::extractors::tests
```
Expected: all 9 tests in this module pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/document_import/extractors.rs
git commit -m "feat: add PDF text extraction, format dispatcher, and content validation"
```

---

## Task 5: Tauri commands — select files and import

**Files:**
- Create: `frontend/src-tauri/src/document_import/commands.rs`
- Modify: `frontend/src-tauri/src/document_import/mod.rs`

- [ ] **Step 1: Uncomment the `commands` module declaration**

In `frontend/src-tauri/src/document_import/mod.rs`, make sure it reads:
```rust
pub mod extractors;
pub mod commands;
```

- [ ] **Step 2: Write `commands.rs`**

`frontend/src-tauri/src/document_import/commands.rs`:
```rust
use crate::state::AppState;
use log::info;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use super::extractors;

/// Result of a successful document import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentImportResult {
    pub meeting_id: String,
    pub title: String,
    pub files_count: usize,
}

/// Open a native multi-file picker filtered to supported document formats.
/// Returns an empty vec if the user cancels.
#[tauri::command]
pub async fn api_select_document_files<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<String>, String> {
    info!("Opening file dialog for document import");

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

/// Extract text from every file in `paths`, combine it into one transcript
/// (each file's content prefixed with a "--- Tài liệu: <filename> ---" header),
/// and create a new meeting from it. All-or-nothing: if any file fails to
/// yield real text content, the whole import is rejected and no meeting is
/// created.
#[tauri::command]
pub async fn api_import_documents<R: Runtime>(
    app: AppHandle<R>,
    paths: Vec<String>,
    title: String,
) -> Result<DocumentImportResult, String> {
    if paths.is_empty() {
        return Err("Chưa chọn file nào để nhập".to_string());
    }
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Vui lòng nhập tiêu đề cuộc họp".to_string());
    }

    let files_count = paths.len();
    info!("Starting document import: {} file(s), title='{}'", files_count, title);

    let combined = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let mut combined = String::new();
        for path_str in &paths {
            let path = Path::new(path_str);
            let filename = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(path_str)
                .to_string();

            let text = extractors::extract_text_validated(path)
                .map_err(|e| format!("{}: {}", filename, e))?;

            if !combined.is_empty() {
                combined.push_str("\n\n");
            }
            combined.push_str(&format!("--- Tài liệu: {} ---\n{}", filename, text));
        }
        Ok(combined)
    })
    .await
    .map_err(|e| format!("Lỗi xử lý file: {}", e))??;

    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Không thể truy cập trạng thái ứng dụng".to_string())?;

    let meeting_id = create_document_meeting(app_state.db_manager.pool(), &title, &combined)
        .await
        .map_err(|e| e.to_string())?;

    info!("Document import complete: meeting_id={}", meeting_id);

    Ok(DocumentImportResult {
        meeting_id,
        title,
        files_count,
    })
}

/// Create a new meeting (no folder_path — there's no audio file) with a
/// single transcript row holding the combined document text.
async fn create_document_meeting(
    pool: &sqlx::SqlitePool,
    title: &str,
    combined_text: &str,
) -> Result<String, anyhow::Error> {
    let meeting_id = format!("meeting-{}", Uuid::new_v4());
    let transcript_id = format!("transcript-{}", Uuid::new_v4());
    let now = chrono::Utc::now();

    let mut conn = pool.acquire().await?;
    let mut tx = sqlx::Connection::begin(&mut *conn).await?;

    sqlx::query(
        "INSERT INTO meetings (id, title, created_at, updated_at, folder_path)
         VALUES (?, ?, ?, ?, NULL)",
    )
    .bind(&meeting_id)
    .bind(title)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO transcripts (id, meeting_id, transcript, timestamp)
         VALUES (?, ?, ?, ?)",
    )
    .bind(&transcript_id)
    .bind(&meeting_id)
    .bind(combined_text)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    info!("Created document-import meeting '{}'", meeting_id);

    Ok(meeting_id)
}
```

- [ ] **Step 3: Verify it compiles**

Run (from `frontend/src-tauri`):
```
cargo check -p meetingone --lib
```
Expected: compiles. The two new commands will show as unused (dead_code) until Task 6 registers
them — that's fine.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/document_import/commands.rs frontend/src-tauri/src/document_import/mod.rs
git commit -m "feat: add Tauri commands to select and import documents"
```

---

## Task 6: Register commands in `lib.rs`

**Files:**
- Modify: `frontend/src-tauri/src/lib.rs:651` (end of the "Import audio commands" block)

- [ ] **Step 1: Add the commands to the invoke handler**

In `frontend/src-tauri/src/lib.rs`, right after line 651
(`audio::import::is_import_in_progress_command,`) and before the closing `])` of
`generate_handler!`, add:

```rust
            // Import document commands
            document_import::commands::api_select_document_files,
            document_import::commands::api_import_documents,
```

- [ ] **Step 2: Verify the full app compiles**

Run (from `frontend/src-tauri`):
```
cargo check -p meetingone
```
Expected: compiles with no errors.

- [ ] **Step 3: Run the full document_import test suite one more time**

Run (from `frontend/src-tauri`):
```
cargo test -p meetingone --lib document_import
```
Expected: all tests (from Tasks 2-4) pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/lib.rs
git commit -m "feat: register document import Tauri commands"
```

---

## Task 7: Frontend hook `useImportDocuments`

**Files:**
- Create: `frontend/src/hooks/useImportDocuments.ts`

- [ ] **Step 1: Write the hook**

`frontend/src/hooks/useImportDocuments.ts`:
```typescript
import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface DocumentImportResult {
  meeting_id: string;
  title: string;
  files_count: number;
}

export type DocumentImportStatus = 'idle' | 'selecting' | 'importing' | 'error';

export interface UseImportDocumentsReturn {
  status: DocumentImportStatus;
  selectedPaths: string[];
  error: string | null;
  isBusy: boolean;
  selectFiles: () => Promise<string[]>;
  importDocuments: (paths: string[], title: string) => Promise<DocumentImportResult | null>;
  reset: () => void;
}

function extractErrorMessage(err: unknown, fallback: string): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  return fallback;
}

export function useImportDocuments(): UseImportDocumentsReturn {
  const [status, setStatus] = useState<DocumentImportStatus>('idle');
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const selectFiles = useCallback(async (): Promise<string[]> => {
    setStatus('selecting');
    setError(null);

    try {
      const paths = await invoke<string[]>('api_select_document_files');
      setSelectedPaths(paths);
      setStatus('idle');
      return paths;
    } catch (err) {
      const errorMsg = extractErrorMessage(err, 'Không thể chọn file');
      setStatus('error');
      setError(errorMsg);
      return [];
    }
  }, []);

  const importDocuments = useCallback(
    async (paths: string[], title: string): Promise<DocumentImportResult | null> => {
      setStatus('importing');
      setError(null);

      try {
        const result = await invoke<DocumentImportResult>('api_import_documents', { paths, title });
        setStatus('idle');
        return result;
      } catch (err) {
        const errorMsg = extractErrorMessage(err, 'Nhập tài liệu thất bại');
        setStatus('error');
        setError(errorMsg);
        return null;
      }
    },
    []
  );

  const reset = useCallback(() => {
    setStatus('idle');
    setSelectedPaths([]);
    setError(null);
  }, []);

  return {
    status,
    selectedPaths,
    error,
    isBusy: status === 'selecting' || status === 'importing',
    selectFiles,
    importDocuments,
    reset,
  };
}
```

- [ ] **Step 2: Verify it type-checks**

Run (from `frontend`):
```
pnpm exec tsc --noEmit
```
Expected: no new type errors from `useImportDocuments.ts` (pre-existing unrelated errors elsewhere,
if any, are not this task's concern).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/useImportDocuments.ts
git commit -m "feat: add useImportDocuments hook for document import"
```

---

## Task 8: Frontend dialog context

**Files:**
- Create: `frontend/src/contexts/DocumentImportDialogContext.tsx`

- [ ] **Step 1: Write the context**

Mirrors `frontend/src/contexts/ImportDialogContext.tsx` exactly, for a second independent dialog.

`frontend/src/contexts/DocumentImportDialogContext.tsx`:
```tsx
'use client';

import { createContext, useContext, useCallback, ReactNode } from 'react';

interface DocumentImportDialogContextType {
  openDocumentImportDialog: () => void;
}

const DocumentImportDialogContext = createContext<DocumentImportDialogContextType | null>(null);

export const useDocumentImportDialog = () => {
  const ctx = useContext(DocumentImportDialogContext);
  if (!ctx) {
    throw new Error('useDocumentImportDialog must be used within DocumentImportDialogProvider');
  }
  return ctx;
};

interface DocumentImportDialogProviderProps {
  children: ReactNode;
  onOpen: () => void;
}

export function DocumentImportDialogProvider({ children, onOpen }: DocumentImportDialogProviderProps) {
  const openDocumentImportDialog = useCallback(() => {
    onOpen();
  }, [onOpen]);

  return (
    <DocumentImportDialogContext.Provider value={{ openDocumentImportDialog }}>
      {children}
    </DocumentImportDialogContext.Provider>
  );
}
```

- [ ] **Step 2: Verify it type-checks**

Run (from `frontend`):
```
pnpm exec tsc --noEmit
```
Expected: no new type errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/contexts/DocumentImportDialogContext.tsx
git commit -m "feat: add DocumentImportDialogContext"
```

---

## Task 9: Frontend `DocumentImportDialog` component

**Files:**
- Create: `frontend/src/components/ImportDocuments/DocumentImportDialog.tsx`
- Create: `frontend/src/components/ImportDocuments/index.ts`

- [ ] **Step 1: Write the dialog component**

`frontend/src/components/ImportDocuments/DocumentImportDialog.tsx`:
```tsx
'use client';

import React, { useState, useEffect, useRef } from 'react';
import { Upload, Loader2, FileText } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { Textarea } from '../ui/textarea';
import { toast } from 'sonner';
import { useRouter } from 'next/navigation';
import { useSidebar } from '../Sidebar/SidebarProvider';
import { useImportDocuments } from '@/hooks/useImportDocuments';

interface DocumentImportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function filenameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function DocumentImportDialog({ open, onOpenChange }: DocumentImportDialogProps) {
  const router = useRouter();
  const { refetchMeetings } = useSidebar();

  const [title, setTitle] = useState('');
  const [titleModifiedByUser, setTitleModifiedByUser] = useState(false);
  const prevOpenRef = useRef(false);

  const { status, selectedPaths, error, selectFiles, importDocuments, reset } = useImportDocuments();

  // Reset state only when the dialog transitions from closed to open
  useEffect(() => {
    const wasOpen = prevOpenRef.current;
    prevOpenRef.current = open;

    if (open && !wasOpen) {
      reset();
      setTitle('');
      setTitleModifiedByUser(false);
    }
  }, [open, reset]);

  useEffect(() => {
    if (error) {
      toast.error('Nhập tài liệu thất bại', { description: error });
    }
  }, [error]);

  const handleSelectFiles = async () => {
    const paths = await selectFiles();
    if (paths.length > 0 && !titleModifiedByUser) {
      setTitle(filenameFromPath(paths[0]));
    }
  };

  const handleImport = async () => {
    if (selectedPaths.length === 0) return;
    const finalTitle = title.trim() || filenameFromPath(selectedPaths[0]);

    const result = await importDocuments(selectedPaths, finalTitle);
    if (result) {
      toast.success(`Đã tạo cuộc họp từ ${result.files_count} tài liệu`);
      refetchMeetings();
      onOpenChange(false);
      router.push(`/meeting-details?id=${result.meeting_id}&source=import`);
    }
  };

  const isImporting = status === 'importing';

  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen && isImporting) return;
    onOpenChange(newOpen);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {isImporting ? (
              <>
                <Loader2 className="h-5 w-5 animate-spin text-blue-600" />
                Đang nhập tài liệu...
              </>
            ) : (
              <>
                <Upload className="h-5 w-5 text-blue-600" />
                Tải tài liệu lên
              </>
            )}
          </DialogTitle>
          <DialogDescription>
            Chọn một hoặc nhiều file PDF, DOCX, TXT, SRT hoặc VTT để tạo cuộc họp mới từ nội dung có sẵn
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {selectedPaths.length > 0 ? (
            <div className="bg-gray-50 rounded-lg p-4 space-y-3">
              <ul className="space-y-1 max-h-32 overflow-y-auto">
                {selectedPaths.map((path) => (
                  <li key={path} className="flex items-center gap-2 text-sm text-gray-700">
                    <FileText className="h-4 w-4 text-blue-600 shrink-0" />
                    <span className="truncate">{filenameFromPath(path)}</span>
                  </li>
                ))}
              </ul>

              <div className="space-y-1">
                <label className="text-sm font-medium text-gray-700">Tiêu đề cuộc họp</label>
                <Textarea
                  value={title}
                  onChange={(e) => {
                    setTitle(e.target.value);
                    setTitleModifiedByUser(true);
                  }}
                  placeholder="Nhập tiêu đề cuộc họp"
                  rows={2}
                />
              </div>

              <Button variant="outline" size="sm" onClick={handleSelectFiles} className="w-full">
                Chọn file khác
              </Button>
            </div>
          ) : (
            <div className="border-2 border-dashed border-gray-300 rounded-lg p-8 text-center">
              <FileText className="h-12 w-12 text-gray-400 mx-auto mb-4" />
              <Button onClick={handleSelectFiles} disabled={status === 'selecting'}>
                {status === 'selecting' ? (
                  <>
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    Đang chọn...
                  </>
                ) : (
                  <>
                    <Upload className="h-4 w-4 mr-2" />
                    Chọn tài liệu
                  </>
                )}
              </Button>
              <p className="text-sm text-gray-500 mt-2">PDF, DOCX, TXT, SRT, VTT</p>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isImporting}>
            Hủy
          </Button>
          <Button
            onClick={handleImport}
            className="bg-[#16478e] hover:bg-[#1a55ab]"
            disabled={selectedPaths.length === 0 || isImporting}
          >
            {isImporting ? (
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            ) : (
              <Upload className="h-4 w-4 mr-2" />
            )}
            Tạo cuộc họp
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Write the barrel export**

`frontend/src/components/ImportDocuments/index.ts`:
```typescript
export { DocumentImportDialog } from './DocumentImportDialog';
```

- [ ] **Step 3: Verify it type-checks**

Run (from `frontend`):
```
pnpm exec tsc --noEmit
```
Expected: no new type errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/ImportDocuments
git commit -m "feat: add DocumentImportDialog component"
```

---

## Task 10: Wire the dialog into the app layout

**Files:**
- Modify: `frontend/src/app/layout.tsx`

- [ ] **Step 1: Import the new pieces**

In `frontend/src/app/layout.tsx`, after line 24
(`import { ImportDialogProvider } from '@/contexts/ImportDialogContext'`), add:
```tsx
import { DocumentImportDialog } from '@/components/ImportDocuments'
import { DocumentImportDialogProvider } from '@/contexts/DocumentImportDialogContext'
```

- [ ] **Step 2: Add a stable wrapper component**

After the existing `ConditionalImportDialog` component (ends at line 53), add:
```tsx
function ConditionalDocumentImportDialog({
  showDocumentImportDialog,
  handleDocumentImportDialogClose,
}: {
  showDocumentImportDialog: boolean;
  handleDocumentImportDialogClose: (open: boolean) => void;
}) {
  return (
    <DocumentImportDialog
      open={showDocumentImportDialog}
      onOpenChange={handleDocumentImportDialogClose}
    />
  );
}
```

- [ ] **Step 3: Add state for the new dialog**

Right after line 68 (`const [importFilePath, setImportFilePath] = useState<string | null>(null)`),
add:
```tsx
  const [showDocumentImportDialog, setShowDocumentImportDialog] = useState(false)
```

- [ ] **Step 4: Add open/close handlers**

Right after the existing `handleOpenImportDialog` callback (around line 202, ends with
`}, []);`), add:
```tsx
  const handleDocumentImportDialogClose = useCallback((open: boolean) => {
    setShowDocumentImportDialog(open);
  }, []);

  const handleOpenDocumentImportDialog = useCallback(() => {
    setShowDocumentImportDialog(true);
  }, []);
```

- [ ] **Step 5: Nest the provider and render the dialog**

Wrap the existing `ImportDialogProvider` block with `DocumentImportDialogProvider`, and render
`ConditionalDocumentImportDialog` next to `ConditionalImportDialog`. Change:
```tsx
                            <ImportDialogProvider onOpen={handleOpenImportDialog}>
                              {/* Download progress toast provider - listens for background downloads */}
                              <DownloadProgressToastProvider />

                              {/* Show onboarding or main app */}
                              {showOnboarding ? (
                                <OnboardingFlow onComplete={handleOnboardingComplete} />
                              ) : (
                                <div className="flex">
                                  <Sidebar />
                                  <MainContent>{children}</MainContent>
                                </div>
                              )}
                              {/* Import audio overlay and dialog */}
                              <ImportDropOverlay visible={showDropOverlay} />
                              <ConditionalImportDialog
                                showImportDialog={showImportDialog}
                                handleImportDialogClose={handleImportDialogClose}
                                importFilePath={importFilePath}
                              />
                            </ImportDialogProvider>
```
to:
```tsx
                            <ImportDialogProvider onOpen={handleOpenImportDialog}>
                            <DocumentImportDialogProvider onOpen={handleOpenDocumentImportDialog}>
                              {/* Download progress toast provider - listens for background downloads */}
                              <DownloadProgressToastProvider />

                              {/* Show onboarding or main app */}
                              {showOnboarding ? (
                                <OnboardingFlow onComplete={handleOnboardingComplete} />
                              ) : (
                                <div className="flex">
                                  <Sidebar />
                                  <MainContent>{children}</MainContent>
                                </div>
                              )}
                              {/* Import audio overlay and dialog */}
                              <ImportDropOverlay visible={showDropOverlay} />
                              <ConditionalImportDialog
                                showImportDialog={showImportDialog}
                                handleImportDialogClose={handleImportDialogClose}
                                importFilePath={importFilePath}
                              />
                              {/* Document import dialog */}
                              <ConditionalDocumentImportDialog
                                showDocumentImportDialog={showDocumentImportDialog}
                                handleDocumentImportDialogClose={handleDocumentImportDialogClose}
                              />
                            </DocumentImportDialogProvider>
                            </ImportDialogProvider>
```

- [ ] **Step 6: Verify it type-checks**

Run (from `frontend`):
```
pnpm exec tsc --noEmit
```
Expected: no new type errors.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/app/layout.tsx
git commit -m "feat: wire DocumentImportDialog into app layout"
```

---

## Task 11: Add the sidebar button

**Files:**
- Modify: `frontend/src/components/Sidebar/index.tsx`

- [ ] **Step 1: Import the hook and the icon**

Find the `useImportDialog` import/usage near line 61 and the `Upload` icon import at the top of the
file; add `FileText` to the existing `lucide-react` import, and add the new hook call right after
line 61 (`const { openImportDialog } = useImportDialog();`):
```tsx
  const { openDocumentImportDialog } = useDocumentImportDialog();
```
Add the import at the top of the file next to the existing `useImportDialog` import:
```tsx
import { useDocumentImportDialog } from '@/contexts/DocumentImportDialogContext';
```

- [ ] **Step 2: Add the collapsed (icon-only) button**

In `frontend/src/components/Sidebar/index.tsx`, right after the existing import-audio `Tooltip`
block that ends at line 504 (`</Tooltip>`, the one with `<p>Nhập file âm thanh</p>`), add:
```tsx
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => openDocumentImportDialog()}
                className="p-2 rounded-lg transition-colors duration-150 bg-[rgba(22,71,142,0.08)] hover:bg-[rgba(22,71,142,0.15)]"
              >
                <FileText className="w-5 h-5 text-[#16478e]" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Tải tài liệu lên</p>
            </TooltipContent>
          </Tooltip>
```

- [ ] **Step 3: Add the expanded (labeled) button**

Right after the existing expanded import-audio button block that ends at line 801 (`</button>`,
the one with `<span>Nhập file âm thanh</span>`), add:
```tsx
            {/* Import documents */}
            <button
              onClick={() => openDocumentImportDialog()}
              className="w-full flex items-center justify-center gap-2 px-3 py-2 text-sm font-medium text-[#16478e] border border-[#16478e] bg-transparent hover:bg-[rgba(22,71,142,0.08)] rounded-lg transition-colors"
            >
              <FileText className="w-3.5 h-3.5 shrink-0" />
              <span>Tải tài liệu lên</span>
            </button>
```

- [ ] **Step 4: Verify it type-checks**

Run (from `frontend`):
```
pnpm exec tsc --noEmit
```
Expected: no new type errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/Sidebar/index.tsx
git commit -m "feat: add 'Tải tài liệu lên' button to sidebar"
```

---

## Task 12: Manual end-to-end verification

**Files:** none (manual QA only)

- [ ] **Step 1: Build and run the app**

From `frontend`:
```
pnpm run tauri:dev
```

- [ ] **Step 2: Prepare test files**

Prepare, in any scratch folder:
- one small `.txt` file with a few lines of fake meeting notes in Vietnamese
- one `.srt` file (can reuse the sample from Task 2's test, saved as a real file)
- one `.docx` file with 2-3 paragraphs of text (e.g. saved from Word/Google Docs)

- [ ] **Step 3: Import multiple files into one meeting**

In the running app, click "Tải tài liệu lên" in the sidebar (test both the expanded sidebar and, if
present in this UI, the collapsed icon-only rail). Select all three prepared files at once, confirm
the file list shows all three names, enter a title, click "Tạo cuộc họp". Verify:
- Toast shows success with "3 tài liệu"
- App navigates to the new meeting's detail page
- The transcript panel shows the combined text, with `--- Tài liệu: <filename> ---` headers
  separating each file's content, in the order selected
- No audio player / no "Phát audio ghi âm" button appears (there's no audio for this meeting)

- [ ] **Step 4: Generate a report from the imported content**

On that meeting's detail page, pick an existing template (e.g. the default one) and generate a
report the same way you would for a recorded meeting. Verify the report generates successfully and
reflects the imported content.

- [ ] **Step 5: Export the report**

Export the generated report to DOCX and/or PDF (existing export feature) and confirm the file opens
correctly.

- [ ] **Step 6: Verify the all-or-nothing error path**

Try importing a `.pdf` you know has no real text layer (e.g. a screenshot saved as PDF, or just a
corrupted/renamed file) alongside a valid `.txt` file. Verify:
- The import is rejected with an error naming the offending file
- No new meeting was created (check the meetings list — count unchanged)

- [ ] **Step 7: Report results**

If all checks pass, this task is done — no commit needed (manual QA only). If something fails, fix
the relevant task above and re-run this task's checks.
