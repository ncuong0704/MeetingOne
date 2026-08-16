# Sao chép bản ghi khớp giao diện

**Ngày:** 2026-08-16
**Trạng thái:** Chốt để implement

## Vấn đề

Nút **Sao chép** trên panel **Bản ghi** (chi tiết cuộc họp) ghi clipboard dạng:

```
# Bản ghi cuộc họp: …
Ngày: …

[00:12] nội dung câu
```

Giao diện `FlowingTranscriptView` **không** hiện timestamp, không hiện tiêu đề/ngày, và (khi có diarization) gom đoạn cùng người nói thành khối với nhãn tên phía trên.

Khi dán ra file/Notepad/Word, timestamp vẫn còn; tên người nói thì mất. Người dùng muốn: **dán ra giống những gì đang thấy**.

Nút Sao chép lúc đang ghi (`TranscriptContext.copyTranscript`) cũng nhét `[MM:SS]`.

## Giao diện hiện tại (nguồn sự thật)

`MeetingDetails/TranscriptPanel` → `FlowingTranscriptView`:

- Không timestamp.
- Không tiêu đề cuộc họp / ngày.
- Có `speakerId` ở bất kỳ đoạn nào → nhóm đoạn liên tiếp cùng `speakerId`; mỗi khối: nhãn (`speakerName` hoặc «Người nói») rồi đoạn văn chảy.
- Không có speaker → một đoạn văn chảy.
- Trong khối: nối bằng khoảng trắng; xuống dòng chỉ sau câu kết thúc `.?!`.
- `cleanStopWords` (`uh`/`um`/…). Đoạn rỗng → `[Im lặng]`.

## Copy hiện tại

| Chỗ | File | Clipboard |
|---|---|---|
| Chi tiết họp | `useCopyOperations.handleCopyTranscript` | Tiêu đề + ngày + `[MM:SS]` + text; HTML Word cùng cấu trúc; **không** speaker |
| Đang ghi | `TranscriptContext.copyTranscript` | `[MM:SS]` + speaker prefix `Tên: ` + text; chỉ `text/plain` |

API `api_get_meeting_transcripts` **đã** trả `speaker_id` / `speaker_name`. Copy chỉ không dùng.

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Nguồn format | Cùng rule với `FlowingTranscriptView` |
| Timestamp | Không đưa vào clipboard (plain lẫn HTML) |
| Tiêu đề / ngày | Không |
| Người nói | Có speaker → nhãn + khối văn như UI; không speaker → chỉ văn chảy |
| Toàn bộ đoạn | Vẫn fetch hết DB (không chỉ trang đang xem) — áp format trên full list |
| HTML Word | Cùng cấu trúc (nhãn `<strong>`, `<br>` sau hết câu); không span timestamp |
| Live copy | Cùng hàm format (bỏ `[MM:SS]`); nếu có `speaker_name` thì có nhãn |
| Prompt tóm tắt | Không đổi (`useSummaryGeneration` vẫn có timestamp cho LLM) |

## Ngoài phạm vi

- Ẩn timestamp trên `VirtualizedTranscriptView` lúc live (UI live vẫn hiện giờ ghi).
- Đổi copy tóm tắt AI.
- Export file transcript / `transcripts.json`.

## Tiêu chí xong

- Unit test: không `[MM:SS]`; có/không speaker; gộp cùng người nói; xuống dòng sau hết câu; `[Im lặng]`.
- Copy tóm tắt / attach tài liệu không đổi.
- App restart để kiểm thử dán Notepad và Word.
