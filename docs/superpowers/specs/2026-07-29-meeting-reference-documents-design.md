# Tài liệu tham khảo đính kèm cuộc họp

## Vấn đề

Meetily hiện chỉ đưa nội dung transcript (lời thoại được ghi âm) vào LLM khi tạo báo cáo. Trong
thực tế, nhiều cuộc họp có tài liệu tham khảo đi kèm (slide PPT, văn bản DOCX, PDF) mà người
tham dự dựa vào để thảo luận — ví dụ người nói "như trên slide 3" nhưng nội dung slide đó không
hề xuất hiện trong lời thoại. Báo cáo tạo ra vì vậy thiếu ngữ cảnh quan trọng.

Người dùng cần: đính kèm tài liệu tham khảo vào một cuộc họp đã tồn tại (bất kỳ lúc nào trên
trang chi tiết cuộc họp), để nội dung tài liệu đó được đưa vào cùng transcript khi tạo báo cáo.

## Ngoài phạm vi

- **Định dạng .ppt cũ (nhị phân, trước Office 2007) không được hỗ trợ** — chỉ PPTX (XML-based,
  từ PowerPoint 2007 trở lên). Lý do: .ppt cũ dùng định dạng OLE Compound File phức tạp, cần thư
  viện phân tích riêng biệt, không có sẵn crate nhẹ nào trong hệ sinh thái Rust hiện tại đủ tin
  cậy để thêm vào mà không tốn nhiều công sức. Nếu cần .ppt cũ, đây là một tính năng riêng, không
  nằm trong spec này.
- Không lưu file gốc — chỉ lưu văn bản đã trích xuất trong DB (đã xác nhận với người dùng).
- Không giới hạn độ dài văn bản trích xuất được đưa vào prompt (không thêm logic cắt/chunk) —
  nhất quán với cách `transcript`/`custom_prompt` hiện tại cũng không bị giới hạn độ dài trong
  `processor.rs`.
- Đây là tính năng độc lập với module `document_import` hiện có (module đó tạo **cuộc họp mới**
  từ tài liệu; tính năng này đính tài liệu vào **cuộc họp đã tồn tại**). Tuy nhiên phần trích
  xuất văn bản (extractor) sẽ được **tái sử dụng và mở rộng chung** cho cả hai, không viết trùng.

## Kiến trúc tổng quan

```
User chọn file (PDF/DOCX/PPTX)
        ↓
api_attach_meeting_document(meeting_id, path)   [Tauri command]
        ↓
document_import::extractors::extract_text_validated(path)   [tái dùng + thêm PPTX]
        ↓
MeetingDocumentsRepository::create(...)   → bảng `meeting_documents`
        ↓
(khi bấm "Tạo tóm tắt")
service.rs: MeetingDocumentsRepository::list_by_meeting(meeting_id)
        ↓
processor.rs: generate_meeting_summary(..., documents_context: Option<String>)
        ↓
build_final_user_prompt() ghép <meeting_documents> vào final_user_prompt, cạnh <transcript>
        ↓
LLM
```

## 1. Schema DB

Migration mới: `frontend/src-tauri/migrations/20260729010000_add_meeting_documents.sql`

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

(Theo đúng style của `transcripts`/`summary_processes` — `id TEXT PRIMARY KEY`, FK cascade khi xóa
meeting, `created_at` dạng `TEXT`.)

## 2. Rust: model + repository

`frontend/src-tauri/src/database/models.rs` — thêm:

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

`frontend/src-tauri/src/database/repositories/meeting_document.rs` (mới) — theo đúng pattern của
`transcript_chunk.rs`/`meeting.rs`:

```rust
pub struct MeetingDocumentsRepository;

impl MeetingDocumentsRepository {
    pub async fn create(pool: &SqlitePool, meeting_id: &str, filename: &str, extracted_text: &str) -> Result<MeetingDocument, sqlx::Error>;
    pub async fn list_by_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<MeetingDocument>, sqlx::Error>;
    pub async fn delete(pool: &SqlitePool, document_id: &str) -> Result<bool, sqlx::Error>;
}
```

Đăng ký `pub mod meeting_document;` trong `database/repositories/mod.rs`.

## 3. Rust: mở rộng extractor (thêm PPTX)

`frontend/src-tauri/src/document_import/extractors.rs`:

- Thêm `"pptx"` vào `SUPPORTED_EXTENSIONS`.
- Thêm hàm `extract_from_pptx(path: &Path) -> Result<String, String>`: PPTX là file ZIP chứa các
  slide XML tại `ppt/slides/slideN.xml`. Dùng crate `zip` (đã có sẵn trong Cargo.toml, dùng ở nơi
  khác trong repo) để mở archive, duyệt các entry khớp pattern `ppt/slides/slide*.xml` theo đúng
  thứ tự số, và dùng regex (crate `regex` đã có sẵn, đã dùng cho SRT/VTT) để bóc nội dung trong
  thẻ `<a:t>...</a:t>` (namespace DrawingML text run) — nối các slide lại, mỗi slide cách nhau
  bằng dòng trống hoặc nhãn `--- Slide N ---`.
- Thêm nhánh `"pptx" => extract_from_pptx(path)` vào `extract_text()` và `parse_file_segments()`.
- Việc này tự động làm lợi cho `document_import` (tạo cuộc họp mới từ tài liệu) — giờ cũng nhận
  PPTX luôn, dù không ai yêu cầu, nhưng là hệ quả tự nhiên của việc mở rộng extractor dùng chung
  (không phải scope creep, vì không cần code thêm nào khác cho document_import).

`extract_text_validated()` (hàm public đã có sẵn, dùng để "trích xuất + validate độ dài tối
thiểu") sẽ được tính năng mới này gọi trực tiếp — không cần hàm mới.

## 4. Rust: Tauri commands

Module mới: `frontend/src-tauri/src/meeting_documents/commands.rs` (module riêng, không nhét vào
`document_import` vì mục đích khác — đính vào meeting có sẵn, không tạo meeting mới).

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingDocumentInfo {
    pub id: String,
    pub filename: String,
    pub char_count: i64,
    pub created_at: String,
}

#[tauri::command]
pub async fn api_select_meeting_document_files<R: Runtime>(app: AppHandle<R>) -> Result<Vec<String>, String>;
// Mở dialog chọn nhiều file, filter theo document_import::extractors::SUPPORTED_EXTENSIONS
// (tái dùng y hệt logic của api_select_document_files trong document_import/commands.rs).

#[tauri::command]
pub async fn api_attach_meeting_document<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    path: String,
) -> Result<MeetingDocumentInfo, String>;
// Trích xuất text (extract_text_validated), lưu vào meeting_documents, trả về info (không trả
// extracted_text về frontend — chỉ cần filename/char_count để hiển thị danh sách).

#[tauri::command]
pub async fn api_list_meeting_documents<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
) -> Result<Vec<MeetingDocumentInfo>, String>;

#[tauri::command]
pub async fn api_delete_meeting_document<R: Runtime>(
    app: AppHandle<R>,
    document_id: String,
) -> Result<(), String>;
```

Đăng ký `pub mod meeting_documents;` + 4 command trên trong `invoke_handler` tại `lib.rs`, theo
đúng cách `document_import::commands::api_select_document_files` đã đăng ký.

## 5. Rust: đưa vào prompt (processor.rs / service.rs)

`processor.rs`:

- `generate_meeting_summary` thêm tham số `documents_context: Option<String>` (tham số cuối
  cùng, sau `meeting_created_at`).
- Trong đoạn build `final_user_prompt` (nơi hiện đang nối `<transcript>` rồi `<user_context>`),
  chèn thêm khối `<meeting_documents>` **giữa** `<transcript>` và `<user_context>` khi
  `documents_context` có giá trị:

```rust
if let Some(docs) = &documents_context {
    if !docs.is_empty() {
        final_user_prompt.push_str("\n\n<meeting_documents>\n");
        final_user_prompt.push_str(docs);
        final_user_prompt.push_str("\n</meeting_documents>");
    }
}
```

- Cập nhật `SYSTEM_PROMPT_FINAL_TEMPLATE` (`prompts.rs`) thêm một câu hướng dẫn ngắn giải thích
  cho model biết `<meeting_documents>` là tài liệu tham khảo đi kèm (không phải lời thoại), có
  thể dùng để đối chiếu số liệu/thuật ngữ/tên riêng nhưng **không được lẫn vào phần lời thoại**
  khi trích dẫn "ai đã nói gì".

`service.rs`:

- Trong `process_transcript_background`, ngay cạnh chỗ đã fetch `meeting_created_at` (tính năng
  timestamp vừa xong), fetch thêm danh sách tài liệu:

```rust
let documents_context = match MeetingDocumentsRepository::list_by_meeting(&pool, &meeting_id).await {
    Ok(docs) if !docs.is_empty() => Some(
        docs.iter()
            .map(|d| format!("--- Tài liệu: {} ---\n{}", d.filename, d.extracted_text))
            .collect::<Vec<_>>()
            .join("\n\n")
    ),
    Ok(_) => None,
    Err(e) => {
        warn!("Failed to fetch meeting documents for prompt context: {}. Continuing without them.", e);
        None
    }
};
```

- Truyền `documents_context` vào `generate_meeting_summary(...)` làm tham số cuối.
- Lỗi khi fetch tài liệu **không được** làm fail toàn bộ quá trình tạo báo cáo (fail-soft, giống
  cách xử lý `meeting_created_at`) — báo cáo vẫn tạo được, chỉ thiếu phần tài liệu.

## 6. Frontend: hook + dialog

`frontend/src/hooks/useMeetingDocuments.ts` (mới) — theo pattern của `useImportDocuments.ts`:
state `documents`, `status` (`idle | loading | attaching | error`), `error`; các hàm
`selectAndAttach(meetingId)`, `refetch(meetingId)`, `remove(documentId)`.

`frontend/src/components/MeetingDetails/MeetingDocumentsDialog.tsx` (mới) — dialog liệt kê tài
liệu đã đính kèm (tên file, số ký tự trích xuất, nút xóa) + nút "Thêm tài liệu" mở file picker
(PDF/DOCX/PPTX). Tái dùng UI pattern/style của `DocumentImportDialog.tsx` nhưng không có bước
nhập "tiêu đề cuộc họp" (vì meeting đã tồn tại) và cho phép thêm/xóa nhiều lần, không phải luồng
một-lần "chọn rồi import" như dialog cũ.

## 7. Frontend: tích hợp vào toolbar

`frontend/src/components/MeetingDetails/SummaryGeneratorButtonGroup.tsx`:
- Thêm prop `meetingId: string`.
- Thêm 1 nút "Tài liệu" (icon `Paperclip` từ `lucide-react`, đã có sẵn trong dependency vì
  `lucide-react` đã dùng khắp nơi trong repo) trong `<ButtonGroup>`, cạnh nút "Mẫu" — mở
  `MeetingDocumentsDialog` với `meetingId`. Hiển thị số lượng tài liệu đã đính kèm dưới dạng badge
  số nhỏ trên góc nút (giống cách các nút khác trong app hiển thị trạng thái) khi > 0, để người
  dùng biết đã có tài liệu mà không cần mở dialog.

`frontend/src/components/MeetingDetails/SummaryPanel.tsx`: thêm `meeting.id` vào
`sharedGeneratorProps` truyền xuống `SummaryGeneratorButtonGroup`.

## Testing

- Rust: unit test cho `extract_from_pptx` (dựng file PPTX tối thiểu bằng crate `zip` trong test,
  giống cách `test_extract_from_docx_reads_paragraph_text` tự dựng DOCX bằng `docx_rs`) — theo
  đúng pattern test hiện có trong `extractors.rs`.
- Rust: unit test cho việc chèn `<meeting_documents>` vào `final_user_prompt` (tách logic ghép
  chuỗi này ra một hàm thuần để test được, tương tự `build_final_system_prompt` đã làm ở tính
  năng timestamp).
- Không có hạ tầng test DB trong repo này (đã xác nhận ở tính năng trước) — các hàm
  repository/command chạm DB sẽ không có unit test riêng, xác minh bằng `cargo check` + kiểm thử
  thủ công qua UI.
