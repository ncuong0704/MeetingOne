# Live microphone capture giống test ASR

**Ngày:** 2026-08-17  
**Trạng thái:** Chốt để implement

## Vấn đề

Khi ghi **micro trực tiếp**, transcript Meetily chậm và không đều so với:

- app **test ASR** (cùng ZipFormer streaming chunk-64)
- nhánh **âm thanh hệ thống** trong cùng Meetily

Nguyên nhân không phải model. Test ASR đưa PCM raw 16 kHz vào `OnlineRecognizer`. Meetily chỉ xử lý **micro** bằng HPF 80 Hz + EBU R128 (−23 LUFS) + limiter lookahead 10 ms **trong callback cpal**, rồi mới mix/ASR. Âm thanh hệ thống đi raw.

## Tham chiếu test ASR

| Thành phần | Cách làm |
|---|---|
| Capture | Qt `QAudioSource`, **16 kHz mono Int16** (`MicrophoneRecordThread` trong `common.py`) |
| Chunk | Gom **50 ms** (800 mẫu @ 16 kHz), không HPF, không EBU, không RNNoise |
| ASR live | `OnlineStreamingASRThread`: `accept_waveform(16000, audio)` — **NO VAD** |
| Endpoint | Rule 3.0s / 2.0s / 20.0s; max utterance 12s |
| File import | Preprocess riêng (`audio_preprocessing.py`) — **không** chạy trên live mic |

## Khác biệt Meetily hiện tại

| | test ASR live mic | Meetily live mic | Meetily live system |
|---|---|---|---|
| Sample rate capture | 16 kHz native | 48 kHz (resample nếu lệch) | 48 kHz raw |
| HPF 80 Hz | Không | Có, trên audio thread | Không |
| EBU R128 + limiter 10 ms | Không | Có, trên audio thread | Không |
| Mix cửa sổ 50 ms | Không (chunk 50 ms thuần) | Có (pad silence nếu thiếu nhánh kia) | Có |
| Downsample → ASR | Không (đã 16 kHz) | Trung bình 3 mẫu 48k→16k | Cùng downsample |
| RNNoise | Không | Flag `false` | Không |

Hai hop **48 kHz mix** và **downsample 48k→16k** giữ nguyên: cần để mix với system và WAV. Chỉ bỏ enhancement **micro live**.

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Live mic trước mix/ASR | **PCM raw** sau mono + resample 48 kHz — giống test ASR |
| HPF + EBU trên `AudioCapture` (device Microphone) | **Tắt / gỡ khỏi hot path** |
| HPF + EBU file import | **Giữ** `preprocess_file_audio` — không đụng |
| System capture | Không đổi (đã raw) |
| Mixer 50 ms, streaming vs VAD, model endpoint | Không đổi |
| Pipeline 48 kHz | Không đổi (không ép capture 16 kHz — sẽ gãy mix với system) |
| WAV họp live | Raw mix (micro không còn −23 LUFS). Chấp nhận, giống test ASR |
| Mic quality / Đánh giá | Không đổi (đã capture 16 kHz riêng) |

## Pipeline sau thay đổi

```
Mic (raw, 48 kHz, không HPF/EBU)
System (raw, 48 kHz)
        ↓
   mix cửa sổ 50 ms
        ├── recording_saver (WAV)
        └── streaming: downsample 16 kHz → OnlineRecognizer
            hoặc offline: VAD → OfflineRecognizer
```

## Tiêu chí xong

1. Impulse / sample micro live ra pipeline **không** bị trễ lookahead limiter (identity trên PCM sau mono/resample).
2. `LoudnessNormalizer` / `HighPassFilter` **vẫn** dùng và pass test ở file import.
3. Pipeline constructor, mixer, streaming downsample không regress.
4. Ghi micro live: callback nhẹ hơn; partial ASR gần test ASR hơn (cùng model).

## Ngoài phạm vi

- Đổi sample rate toàn pipeline sang 16 kHz
- Bỏ mixer khi chỉ ghi micro
- Bật lại RNNoise
- Đổi endpoint / chunk-64 model
- Volume cân mic+system khi ghi «Cả hai» (mic có thể nhỏ hơn system sau khi bỏ EBU)
