# Thiết kế: Import tài liệu văn bản để tạo báo cáo

**Ngày**: 2026-07-27
**Trạng thái**: Đã duyệt, chờ lập kế hoạch triển khai

## Bối cảnh

Hiện tại app chỉ tạo transcript từ hai nguồn: ghi âm trực tiếp, hoặc import file âm thanh
(`frontend/src-tauri/src/audio/import.rs`) rồi chạy ASR (ZipFormer/sherpa-onnx) để sinh transcript.
Từ transcript, người dùng chọn một template công ty (JSON có cấu trúc `sections[]`, xem
`frontend/src-tauri/src/summary/templates/types.rs`) và gọi `api_process_transcript`
(`frontend/src-tauri/src/summary/commands.rs:167-240`) để LLM sinh báo cáo, lưu vào
`summary_processes.result`.

Người dùng muốn thêm khả năng: tải lên các tài liệu có sẵn (DOCX, PDF, TXT/SRT/VTT transcript từ
nguồn khác như Zoom/Teams) và dùng nội dung đó (không phải audio) làm đầu vào để tạo báo cáo theo
template công ty, tái sử dụng toàn bộ pipeline tạo báo cáo hiện có.

## Mục tiêu

- Cho phép chọn **nhiều file** (PDF/DOCX/TXT/SRT/VTT) trong một lần import.
- Gộp nội dung tất cả file thành **một meeting mới** (một transcript), không phải nhiều meeting.
- Từ meeting đó, người dùng dùng lại UI/luồng chọn template + tạo báo cáo đã có sẵn — không có
  logic tạo báo cáo mới, không có template mới.
- Không cần audio, không cần ASR, không cần giữ speaker/timestamp cho nội dung import.

## Ngoài phạm vi (Out of scope)

- OCR cho PDF dạng scan/ảnh (không có text thật) — ghi nhận là hướng mở rộng tương lai, không làm ở giai đoạn này.
- Chỉnh sửa nội dung transcript đã import trong UI trước khi tạo báo cáo.
- Import hàng loạt để tạo nhiều meeting cùng lúc (mỗi lần import luôn tạo đúng 1 meeting, dù chọn bao nhiêu file).

## Kiến trúc

Tài liệu import không phải một tính năng song song — nó chỉ là một **nguồn transcript mới**.
Sau khi tạo xong `meetings` + `transcripts`, mọi thứ phía sau (chọn template, gọi LLM, xuất
docx/pdf) dùng **nguyên vẹn** pipeline đã có, không sửa `summary/*`.

```
Chọn nhiều file (PDF/DOCX/TXT/SRT/VTT)
        ↓
Trích xuất text từng file (theo định dạng)
        ↓
Validate: mỗi file phải có text thật (không rỗng/không phải scan)
        ↓
Gộp: "--- Tài liệu: <tên_file> ---\n<nội dung>" nối giữa các file
        ↓
Tạo 1 record `meetings` (source_type = 'document_import') + 1 record `transcripts`
        ↓
[Không đổi] Người dùng chọn template → api_process_transcript → LLM → summary_processes.result
        ↓
[Không đổi] Xem/xuất báo cáo (docx/pdf) như luồng hiện tại
```

## Thành phần

### 1. Module Rust mới: `frontend/src-tauri/src/document_import/`

- **`extractors.rs`**
  - `extract_text_from_pdf(path) -> Result<String, String>` — dùng crate mới `pdf-extract`.
  - `extract_text_from_docx(path) -> Result<String, String>` — dùng crate `docx-rs` đã có sẵn
    trong `Cargo.toml:98` (hiện chỉ dùng để ghi file khi export; dùng thêm API đọc `read_docx`).
  - `extract_text_from_plain(path) -> Result<String, String>` — cho `.txt`.
  - `extract_text_from_subtitle(path) -> Result<String, String>` — cho `.srt`/`.vtt`, loại bỏ số
    thứ tự khối và dòng timestamp (`00:00:01,000 --> 00:00:04,000`), chỉ giữ lại các dòng lời thoại.
  - Hàm điều phối `extract_text(path) -> Result<String, String>` chọn extractor theo đuôi file.
  - Ngưỡng hợp lệ: sau khi trim, text phải dài hơn một ngưỡng tối thiểu nhỏ (ví dụ 20 ký tự)
    để coi là "có nội dung thật"; nếu không, trả lỗi nêu rõ tên file.

- **`commands.rs`** (Tauri commands mới, đăng ký trong `lib.rs`)
  - `api_select_document_files() -> Result<Vec<String>, String>` — mở dialog chọn nhiều file,
    filter theo đuôi hỗ trợ (`pdf`, `docx`, `txt`, `srt`, `vtt`).
  - `api_import_documents(paths: Vec<String>, title: String) -> Result<i64, String>`:
    1. Với mỗi file: gọi `extract_text`; nếu bất kỳ file nào lỗi/rỗng → **hủy toàn bộ**, trả lỗi
       liệt kê (các) file gây lỗi, không tạo meeting.
    2. Gộp nội dung các file hợp lệ theo thứ tự chọn, mỗi đoạn có header
       `--- Tài liệu: <tên_file gốc> ---`.
    3. Tạo `meetings` row mới (`source_type = 'document_import'`, `title` = tham số truyền vào).
    4. Tạo `transcripts` row với `transcript` = nội dung đã gộp, `timestamp` = thời điểm import,
       để trống các trường audio-only (`audio_start_time`, `audio_end_time`, `duration`).
    5. Trả `meeting_id`.

### 2. Database

Migration mới, thêm cột trên bảng `meetings`:

```sql
ALTER TABLE meetings ADD COLUMN source_type TEXT NOT NULL DEFAULT 'recorded';
```

Giá trị: `'recorded'` (mặc định, ghi âm trực tiếp hoặc import audio hiện có), `'document_import'`
(mới). Frontend dùng cột này để ẩn các UI liên quan đến audio (player, waveform) cho meeting loại
`document_import` vì không có file âm thanh thật.

### 3. Frontend

- Nút **"Tải tài liệu lên"** đặt ngay dưới nút import audio hiện có (theo yêu cầu người dùng).
- Flow: bấm nút → dialog chọn nhiều file (gọi `api_select_document_files`) → modal nhập tên
  meeting → gọi `api_import_documents(paths, title)` → điều hướng sang trang chi tiết meeting vừa
  tạo.
- Trang chi tiết meeting / `TranscriptPanel`: khi `meeting.source_type === 'document_import'`, ẩn
  audio player/waveform/nút phát lại; phần transcript + chọn template + tạo báo cáo giữ nguyên
  UI/logic hiện có (`useTemplates`, luồng gọi `api_process_transcript`).

## Xử lý lỗi

- Đuôi file không được hỗ trợ → chặn ngay ở bộ lọc dialog chọn file.
- File PDF scan/ảnh (không trích xuất được text thật) hoặc file lỗi/hỏng → hủy toàn bộ import,
  báo lỗi rõ tên (các) file gây lỗi, **không tạo meeting** (tất cả-hoặc-không-gì, tránh báo cáo
  tạo từ dữ liệu thiếu).
- Lỗi khi tạo báo cáo (gọi LLM) → không đổi, dùng nguyên cơ chế lỗi/retry đã có trong
  `summary/processor.rs` và `summary/service.rs`.

## Testing

- Unit test Rust cho từng extractor (`extractors.rs`) với file mẫu nhỏ: `.docx`, `.pdf` (có text
  thật), `.pdf` (giả lập scan/rỗng để test path lỗi), `.txt`, `.srt`, `.vtt`.
- Test thủ công qua UI: import 2-3 file định dạng khác nhau → kiểm tra transcript gộp đúng thứ
  tự, có header phân tách theo tên file → chọn template có sẵn → tạo báo cáo → kiểm tra xuất
  docx/pdf vẫn hoạt động như với meeting ghi âm thông thường.

## Quyết định đã chốt (từ trao đổi với người dùng)

- File import → tạo **meeting mới** dùng chung luồng summary hiện có (không tách luồng riêng).
- Định dạng hỗ trợ giai đoạn đầu: PDF, DOCX, TXT/SRT/VTT.
- Không cần giữ speaker/timestamp cho nội dung import — coi là văn bản thuần.
- Tạo báo cáo vẫn qua LLM như luồng hiện tại (không chèn raw text vào template).
- Nút import đặt ngay dưới nút import audio hiện có.
- Cho phép chọn nhiều file trong 1 lần import, nhưng luôn gộp thành **1 meeting duy nhất** (không
  phải nhiều meeting).
- Gộp nội dung có đánh dấu tên file phân tách từng tài liệu.
- Tiêu đề meeting do người dùng nhập tay khi import.
- PDF dạng scan không trích xuất được text → báo lỗi, không cố gắng OCR (giai đoạn đầu).
