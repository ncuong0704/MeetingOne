# Hướng dẫn Developer: Template & Prompts cho Báo cáo Cuộc họp

> **Phiên bản:** 2026-07-04  
> **Đối tượng:** Developer làm việc trên MeetingOne  
> **Mục tiêu:** Biết khi nào sửa template JSON, khi nào sửa prompts, và cách thêm/sửa từng loại một cách an toàn.

---

## Mục lục

1. [Tổng quan kiến trúc](#1-tổng-quan-kiến-trúc)
2. [Sơ đồ quyết định: Template hay Prompts?](#2-sơ-đồ-quyết-định-template-hay-prompts)
3. [Reference: Template JSON](#3-reference-template-json)
4. [Reference: Prompts](#4-reference-prompts)
5. [Cookbook Developer](#5-cookbook-developer)
6. [Best practices & Anti-patterns](#6-best-practices--anti-patterns)
7. [Phụ lục: File map](#7-phụ-lục-file-map)

---

## 1. Tổng quan kiến trúc

### 1.1 Luồng tạo báo cáo

Sau khi có **transcript**, hệ thống tạo báo cáo Markdown qua **một giai đoạn duy nhất** — gửi **toàn bộ transcript** vào LLM:

```
Transcript (đầy đủ)
        │
        ▼
┌───────────────────────────────────────┐
│ Final Template Fill                   │
│ Load template JSON → sinh prompt      │
│ → LLM điền vào khung Markdown         │
│   (transcript gốc, không chunk)       │
└───────────────────────────────────────┘
        │
        ▼
Báo cáo Markdown (lưu DB, hiển thị UI)
```

**File xử lý chính:** `frontend/src-tauri/src/summary/processor.rs` — hàm `generate_meeting_summary()`.

### 1.2 Hai lớp cấu hình

| Lớp | Vị trí | Kiểm soát gì |
|-----|--------|--------------|
| **Template JSON** | `frontend/src-tauri/templates/*.json` (built-in) hoặc `%APPDATA%\MeetingOne\templates\` (custom) | Cấu trúc báo cáo: tiêu đề từng mục, hướng dẫn trích xuất, định dạng list/bảng |
| **Prompts** | `frontend/src-tauri/src/summary/prompts.rs` (mặc định) hoặc DB qua UI PromptSettings | Hành vi AI toàn cục: ngôn ngữ, quy tắc ngày tháng, xử lý dữ liệu thiếu |

**Nguyên tắc vàng:** Template định nghĩa *báo cáo trông như thế nào*; Prompts định nghĩa *AI phải tuân thủ quy tắc gì khi viết*.

### 1.3 Cách template được nạp vào prompt

Khi tạo báo cáo, Rust gọi hai method trên struct `Template`:

```rust
// types.rs
template.to_markdown_structure()   // → khung Markdown rỗng (placeholder)
template.to_section_instructions() // → hướng dẫn chi tiết từng mục
```

Hai chuỗi này được inject vào `SYSTEM_PROMPT_FINAL_TEMPLATE`:

```
{section_instructions}  ← từ template.sections[].instruction
{template_markdown}     ← từ template.sections[].title + format
```

Toàn bộ transcript được đưa vào **user prompt** trong thẻ `<transcript_chunks>`.

### 1.4 Thứ tự ưu tiên khi load template

`templates/loader.rs` — hàm `get_template(id)`:

1. **Custom** — `%APPDATA%\MeetingOne\templates\{id}.json` (Windows)
2. **Bundled** — thư mục resources khi cài app
3. **Built-in** — nhúng trong binary qua `defaults.rs` + `include_str!`

Custom file **ghi đè** built-in cùng `id` mà không cần rebuild app.

---

## 2. Sơ đồ quyết định: Template hay Prompts?

Dùng sơ đồ này trước khi sửa bất kỳ file nào:

```
Bạn muốn thay đổi điều gì?
│
├─ Thêm / bớt / đổi tên mục trong báo cáo
│     (VD: thêm "Phụ lục", bỏ "Ghi chú")
│     → SỬA TEMPLATE JSON (sections)
│
├─ Thay đổi nội dung AI trích xuất cho MỘT mục cụ thể
│     (VD: "phải ghi đích danh người phát biểu", "dùng bảng 3 cột")
│     → SỬA instruction của section đó trong TEMPLATE JSON
│
├─ Thay đổi định dạng bảng / bullet của một mục
│     (VD: thêm cột "Mức độ ưu tiên")
│     → SỬA item_format (nếu format=list) HOẶC instruction (nếu format=paragraph có bảng inline)
│
├─ Quy tắc áp dụng cho TOÀN BỘ báo cáo, mọi template
│     (VD: ngày tháng dd/mm/yyyy, ghi "(không rõ)", cấm suy diễn)
│     → SỬA SYSTEM_PROMPT_FINAL_TEMPLATE trong prompts.rs
│        hoặc PromptSettings UI (không cần rebuild)
│
├─ Output đúng cấu trúc nhưng thiếu chi tiết dù instruction đã rõ
│     → SỬA CẢ HAI: làm instruction cụ thể hơn + tăng cường quy tắc trong final prompt
│
└─ Tạo loại báo cáo hoàn toàn mới (VD: biên bản HĐQT, báo cáo tuần)
      → TẠO TEMPLATE JSON MỚI (ưu tiên)
         Chỉ sửa prompts nếu quy tắc mới áp dụng cho mọi template
```

### Bảng tra nhanh

| Triệu chứng | Sửa gì trước |
|-------------|--------------|
| Thiếu mục "Kết luận" trong output | Template — thêm section |
| Mục có nhưng nội dung quá ngắn / gộp ý | Template — instruction chi tiết hơn |
| Ngày tháng format lộn xộn (4/5 vs 05/04/2026) | Prompts — `SYSTEM_PROMPT_FINAL_TEMPLATE` |
| Bảng action items sai số cột | Template — `item_format` hoặc instruction |
| Cuộc họp dài, mất số liệu ở đoạn đầu/cuối | Template — instruction chi tiết hơn + final prompt (trích xuất toàn diện) |
| AI thêm thông tin không có trong transcript | Prompts — nguyên tắc toàn vẹn trong final prompt |
| Muốn template mới chỉ cho 1 team | Custom JSON trong AppData (không rebuild) |

---

## 3. Reference: Template JSON

### 3.1 Schema

```json
{
  "name": "Tên hiển thị trong UI",
  "description": "Mô tả ngắn mục đích template",
  "sections": [
    {
      "title": "Tiêu đề mục (thành ## trong Markdown)",
      "instruction": "Chỉ dẫn chi tiết cho AI về mục này",
      "format": "paragraph | list | string",
      "item_format": "| Cột 1 | Cột 2 |\n| --- | --- |",
      "example_item_format": "..."
    }
  ]
}
```

**Validation** (`types.rs` → `Template::validate()`):

- `name`, `description` không được rỗng
- Phải có ít nhất 1 section
- Mỗi section: `title`, `instruction` không rỗng
- `format` chỉ nhận: `"paragraph"`, `"list"`, `"string"`

### 3.2 Ý nghĩa từng field section

| Field | Bắt buộc | Mô tả |
|-------|----------|-------|
| `title` | Có | Tiêu đề `##` trong báo cáo output |
| `instruction` | Có | **Phần quan trọng nhất** — AI đọc để biết trích xuất gì, viết thế nào |
| `format` | Có | Gợi ý cấu trúc placeholder trong khung Markdown |
| `item_format` | Không | Header bảng Markdown khi `format=list` |
| `example_item_format` | Không | Alias của `item_format` (dùng khi chưa có `item_format`) |

### 3.3 Ba loại `format` và output placeholder

Code trong `to_markdown_structure()`:

| format | Placeholder sinh ra | Khi nào dùng |
|--------|---------------------|--------------|
| `string` | `(điền nội dung)` | Mục ngắn: ngày, tên cuộc họp, 1–2 dòng |
| `paragraph` | `(điền nội dung)` | Đoạn văn dài, có thể chứa markdown phức tạp trong instruction |
| `list` | Bullet hoặc bảng từ `item_format` | Danh sách, action items, bảng |

**Lưu ý:** Với `format=paragraph`, bạn vẫn có thể yêu cầu bảng Markdown **bên trong instruction** (xem `theo_mau_act.json` mục "II. KẾT LUẬN CHUNG"). `format` chỉ ảnh hưởng placeholder khung, không giới hạn output thực tế.

### 3.4 Cách `instruction` được inject vào prompt

`to_section_instructions()` sinh chuỗi dạng:

```markdown
- **Cho tiêu đề chính (`# [Tiêu đề do AI tạo]`):** Phân tích toàn bộ nội dung...
- **Cho phần 'Tóm tắt':** Cung cấp một đoạn tóm tắt ngắn gọn...
  - Các mục trong phần này nên theo định dạng: `| Cột 1 | Cột 2 |`.
```

→ Viết `instruction` như đang nói chuyện trực tiếp với AI, bằng tiếng Việt, càng cụ thể càng tốt.

### 3.5 Ví dụ phân tích

#### `standard_meeting.json` — Mẫu đa dụng

- 4 sections: Tóm tắt → Quyết định → Action items (bảng) → Thảo luận
- Action items dùng `format=list` + `item_format` 5 cột (có cột tham chiếu transcript)
- Phù hợp: cuộc họp nội bộ, cần traceability

#### `theo_mau_act.json` vs `theo_mau_act_no_table.json`

| Khác biệt | Có bảng | Không bảng |
|-----------|---------|------------|
| Mục II | Bảng 3 cột trong instruction | Liệt kê dòng, không bảng |
| `format` | `paragraph` (bảng nằm trong instruction) | `paragraph` |
| Khi chọn | Báo cáo formal, in ấn | Giao ban nhanh, đọc trên mobile |

→ Tạo biến thể template bằng cách **clone JSON + sửa instruction**, không cần sửa Rust.

#### `daily_standup.json` — Standup kỹ thuật

- Dùng `example_item_format` thay vì `item_format` ở một số section
- `format=string` cho mục "Ngày" với instruction ngắn `"YYYY-MM-DD"`
- Minh họa: template có thể mix cả 3 format types

### 3.6 Checklist viết `instruction` hiệu quả

- [ ] Nêu rõ **đầu ra mong muốn** (đoạn văn / bullet / bảng / cấu trúc `###`)
- [ ] Liệt kê **thông tin bắt buộc** phải trích xuất (tên, số liệu, deadline)
- [ ] Quy định **khi thiếu dữ liệu**: `"Không đề cập"`, `"(không rõ)"`, `"chưa xác định"`
- [ ] Cấm hành vi không mong muốn: không suy diễn, không dùng đại từ mơ hồ
- [ ] Nếu cần bảng: paste **header Markdown đầy đủ** vào instruction hoặc `item_format`
- [ ] Ghi rõ ngôn ngữ: `"Viết bằng tiếng Việt"`
- [ ] Với mục phức tạp: đưa **ví dụ cấu trúc output** ngay trong instruction

---

## 4. Reference: Prompts

### 4.1 Prompt constant chính

Trong `frontend/src-tauri/src/summary/prompts.rs`, prompt duy nhất cần quan tâm khi tích hợp:

| Constant | Mục đích | Placeholders | Provider |
|----------|----------|--------------|----------|
| `SYSTEM_PROMPT_FINAL_TEMPLATE` | Tạo báo cáo cuối | `{section_instructions}`, `{template_markdown}` | **Tất cả** (Claude, Groq, OpenAI, OpenRouter, Custom OpenAI...) |

> **Lưu ý:** Repo desktop vẫn còn các constant `SYSTEM_PROMPT_CHUNK`, `USER_PROMPT_TEMPLATE_CHUNK`, `SYSTEM_PROMPT_COMBINE`, `USER_PROMPT_TEMPLATE_COMBINE` cho map-reduce transcript dài. **Luồng mobile bỏ qua hoàn toàn** — luôn gửi full transcript.

### 4.2 Chi tiết final template

**Luôn chạy** với mọi provider cloud. Toàn bộ transcript được đưa thẳng vào user prompt, không qua bước tóm tắt trung gian.

**`SYSTEM_PROMPT_FINAL_TEMPLATE` hiện quy định:**
- Ngôn ngữ: Tiếng Việt
- Nguyên tắc toàn vẹn (không suy diễn)
- Trích xuất toàn diện (số liệu, tên, deadline)
- Chuẩn hóa ngày `dd/mm/yyyy`
- Chống tóm tắt sơ sài
- Xử lý dữ liệu thiếu: `"(không rõ)"`

**Khi nào sửa:** Quy tắc trên cần thay đổi cho **mọi template**, không chỉ một loại báo cáo.

**Transcript dài:** Chọn model có context window đủ lớn (VD: Claude Sonnet, GPT-4o, Gemini 1.5 Pro). Không chunk — nếu thiếu chi tiết, làm `instruction` cụ thể hơn hoặc tăng cường quy tắc trích xuất trong final prompt.

### 4.3 PromptConfig — Override không cần rebuild

```rust
// prompt_config.rs
pub struct PromptConfig {
    pub system_prompt_final_template: String,
    // Các field chunk/combine vẫn tồn tại trong struct desktop — bỏ qua khi tích hợp mobile
}
```

- **Mặc định:** `PromptConfig::defaults()` đọc từ `prompts.rs`
- **Override:** Lưu JSON trong SQLite (`SettingsRepository::save_prompt_settings`)
- **UI:** `frontend/src/components/PromptSettings.tsx` — chỉ hiển thị final prompt
- **API Tauri:** `api_get_prompt_settings`, `api_save_prompt_settings`, `api_reset_prompt_settings`

**Workflow khuyến nghị:**
1. Thử chỉnh qua UI PromptSettings → test ngay
2. Khi ổn định → copy vào `prompts.rs` để làm default cho bản build mới

### 4.4 Placeholder — quy tắc không được phá

| Placeholder | File thay thế | Ghi chú |
|-------------|---------------|---------|
| `{section_instructions}` | `processor.rs` | Sinh từ `Template::to_section_instructions()` |
| `{template_markdown}` | `processor.rs` | Sinh từ `Template::to_markdown_structure()` |

### 4.5 User prompt bổ sung (không nằm trong prompts.rs)

`processor.rs` ghép thêm vào user prompt:

```xml
<transcript_chunks>
  {transcript}
</transcript_chunks>

<!-- Nếu người dùng nhập custom context trong UI -->
<user_context>
  {custom_prompt}
</user_context>
```

`{transcript}` là **toàn bộ nội dung transcript gốc** — không cắt, không tóm tắt trước.

`custom_prompt` đến từ UI khi generate summary — dùng cho context bổ sung (tên dự án, quy ước nội bộ), không thay thế template.

---

## 5. Cookbook Developer

### 5.1 Thêm built-in template vào repo (cần rebuild)

**Bước 1:** Tạo file JSON

```
frontend/src-tauri/templates/my_new_template.json
```

**Bước 2:** Đăng ký trong `defaults.rs`

```rust
pub const MY_NEW_TEMPLATE: &str = include_str!("../../../templates/my_new_template.json");

// Thêm vào get_builtin_templates() và get_builtin_template() và list_builtin_template_ids()
```

**Bước 3:** Validate JSON

```bash
cd frontend/src-tauri
cargo test summary::templates::defaults::tests::test_builtin_templates_valid_json -- --nocapture
cargo test summary::templates::types::tests -- --nocapture
```

**Bước 4:** (Tùy chọn) Đặt làm default qua migration hoặc setting `api_set_default_template`

### 5.2 Tạo template custom không rebuild

**Cách 1 — UI:** Settings → Template Settings → Tạo mẫu mới

**Cách 2 — File thủ công:**

```
Windows: %APPDATA%\MeetingOne\templates\{template_id}.json
macOS:   ~/Library/Application Support/MeetingOne/templates/{template_id}.json
Linux:   ~/.config/MeetingOne/templates/{template_id}.json
```

**Quy tắc `template_id`:** Chỉ `[a-zA-Z0-9_-]`, không có `.json` trong id.

**Validate qua Tauri:**

```typescript
await invoke('api_validate_template', { templateJson: jsonString });
await invoke('api_save_custom_template', { templateId: 'my_template', templateJson: jsonString });
```

### 5.3 Clone template có sẵn làm điểm bắt đầu

```typescript
// Lấy JSON template gốc
const json = await invoke<string>('api_get_template_json', { templateId: 'theo_mau_act' });
const data = JSON.parse(json);
// Sửa name, description, sections...
await invoke('api_save_custom_template', { templateId: 'theo_mau_act_phong_kd', templateJson: JSON.stringify(data, null, 2) });
```

### 5.4 Sửa prompts — hai đường

| Đường | Khi dùng | Cần rebuild |
|-------|----------|-------------|
| UI PromptSettings | Thử nghiệm nhanh, 1 máy | Không |
| Sửa `prompts.rs` | Default cho mọi user, release | Có |

Sau khi sửa `prompts.rs`, chạy `cargo build` trong `frontend/src-tauri`.

### 5.5 Debug output sai — quy trình 4 bước

1. **Xem template đang dùng:**
   ```typescript
   await invoke('api_get_template_json', { templateId: selectedTemplate });
   ```

2. **Mô phỏng prompt sinh ra:** Đọc `to_section_instructions()` + `to_markdown_structure()` trong `types.rs` với template của bạn.

3. **Test instruction:** Tạm thời làm instruction cụ thể hơn (thêm ví dụ output) → regenerate summary.

4. **Nếu vẫn sai toàn cục:** Kiểm tra `SYSTEM_PROMPT_FINAL_TEMPLATE` (UI hoặc `prompts.rs`).

### 5.6 Gọi summary từ code — tham số liên quan template

Frontend (`useSummaryGeneration.ts`) gọi Tauri command với:

- `selectedTemplate` — template id (VD: `"theo_mau_act_no_table"`)
- `customPrompt` — context bổ sung (optional)
- `modelConfig` — provider, model, API key

Rust `SummaryService` load `PromptConfig` từ DB (hoặc defaults) rồi truyền vào `generate_meeting_summary()`.

### 5.7 Built-in templates hiện có

| ID | Tên | Đặc điểm |
|----|-----|----------|
| `daily_standup` | Họp nhanh hàng ngày | Standup Scrum, bảng người-thực hiện |
| `standard_meeting` | Biên bản họp thông thường | 4 mục, action items 5 cột |
| `theo_mau_act` | Theo mẫu ACT - Có bảng | Kết luận giao ban ACT, bảng 3 cột |
| `theo_mau_act_no_table` | Theo mẫu ACT - Không bảng | **Default** hiện tại |
| `project_sync` | Cập nhật dự án | Tiến độ, rủi ro |
| `retrospective` | Sprint retrospective | What went well / improve |
| `sales_marketing_client_call` | Gọi khách hàng | Sales & marketing |

---

## 6. Best practices & Anti-patterns

### 6.1 Nên làm

- **Một mục = một section.** Không nhồi nhiều mục vào một `instruction` trừ khi chúng luôn đi cùng nhau (như ACT mục II có nhiều chủ đề con).
- **Instruction cụ thể > prompt global dài.** Quy tắc riêng cho một mục nên nằm trong `instruction` của mục đó.
- **Dùng custom template** để thử nghiệm; chỉ merge vào `templates/` + `defaults.rs` khi đã ổn định.
- **Giữ `title` ngắn, rõ.** AI dùng title làm heading `##` — khớp với quy chuẩn văn bản công ty.
- **Test với transcript thật** dài ít nhất 30 phút — đảm bảo model đủ context window để xử lý full transcript.

### 6.2 Không nên

| Anti-pattern | Vì sao | Thay bằng |
|--------------|--------|-----------|
| Sửa `prompts.rs` để thêm mục mới | Prompts không định nghĩa cấu trúc báo cáo | Thêm section trong template JSON |
| Duplicate quy tắc ở cả instruction và final prompt | Model bối rối, khó maintain | Global rule → prompt; mục-specific → instruction |
| `format=invalid` hoặc thiếu field | `validate()` fail, summary error | Dùng đúng schema |
| Instruction mơ hồ: "Tóm tắt nội dung" | Output chung chung | Liệt kê field cần trích xuất |
| Template id có dấu cách hoặc Unicode | `api_save_custom_template` reject | `snake_case` ASCII |
| Xóa built-in template khỏi `defaults.rs` mà không migration | User cũ có thể còn default id đó | Giữ id, deprecate bằng description |

### 6.3 Khi nào tách template mới vs sửa template cũ

| Tình huống | Hành động |
|------------|-----------|
| Cùng cấu trúc, khác vài câu instruction | Sửa template hiện tại hoặc custom override |
| Khác hoàn toàn cấu trúc (VD: 2 mục vs 6 mục) | Template mới với `id` mới |
| Biến thể có/không bảng | Hai template riêng (pattern ACT) |
| Một team dùng riêng | Custom template trong AppData |

---

## 7. Phụ lục: File map

```
frontend/src-tauri/
├── templates/                          # JSON built-in templates
│   ├── theo_mau_act.json
│   ├── standard_meeting.json
│   └── ...
├── src/summary/
│   ├── prompts.rs                      # Prompt constants (SỬA DEFAULT Ở ĐÂY)
│   ├── prompt_config.rs                # PromptConfig struct + defaults()
│   ├── processor.rs                    # Pipeline tạo báo cáo (full transcript → LLM)
│   ├── service.rs                      # SummaryService, load PromptConfig từ DB
│   ├── template_commands.rs            # Tauri API: list/save/validate template
│   └── templates/
│       ├── types.rs                    # Template, TemplateSection, validate, to_markdown*
│       ├── loader.rs                   # get_template(), list_template_ids()
│       └── defaults.rs                 # include_str! built-in templates

frontend/src/components/
├── TemplateSettings/                   # UI quản lý template
│   ├── TemplateEditor.tsx
│   ├── SectionEditor.tsx
│   └── useTemplateSettings.ts
├── PromptSettings.tsx                  # UI override prompts
└── MeetingDetails/
    └── SummaryGeneratorButtonGroup.tsx # Chọn template khi generate

%APPDATA%/MeetingOne/templates/         # Custom templates (runtime)
```

---

## Tóm tắt một dòng

> **Template = cấu trúc & nội dung từng mục. Prompts = quy tắc AI toàn cục. Full transcript → LLM → báo cáo. Sửa template trước khi đụng prompts.**

---

*Tài liệu này mô tả luồng mobile (full transcript, không chunk) tại thời điểm 2026-07-04. Khi thêm provider mới, cập nhật Phần 1 và 4 tương ứng.*
