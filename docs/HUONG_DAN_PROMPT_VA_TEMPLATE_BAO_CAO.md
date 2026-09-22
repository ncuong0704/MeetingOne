# Hướng dẫn Prompt & Template tạo Báo cáo Cuộc họp

> **Đối tượng:** Developer đang xây dựng app tích hợp SST (Speech-to-Text) và cần thêm bước tạo báo cáo  
> **Phiên bản:** 2026-07-04  
> **Đầu vào:** Chuỗi transcript (text)  
> **Đầu ra:** Báo cáo Markdown theo mẫu (template)

---

## Mục lục

1. [Tóm tắt nhanh](#1-tóm-tắt-nhanh)
2. [Luồng xử lý sau SST](#2-luồng-xử-lý-sau-sst)
3. [Hai thành phần: Template và Prompts](#3-hai-thành-phần-template-và-prompts)
4. [Tích hợp vào app của bạn](#4-tích-hợp-vào-app-của-bạn)
5. [Template JSON — cấu trúc báo cáo](#5-template-json--cấu-trúc-báo-cáo)
6. [Prompts — quy tắc AI](#6-prompts--quy-tắc-ai)
7. [Ví dụ ghép prompt hoàn chỉnh](#7-ví-dụ-ghép-prompt-hoàn-chỉnh)
8. [Mẫu template có sẵn](#8-mẫu-template-có-sẵn)
9. [Xử lý transcript dài](#9-xử-lý-transcript-dài)
10. [Checklist tích hợp](#10-checklist-tích-hợp)
11. [File tham chiếu trong repo](#11-file-tham-khảo-trong-repo)

---

## 1. Tóm tắt nhanh

Sau khi SST cho ra **transcript**, dự án Meetily/MeetingOne tạo báo cáo qua pipeline LLM gồm **3 giai đoạn**:

| Giai đoạn | Mục đích | Khi nào chạy |
|-----------|----------|--------------|
| **1. Chunk** | Tóm tắt từng đoạn transcript dài | Chỉ Ollama/Groq khi transcript vượt ~4000 token |
| **2. Combine** | Gộp các tóm tắt chunk thành một văn bản | Chỉ khi có >1 chunk |
| **3. Final Template** | LLM điền nội dung vào khung Markdown theo template | **Luôn chạy** — mọi provider |

**Nguyên tắc thiết kế:**

- **Template JSON** → định nghĩa *báo cáo có những mục nào, mỗi mục trích xuất gì*
- **Prompts** → định nghĩa *AI phải tuân thủ quy tắc gì* (ngôn ngữ, ngày tháng, không suy diễn…)

> Muốn đổi cấu trúc báo cáo → sửa **template**.  
> Muốn đổi quy tắc chung cho mọi báo cáo → sửa **prompts**.

---

## 2. Luồng xử lý sau SST

```
┌─────────────────────────────────────────────────────────────┐
│  Bước bạn đã có: SST → transcript (text)                    │
│  Ví dụ: "[00:01] Xin chào...\n[00:15] Hôm nay thảo luận..." │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
              ┌────────────────────────────────┐
              │ Giai đoạn 1–2 (tùy chọn)       │
              │ Chunk → Combine                │
              │ Chỉ Ollama/Groq + transcript dài│
              └────────────────┬───────────────┘
                               │
                               ▼  content_to_summarize
              ┌────────────────────────────────┐
              │ Giai đoạn 3 (bắt buộc)         │
              │ Load template JSON             │
              │ → sinh system prompt           │
              │ → gọi LLM với transcript       │
              └────────────────┬───────────────┘
                               │
                               ▼
              ┌────────────────────────────────┐
              │ Báo cáo Markdown               │
              │ { "markdown": "# Tiêu đề\n..." }│
              └────────────────────────────────┘
```

**Logic chính** nằm tại `frontend/src-tauri/src/summary/processor.rs`, hàm `generate_meeting_summary()`.

**Format transcript khuyến nghị** (giống app gốc):

```
[MM:SS] Nội dung câu nói
[01:23] Ông A báo cáo doanh thu quý 1 đạt 120 tỷ...
```

Timestamp giúp AI ghi tham chiếu trong báo cáo (đặc biệt mục action items).

---

## 3. Hai thành phần: Template và Prompts

### 3.1 Template — “Báo cáo trông như thế nào”

File JSON mô tả các **section** (mục) của báo cáo:

```
frontend/src-tauri/templates/
├── theo_mau_act_no_table.json   ← mặc định hiện tại
├── standard_meeting.json
├── daily_standup.json
└── ...
```

Mỗi section có:
- `title` — tiêu đề `##` trong Markdown output
- `instruction` — chỉ dẫn chi tiết cho AI về mục đó
- `format` — gợi ý cấu trúc (`paragraph`, `list`, `string`)
- `item_format` — (tùy chọn) header bảng Markdown

Rust chuyển template thành 2 chuỗi inject vào prompt:

| Method | Output | Dùng cho |
|--------|--------|----------|
| `to_markdown_structure()` | Khung Markdown rỗng với placeholder `(điền nội dung)` | `{template_markdown}` |
| `to_section_instructions()` | Hướng dẫn từng mục bằng tiếng Việt | `{section_instructions}` |

### 3.2 Prompts — “AI phải làm thế nào”

Năm prompt constants trong `frontend/src-tauri/src/summary/prompts.rs`:

| Constant | Giai đoạn |
|----------|-----------|
| `SYSTEM_PROMPT_CHUNK` | Tóm tắt từng chunk |
| `USER_PROMPT_TEMPLATE_CHUNK` | User prompt chunk — placeholder `{chunk_content}` |
| `SYSTEM_PROMPT_COMBINE` | Gộp chunk summaries |
| `USER_PROMPT_TEMPLATE_COMBINE` | User prompt combine — placeholder `{combined_summaries}` |
| `SYSTEM_PROMPT_FINAL_TEMPLATE` | Báo cáo cuối — placeholders `{section_instructions}`, `{template_markdown}` |

Prompt giai đoạn 3 quy định quy tắc toàn cục: tiếng Việt, định dạng ngày `dd/mm/yyyy`, ghi `"(không rõ)"` khi thiếu dữ liệu, không suy diễn.

---

## 4. Tích hợp vào app của bạn

Có **3 hướng** tùy kiến trúc app:

### Hướng A — Gọi pipeline có sẵn của Meetily (Tauri)

Nếu app của bạn là frontend Tauri hoặc có thể gọi Tauri commands:

```typescript
// 1. Gửi transcript + chọn template
const result = await invoke('api_process_transcript', {
  text: transcriptText,           // Chuỗi transcript từ SST
  model: 'ollama',                // ollama | groq | claude | openai | openrouter | customOpenai
  modelName: 'qwen2.5:7b',
  meetingId: 'meeting-uuid-123',
  chunkSize: 40000,
  overlap: 1000,
  customPrompt: '',               // Context bổ sung (tùy chọn)
  templateId: 'theo_mau_act_no_table',
});

// 2. Poll trạng thái
const summary = await invoke('api_get_summary', {
  meetingId: result.process_id,
});

// 3. Lấy báo cáo
if (summary.status === 'completed' && summary.data?.markdown) {
  const reportMarkdown = summary.data.markdown;
}
```

**Tham số quan trọng:**

| Tham số | Mô tả |
|---------|-------|
| `text` | Toàn bộ transcript (nối các segment SST) |
| `templateId` | ID template — xem [mục 8](#8-mẫu-template-có-sẵn) |
| `customPrompt` | Bối cảnh thêm (tên dự án, quy ước nội bộ) — **không** thay template |
| `model` / `modelName` | Provider và model LLM |

### Hướng B — Port logic sang app riêng (khuyến nghị nếu app độc lập)

Copy/reimplement 4 bước từ `processor.rs`:

```
1. (Tùy chọn) Chunk + Combine nếu dùng Ollama/Groq và transcript dài
2. Load template JSON theo template_id
3. Gọi template.to_markdown_structure() và to_section_instructions()
4. Ghép system/user prompt → gọi LLM Chat API → nhận Markdown
```

**Pseudo-code (Python/Node/Go — ngôn ngữ tùy app):**

```python
def generate_report(transcript: str, template_id: str, llm_client) -> str:
    # Bước 1: Xử lý transcript dài (chỉ Ollama/Groq)
    content = maybe_chunk_and_combine(transcript, llm_client)

    # Bước 2: Load template
    template = load_template_json(template_id)  # từ file hoặc API

    # Bước 3: Sinh prompt
    section_instructions = template.to_section_instructions()
    template_markdown = template.to_markdown_structure()

    system_prompt = SYSTEM_PROMPT_FINAL_TEMPLATE \
        .replace("{section_instructions}", section_instructions) \
        .replace("{template_markdown}", template_markdown)

    user_prompt = f"""
<transcript_chunks>
{content}
</transcript_chunks>
"""

    # Bước 4: Gọi LLM
    raw = llm_client.chat(system=system_prompt, user=user_prompt)
    return clean_markdown(raw)
```

**File cần copy/tham chiếu:**

| File | Nội dung |
|------|----------|
| `frontend/src-tauri/templates/*.json` | Template mẫu |
| `frontend/src-tauri/src/summary/prompts.rs` | Năm prompt constants |
| `frontend/src-tauri/src/summary/templates/types.rs` | Logic `to_markdown_structure()`, `to_section_instructions()` |
| `frontend/src-tauri/src/summary/processor.rs` | Pipeline đầy đủ |

### Hướng C — Backend Python cũ (legacy)

Repo có backend FastAPI tại `backend/` với endpoint `POST /process-transcript`, nhưng pipeline này trả về **JSON blocks** (cấu trúc cũ), **không** dùng template Markdown mới.

→ **Không khuyến nghị** nếu bạn muốn báo cáo theo template ACT/standard_meeting.  
→ Dùng pipeline Rust (Hướng A hoặc B) thay thế.

---

## 5. Template JSON — cấu trúc báo cáo

### 5.1 Schema

```json
{
  "name": "Tên hiển thị trong UI",
  "description": "Mô tả ngắn",
  "sections": [
    {
      "title": "Tiêu đề mục (thành ## trong Markdown)",
      "instruction": "Chỉ dẫn chi tiết cho AI về mục này",
      "format": "paragraph | list | string",
      "item_format": "| Cột 1 | Cột 2 |\n| --- | --- |"
    }
  ]
}
```

### 5.2 Ba loại `format`

| format | Placeholder trong khung | Khi nào dùng |
|--------|-------------------------|--------------|
| `string` | `(điền nội dung)` | Mục ngắn: ngày, thông tin cuộc họp |
| `paragraph` | `(điền nội dung)` | Đoạn văn dài, có thể kèm bảng trong instruction |
| `list` | Bullet hoặc bảng từ `item_format` | Danh sách, action items |

### 5.3 Ví dụ section action items

```json
{
  "title": "Công việc cần thực hiện",
  "instruction": "Liệt kê TẤT CẢ nhiệm vụ được giao. Kèm người phụ trách và thời hạn. Viết bằng tiếng Việt.",
  "format": "list",
  "item_format": "| Người phụ trách | Nhiệm vụ | Thời hạn |\n| --- | --- | --- |"
}
```

### 5.4 Tạo template custom

**Không cần rebuild app** — lưu file JSON tại:

```
Windows: %APPDATA%\MeetingOne\templates\{template_id}.json
macOS:   ~/Library/Application Support/MeetingOne/templates/{template_id}.json
Linux:   ~/.config/MeetingOne/templates/{template_id}.json
```

Quy tắc `template_id`: chỉ `[a-zA-Z0-9_-]`.

**Clone từ template có sẵn:**

```typescript
const json = await invoke('api_get_template_json', { templateId: 'standard_meeting' });
const data = JSON.parse(json);
data.name = 'Biên bản phòng Kinh doanh';
// sửa data.sections...
await invoke('api_save_custom_template', {
  templateId: 'bien_ban_kd',
  templateJson: JSON.stringify(data, null, 2),
});
```

---

## 6. Prompts — quy tắc AI

### 6.1 Giai đoạn 3 — prompt cuối (quan trọng nhất)

**System prompt** = `SYSTEM_PROMPT_FINAL_TEMPLATE` sau khi thay:

- `{section_instructions}` ← từ template
- `{template_markdown}` ← khung Markdown rỗng từ template

**User prompt** = transcript bọc trong XML:

```xml
<transcript_chunks>
{content_to_summarize}
</transcript_chunks>

<!-- Tùy chọn -->
<user_context>
{custom_prompt}
</user_context>
```

### 6.2 Giai đoạn 1–2 — chunk/combine

Chỉ cần khi:
- Provider là **Ollama** hoặc **Groq**
- Transcript ≥ ~4000 token (ước lượng: `ceil(số_ký_tự × 0.35)`)

**Chunk:** Mỗi đoạn transcript → LLM với `USER_PROMPT_TEMPLATE_CHUNK.replace("{chunk_content}", chunk)`

**Combine:** Nối summaries bằng `\n---\n` → LLM với `USER_PROMPT_TEMPLATE_COMBINE.replace("{combined_summaries}", combined)`

### 6.3 Override prompt không cần rebuild

App lưu override trong SQLite. API Tauri:

- `api_get_prompt_settings`
- `api_save_prompt_settings`
- `api_reset_prompt_settings`

UI: **Settings → Prompt Settings**

---

## 7. Ví dụ ghép prompt hoàn chỉnh

Giả sử dùng template `standard_meeting.json` với transcript ngắn (bỏ qua chunk/combine).

### System prompt (rút gọn)

```
Bạn là Trợ lý Tóm tắt Cuộc họp AI...
**NGÔN NGỮ:** Trả lời hoàn toàn bằng tiếng Việt.
**CÁC NGUYÊN TẮC CỐT LÕI:**
1. Nguyên tắc toàn vẹn: Chỉ sử dụng thông tin có sẵn trong transcript...
...

**HƯỚNG DẪN THEO TỪNG MỤC:**
- **Cho tiêu đề chính (`# [Tiêu đề do AI tạo]`):** Phân tích toàn bộ nội dung...
- **Cho phần 'Tóm tắt':** Cung cấp một đoạn tóm tắt ngắn gọn...
- **Cho phần 'Công việc cần thực hiện':** Liệt kê TẤT CẢ các nhiệm vụ...
  - Các mục trong phần này nên theo định dạng: `| Người phụ trách | Nhiệm vụ | ...`.

<template>
# <Tiêu đề cuộc họp>

## Tóm tắt

(điền nội dung)

## Quyết định chính

- (điền mục 1)
...
</template>
```

### User prompt

```xml
<transcript_chunks>
[00:00] Chào mừng các anh chị đến họp giao ban tuần 12.
[00:15] Phòng Kinh doanh báo cáo doanh thu tháng 3 đạt 15 tỷ, vượt 10% kế hoạch.
[01:02] Anh Minh được giao hoàn thành báo cáo chi tiết trước ngày 10/04/2026.
</transcript_chunks>
```

### Output mong đợi

```markdown
# Họp giao ban tuần 12 — Báo cáo Kinh doanh tháng 3

## Tóm tắt

Cuộc họp giao ban tuần 12 tập trung vào kết quả kinh doanh tháng 3...

## Quyết định chính

- Phòng Kinh doanh đạt doanh thu vượt kế hoạch 10%

## Công việc cần thực hiện

| Người phụ trách | Nhiệm vụ | Thời hạn | ... |
| --- | --- | --- | --- |
| Anh Minh | Hoàn thành báo cáo chi tiết | 10/04/2026 | ... |
```

---

## 8. Mẫu template có sẵn

| ID | Tên | Phù hợp khi |
|----|-----|-------------|
| `theo_mau_act_no_table` | Theo mẫu ACT — Không bảng | **Mặc định** — giao ban ACT, văn bản hành chính |
| `standard_meeting` | Biên bản họp thông thường | Cuộc họp nội bộ, action items 5 cột |
| `daily_standup` | Họp nhanh hàng ngày | Standup Scrum |
| `project_sync` | Cập nhật dự án | Tiến độ, rủi ro |
| `retrospective` | Sprint retrospective | What went well / improve |
| `sales_marketing_client_call` | Gọi khách hàng | Sales & marketing |

File JSON gốc: `frontend/src-tauri/templates/{id}.json`

---

## 9. Xử lý transcript dài

### Ước lượng token

```rust
// processor.rs
token_count = ceil(char_count × 0.35)
```

Ví dụ: transcript 20.000 ký tự ≈ 7.000 token → cần chunk nếu dùng Ollama/Groq.

### Ngưỡng chunk theo provider

| Provider | Ngưỡng | Chunk size |
|----------|--------|------------|
| Ollama | 4.000 token (hoặc `token_threshold`) | limit − 300, overlap 50 |
| Groq | 4.500 token | limit − 300, overlap 50 |
| Claude, OpenAI, OpenRouter, CustomOpenAI | Không chunk | Gửi full transcript |

### Provider LLM hỗ trợ

Trong `llm_client.rs`: `openai`, `claude`, `groq`, `ollama`, `openrouter`, `customOpenai`

API OpenAI-compatible dùng format:

```json
{
  "model": "gpt-4o",
  "messages": [
    { "role": "system", "content": "..." },
    { "role": "user", "content": "..." }
  ]
}
```

Claude dùng field `system` riêng (không nằm trong `messages`).

---

## 10. Checklist tích hợp

Sau khi SST hoạt động, làm lần lượt:

- [ ] **Nối transcript:** Gộp các segment SST thành một chuỗi, thêm timestamp `[MM:SS]` nếu có
- [ ] **Chọn template:** Bắt đầu với `theo_mau_act_no_table` hoặc `standard_meeting`
- [ ] **Cấu hình LLM:** Ollama local hoặc API cloud (Groq, Claude, OpenAI…)
- [ ] **Implement giai đoạn 3:** Load template → ghép prompt → gọi LLM → lưu Markdown
- [ ] **(Tùy chọn) Giai đoạn 1–2:** Nếu dùng Ollama/Groq và cuộc họp > ~30 phút
- [ ] **Test với transcript thật:** Kiểm tra action items, tên người, ngày tháng
- [ ] **Tùy chỉnh template:** Clone JSON, sửa `instruction` cho phù hợp quy trình công ty
- [ ] **Hiển thị báo cáo:** Render Markdown hoặc convert sang PDF/Word

### Sửa output sai — tra nhanh

| Triệu chứng | Sửa gì |
|-------------|--------|
| Thiếu mục trong báo cáo | Thêm section vào template JSON |
| Mục có nhưng nội dung quá ngắn | Làm `instruction` chi tiết hơn |
| Ngày tháng format lộn xộn | Sửa `SYSTEM_PROMPT_FINAL_TEMPLATE` |
| Mất chi tiết cuộc họp dài (Ollama) | Sửa chunk/combine prompts |
| AI thêm thông tin không có trong transcript | Tăng cường quy tắc toàn vẹn trong final prompt |

---

## 11. File tham khảo trong repo

```
frontend/src-tauri/
├── templates/                          # Template JSON built-in
│   ├── theo_mau_act_no_table.json
│   ├── standard_meeting.json
│   └── ...
├── src/summary/
│   ├── prompts.rs                      # 5 prompt constants
│   ├── prompt_config.rs                # Override từ DB
│   ├── processor.rs                    # Pipeline chính ★
│   ├── service.rs                      # Background task + lưu DB
│   ├── llm_client.rs                     # Gọi API từng provider
│   ├── commands.rs                     # api_process_transcript, api_get_summary
│   └── templates/
│       ├── types.rs                    # Template struct + to_markdown*
│       ├── loader.rs                   # Load custom/built-in
│       └── defaults.rs                 # Danh sách template ID

frontend/src/hooks/meeting-details/
└── useSummaryGeneration.ts             # Frontend gọi pipeline

docs/superpowers/specs/
└── 2026-07-04-template-prompts-guide.md # Hướng dẫn developer chi tiết (sửa template/prompts)
```

---

## Tóm tắt một dòng

> **SST → transcript text → (chunk/combine nếu cần) → load template JSON → ghép prompt → LLM → báo cáo Markdown**

Nếu bạn chỉ cần tích hợp nhanh: copy file template JSON + nội dung `prompts.rs`, implement hàm `to_markdown_structure()` / `to_section_instructions()` từ `types.rs`, rồi gọi LLM với system/user prompt như [mục 7](#7-ví-dụ-ghép-prompt-hoàn-chỉnh).
