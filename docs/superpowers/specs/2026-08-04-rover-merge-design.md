# ROVER Merge (`rover_engine`) — ghép kết quả 2 model ASR theo confidence

## Vấn đề

Phase A (`docs/superpowers/specs/2026-08-04-rnnt-decoder-core-design.md`) đã xong và **đã verify
thực tế**: `RnntDecoder` decode đúng trên cả 3 family (ZipFormer 30M, Gipformer 65M, Sherpa VI 2025)
— cả 3 cho ra cùng một câu tiếng Việt hoàn chỉnh, có nghĩa, trên cùng file audio mẫu. Trong quá
trình verify phát hiện 1 lỗi thật: output của joiner tên là `logit` (số ít), không phải `logits`
như giả định ban đầu trong plan — đã sửa trực tiếp trong `sessions.rs`. Đồng thời xác nhận
`CONTEXT_SIZE = 2` và `BLANK_ID = 0` đúng cho **cả 3 family** (kiểm tra trực tiếp qua
`onnxruntime.InferenceSession.get_inputs()/get_outputs()`), nên Phase B không cần xử lý riêng theo
family cho các tham số này.

Bước tiếp theo: ghép (merge) kết quả của 2 `RnntDecoder` bất kỳ trong 3 family, theo đúng thuật
toán `rover_merge_words` trong app tham khảo (`core/asr_engine.py`).

## Ngoài phạm vi

- UI chọn cặp model, Settings, database schema, wiring vào pipeline ghi âm/import/retranscribe —
  đây là Phase C (spec sau).
- Hotword bonus (`HOTWORD_ROVER_BONUS`, context graph) — Meetily không có tính năng hotword.
- Xử lý overlap giữa các chunk dài (`find_overlap_alignment`, `merge_chunks_with_overlap` trong app
  tham khảo) — đó là logic ghép các **chunk liên tiếp cùng 1 model** cho audio dài, khác với ROVER
  (ghép **2 model khác nhau** trên cùng 1 đoạn audio). `rover_engine` nhận một đoạn audio đã cắt sẵn
  (giống input hiện tại của `AsrEngine::transcribe_audio`), không tự chia chunk.
- Xóa filler word (`remove_filler_words`) — không liên quan ROVER, có thể làm riêng nếu cần sau.

## Kiến trúc

```
rover_engine/
├── mod.rs
├── normalize.rs   — chuẩn hóa từ để so sánh (lowercase, NFC, bỏ ký tự không phải chữ/số)
├── merge.rs         — rover_merge_words: align + chọn theo confidence
└── engine.rs           — RoverDecoder: chạy 2 RnntDecoder song song + gọi merge
```

Phụ thuộc `rnnt_decoder::engine::{RnntDecoder, DecodeResult, WordResult}` trực tiếp — tái dùng type
đã có, không định nghĩa lại.

### Chuẩn hóa từ (`normalize.rs`)

Tương đương `normalize_word_for_overlap` trong app tham khảo:

```rust
pub fn normalize_word(word: &str) -> String {
    // lowercase → NFC → chỉ giữ ký tự alphanumeric (Unicode-aware)
}
```

### Thuật toán merge (`merge.rs`)

Input: `words_a: &[WordResult]`, `words_b: &[WordResult]` (đã decode xong, độc lập, từ 2 model).
Output: `Vec<WordResult>` đã merge.

1. Chuẩn hóa `words_a`/`words_b` thành 2 `Vec<String>` (dùng `normalize_word`).
2. Align bằng `similar::capture_diff_slices(Algorithm::Myers, &norm_a, &norm_b)` → `Vec<DiffOp>`.
   Đây là thay thế trực tiếp cho `difflib.SequenceMatcher(...).get_opcodes()` trong Python — cùng
   khái niệm Equal/Delete/Insert/Replace trên range chỉ số.
3. Duyệt từng `DiffOp` theo thứ tự gốc:
   - **Equal** → lấy nguyên `words_a[range]`.
   - **Replace** → tính `block_confidence(words_a[range_a])` và `block_confidence(words_b[range_b])`
     (trung bình `word.confidence` — field đã có sẵn từ Phase A, không cần tính lại). Chọn block có
     confidence cao hơn; các từ trong block được chọn đánh dấu `disagree = true`.
   - **Delete** (chỉ có ở A) → giữ `words_a[range]`.
   - **Insert** (chỉ có ở B) → với mỗi từ trong `words_b[range]`, chỉ thêm vào kết quả nếu
     `word.confidence > 0.20` (ngưỡng lấy nguyên từ app tham khảo); đánh dấu `disagree = true`.
4. Sort kết quả theo `start` (từ bổ sung từ B có thể không nằm đúng thứ tự thời gian so với A).
5. Dedup: với mỗi từ được thêm từ bước Insert (B), nếu có từ khác trong kết quả (không phải từ
   Insert) trùng `normalize_word` và `|start - start khác| < 0.15s` → bỏ từ Insert đó (tránh lặp).
6. Không có hotword bonus, không có `ctx_graph`.

```rust
pub struct MergedWord {
    pub word: WordResult,   // giữ nguyên WordResult của Phase A
    pub disagree: bool,     // true nếu từ này đến từ việc "thắng" ở Replace, hoặc là Insert-supplement
}

pub fn rover_merge_words(words_a: &[WordResult], words_b: &[WordResult]) -> Vec<MergedWord> { ... }
```

`disagree` không bắt buộc phải hiển thị ở UI ngay (Phase C chưa thiết kế) — giữ lại vì gần như miễn
phí về mặt thuật toán (là sản phẩm phụ tự nhiên của quyết định merge) và giúp việc smoke-test Phase B
dễ quan sát hơn (in ra từ nào bị ghi đè, từ nào là bổ sung).

### `RoverDecoder` (`engine.rs`)

```rust
pub struct RoverDecoder {
    decoder_a: RnntDecoder,
    decoder_b: RnntDecoder,
}

impl RoverDecoder {
    pub fn load(
        family_a: (&Path, &Path, &Path, &Path), // encoder, decoder, joiner, tokens
        family_b: (&Path, &Path, &Path, &Path),
        beam_size: usize,
    ) -> Result<Self> { ... }

    pub fn decode(&mut self, samples: &[f32], sample_rate: f32) -> Result<RoverDecodeResult> {
        // std::thread::scope: chạy decoder_a.decode() và decoder_b.decode() song song.
        // An toàn vì ort::Session là Send + Sync (đã xác nhận qua source ort 2.0.0-rc.10).
    }
}

pub struct RoverDecodeResult {
    pub text: String,
    pub words: Vec<MergedWord>,
}
```

Không phụ thuộc `asr_engine`/sherpa-onnx — giống Phase A, hoàn toàn độc lập. Chưa có Tauri command
nào gọi `RoverDecoder` (Phase C mới wiring).

## Dependency mới

```toml
similar = "2"
unicode-normalization = "0.1"
```

Cả hai đều là crate phổ biến, ổn định lâu năm, không cần link thư viện ngoài.

## Kiểm thử

### Tự động (Rust, không cần model thật — phần lớn giá trị test nằm ở đây)
- `normalize.rs`: chuẩn hóa string có dấu tiếng Việt, kiểm tra NFC (ví dụ tổ hợp dấu decompose vs
  precompose phải normalize về cùng 1 kết quả).
- `merge.rs`: dùng `WordResult` tổng hợp (không cần ONNX) để test từng nhánh:
  - Toàn bộ Equal (2 chuỗi từ giống hệt nhau) → kết quả = A nguyên vẹn, không `disagree`.
  - Replace với A confidence cao hơn → giữ A.
  - Replace với B confidence cao hơn → chọn B, đánh dấu `disagree`.
  - Insert từ B với confidence > 0.20 → được thêm.
  - Insert từ B với confidence ≤ 0.20 → bị bỏ.
  - Dedup: 2 từ trùng text + gần thời gian → chỉ giữ 1.

### Manual (cần model thật + audio mẫu)
- Chạy `RoverDecoder` với cặp (ZipFormer 30M, Gipformer 65M) trên audio mẫu đã dùng ở Phase A.
  Baseline đã biết: cả 3 model **đơn lẻ** đều ra đúng cùng 22 từ trên clip này — nên kỳ vọng hợp lý
  là ROVER cũng ra đúng câu đó (không có bất đồng thật để trọng tài), đây là phép thử "không phá
  vỡ trường hợp đồng thuận", không phải phép thử nhánh Replace/Insert (những nhánh đó đã có unit
  test riêng với dữ liệu tổng hợp).
- Log số lượng Equal/Replace/Insert/Delete block cho 1 lần chạy thật, để có cảm quan tỷ lệ bất đồng
  thực tế giữa 2 model trên audio tiếng Việt thật (không bắt buộc phải khác 0 để coi là pass).

## Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| `similar::DiffOp` không khớp 100% ngữ nghĩa với `difflib.get_opcodes()` ở edge case (chuỗi rỗng, toàn bộ khác nhau) | Unit test phủ các trường hợp biên (2 vec rỗng, A rỗng, B rỗng, không có phần tử nào giống) |
| Chạy 2 decoder song song tốn gấp đôi CPU/RAM so với 1 model | Chấp nhận ở Phase B — đây là bản chất của ROVER; đo tác động thực tế khi có Phase C (settings UI có thể cảnh báo người dùng máy yếu) |
| NFC normalization sai với 1 số ký tự tiếng Việt hiếm | Test trực tiếp với chuỗi có dấu thật lấy từ kết quả Phase A (ví dụ "ĐẤT", "HOẠCH") |
| `RoverDecoder::decode` panic nếu 1 trong 2 thread panic (ONNX lỗi) | `thread::scope` + `JoinHandle::join()` trả `Result` — propagate lỗi rõ ràng thay vì để panic xuyên qua, không dùng `.unwrap()` trên kết quả thread |
