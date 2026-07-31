# Hồi dấu câu tiếng Việt (Punctuation + Capitalization) bằng ViBERT-CAPU ONNX

## Vấn đề

Engine ASR hiện tại của Meetily (ZipFormer/sherpa-onnx, `zipformer-vi-30m` —
[frontend/src-tauri/src/zipformer_engine/zipformer_engine.rs](../../../frontend/src-tauri/src/zipformer_engine/zipformer_engine.rs))
là một mô hình RNNT transducer, xuất ra text thô **hoàn toàn không có dấu câu và không viết
hoa**. Mỗi segment do VAD cắt ra được coi là "final" ngay khi transcribe xong (`is_partial`
luôn `false`) và emit thẳng ra UI qua event `transcript-update`
([worker.rs:228-248](../../../frontend/src-tauri/src/audio/transcription/worker.rs)).
Kết quả: cả live transcript lẫn transcript lưu trong DB đều khó đọc, và chất lượng đầu vào cho
bước summarization (LLM) cũng giảm.

Đã có sẵn `PostProcessor`
([audio/post_processor.rs](../../../frontend/src-tauri/src/audio/post_processor.rs)) với vài
rule đơn giản (capitalize chữ đầu câu, sửa contraction tiếng Anh) nhưng là **dead code** — không
được gọi ở đâu — và không phù hợp cho tiếng Việt.

Người dùng muốn dùng model
[welcomyou/vibert-capu-onnx](https://huggingface.co/welcomyou/vibert-capu-onnx) (ViBERT fine-tune
theo kiến trúc GECToR/seq2labels, dạng ONNX) để hồi dấu câu + viết hoa cho transcript tiếng Việt,
cả trong lúc họp (live) lẫn khi re-transcribe một meeting cũ.

## Ngoài phạm vi

- **Không dùng backend Python.** CLAUDE.md mô tả kiến trúc 3 tầng có backend FastAPI riêng, nhưng
  thực tế thư mục `backend/` trong working tree hiện tại chỉ còn `venv/`, `.env`, file `.db` — không
  có source Python nào. `summary/llm_client.rs` cũng gọi thẳng LLM API bằng `reqwest` từ Rust, không
  qua backend. Vì vậy phương án "sidecar Python" (dùng lại `gec_model.py`/`utils.py` gốc gần như
  nguyên bản) không khả thi và không được chọn — xem phần Kiến trúc.
- Không đóng gói một Python sidecar mới (PyInstaller hay tương tự) — đi ngược hướng dự án đang tự
  chứa hoàn toàn trong Tauri/Rust, và làm phình kích thước cài đặt.
- Không implement toàn bộ action space GECToR tổng quát (~5000 action của GEC đầy đủ) — chỉ 13
  action thực tế mà model `vibert-capu-onnx` này dùng (xem mục 3).
- Không thêm UI chọn bật/tắt tính năng trong spec này — mặc định bật khi model đã tải xong, tương
  tự cách ZipFormer hoạt động hiện tại. UI Settings (nếu cần) là việc riêng.
- Không tự nghiên cứu lại ngữ nghĩa `$TRANSFORM_VERB_VB_VBN`/`$TRANSFORM_VERB_VB_VBC` sâu — 2 action
  này thuộc template GECToR chung, gần như không được model capu này predict trong thực tế, nhưng
  vẫn phải implement (áp dụng y hệt) để không crash khi model lỡ predict ra.

## Kiến trúc tổng quan

```
Segment audio (VAD) → ZipFormer.transcribe_audio() → text thô (không dấu câu)
                                                              ↓
                                          capu_engine::restore_punctuation(
                                              trailing_context + text thô)
                                                              ↓
                                    (BERT WordPiece tokenize, offset theo từ)
                                                              ↓
                                    ONNX forward (ort) → logits action + detect_logits
                                                              ↓
                              decode action theo offset → áp edits.rs → text mới
                                                              ↓
                          lặp tối đa 3 vòng (dừng sớm nếu toàn $KEEP/CORRECT)
                                                              ↓
                        cắt lại phần thuộc segment mới (bỏ phần trailing_context)
                                                              ↓
                    worker.rs: TranscriptUpdate.text = text đã hồi dấu câu → emit
                                                              ↓
                              (retranscription.rs dùng lại đúng hàm này)
```

**Vì sao không dùng backend Python (đã xác nhận với người dùng):** repo model có sẵn code Python
tham chiếu (`gec_model.py`, `utils.py`, `vocabulary.py`) đúng ra sẽ giảm rủi ro triển khai đáng kể,
nhưng vì không còn backend Python nào chạy trong kiến trúc hiện tại của app, và việc thêm sidecar
Python mới đi ngược hướng tự chứa hoàn toàn trong Rust, ta chấp nhận đánh đổi: **port thuần Rust**,
dùng code Python nói trên làm tài liệu tham chiếu khi implement (không phải chạy trực tiếp).

## 1. Model & tài nguyên cần tải

Từ repo `welcomyou/vibert-capu-onnx` (license **CC-BY-SA-4.0** — cần ghi attribution trong app):

| File | Vai trò |
|---|---|
| `vibert-capu.int8.onnx` (110MB, quantized) | Model chính — **ưu tiên bản này** thay vì bản FP32 (438MB), nhẹ hơn nhiều & đủ nhanh cho real-time |
| `vocab.txt` | Vocabulary BERT WordPiece (base `FPTAI/vibert-base-cased`) |
| `config.json` | `max_position_embeddings=512`, `num_labels=15`, `num_detect_classes=4` |
| `vocabulary/labels.txt` | 13 action + `@@UNKNOWN@@`/`@@PADDING@@` (danh sách đầy đủ ở mục 3) |
| `vocabulary/d_tags.txt` | `CORRECT` / `INCORRECT` / `@@UNKNOWN@@` / `@@PADDING@@` |

Tải và lưu trong cùng thư mục models mà ZipFormer đang dùng (`engine.get_models_directory()`),
dưới subfolder riêng, ví dụ `capu-vi/`.

## 2. Module `capu_engine/` (mirror `zipformer_engine/`)

```
frontend/src-tauri/src/capu_engine/
├── mod.rs
├── config.rs        // CAPU_MODEL_NAME, danh sách file cần tải, đường dẫn
├── vocabulary.rs     // load labels.txt/d_tags.txt → enum Action, enum DetectTag
├── tokenizer.rs      // BERT WordPiece tokenizer (crate `tokenizers`) + word_ids()-based offset
├── edits.rs          // áp Action vào Vec<String> (từ) → String (thuần logic, test không cần ONNX)
├── capu_engine.rs     // CapuEngine: ONNX session (crate `ort`), vòng lặp GECToR, trailing-context
└── commands.rs        // Tauri commands: capu_download_model, capu_get_models_directory,
                        //   capu_is_model_downloaded (mirror frontend/src-tauri/src/zipformer_engine/commands.rs)
```

Thêm 2 dependency vào `frontend/src-tauri/Cargo.toml`:
- `ort` — ONNX Runtime bindings cho Rust.
- `tokenizers` — HF Rust tokenizer, dùng để dựng BERT WordPiece tokenizer từ `vocab.txt` và lấy
  `word_ids()` cho từng token (khớp đúng cách `gec_model.py` gốc tính offset qua
  `batch.word_ids(batch_index=i)`).

## 3. Action vocabulary (đã xác nhận từ repo, không phải suy đoán)

`vocabulary/labels.txt` (15 dòng, output head chính):

```
$KEEP
$TRANSFORM_CASE_CAPITAL
$APPEND_,
$APPEND_.
$TRANSFORM_VERB_VB_VBN
$TRANSFORM_CASE_UPPER
$APPEND_:
$APPEND_?
$TRANSFORM_VERB_VB_VBC
$TRANSFORM_CASE_LOWER
$TRANSFORM_CASE_CAPITAL_1
$TRANSFORM_CASE_UPPER_-1
$MERGE_SPACE
@@UNKNOWN@@
@@PADDING@@
```

`vocabulary/d_tags.txt` (4 dòng, output head phụ `detect_logits` — dùng để gate việc áp edit theo
threshold, giống GECToR gốc):

```
CORRECT
INCORRECT
@@UNKNOWN@@
@@PADDING@@
```

`vocabulary.rs` load 2 file này thành `enum Action` (13 biến thể thật + 2 biến thể đặc biệt) và
`enum DetectTag`, với index khớp thứ tự dòng trong file (đây là format chuẩn AllenNLP `Vocabulary`
mà GECToR dùng — index 0 = dòng đầu tiên).

## 4. Model I/O & vòng lặp GECToR (`capu_engine.rs`)

**Input (int64):** `input_ids`, `attention_mask`, `token_type_ids` (luôn 0), `input_offsets`
(vị trí subword đầu tiên của mỗi từ).

**Output (float32):** `logits` shape `(batch, num_words, 15)`, `detect_logits` shape
`(batch, num_words, 4)`.

**Vòng lặp** (khớp `gec_model.py` gốc — `for n_iter in range(self.iterations)`, mặc định
`iterations=3`):

1. Tokenize câu hiện tại (word-level split + WordPiece + tính offset).
2. Forward ONNX → lấy action có xác suất cao nhất mỗi từ (áp threshold theo `detect_logits`,
   giống cơ chế lọc theo probability của `vocabulary.py`/`gec_model.py` gốc — **cần đọc kỹ file
   gốc lúc code để lấy đúng giá trị threshold**, xem mục 11).
3. Nếu tất cả action là `$KEEP` (không có edit nào) → dừng, trả câu hiện tại.
4. Áp edits (mục 5) → câu mới → quay lại bước 1, tối đa 3 lần.

## 5. Áp edit vào text (`edits.rs`, port từ `get_target_sent_by_edits` + `apply_reverse_transformation`)

Input: `Vec<String>` (các từ), `Vec<(usize, usize, Action)>` (start, end, action theo vị trí từ).
Output: `Vec<String>` mới.

- `$KEEP` → giữ nguyên.
- `$APPEND_,` / `$APPEND_.` / `$APPEND_:` / `$APPEND_?` → chèn dấu câu tương ứng ngay sau từ.
- `$TRANSFORM_CASE_CAPITAL` → viết hoa chữ cái đầu của từ.
- `$TRANSFORM_CASE_UPPER` → viết hoa toàn bộ từ.
- `$TRANSFORM_CASE_LOWER` → viết thường toàn bộ từ.
- `$TRANSFORM_CASE_CAPITAL_1` / `$TRANSFORM_CASE_UPPER_-1` → áp case lên từ ở vị trí lệch (+1/-1)
  so với từ hiện tại — **ngữ nghĩa chính xác cần đối chiếu `convert_using_case()` trong
  `gec_model.py`/`utils.py` gốc khi code** (xem mục 11).
- `$MERGE_SPACE` → nối từ hiện tại với từ kế tiếp, bỏ khoảng trắng giữa.
- `$TRANSFORM_VERB_VB_VBN` / `$TRANSFORM_VERB_VB_VBC` → implement theo `convert_using_verb()` gốc
  cho đầy đủ, dù thực tế gần như không được predict với model capu này.

Toàn bộ file này **không phụ thuộc ONNX** → unit test được bằng fixture thuần (từ + action đã biết
→ so khớp string kết quả).

## 6. Tích hợp real-time + trailing-context

Hook vào [worker.rs:228](../../../frontend/src-tauri/src/audio/transcription/worker.rs) — ngay sau
khi có `transcript` hợp lệ, **trước khi** build `TranscriptUpdate`:

```rust
let punctuated = capu_engine
    .restore_punctuation(&trailing_context_buffer, &transcript)
    .await
    .unwrap_or_else(|e| { error!("CAPU inference failed: {}", e); transcript.clone() });

let update = TranscriptUpdate { text: punctuated, /* ... giữ nguyên các field khác */ };
```

`trailing_context_buffer`: state theo phiên ghi âm (giữ ~15 từ cuối của segment **thô** trước đó,
chưa hồi dấu câu — đúng phân phối input mà model được train). Ghép
`trailing_context + text mới` → chạy vòng lặp GECToR trên chuỗi ghép → chỉ giữ lại phần action
thuộc segment mới (offset = độ dài `trailing_context` tính theo số từ) → cập nhật buffer bằng đuôi
của segment mới cho lần gọi kế tiếp.

Reset buffer ở cùng chỗ mà `SEQUENCE_COUNTER`/`SPEECH_DETECTED_EMITTED`
(worker.rs) được reset khi bắt đầu recording mới, để không rò rỉ ngữ cảnh giữa các meeting khác
nhau.

## 7. Tích hợp `retranscription.rs`

Vòng lặp per-segment có sẵn ở
[retranscription.rs:255](../../../frontend/src-tauri/src/audio/retranscription.rs) (gọi
`engine.transcribe_audio(...)` cho từng segment theo thứ tự) — chèn cùng lệnh gọi
`capu_engine.restore_punctuation(...)` ngay sau đó, với buffer trailing-context cục bộ trong vòng
lặp (segment đã xử lý tuần tự sẵn, không cần state global). Kết quả đã hồi dấu câu được ghi vào
`create_transcript_segments` (dòng ~295) trước khi ghi đè DB — đảm bảo re-transcribe một meeting cũ
cho kết quả nhất quán với live.

## 8. Tauri commands & tải model

Mirror [zipformer_engine/commands.rs](../../../frontend/src-tauri/src/zipformer_engine/commands.rs):
`capu_download_model` (progress event `capu-model-download-progress`, hoàn tất
`capu-model-download-complete`, lỗi `capu-model-download-error`), `capu_get_models_directory`,
`capu_is_model_downloaded`. Đăng ký trong `invoke_handler` ở `lib.rs`.

## 9. Xử lý lỗi / fallback

- Model chưa tải → bỏ qua bước hồi dấu câu, dùng text thô (hành vi hiện tại), không chặn pipeline.
- Lỗi inference cho 1 segment cụ thể → log lỗi, fallback text thô **chỉ cho segment đó**, các
  segment khác không bị ảnh hưởng.
- Không crash/panic pipeline transcription vì lỗi ở bước hồi dấu câu trong bất kỳ trường hợp nào.

## 10. Testing

- Unit test `edits.rs`: fixture (từ + action đã biết) → so khớp string kết quả — không cần model.
- Unit test `tokenizer.rs`: câu mẫu → so khớp offset từng từ tính đúng.
- Integration test: chạy ONNX model thật trên vài câu tiếng Việt mẫu (raw, không dấu câu), kiểm tra
  output hợp lý (có thể gate/skip nếu model file chưa có sẵn trong môi trường CI).
- Test tay trong app: ghi một đoạn hội thoại tiếng Việt thật, quan sát live transcript; sau đó
  re-transcribe một meeting cũ và so sánh tính nhất quán.

## 11. Rủi ro & điểm cần xác minh khi code (không đoán, đọc trực tiếp lúc implement)

- **Ngữ nghĩa `$TRANSFORM_CASE_CAPITAL_1` / `$TRANSFORM_CASE_UPPER_-1`** — cần đọc hàm
  `convert_using_case()` đầy đủ trong `gec_model.py`/`utils.py` của repo model trước khi viết
  `edits.rs`.
- **Ngưỡng probability để gate action** (theo `detect_logits`/threshold trong `vocabulary.py` gốc)
  — cần đọc file này đầy đủ, không suy đoán giá trị.
- **Xung đột ONNX Runtime giữa `sherpa-onnx` và `ort`** — cả hai crate đều tự bundle ONNX Runtime
  riêng. Trên Windows đặc biệt cần kiểm tra xung đột tên file `onnxruntime.dll` lúc build/đóng gói.
  Cân nhắc dùng feature `load-dynamic` của `ort` trỏ vào cùng thư viện mà `sherpa-onnx` đã mang
  theo, thay vì bundle 2 bản ONNX Runtime riêng biệt (giảm kích thước cài đặt + tránh xung đột).
  Đây là quyết định kỹ thuật cần chốt ở bước viết plan/implementation, không chốt trước trong spec
  này.
