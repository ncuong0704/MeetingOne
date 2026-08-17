# Dialog danh sách người nói sau diarization

**Ngày:** 2026-08-17  
**Trạng thái:** Chốt để implement

## Vấn đề

Khi nhập file có bật **Phân biệt người nói**, Meetily đã có cluster (`meeting_speakers`) và nhãn trên transcript. User muốn **một nút «Người nói»** sau khi xử lý xong: mở dialog liệt kê các người đã phát hiện, **đổi tên**, **nghe ~15 giây** đoạn đầu tiên của từng người, **gộp** hai cluster (model hay tách 1 người thành 2).

## Hiện trạng

| Thành phần | Có sẵn | Thiếu |
|---|---|---|
| Import file + Senko CAM++ → `meeting_speakers` + `transcripts.speaker_id` | Có | — |
| Đổi tên (`rename_meeting_speaker`) | Có (popover từng khối) | Không có dialog tổng |
| «Gộp với trước» (`merge_speaker_segment`) | Có — **chỉ 1 đoạn** lấy speaker của đoạn liền trước | Không gộp **cả cluster** A → B |
| Nút header «Người nói» | Không | Cần |
| Play preview ~15s tại lần xuất hiện đầu | Không | Cần |

Không đụng: hotkey live 1–9, `speaker_directory.json`, «Gộp với trước» trên khối transcript.

## Tham chiếu test ASR

`SpeakerRenameDialog` (test ASR): đổi tên + màu. Meetily **không** thêm color picker (user không yêu cầu). Gộp cluster là nhu cầu Meetily (diarization tách nhầm), không copy UI gộp block của tab File test ASR.

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Hiện nút | Header «Bản ghi» (cạnh tai nghe), **khi** cuộc họp có ≥1 hàng `meeting_speakers`. File không bật diarization → không nút. Live có stamp tên cũng hiện (cùng bảng). |
| Dialog | 1 hàng / cluster: chấm màu, input tên, Play, Gộp |
| Đổi tên | `SpeakerNameCombobox` + danh bạ tab Cài đặt → Danh sách. Lưu Enter / chọn người. Dialog **không đóng**. |
| Play | Bật trình phát họp hiện có; tua tới `MIN(audio_start_time)` của speaker; phát **15 giây** (hoặc đến hết file nếu ngắn hơn); tự pause. Không có timestamp / không có WAV → disable Play. |
| Gộp | Chọn **người đích** (select các speaker còn lại). `UPDATE transcripts SET speaker_id = đích WHERE speaker_id = nguồn`; `DELETE meeting_speakers` nguồn. Giữ tên/màu đích. Dialog **không đóng**. Không dùng `merge_with_previous`. |
| Nguồn dữ liệu dialog | Command `list_meeting_speakers(meeting_id)` — không derive từ trang pagination (thiếu speaker muộn). |
| Color picker / sửa từng đoạn | Ngoài phạm vi |

## Luồng

```
Import file (diarization bật)
        ↓
transcript + meeting_speakers
        ↓
Nút «Người nói» trên chi tiết họp
        ↓
Dialog: đổi tên | play 15s | gộp cluster A → B
        ↓
refetch transcript (nhãn khối cập nhật)
```

## Play 15s

`preview_stop = min(preview_start + 15, duration)`. Dừng theo đồng hồ audio (`timeupdate`), không chỉ `setTimeout`. Lần bấm Play mới ghi đè cửa sổ preview. User pause tay thì hủy auto-stop.

## Tiêu chí xong

1. Meeting không có speaker → không nút.
2. Meeting có speaker → nút mở dialog đúng số cluster + `preview_start`.
3. Đổi tên persist, transcript JOIN hiện tên mới.
4. Play tua đúng `preview_start`, dừng khoảng 15s.
5. Gộp A vào B: mọi đoạn A thành B; hàng A biến mất; «Gộp với trước» vẫn hoạt động.
6. Helper TS + repo Rust có unit test; `transcriptDisplay` / `merge_with_previous` không regress.

## Ngoài phạm vi

- Đổi màu speaker
- Diarization live
- Player riêng trong dialog (dùng trình phát họp)
- Gộp nhiều-một hàng loạt
