# Tài liệu tham khảo: chuyển sang Markdown kiểu MarkItDown

**Ngày:** 2026-08-16
**Trạng thái:** Chốt để implement
**Nguồn:** [microsoft/markitdown](https://github.com/microsoft/markitdown)

## Vấn đề

Khi đính **Tài liệu tham khảo** vào cuộc họp, MeetingOne chỉ lấy **plain text phẳng**: không heading, không bảng Markdown, PPTX không đánh số slide. LLM khi tóm tắt không thấy cấu trúc tài liệu (tiêu đề, bảng số liệu, ranh giới slide) dù nội dung chữ vẫn còn.

Người dùng muốn dùng kỹ thuật của MarkItDown — chuyển file thành Markdown để LLM đọc tốt hơn — cho đúng các định dạng app đang nhận.

## MarkItDown làm gì (phạm vi MeetingOne)

MarkItDown là **Python**, mục tiêu: file → Markdown cho LLM (heading, list, table, link). Converter **local** (không Azure, không plugin OCR) cho các định dạng MeetingOne đang hỗ trợ:

| Định dạng | Converter MarkItDown | Thư viện | Output |
|---|---|---|---|
| PDF | `PdfConverter` | pdfplumber + pdfminer.six | Text layer; bảng/form nếu pdfplumber nhận ra; **không OCR** |
| DOCX | `DocxConverter` | mammoth → HTML → Markdown | Heading, bảng, list, link |
| PPTX | `PptxConverter` | python-pptx | `<!-- Slide number: N -->`, `#` cho title, bảng Markdown, chart, notes; ảnh chỉ caption nếu có LLM |
| TXT | `PlainTextConverter` | charset_normalizer | Passthrough (detect encoding) |
| SRT / VTT | Không có converter riêng | — | Không phải format first-class |

OCR / PDF scan: không nằm trong core. Cần `markitdown-ocr` + LLM Vision, hoặc Azure Document Intelligence / Content Understanding (cloud, trả phí).

## MeetingOne đang làm gì

Luồng giữ nguyên: chọn file → `api_attach_meeting_document` → `extract_text_validated` → SQLite `meeting_documents.extracted_text` → khi tóm tắt ghép `<meeting_documents>`.

Extractor hiện tại (`frontend/src-tauri/src/document_import/extractors.rs`):

| Định dạng | Kỹ thuật | Mất gì so với MarkItDown |
|---|---|---|
| PDF | `pdf_extract` (text layer) | Không tái tạo bảng; scan/ảnh → rỗng (giống core MarkItDown) |
| DOCX | `docx_rs`: chỉ `Paragraph` → `Run` → `Text` | Bỏ table, heading, hyperlink, list |
| PPTX | Unzip + regex `<a:t>` theo số slide | Không đánh số slide, không title `#`, không bảng, text dồn một hàng |
| TXT | `read_to_string` (UTF-8) | Không detect encoding |
| SRT/VTT | Bỏ timestamp / `WEBVTT` | MarkItDown không có tương đương; **giữ** |

Ngưỡng: `MIN_CONTENT_LENGTH = 20`. Định dạng: `pdf, docx, pptx, txt, srt, vtt`. Không copy file gốc.

## Vì sao không nhúng gói Python `markitdown`

1. Attach chạy **in-process trong Tauri/Rust**, không qua FastAPI.
2. App privacy-first, local; không Azure / không bắt buộc LLM Vision.
3. Đóng gói Python 3.10+ + `markitdown[pdf,docx,pptx]` trên Windows installer là thay đổi kiến trúc lớn, ngoài phạm vi lần này.

Quyết định: **bắt chước converter local của MarkItDown trong Rust**, cùng API hiện có. Không subprocess Python.

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Điểm vào | Vẫn `extract_text_validated`; `extracted_text` giờ là Markdown |
| PDF | Giữ `pdf_extract`. Không port pdfplumber. Scan/ảnh vẫn lỗi như cũ |
| DOCX | Heading 1–6 / Title → `#`…`######`; bảng → Markdown table; list có numbering → `- `; lấy text trong hyperlink (bỏ URL) |
| PPTX | `<!-- Slide number: N -->`; shape title/ctrTitle → `#`; bảng `<a:tbl>` → Markdown table; paragraph còn lại từng dòng; XML tối giản (chỉ `<a:t>`) fallback như cũ |
| TXT / SRT / VTT | Không đổi |
| Định dạng mới | Không thêm xlsx/html/zip/epub/ảnh/audio |
| OCR / Azure / plugin | Ngoài phạm vi |
| UI / DB / prompt XML | Không đổi schema. `<meeting_documents>` nhận Markdown |
| `parse_file_segments` | Không đụng (không còn luồng tạo họp từ transcript file) |

## Ngoài phạm vi

- Nhúng runtime Python / CLI `markitdown`
- Azure Document Intelligence, Content Understanding, `markitdown-ocr`
- Tái tạo bảng PDF, chart PPTX, speaker notes, ảnh/alt, detect encoding TXT
- Thêm filter file picker

## Tiêu chí xong

- Unit test: DOCX heading + bảng; PPTX slide marker + bảng; TXT/SRT/PDF lỗi không đổi hành vi
- `extract_text_validated` vẫn từ chối < 20 ký tự
- `api_attach_meeting_document` không đổi chữ ký
- Test `document_import` + prompt `<meeting_documents>` pass
