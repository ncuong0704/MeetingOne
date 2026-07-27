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
