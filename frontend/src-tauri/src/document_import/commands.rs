use crate::state::AppState;
use log::info;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use super::extractors;
use super::transcript_parser::ParsedSegment;

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
            .add_filter("File transcript", extractors::TRANSCRIPT_IMPORT_EXTENSIONS)
            .blocking_pick_files()
    })
    .await
    .map_err(|e| format!("Lỗi mở hộp thoại chọn file: {}", e))?;

    match file_paths {
        Some(paths) => Ok(paths.into_iter().map(|p| p.to_string()).collect()),
        None => Ok(Vec::new()),
    }
}

/// Extract text from every file in `paths`, parse timestamps when present,
/// and create a new meeting from the resulting transcript segments.
/// All-or-nothing: if any file fails to yield real text content, the whole
/// import is rejected and no meeting is created.
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

    let segments = tokio::task::spawn_blocking(move || -> Result<Vec<ParsedSegment>, String> {
        let mut all_segments: Vec<ParsedSegment> = Vec::new();
        let mut total_chars = 0usize;
        let multiple_files = paths.len() > 1;

        for path_str in &paths {
            let path = Path::new(path_str);
            let filename = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(path_str)
                .to_string();

            let file_segments = extractors::parse_file_segments(path)
                .map_err(|e| format!("{}: {}", filename, e))?;

            total_chars += file_segments
                .iter()
                .map(|segment| segment.text.chars().count())
                .sum::<usize>();

            if multiple_files {
                all_segments.push(ParsedSegment {
                    text: format!("--- Tài liệu: {} ---", filename),
                    start_seconds: None,
                    end_seconds: None,
                });
            }

            all_segments.extend(file_segments);
        }

        if total_chars < extractors::MIN_CONTENT_LENGTH {
            return Err(
                "Không trích xuất được nội dung văn bản (file rỗng hoặc quá ngắn)".to_string(),
            );
        }

        Ok(all_segments)
    })
    .await
    .map_err(|e| format!("Lỗi xử lý file: {}", e))??;

    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Không thể truy cập trạng thái ứng dụng".to_string())?;

    let meeting_id =
        create_document_meeting(app_state.db_manager.pool(), &title, &segments)
            .await
            .map_err(|e| e.to_string())?;

    info!(
        "Document import complete: meeting_id={}, segments={}",
        meeting_id,
        segments.len()
    );

    Ok(DocumentImportResult {
        meeting_id,
        title,
        files_count,
    })
}

/// Create a new meeting (no folder_path — there's no audio file) with one or
/// more transcript rows. Timestamped uploads become multiple segments.
async fn create_document_meeting(
    pool: &sqlx::SqlitePool,
    title: &str,
    segments: &[ParsedSegment],
) -> Result<String, anyhow::Error> {
    let meeting_id = format!("meeting-{}", Uuid::new_v4());
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

    for segment in segments {
        let transcript_id = format!("transcript-{}", Uuid::new_v4());
        let duration = match (segment.start_seconds, segment.end_seconds) {
            (Some(start), Some(end)) if end >= start => Some(end - start),
            _ => None,
        };

        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&transcript_id)
        .bind(&meeting_id)
        .bind(&segment.text)
        .bind(now.to_rfc3339())
        .bind(segment.start_seconds)
        .bind(segment.end_seconds)
        .bind(duration)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    info!(
        "Created document-import meeting '{}' with {} transcript segment(s)",
        meeting_id,
        segments.len()
    );

    Ok(meeting_id)
}
