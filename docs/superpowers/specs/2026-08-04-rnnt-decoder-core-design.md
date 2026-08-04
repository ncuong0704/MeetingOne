# Custom RNN-T Decoder Core (`rnnt_decoder`) — nền tảng cho ROVER

## Vấn đề

Sau khi có 3 ASR family (`docs/superpowers/specs/2026-08-03-sherpa-zipformer-2025-asr-design.md`), bước
tiếp theo là ROVER ensemble — chạy song song 2 model bất kỳ trong 3 family và merge kết quả theo
confidence, giống ứng dụng tham khảo `C:\Users\HP\Desktop\test ASR`.

Đã xác nhận qua đọc code (`core/asr_engine.py`, hàm `_ort_beam_search`, `_compute_token_entropy`,
`_finalize_word_entropy`, `_word_confidence`): ứng dụng tham khảo không dùng confidence từ
sherpa-onnx — nó **tự viết decoder RNN-T bằng raw ONNX Runtime** để lấy được logits thô của joiner,
từ đó tính margin/Tsallis entropy per-token. Rust crate `sherpa-onnx` hiện tại
(`OfflineRecognizerResult { text, tokens, timestamps, durations }`, xác nhận qua
`sherpa-onnx-1.13.0/src/offline_asr.rs`) không có field confidence/log-prob nào — không có cách nào
lấy confidence qua binding hiện tại.

**Quyết định đã chốt (brainstorming):** làm đúng theo ứng dụng tham khảo — viết một decoder RNN-T
tùy chỉnh bằng Rust, tách thành 3 spec riêng:

1. **Spec này (Phase A):** decoder core — encoder/decoder/joiner + beam search + confidence, chạy
   độc lập trên 1 model, chưa merge, chưa UI.
2. **Phase B (spec sau):** ROVER merge — chạy 2 decoder song song, align + chọn theo confidence.
3. **Phase C (spec sau):** wiring — Settings UI chọn cặp model, DB schema, thay thế
   `AsrEngine::transcribe_audio` khi ROVER bật.

## Ngoài phạm vi

- ROVER merge logic (Phase B).
- UI, Settings, database schema, bất kỳ thay đổi nào ảnh hưởng người dùng cuối (Phase C). Sau spec
  này, `rnnt_decoder` là code chết — không có call site nào trong app dùng nó.
- Hotword/vocabulary biasing (context graph Aho-Corasick trong app tham khảo) — Meetily không có
  tính năng hotword, không port phần này.
- Đường dẫn decode single-model hiện tại (`asr_engine` + sherpa-onnx) — không đụng vào, không thay
  thế. `rnnt_decoder` là engine mới, cộng thêm, độc lập hoàn toàn.
- Tối ưu hiệu năng beam search (batch nhiều hypothesis hiệu quả hơn, GPU-specific tuning) — đúng
  thuật toán trước, tối ưu tốc độ để dành cho lúc đo được ROVER có chậm quá không (Phase C).
- Streaming/online decode — vẫn offline/batch như toàn bộ `asr_engine` hiện tại.

## Kiến trúc

```
rnnt_decoder/
├── mod.rs
├── features.rs      — fbank extraction (crate kaldi-native-fbank, mới)
├── sessions.rs       — ONNX session cho encoder/decoder/joiner (crate ort, đã có)
├── beam_search.rs    — hypothesis state + modified beam search
├── confidence.rs      — margin/Tsallis entropy per-token + aggregate per-word
├── vocab.rs            — đọc tokens.txt (id→piece), gộp BPE piece thành từ theo ranh giới ▁
└── engine.rs            — RnntDecoder: load() + decode() facade
```

`rnnt_decoder` không phụ thuộc `asr_engine::engine::AsrEngine` hay `sherpa_onnx` crate.
`RnntDecoder::load()` nhận thẳng đường dẫn file (encoder/decoder/joiner/tokens) — không biết gì về
`ModelFamily`. Phase C sẽ là nơi map `ModelFamily` → đường dẫn rồi gọi `RnntDecoder::load()`.

### Luồng dữ liệu

```
Vec<f32> 16kHz mono
  → features::compute_fbank()   → ndarray [T, 80] f32
  → sessions::run_encoder()      → [T, D] encoder_out
  → beam_search::run()            → best hypothesis: token_ids + per-token joiner logits + frame idx
  → confidence::score_tokens()     → per-token { margin, tsallis_norm }
  → vocab::pieces_to_words()        → Vec<WordResult>
  → engine::DecodeResult { text, words }
```

```rust
pub struct WordResult {
    pub text: String,
    pub start: f32,       // giây, từ frame index × frame_shift
    pub end: f32,
    pub margin_min: f32,
    pub tsallis_max: f32,
    pub confidence: f32,  // margin_min * (1.0 - tsallis_max)
}

pub struct DecodeResult {
    pub text: String,
    pub words: Vec<WordResult>,
}
```

### Tham số bắt buộc khớp app tham khảo

Đây là phần quyết định decoder có chạy đúng hay không — sai một tham số là ra rác.

**Fbank** (từ `compute_fbank_ort` trong `core/asr_engine.py`):

| Tham số | Giá trị |
|---|---|
| Sample rate | 16000 |
| frame_length_ms | 25.0 |
| frame_shift_ms | 10.0 |
| window_type | povey |
| dither | 0.0 |
| snip_edges | false |
| num_mel_bins | 80 |
| low_freq | 20.0 |
| high_freq | 7600.0 |
| energy_floor | 1.0 |

**Encoder/decoder/joiner I/O** (từ `_ort_beam_search`, quy ước transducer chuẩn icefall/k2 —
giống hệt sherpa-onnx đang dùng cho 3 family hiện có):

| Session | Input | Output |
|---|---|---|
| Encoder | `x: [1,T,80]` f32, `x_lens: [1]` i64 | `encoder_out: [1,T,D]`, `encoder_out_lens: [1]` |
| Decoder | `y: [B,2]` i64 (context size 2, stateless) | `decoder_out: [B,D_dec]` |
| Joiner | `encoder_out: [B,D]`, `decoder_out: [B,D_dec]` | `logits: [B,V]` |

`BLANK_ID = 0`, `CONTEXT_SIZE = 2` — đúng theo cả 3 family hiện có (ZipFormer 30M, Gipformer 65M,
Sherpa VI 2025 đều export theo cùng convention icefall). Xác nhận lại bằng smoke test thực tế cho
từng family khi implement — không giả định mù.

**Beam search** (per frame):
1. Log-softmax logits của mọi hypothesis đang active, cộng vào log-prob tích lũy.
2. Global top-k trên toàn bộ ma trận (hypothesis × vocab) — không phải top-k riêng từng hypothesis.
3. Token trùng nhau (2 hypothesis khác nhau hội tụ về cùng chuỗi token) → merge bằng log-sum-exp
   (`_log_add`), không giữ cả hai.
4. Decoder output cache theo context tuple — tránh forward pass thừa khi nhiều hypothesis share cùng
   2 token cuối.
5. Chọn hypothesis cuối theo `log_prob / độ_dài_token` (length-normalized).
6. Không hotword context graph.

**Confidence** (từ `_compute_token_entropy`, `_finalize_word_entropy`, `_word_confidence`):
- Với mỗi token đã chọn: softmax trên raw joiner logits tại bước đó → `margin` = top1_prob −
  top2_prob; Tsallis entropy (α = 1/3), chuẩn hóa theo giá trị Tsallis lớn nhất có thể với vocab
  size V.
- Gộp theo từ (nhiều token BPE → 1 từ, ranh giới là piece bắt đầu bằng `▁` U+2581):
  `margin_min` = min margin trong các token của từ, `tsallis_max` = max tsallis_norm.
- `confidence` cuối = `margin_min * (1.0 - tsallis_max)`.

## Dependency mới

```toml
kaldi-native-fbank = "0.1"
```

Rust port thuần (không cần link thư viện C++ ngoài, không cần vendor thêm gì) — cùng tinh thần với
việc dùng `ort` cho CAPU thay vì gọi ra process Python. **Việc đầu tiên khi implement:** đọc source
thật của crate này trong Cargo registry cache (`~/.cargo/registry/src/.../kaldi-native-fbank-*/src/`)
để lấy đúng tên struct/field — mô tả API ở trên (`FbankOptions`, `frame_opts`, `mel_opts`, …) là suy
từ package Python/C++ gốc, **chưa xác nhận chữ ký Rust chính xác**.

## Thứ tự xây dựng theo giai đoạn (định hướng cho plan)

1. **Feature extraction** — chạy fbank trên 1 file WAV mẫu, kiểm tra shape `[T, 80]` hợp lý.
2. **Greedy decode** (argmax mỗi bước, không beam) — cột mốc đầu tiên có text đọc được. Đơn giản
   nhất để cô lập lỗi (nếu greedy đã ra rác thì lỗi nằm ở fbank hoặc encoder/decoder/joiner, không
   phải ở beam search).
3. **Modified beam search đầy đủ** — thay greedy, kỳ vọng chất lượng bằng hoặc tốt hơn.
4. **Confidence scoring** — thêm sau khi text đã đúng, vì confidence không quan sát được bằng mắt
   như text.

## Kiểm thử

### Tự động (Rust, không cần model thật)
- `confidence.rs`: test margin/Tsallis trên logits tổng hợp (tự tính tay giá trị kỳ vọng).
- `vocab.rs`: test gộp piece → từ trên danh sách token giả lập, bao gồm case từ 1 piece và từ nhiều
  piece.

### Manual (bắt buộc, cần model thật + audio mẫu)
1. Fbank: chạy trên 1 đoạn audio ngắn, log ra shape + vài giá trị đầu — so sánh cảm quan với việc
   `AsrEngine` hiện tại (dùng sherpa-onnx) transcribe cùng file ra kết quả hợp lý (gián tiếp xác nhận
   audio input pipeline giống nhau).
2. Greedy decode trên ZipFormer 30M int8 (đã có sẵn, không cần tải thêm) → so kết quả text với
   `AsrEngine::transcribe_audio` cùng model, cùng file. Không cần giống hệt, nhưng phải cùng nội
   dung — nếu ra rác/lặp/rỗng thì dừng lại, không sang bước beam search.
3. Beam search → so với greedy, kỳ vọng chất lượng tương đương hoặc tốt hơn, không tệ hơn rõ rệt.
4. Confidence → log margin_min/tsallis_max/confidence per word trên 1 câu, kiểm tra bằng mắt: từ
   phát âm rõ có confidence cao hơn từ bị nuốt/nhiễu.

## Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| Tên field thật của `kaldi-native-fbank` khác giả định | Đọc source crate trước khi viết `features.rs`, không đoán |
| Sai tham số fbank/context_size/blank_id → decode ra rác | Xây theo giai đoạn (greedy trước), so sánh với `AsrEngine` hiện có trên cùng model |
| `context_size`/`blank_id` không đúng cho 1 trong 3 family | Xác nhận qua smoke test riêng từng family lúc implement, không giả định chung cho cả 3 |
| Beam search chậm (global top-k mỗi frame, chưa tối ưu) | Chấp nhận trong Phase A — đúng thuật toán trước; tối ưu để dành Phase C nếu đo thấy cần |
| `rnnt_decoder` vô tình ảnh hưởng đường decode hiện tại | Module hoàn toàn tách biệt, không import/sửa `asr_engine`; Phase A không có call site nào trong app |
