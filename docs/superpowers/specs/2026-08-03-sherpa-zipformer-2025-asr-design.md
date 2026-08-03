# Thêm Sherpa-ONNX Zipformer VI (2025) làm model ASR thứ ba

## Vấn đề

Sau khi Gipformer 65M được thêm làm model ASR thứ hai (`docs/superpowers/specs/2026-08-03-gipformer-asr-design.md`),
Meetily có `asr_engine` thống nhất hỗ trợ `ModelFamily` × `ModelVariant`. Người dùng muốn thêm
[csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20](https://huggingface.co/csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20)
làm **lựa chọn thứ ba**, dựa trên tham khảo một ứng dụng ASR tiếng Việt độc lập
(`C:\Users\HP\Desktop\test ASR`) vốn dùng chính model này (cùng với ZipFormer 30M) làm cặp
model cho chế độ ROVER voting của ứng dụng đó.

**Mục đích của spec này:** chỉ thêm model làm family thứ ba trong `asr_engine`, chưa làm ROVER.
Model thứ ba này (cùng ZipFormer 30M và Gipformer 65M) sẽ là nền tảng — 3 family để chọn — cho
spec ROVER ensemble sau (`docs/superpowers/specs/2026-08-03-rover-asr-ensemble-design.md`, chưa viết).

## Ngoài phạm vi

- Không làm ROVER ensemble/merge (spec riêng, sau).
- Không thêm streaming ASR.
- Không thay đổi pipeline CAPU.
- Không thêm hotword/vocabulary biasing (không tồn tại trong Meetily hiện tại).
- Không tự quantize model này thành int8 — HF repo chỉ có bản full precision.

## Model mới

| Thuộc tính | Giá trị |
|---|---|
| ID | `sherpa-onnx-zipformer-vi-2025-04-20` (giữ nguyên tên repo HF gốc) |
| Label (UI) | "Sherpa-ONNX Zipformer VI (2025)" |
| HF repo | `csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20` |
| Encoder | `encoder-epoch-12-avg-8.onnx` (~261 MB) |
| Decoder | `decoder-epoch-12-avg-8.onnx` (~5.2 MB) |
| Joiner | `joiner-epoch-12-avg-8.onnx` (~4.1 MB) |
| Token file | `tokens.txt` (~26 kB) — không cần fallback như Gipformer |
| Shared | `bpe.model` (~271 kB, không set `bpe_vocab`, giữ comment hiện tại) |
| Biến thể có sẵn | **Chỉ Full** — HF repo không có bản int8 |

Đã xác nhận qua danh sách file trên HuggingFace (`huggingface.co/csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20/tree/main`):
không có file `.int8.onnx` nào, chỉ có bản full precision cho cả 3 file encoder/decoder/joiner.

## Điểm khác biệt so với pattern Gipformer: family chỉ có một biến thể

Kiến trúc hiện tại (`ModelFamily` × `ModelVariant`) giả định mỗi family có cả `Int8` và `Full`.
Family mới này chỉ có `Full`. Cách xử lý:

- Thêm `ModelFamily::available_variants(self) -> &'static [ModelVariant]`:
  - `ZipFormer30M` → `&[Int8, Full]`
  - `Gipformer65M` → `&[Int8, Full]`
  - `SherpaZipformerVi2025` → `&[Full]`
- `AsrModelManager` (frontend) lọc dropdown "Biến thể" theo `available_variants()` của family đang
  chọn; nếu chỉ có 1 lựa chọn, tự chọn luôn (ẩn hoặc disable dropdown).
- Không tạo thư mục `sherpa-vi-2025-int8/` — chỉ `sherpa-vi-2025-full/`.

## Lưu trữ file trên đĩa

```
models/
├── zipformer-vi-int8/       ← không đổi
├── zipformer-vi-full/       ← không đổi
├── gipformer-vi-int8/       ← không đổi
├── gipformer-vi-full/       ← không đổi
└── sherpa-vi-2025-full/     ← mới, chỉ có bản full
```

## Backend Rust

### `config.rs`

Thêm block hằng số mới theo pattern `GIPFORMER_*` hiện có, chỉ với biến thể Full:

```rust
pub const SHERPA_VI_2025_MODEL_NAME: &str = "sherpa-onnx-zipformer-vi-2025-04-20";
pub const SHERPA_VI_2025_HF_URL: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20/resolve/main";
pub const SHERPA_VI_2025_SUBDIR: &str = "sherpa-vi-2025-full";
pub const SHERPA_VI_2025_ENCODER: &str = "encoder-epoch-12-avg-8.onnx";
pub const SHERPA_VI_2025_DECODER: &str = "decoder-epoch-12-avg-8.onnx";
pub const SHERPA_VI_2025_JOINER: &str = "joiner-epoch-12-avg-8.onnx";
pub const SHERPA_VI_2025_SIZE_BYTES: u64 = 261_000_000;
pub const SHERPA_VI_2025_BPE: &str = "bpe.model";
pub const SHERPA_VI_2025_TOKENS: &str = "tokens.txt";
```

### `asr_engine/model_family.rs`

- Thêm variant `SherpaZipformerVi2025` vào enum `ModelFamily`.
- Thêm `available_variants()` như mô tả trên.
- Mở rộng mọi `match (self, variant)` hiện có (`variant_subdir`, `hf_url`, `encoder_file`,
  `decoder_file`, `joiner_file`, `encoder_size_bytes`) với nhánh mới — với `ModelVariant::Int8`
  cho family này thì các hàm này không được gọi (UI đã lọc), nhưng để tránh panic khi match không
  đủ nhánh, cần match tường minh cả `(SherpaZipformerVi2025, Int8)` — trả lỗi rõ ràng hoặc
  `unreachable!()` với message giải thích, KHÔNG âm thầm trả về dữ liệu Full khi được gọi với Int8.
- `token_file()`: trả `crate::config::SHERPA_VI_2025_TOKENS` — không cần fallback.
- `id()`, `label()`, `from_id()`: thêm nhánh mới.

### Database

Không cần migration mới — migration của Gipformer
(`20260803000000_add_asr_family.sql`) đã tổng quát hoá cột `model` thành family-id dạng string tự
do. Family mới chỉ là một giá trị hợp lệ khác của cột đó.

## Frontend

- `lib/asr.ts`: thêm entry vào `ASR_MODELS`, thêm `'sherpa-onnx-zipformer-vi-2025-04-20'` vào type
  `AsrModelFamily`. Thêm field `availableVariants: ModelVariant[]` vào `AsrModelInfo` (hoặc suy ra
  từ danh sách cứng trong component) để UI biết ẩn/disable dropdown biến thể.
- `AsrModelManager.tsx`: khi đổi family, nếu family mới chỉ có 1 biến thể, tự set
  `selectedVariant` = biến thể đó và disable dropdown; khi đổi sang family có 2 biến thể, bật lại
  dropdown.

## Kiểm thử

### Tự động (Rust)
- Unit test `available_variants()` trả đúng danh sách cho cả 3 family.
- Unit test `model_files(Full)` cho `SherpaZipformerVi2025` đúng 5 file, đúng tên.
- Unit test `variant_subdir` không đụng các thư mục family khác.

### Manual (bắt buộc trước merge)
1. Settings → Nhận dạng → chọn "Sherpa-ONNX Zipformer VI (2025)" → dropdown biến thể tự khóa ở Full.
2. Tải model (~270 MB) → theo dõi progress bar.
3. Load model → transcribe file audio mẫu tiếng Việt → có text.
4. CAPU chạy sau ASR như bình thường.
5. Chuyển đổi qua lại giữa cả 3 family (ZipFormer ↔ Gipformer ↔ Sherpa VI 2025).
6. Retranscribe một meeting với model mới.

## Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| `match` thiếu nhánh `(SherpaZipformerVi2025, Int8)` gây panic hoặc build fail | Match tường minh mọi cặp, dùng `unreachable!()` có message cho tổ hợp không hợp lệ thay vì để compiler tự suy diễn |
| UI vẫn cho chọn "int8" cho family này (dead option, tải file không tồn tại → 404) | `available_variants()` là nguồn sự thật duy nhất cho dropdown; không hard-code `['int8', 'full']` ở component |
| Model 261 MB tải chậm trên mạng yếu | Progress bar + size label rõ ràng trong UI, giữ pattern hiện có |
| Nhầm model này với model tương lai khác cũng thuộc họ Zipformer | Giữ nguyên ID = tên repo HF gốc, tránh alias mơ hồ |
