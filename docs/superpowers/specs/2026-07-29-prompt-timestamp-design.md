# Truyền timestamp vào system prompt tạo báo cáo

## Vấn đề

`SYSTEM_PROMPT_FINAL_TEMPLATE` (`frontend/src-tauri/src/summary/prompts.rs`) yêu cầu model
chuẩn hóa mọi mốc ngày tháng trong báo cáo, và ở rule #3 nói model nên "tự động sử dụng năm
hiện tại của cuộc họp" khi transcript không có năm. Nhưng không có timestamp nào thực sự được
truyền vào prompt — LLM không có cách nào biết "năm hiện tại" là năm nào, hay báo cáo đang được
tạo vào thời điểm nào.

## Giải pháp

Truyền hai giá trị thời gian riêng biệt vào system prompt qua hai placeholder mới:

- `{current_datetime}` — thời điểm tạo báo cáo (lúc LLM chạy)
- `{meeting_datetime}` — thời điểm cuộc họp diễn ra (`meetings.created_at`)

Cả hai đều format `HH:MM ngày dd/mm/yyyy`, theo giờ local của máy người dùng.

## Thay đổi theo file

### `frontend/src-tauri/src/summary/service.rs`

Trong `process_transcript_background`, trước khi gọi `generate_meeting_summary`:

```rust
let meeting_created_at = match MeetingsRepository::get_meeting_metadata(&pool, &meeting_id).await {
    Ok(Some(meeting)) => meeting.created_at.0,
    Ok(None) => {
        warn!("Meeting {} not found when fetching created_at for prompt timestamp; using now()", meeting_id);
        Utc::now()
    }
    Err(e) => {
        warn!("Failed to fetch meeting created_at for prompt timestamp: {}. Using now()", e);
        Utc::now()
    }
};
```

Truyền `meeting_created_at` vào `generate_meeting_summary(...)`.

### `frontend/src-tauri/src/summary/processor.rs`

Thêm tham số `meeting_created_at: DateTime<Utc>` vào `generate_meeting_summary`. Tính:

```rust
let current_datetime = Local::now().format("%H:%M ngày %d/%m/%Y").to_string();
let meeting_datetime = meeting_created_at
    .with_timezone(&Local)
    .format("%H:%M ngày %d/%m/%Y")
    .to_string();
```

Replace vào `final_system_prompt` cùng cách với `{section_instructions}` / `{template_markdown}`
hiện có (`.replace("{current_datetime}", ...)`, `.replace("{meeting_datetime}", ...)`).

### `frontend/src-tauri/src/summary/prompts.rs`

Thêm block mới ngay đầu `SYSTEM_PROMPT_FINAL_TEMPLATE`, trước "**CÁC NGUYÊN TẮC CỐT LÕI:**":

```
**THÔNG TIN THỜI GIAN:**
- Thời điểm cuộc họp diễn ra: {meeting_datetime}
- Thời điểm tạo báo cáo này: {current_datetime}
```

Sửa rule #3 (chuẩn hóa ngày tháng) để trỏ rõ vào `{meeting_datetime}` thay vì câu mơ hồ
"năm hiện tại của cuộc họp":

> Nếu không có năm trong transcript, sử dụng năm của thời điểm cuộc họp diễn ra
> ({meeting_datetime} ở trên).

### `frontend/src/components/PromptSettings.tsx`

Thêm `'{current_datetime}'` và `'{meeting_datetime}'` vào mảng `placeholders` của
`systemPromptFinalTemplate` trong `PROMPT_FIELDS`, để UI hiển thị cho người dùng biết các
placeholder có sẵn. Đây chỉ là hiển thị thông tin — không có validation bắt buộc ở backend,
nên prompt tùy chỉnh hiện có của người dùng (chưa có 2 placeholder này) vẫn hoạt động bình
thường (không bị lỗi, chỉ đơn giản là thiếu thông tin thời gian).

## Testing

Unit test trong `processor.rs` xác nhận `{current_datetime}` và `{meeting_datetime}` được
thay thế đúng định dạng trong system prompt cuối cùng.

## Ngoài phạm vi

- Không đổi giá trị mặc định `system_prompt_final_template` đã lưu trong DB của người dùng
  hiện tại (chỉ đổi hằng số mặc định trong code — người dùng cần bấm "Khôi phục mặc định" để
  nhận bản mới).
- Không đổi các luồng LLM khác ngoài `generate_meeting_summary` (đây là prompt system duy nhất
  hiện có trong pipeline tóm tắt).
