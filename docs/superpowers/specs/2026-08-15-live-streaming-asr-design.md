# Live streaming ASR — giống test ASR

**Ngày:** 2026-08-15  
**Trạng thái:** Chốt để implement  
**Thay / bổ sung:** Spec hiệu năng 2026-08-04 để **streaming model (OnlineRecognizer)** ngoài phạm vi — spec này làm phần đó.

## Vấn đề

MeetingOne live dùng `OfflineRecognizer` + VAD endpoint: người nói liên tục thì transcript chỉ bung khi im (hoặc đủ `max_segment_seconds`). test ASR live (model streaming) feed audio liên tục và hiện **partial** ngay.

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Model live mặc định | `zipformer-vi-30m-streaming` — `hynt/Zipformer-30M-RNNT-Streaming-6000h` (encoder/decoder/joiner **chunk-64 left-128 fp16**) |
| Runtime | `sherpa-onnx` `OnlineRecognizer` (crate đã có 1.13.0) |
| VAD trên đường ASR live streaming | **Không** — giống `streaming_asr_online.py` |
| Endpoint | Rule 1/2/3 giống test ASR: 3.0s / 2.0s / 20.0s trailing; **max utterance 12s** force-finalize |
| Partial | `transcript-update` cùng `sequence_id`, `is_partial=true` rồi `false` khi chốt |
| File import / ROVER / CAPU live-hot-path | Không đổi (CAPU vẫn lúc `stop_recording`) |
| Mic + system mix + WAV | Giữ nguyên MeetingOne |
| Model offline live (ZipFormer/Gipformer/Sherpa 2025) | Vẫn chọn được; đường VAD+Offline cũ |

## Pipeline live (streaming)

```
Mic + System → mix 48 kHz
  ├── recording_saver (WAV)
  └── downsample 16 kHz → OnlineStream.accept_waveform
                              decode khi is_ready
                              emit partial khi text đổi
                              endpoint | max 12s → emit final, reset stream
```

## UI

`TranscriptContext`: upsert theo `sequence_id` (đừng skip duplicate). Partial hiện dòng đang nói; final thay cùng dòng.

`LiveAsrPanel`: thêm family streaming (ưu tiên, live-only). `FileAsrPanel` không hiện family này.

## Tokens

HF repo không có `tokens.txt`. Bundle `resources/zipformer-streaming-tokens.txt` (cùng vocab với bản test ASR đã chạy) — copy vào thư mục model khi tải.

## Ngoài phạm vi

- Hotkey người nói / diarization live  
- OnlineRecognizer cho file import  
- Đổi CAPU sang realtime từng partial  

## Tiêu chí xong

1. Nói liên tục: UI cập nhật partial trong vài trăm ms–vài giây, không chờ 20–30s.  
2. Im / endpoint / 12s: chốt đoạn, đoạn mới dùng `sequence_id` mới.  
3. File import + unit test family/session không regress.  
4. Model tải được từ Settings live.  
