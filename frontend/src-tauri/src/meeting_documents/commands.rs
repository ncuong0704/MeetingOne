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

    match MeetingDocumentsRepository::delete(app_state.db_manager.pool(), &document_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!(
            "Không tìm thấy tài liệu để xóa: {}",
            document_id
        )),
        Err(e) => Err(format!("Lỗi xóa tài liệu: {}", e)),
    }
}
