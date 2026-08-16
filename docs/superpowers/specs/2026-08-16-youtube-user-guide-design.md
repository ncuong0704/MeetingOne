# Hướng dẫn sidebar: video YouTube, bỏ Joyride

**Ngày:** 2026-08-16
**Trạng thái:** Chốt để implement

## Vấn đề

App dùng **react-joyride** cho tour overlay (làm quen giao diện, mẫu báo cáo, cài đặt). Nút **Hướng dẫn** trên sidebar mở dialog, chọn một tour rồi Joyride highlight từng `data-tour`.

Người dùng muốn:
- Bỏ hết tour Joyride.
- Dialog Hướng dẫn liệt kê **video YouTube** (title, description, url) từ **một file trong source** họ tự điền.
- Bấm xem → mở video.

## Hiện trạng

- `UserGuideButton` → `USER_GUIDE_TOURS` → `startTour` → `UserGuideJoyride`.
- `data-tour` rải sidebar, settings, template editor.
- `UserGuideContext` + navigation events (`TEMPLATE_TOUR_EVENT`, `SETTINGS_TOUR_TAB_EVENT`) điều khiển Joyride.
- Mở URL ngoài: `open_external_url` (đã dùng cho release notes). Windows `cmd start` gãy với `&` trong `watch?v=` — mở dạng `https://youtu.be/{id}`.

Onboarding lần đầu (`OnboardingFlow`) **không** phải Joyride — giữ nguyên.

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Catalog | `frontend/src/components/UserGuide/guideVideos.ts` — `GUIDE_VIDEOS: GuideVideo[]` |
| Item | `{ id, title, description, youtubeUrl }` |
| Click | Mở trình duyệt mặc định qua `open_external_url`, fallback `window.open` |
| URL hợp lệ | `https` + host `youtube.com` / `youtu.be` / `m.youtube.com` |
| Dialog trống | Catalog rỗng → «Chưa có video hướng dẫn» |
| Joyride | Xóa provider, bước tour, `data-tour`, dependency `react-joyride` |

## Ngoài phạm vi

- Embed iframe YouTube trong app.
- CMS / admin UI để sửa catalog lúc runtime.
- Đổi OnboardingFlow.

## Tiêu chí xong

- Unit test: parse/cho phép URL YouTube; từ chối `javascript:` / host lạ; chuẩn hoá `youtu.be`.
- Dialog Hướng dẫn còn; không còn overlay Joyride.
- Cài đặt / mẫu báo cáo / ghi âm không phụ thuộc tour event.
