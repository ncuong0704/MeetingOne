# Đánh giá chất lượng microphone trước live — giống test ASR

**Ngày:** 2026-08-15  
**Trạng thái:** Chốt để implement

## Vấn đề

Trong **test ASR**, người dùng bấm **Đánh giá** cạnh combo microphone trên tab Live, ghi 10 giây, rồi xem điểm DNSMOS (SIG/BAK/OVRL), độ tự tin ASR-Proxy, chữ nhận dạng, và gợi ý. Họ biết mic có đủ tốt **trước khi** bắt đầu ghi.

**MeetingOne** chỉ có chọn thiết bị + meter mức âm (RMS/peak, phần lớn giả lập). Không có DNSMOS, không VAD trên clip test, không ASR-Proxy, không dialog kết quả.

## Tham chiếu test ASR

| Thành phần | Cách làm |
|---|---|
| Model | Microsoft DNSMOS ONNX `sig_bak_ovr.onnx` (~5MB). SHA-256 `269fbebdb513aa23cddfbb593542ecc540284a91849ac50516870e1ac78f6edd`. Input 9.01s @ 16kHz = **144160** mẫu. **Không** peak-normalize |
| Map MOS | Polynomial Microsoft: `p_sig`, `p_bak`, `p_ovr` rồi clip 1–5 |
| Cửa sổ | Sliding 9.01s, overlap 50% nếu clip dài hơn |
| VAD | Silero, padding ~0.6s, min silence 300ms. Không speech → fallback raw; quá ngắn &lt;0.5s → lỗi |
| ASR-Proxy live | `OnlineRecognizer` (streaming đang chọn). Feed 100ms, `input_finished`, `exp(mean(ys_probs))`. Không đo được → UI 0% |
| Ready | `asr_confidence ≥ 0.60` **và** `dnsmos_ovrl ≥ 2.5` (nếu có DNSMOS) |
| UX | Nút **Đánh giá** cạnh mic. Dialog: hướng dẫn, progress, ghi 10s. Dialog kết quả: SIG/BAK/OVRL, ASRProxy, chữ, gợi ý, ✓/⚠ |
| Download | Nếu thiếu model → hỏi tải (~5MB) |

## Khác biệt MeetingOne (trước thay đổi)

| | test ASR | MeetingOne |
|---|---|---|
| Nút Đánh giá cạnh mic | Có | Không (chỉ «Thử micro» meter) |
| DNSMOS ONNX | Có | Không |
| VAD trên clip test | Silero Python | `silero_rs` chỉ trong pipeline live/file |
| ASR-Proxy | `ys_probs` Python | Rust `RecognizerResult` **không** field `ys_probs` (JSON C API có, serde crate bỏ) |
| Dialog ghi 10s + kết quả | Có | Không |

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Model | App data `models/dnsmos/sig_bak_ovr.onnx`. URL + SHA giống test ASR. CPU ONNX (`ort` rc.10 đã có) |
| VAD | `get_speech_chunks` file-batch, redemption 300ms. Không đổi live VAD |
| DNSMOS | Padding/cắt 144160, polynomial, sliding 50%. Không normalize |
| ASR-Proxy | Streaming `OnlineRecognizer` nếu đã load và **không** đang ghi. Fallback `AsrEngine` offline. Không tạo recognizer mới, không dùng chung lúc live (OnlineRecognizer không thread-safe) |
| `ys_probs` | Crate Rust không expose. Confidence = `None` khi không parse được. UI ẩn hàng ASRProxy nếu 0. **`is_ready` khi thiếu score ASR: chỉ `ovrl ≥ 2.5`** (tránh khóa user vì crate thiếu field) |
| Ghi âm | 10s, 16kHz mono, mic đang chọn (hoặc mặc định). **Không** start pipeline live. Cấm khi đang ghi họp |
| UI | Nút **Đánh giá** cạnh dropdown Micro (`DeviceSelection`) + nút nhỏ trên home (`RecordingControls`) khi chưa ghi |
| Ngưỡng / câu gợi ý | Giống test ASR (copy nguyên) |

## Pipeline

```
Đánh giá (chưa ghi họp)
  → thiếu DNSMOS? hỏi tải (~5MB, SHA)
  → Dialog: nói 10s vào mic đang chọn
  → Resample 16kHz mono
  → VAD segments (fallback raw nếu không speech)
  → DNSMOS từng đoạn ≥ 0.3s, trung bình SIG/BAK/OVRL
  → ASR-Proxy trên concat (nếu model sẵn)
  → Dialog kết quả + gợi ý + is_ready
```

## Ngoài phạm vi

- DNSMOS trên file import / sau họp
- GPU DNSMOS
- Bắt buộc kiểm tra mic trước khi bấm Ghi (vẫn optional)
- Sửa crate `sherpa-onnx` để expose `ys_probs`
- Bật lại meter RMS thật (simple_level_monitor vẫn giả lập)

## Tiêu chí xong

- Unit test (không ONNX): polynomial, pad 144160, sliding window, gợi ý, `is_ready`, nhãn
- Nút Đánh giá mở dialog; sau 10s hiện SIG/BAK/OVRL (khi model đã tải)
- Live ghi / VAD live / hotkey speaker không đổi hành vi
