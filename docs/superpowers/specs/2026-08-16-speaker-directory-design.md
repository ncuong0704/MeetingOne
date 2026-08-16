# Danh bạ người nói: tab Cài đặt + gợi ý khi gán tên

**Ngày:** 2026-08-16
**Trạng thái:** Chốt để implement

## Vấn đề

Khi gán người nói (dialog **Người nói 1–9**), user phải tự gõ họ tên vào từng ô. Tên hay lặp lại giữa các cuộc họp; chức vụ và phòng ban không được lưu để phân biệt người trùng tên.

## Hiện trạng

- `SpeakerHotkeyDialog`: 9 ô text thuần, lưu `speaker_hotkeys.json`.
- Phím 1–9 lúc ghi gọi `insert_live_speaker(name)` — chỉ một chuỗi họ tên.
- Không có danh bạ dùng chung.

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Tab Cài đặt | **Danh sách**, sau **Chung** |
| Bản ghi | `{ id, fullName, title, department }` — họ tên bắt buộc; chức vụ và phòng ban được để trống |
| Lưu | `speaker_directory.json` trong app data (cùng chỗ `speaker_hotkeys.json`) |
| Gợi ý | Ô «Tên người nói» trong dialog Người nói 1–9: gõ hoặc focus → danh sách lọc bên dưới |
| Ghép chuỗi | Không dấu, không hoa thường; khớp họ tên / chức vụ / phòng ban |
| Chọn gợi ý | Điền **họ tên** vào ô (hệ thống gán speaker vẫn chỉ dùng tên) |
| Ngoài danh bạ | Vẫn gõ tên tự do và lưu hotkey |
| Trùng họ tên | Hiện cả hai, phụ đề chức vụ · phòng ban |

## Ngoài phạm vi

- Gắn chức vụ/phòng ban vào từng đoạn transcript.
- Đồng bộ AMS / LDAP.
- Gợi ý trên từng dòng bản ghi (chỉ dialog 1–9).

## Tiêu chí xong

- Unit test: gấp dấu tiếng Việt, lọc gợi ý, từ chối họ tên trống.
- Tab Danh sách thêm/sửa/xóa và persist.
- Dialog 1–9 gợi ý; hotkey và ghi âm không đổi hành vi nếu không chọn gợi ý.
