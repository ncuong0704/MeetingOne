# Live speaker hotkeys — giống test ASR

**Ngày:** 2026-08-15  
**Trạng thái:** Chốt để implement

## Vấn đề

Trong **test ASR**, lúc ghi live người dùng cấu hình phím 1–9 → tên người nói, rồi bấm phím số khi cửa sổ app đang focus để gán người đang nói. Transcript tách theo người.

**MeetingOne** chưa có hotkey, chưa stamp speaker lên `transcript-update`, live UI không hiện nhãn người nói.

## Tham chiếu test ASR

| Thành phần | Cách làm |
|---|---|
| Config | `speaker_hotkeys.json` — `"1"`…`"9"` → tên; rỗng = tắt slot |
| UI config | Dialog 9 dòng: STT / Num i / ô tên |
| Phím | Không OS-global. Chỉ khi cửa sổ focus, đang ghi, không đang gõ `QLineEdit` |
| Engine | `pending_speaker` → `queued_speaker` trên chunk kế; force endpoint; emit final cũ; token `__SPK_SEP__{name}__`; `recognizer.reset` |
| Preview | Dashed line tên (ẩn partial xám) đến khi token commit |

## Khác biệt MeetingOne (trước thay đổi)

| | test ASR | MeetingOne |
|---|---|---|
| Config 1–9 | Có | Không |
| Hotkey lúc live | Có | Không |
| Marker | Token trong stream chữ | Không |
| Gán speaker live | Có | Không (`TranscriptUpdate` không có speaker) |
| Diarization file | Community-1 | File-only (không trộn vào live) |

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Config | JSON `{ "1": "Tên", … "9": "" }` trong **app data** `speaker_hotkeys.json` (không commit repo) |
| Hotkey | `keydown` cửa sổ khi `isRecording`; `event.key` `"1"`–`"9"`; bỏ nếu target INPUT/TEXTAREA/SELECT/contenteditable; bỏ nếu Ctrl/Alt/Meta |
| OS-global shortcut | **Không** (tránh cướp số ở app khác) |
| Marker ASR | **Không** nhét `__SPK_SEP__` vào text (tránh phá CAPU / ITN) |
| State | `LiveSpeakerTracker`: `queue` → force-finalize utterance **cũ** → `apply_pending` → speech sau stamp `speaker_name` |
| Event | `transcript-update` thêm `speaker_name` / `speaker_color` (Option, serde default). Preview: `live-speaker-pending` / `live-speaker-changed` |
| CAPU lúc stop | Flush batch khi `speaker_name` đổi — không gộp hai người vào một đoạn |
| Persist live | `recording_saver::TranscriptSegment.speaker_name` + JSON recovery. Không phụ thuộc module diarization file WIP |
| Cùng tên bấm lại | Vẫn queue (như test ASR — vẫn force-cut) |
| Tên rỗng | Bỏ qua |
| Reset | Mỗi `start_recording` |

## Pipeline

```
Phím 1–9 (cửa sổ focus, đang ghi)
  → insert_live_speaker(name)
  → tracker.queue + emit live-speaker-pending (dashed preview)

Streaming (OnlineRecognizer):
  chunk tới + pending → force endpoint
  emit final utterance cũ (speaker CŨ)
  apply_pending + recognizer.reset
  speech sau stamp speaker MỚI

Offline+VAD:
  chunk hiện tại stamp speaker CŨ; apply_pending trước chunk kế
```

## UI

- Settings → Transcription → Live: nút **Cấu hình hotkey người nói** (dialog 9 ô).
- Home transcript lúc ghi: cùng dialog + hint 1–9; preview dashed tên; nhãn màu khi speaker đổi.
- Live `TranscriptPanel` map `speakerName` / `speakerColor` vào `VirtualizedTranscriptView`.

## Ngoài phạm vi

- OS-global shortcut
- Diarization live Community-1
- Rename/merge speaker như file import
- Token `__SPK_SEP__` trong text ASR
- Ô nhập tên thủ công (test ASR có; không làm v1 trừ khi user hỏi)

## Tiêu chí xong

1. Cấu hình 1–9, lưu app data, tải lại còn.
2. Đang ghi, bấm 1–9: preview tên; đoạn đang nói chốt với người cũ; đoạn sau mang tên mới.
3. Slot trống / đang gõ ô input: không gán.
4. CAPU stop không gộp hai người thành một câu.
5. Unit test tracker + CAPU flush theo speaker.
