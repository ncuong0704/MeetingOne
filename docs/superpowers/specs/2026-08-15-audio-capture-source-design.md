# Chọn nguồn ghi: Micro / Hệ thống / Cả hai

**Ngày:** 2026-08-15  
**Trạng thái:** Chốt để implement

## Vấn đề

MeetingOne mặc định mở **cả microphone và âm thanh hệ thống**. Người dùng chỉ có nút micro nhỏ cạnh Ghi — tắt thì **chỉ hệ thống**. **Không có** cách rõ ràng để chỉ thu microphone. Tab Chung chỉ chọn *thiết bị nào*, không chọn *thu nguồn nào*.

Hai đường start còn mâu thuẫn: `(None, None)` → thu cả hai; `(Some, None)` → chỉ mic. `mic_enabled: false` chỉ áp dụng nhánh defaults.

## Kỹ thuật hiện có (giữ)

- `RecordingManager.start_recording(mic: Option, system: Option)` và `start_streams` **đã** bỏ qua nguồn `None`.
- Pipeline trộn được khi một bên là `"No Microphone"` / `"No System Audio"`.
- Preference thiết bị: `preferred_mic_device` / `preferred_system_device` (store Tauri).

Không cần mixer mới. Cần **một flag nguồn tường minh** và **một** nhánh resolve thiết bị.

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Nguồn | Enum `audio_source`: `microphone` \| `system` \| `both`. Mặc định `both`. Persist cùng `recording_preferences.json` (`#[serde(default)]` = both cho bản cũ) |
| UI | **Một** select (cùng state): compact trên thanh Ghi (thay nút micro), bản đủ chữ trên tab Chung cạnh thiết bị |
| Bỏ | Nút toggle mic trước khi ghi **và** lúc đang ghi (mute giữa phiên). Đổi nguồn = phiên kế tiếp |
| Start | Frontend gửi `audioSource` + tên thiết bị (null = mặc định OS **nếu nguồn cần kênh đó**). Backend không còn hiểu `None` là “bỏ kênh” trừ khi `audio_source` không gồm kênh đó |
| `mic_enabled` | Không dùng trên UI. Command cũ: `mic_enabled: false` không có `audioSource` → map `system`; còn lại → `both` |
| Quyền mic | `microphone` + không quyền → không start, toast. `both` + không quyền → thu hệ thống, log/warn. Không auto-ghi đè `audio_source` lúc đang check quyền |
| Dropdown Chung | Nguồn `microphone` → disable “Âm thanh hệ thống”. Nguồn `system` → disable “Micro”. `both` → cả hai |
| Đánh giá mic | Giữ; hiện khi nguồn gồm micro |
| Giữa phiên | Select disabled, hiện mode đang dùng |

## Pipeline

```
Select nguồn (home hoặc Chung) → lưu preference + Config
Bấm Ghi
  → wants_mic / wants_sys từ enum
  → microphone-only + không quyền → dừng
  → resolve mic/sys: tên Config hoặc preference hoặc default OS
  → start_streams(Option, Option) như hiện tại
```

## Ngoài phạm vi

- Đổi nguồn đang ghi (cần restart stream)
- Mute từng kênh giữa phiên
- Thu nhiều micro
- Đổi mixer / VAD live

## Tiêu chí xong

- Unit test: flags enum, serde default `both`, map `mic_enabled` cũ
- Ba option thu đúng nguồn; nút micro nhỏ đã mất
- Live ASR / Đánh giá / lưu họp không regress
