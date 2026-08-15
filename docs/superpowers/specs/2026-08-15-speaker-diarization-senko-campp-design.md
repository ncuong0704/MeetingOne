# Phân biệt người nói — Senko CAM++ (file import)

**Ngày:** 2026-08-15  
**Trạng thái:** Đã chốt (thay Community-1 vì quá chậm)  
**Thay thế:** Spec `2026-08-15-speaker-diarization-community1-design.md`

## Vấn đề

MeetingOne đang chạy Community-1 Pure ORT (segmentation 10s/1s + ResNet34-LM 256-d + PLDA + VBx). Trên file giao ban ~293s, diarization mất **~245s** — trải nghiệm không dùng được.

App tham chiếu `C:\Users\HP\Desktop\test ASR` có **Senko CAM++**: cùng mục tiêu, nhanh gấp nhiều lần (CAM++ 192-d + spectral, không PLDA/VBx).

## Quyết định đã chốt

| Quyết định | Giá trị |
|---|---|
| Pipeline | Senko CAM++ như test ASR `senko_campp_optimized` (thuật toán clustering + embedding) |
| Model duy nhất v1 | `campplus_cn_en_common_200k.onnx` (~28 MB, 192-dim) |
| VAD | **Không** dùng `segmentation-community-1.onnx`. Energy VAD (cùng thuật toán `_energy_vad` trong `speaker_diarization_senko_campp.py`) |
| Phạm vi | Chỉ file import / retranscription (không live) |
| Fail policy | Lỗi → không speaker, import vẫn thành công |
| UI | Giữ checkbox / số người / đổi tên / gộp; giữ `resegment_by_speaker_turns` (sửa CAPU đoạn dài nuốt speaker) |

## Khác nhau hiện tại (test ASR vs MeetingOne)

| Bước | test ASR Senko CAM++ | MeetingOne (Community-1) |
|---|---|---|
| VAD | Pyannote community-1 **chỉ làm VAD** (optimized: bước 5s) | Community-1 làm **cả** segmentation speaker |
| Embedding | CAM++ 192-d, cửa sổ 1.5s / bước 0.6s, fbank povey, batch 32 | ResNet34-LM 256-d + Gemm, chunk 10s |
| Clustering | Spectral (`pval=0.012`, max 15) nếu audio &lt; 20 phút; UMAP+HDBSCAN nếu ≥ 20 phút | PLDA + VBx |
| Post | `filter_minor` + `mer_cos=0.875` + gộp gap ≤ 4s + bỏ ≤ 0.78s + xếp lại theo thời lượng nói | Chunk-level dominant speaker |
| Thời gian (293s audio) | Mục tiêu: vài–vài chục giây | ~245s đo được |

MeetingOne **không** port UMAP+HDBSCAN (crate nặng). Audio ≥ 20 phút vẫn dùng spectral (cùng tham số).

## Pipeline

```
samples 16 kHz
  ├──→ (đã có) VAD Silero → ASR/ROVER → CAPU → TranscriptSegment[]
  └──→ [nếu bật] energy VAD → cửa sổ 1.5s/0.6s
                              → fbank povey 80-d + CAM++ → embeddings 192-d
                              → spectral + post-process → SpeakerTurn[]
         cả hai xong ─────────┘
                    → resegment_by_speaker_turns (tách CAPU theo ranh giới turn)
```

Diarization tuần tự sau ASR (tránh tranh CPU với ROVER).

## Module

| File | Vai trò |
|---|---|
| `clustering.rs` | Cosine, spectral (eigh + KMeans), filter_minor, merge_by_cos, post-process turn |
| `embedding.rs` | Fbank povey + session CAM++ (`feats` → `embs`) |
| `engine.rs` | Energy VAD + cửa sổ + batch embed + cluster |
| `align.rs` | Giữ nguyên (max-overlap + resegment) |
| `commands.rs` | Vendor 1 file ONNX, init, ready |
| Xóa | `plda.rs`, `segmentation.rs`, testdata npy/plda Community-1 |

**Asset:** `models/diarization-senko-campp/campplus_cn_en_common_200k.onnx`  
Nguồn vendor: `test ASR/models/campp-3dspeaker/campplus_cn_en_common_200k.onnx` (28_283_928 bytes).

## Data / UI

Không đổi schema (`meeting_speakers`, `transcripts.speaker_id`, settings `diarizationEnabled` / `diarizationNumSpeakers`).

## Lỗi

Model chưa có / inference lỗi → cảnh báo log, không speaker.

## Kiểm thử

- Spectral: 2 cụm embedding giả tách rõ → 2 nhãn; `mer_cos` gộp cụm gần
- Energy VAD: silence vs speech giả
- `resegment_by_speaker_turns` (hồi quy giao ban)
- `cargo test --lib diarization_engine` / `capu_engine` / `audio::batch_transcribe`

## Rủi ro

| Rủi ro | Giảm thiểu |
|---|---|
| Energy VAD kém community-1 | Chấp nhận đổi tốc độ; có thể nối Silero ASR sau |
| KMeans Rust ≠ sklearn | Seed cố định; test với cụm tách rõ |
| ≥ 20 phút không UMAP | Spectral vẫn chạy; tinh chỉnh sau nếu cần |
