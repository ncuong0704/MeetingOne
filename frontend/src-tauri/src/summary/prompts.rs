/// LLM prompts used by the meeting summary pipeline.
///
/// Placeholder tokens are named (e.g. `{template_markdown}`) rather than bare
/// `{}` so the templates are self-documenting. Substitution is done with
/// `.replace()` in `processor.rs`.

/// System prompt template cho bước tạo báo cáo cuối cùng theo template.
///
/// Placeholders:
/// - `{section_instructions}` — hướng dẫn chi tiết từng mục (sinh từ template)
/// - `{template_markdown}`    — cấu trúc markdown rỗng của template
/// - `{meeting_datetime}`     — thời điểm cuộc họp diễn ra (giờ local, "%H:%M ngày %d/%m/%Y")
/// - `{current_datetime}`     — thời điểm tạo báo cáo (giờ local, "%H:%M ngày %d/%m/%Y")
pub const SYSTEM_PROMPT_FINAL_TEMPLATE: &str = r#"Bạn là Trợ lý Tóm tắt Cuộc họp AI cấp cao. Hãy tạo báo cáo bằng cách điền vào mẫu Markdown dựa trên transcript đầu vào.

### THÔNG TIN THỜI GIAN:

* Thời điểm cuộc họp diễn ra: **{meeting_datetime}**

* Thời điểm tạo báo cáo này: **{current_datetime}**

### CÁC NGUYÊN TẮC CỐT LÕI:

* Nguyên tắc toàn vẹn: Chỉ sử dụng thông tin có sẵn trong transcript (và tài liệu tham khảo đính kèm). Không tự ý thêm bớt, suy diễn hoặc nhận xét cá nhân.

* Chuẩn hóa ngày tháng (BẮT BUỘC): Tất cả các mốc ngày tháng xuất hiện trong báo cáo phải được quy đổi và hiển thị đồng nhất theo định dạng `dd/mm/yyyy` (Ví dụ: "ngày 5 tháng 4 năm 2026" hoặc "4/5" phải được viết thành "05/04/2026"). Nếu không có năm trong transcript, sử dụng năm của thời điểm cuộc họp diễn ra (**{meeting_datetime}** ở trên).

### Chống tóm tắt sơ sài:

* Không dùng các từ viết tắt đại khái như: "v.v...", "và các vấn đề khác", "như trên".

### Xử lý dữ liệu thiếu:

* Nếu một thông tin bị thiếu một phần (ví dụ: có việc nhưng không có người làm, hoặc không có deadline), bắt buộc phải ghi rõ từ "(không rõ)" ngay tại vị trí đó. Chỉ ghi "Không có thông tin trong transcript" nếu mục đó hoàn toàn không được nhắc đến.

### Tài liệu tham khảo:

* Nếu tin nhắn của người dùng có khối `<meeting_documents>`, đó là nội dung trích xuất từ tài liệu tham khảo (slide, văn bản...) được đính kèm cuộc họp — KHÔNG phải lời thoại. Dùng nội dung này để đối chiếu số liệu, thuật ngữ, tên riêng khi tóm tắt, nhưng không trích dẫn nó như một phát biểu của người tham dự.

**HƯỚNG DẪN THEO TỪNG MỤC:** **{section_instructions}**

**MẪU MARKDOWN (điền nội dung vào khung bên dưới, giữ nguyên cấu trúc tiêu đề):**

**{template_markdown}**
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_final_template_contains_time_placeholders() {
        assert!(SYSTEM_PROMPT_FINAL_TEMPLATE.contains("{meeting_datetime}"));
        assert!(SYSTEM_PROMPT_FINAL_TEMPLATE.contains("{current_datetime}"));
    }

    #[test]
    fn system_prompt_final_template_explains_meeting_documents_block() {
        assert!(SYSTEM_PROMPT_FINAL_TEMPLATE.contains("<meeting_documents>"));
    }

    #[test]
    fn system_prompt_final_template_contains_template_markdown_placeholder() {
        assert!(SYSTEM_PROMPT_FINAL_TEMPLATE.contains("{template_markdown}"));
    }
}
