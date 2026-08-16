# MarkItDown-style meeting documents — Implementation Plan

> **For agentic workers:** Execute inline in this session. Spec: [2026-08-16-markitdown-meeting-documents-design.md](../specs/2026-08-16-markitdown-meeting-documents-design.md)

**Goal:** Tài liệu tham khảo (DOCX/PPTX) được trích thành Markdown có heading/bảng/slide marker giống converter local của MarkItDown; TXT/SRT/VTT/PDF và API attach không đổi chữ ký.

**Architecture:** Thêm helper `to_markdown.rs` trong `document_import`. `extract_from_docx` / `extract_from_pptx` gọi helper; `extract_text_validated` và Tauri commands không đổi interface.

**Tech Stack:** Rust (`docx-rs`, `zip`, `regex`, `once_cell`) — không Python, không crate mới.

---

### Task 1: Helper Markdown

**Files:**
- Create: `frontend/src-tauri/src/document_import/to_markdown.rs`
- Modify: `frontend/src-tauri/src/document_import/mod.rs` — `mod to_markdown;`

- [ ] **Step 1: Viết `to_markdown.rs`** với `heading_prefix`, `to_markdown_table`, `decode_xml_entities`, `pptx_slide_xml_to_markdown`.
- [ ] **Step 2: Unit test helper** (heading, table rỗng, escape `|`, slide XML có bảng).

### Task 2: Đổi extractor DOCX/PPTX

**Files:**
- Modify: `frontend/src-tauri/src/document_import/extractors.rs`

- [ ] **Step 1: DOCX** — paragraph (heading/list/hyperlink) + table Markdown.
- [ ] **Step 2: PPTX** — `<!-- Slide number: N -->` + `pptx_slide_xml_to_markdown`.
- [ ] **Step 3: Test fixture** heading+table DOCX; slide marker+bảng PPTX; test cũ vẫn pass.

### Task 3: Verify + restart

- [ ] `cargo test --lib document_import`
- [ ] `cargo test --lib summary::processor summary::prompts`
- [ ] Restart `pnpm run tauri:dev` (không dùng `clean_run_windows.bat`)
