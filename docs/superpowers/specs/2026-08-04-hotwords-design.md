# Hotwords (tên riêng, thuật ngữ chuyên ngành) — đường sherpa-onnx

## Vấn đề

App tham khảo (`C:\Users\HP\Desktop\test ASR`) có tính năng hotword: người dùng nhập danh sách cụm
từ (tên riêng, thuật ngữ chuyên ngành, tên địa danh...), ASR sẽ ưu tiên nhận diện đúng các cụm từ
này thay vì từ phát âm gần giống. Meetily hiện chưa có tính năng này.

**Phạm vi spec này:** chỉ đường decode 1 model hiện có (`asr_engine`, dùng sherpa-onnx qua
`OfflineRecognizer`) — nơi đa số người dùng thực sự dùng hàng ngày. Custom decoder
(`rnnt_decoder`, chỉ dùng trong ROVER) **không** có hotword trong spec này — beam search tự viết
không có cơ chế Aho-Corasick, cần port riêng nếu muốn, để dành làm spec sau nếu cần.

## Phát hiện quan trọng (đọc code + crate thật, không đoán)

- Rust crate `sherpa-onnx` (đã là dependency) **đã có sẵn** hỗ trợ hotword ở tầng config:
  `OfflineRecognizerConfig.hotwords_file`/`hotwords_score`, và đặc biệt là
  `OfflineRecognizer::create_stream_with_hotwords(&self, hotwords: &str)` — nhận thẳng **text**
  hotword, không cần file, không cần reload model.
- Sherpa-onnx tự tokenize hotword phrase thành BPE piece bên trong C++ — Meetily **không cần** tự
  viết BPE encoder. Điều kiện duy nhất: recognizer phải được tạo với
  `OfflineModelConfig.modeling_unit = "bpe"` và `bpe_vocab` trỏ tới 1 file text dạng
  `piece<TAB>score` liệt kê **toàn bộ** vocab.
- File `bpe_vocab` đó **không phải** `bpe.model` (binary SentencePiece) đã có sẵn trong mỗi thư mục
  model — nó là 1 file text sinh ra TỪ `bpe.model`. App tham khảo sinh file này bằng Python
  (`ensure_bpe_vocab` trong `core/config.py`, dùng thư viện `sentencepiece`) một lần rồi cache.
- Rust có crate thuần (`sentencepiece-model`, build bằng `protox` — parser protobuf thuần Rust,
  **không cần** cài `protoc` hệ thống) đọc trực tiếp file `bpe.model` binary và liệt kê toàn bộ
  `piece`/`score` — đủ để sinh `bpe.vocab` tương đương, không cần Python lúc runtime.

**Kết luận kiến trúc:** không cần tự viết Aho-Corasick hay BPE tokenizer cho đường sherpa-onnx —
chỉ cần (1) sinh `bpe.vocab` một lần từ `bpe.model` có sẵn, (2) set 2 field config khi load model,
(3) đổi `create_stream()` → `create_stream_with_hotwords(text)` khi transcribe. Vì hotword text được
truyền **mỗi lần gọi transcribe**, sửa danh sách hotword trong Settings có hiệu lực ngay từ lần
transcribe tiếp theo — **không cần** unload/reload model.

## Ngoài phạm vi

- Hotword cho custom decoder (`rnnt_decoder`/ROVER) — spec riêng nếu cần sau.
- UI quản lý hotword dạng danh sách/CRUD từng dòng có nút thêm/xóa/sửa trọng số riêng — dùng
  textarea 1 khối, đúng format file text của app tham khảo (đơn giản, không mất công build UI mới).
- Đề xuất hotword tự động (gợi ý từ transcript trước đó, NER...) — không có trong app tham khảo,
  không làm.
- Hotword theo từng meeting/template riêng — 1 danh sách hotword dùng chung toàn app, giống app
  tham khảo (1 file `hotword.txt` toàn cục).

## Thiết kế

### 1. Format hotword (giữ nguyên như app tham khảo)

```
# Dòng bắt đầu bằng # là comment, bị bỏ qua
ỦY BAN NHÂN DÂN :2.5
CHUYỂN ĐỔI SỐ :1.5
CÔNG NGHỆ THÔNG TIN
```

Mỗi dòng 1 cụm từ, có thể kèm `:điểm` (trọng số) — cú pháp này **sherpa-onnx tự hiểu**, Meetily chỉ
cần lọc bỏ dòng trống/dòng comment trước khi truyền vào, không cần parse `:score` thủ công.

### 2. Sinh `bpe.vocab` từ `bpe.model`

Module mới `hotwords/vocab_export.rs` (hoặc thêm vào `asr_engine`): dùng crate `sentencepiece-model`
đọc `bpe.model`, ghi ra `bpe.vocab` (`piece\tscore` mỗi dòng) cùng thư mục. Chỉ sinh nếu chưa tồn
tại (cache) — giống hệt cách `ensure_bpe_vocab` của app tham khảo hoạt động, chỉ khác không cần
Python.

### 3. `AsrEngine::load_model` — bật hotword ở tầng config

Thêm vào `OfflineModelConfig` khi build `OfflineRecognizerConfig`:
```rust
config.model_config.modeling_unit = Some("bpe".to_string());
config.model_config.bpe_vocab = Some(bpe_vocab_path);
```
Nếu sinh `bpe.vocab` thất bại (ví dụ `bpe.model` bị hỏng) → log warning, tiếp tục load model KHÔNG
có hotword (không chặn transcribe — hotword là tính năng cộng thêm, không phải yêu cầu bắt buộc).

### 4. `AsrEngine::transcribe_audio` — dùng hotword mỗi lần gọi

Thay `recognizer.create_stream()` bằng `recognizer.create_stream_with_hotwords(&hotwords_text)`,
với `hotwords_text` đọc từ config đã lưu (đã lọc comment/dòng trống). Nếu hotword rỗng → dùng
`create_stream()` bình thường (tránh gọi API hotword không cần thiết khi người dùng chưa nhập gì).

### 5. Database & Settings

Thêm cột `hotwords TEXT` vào `transcript_settings` (mặc định rỗng). UI: 1 textarea trong Settings →
Nhận dạng, ghi chú ngắn về cú pháp `:điểm`, lưu qua flow `api_save_transcript_config` hiện có (thêm
field `hotwords`).

## Kiểm thử

### Tự động (Rust)
- Test lọc comment/dòng trống từ raw hotword text (input có `#...`, dòng trống, dòng hợp lệ →
  output chỉ còn dòng hợp lệ).
- Test sinh `bpe.vocab` từ 1 file `bpe.model` thật (dùng file có sẵn trong model đã tải) → kiểm tra
  số dòng khớp `processor.GetPieceSize()`... (không có sentencepiece Python để đối chiếu số chính
  xác trong CI, nhưng có thể kiểm tra số dòng > 0 và khớp `tokens.txt`'s vocab size cùng model).

### Manual (bắt buộc trước merge)
1. Chưa nhập hotword → transcribe bình thường, không lỗi (đường không-hotword vẫn hoạt động).
2. Nhập hotword gồm vài cụm từ chuyên ngành/tên riêng có trong app tham khảo's `hotword.txt` →
   transcribe 1 đoạn audio có nhắc tới các cụm đó → so sánh độ chính xác nhận diện cụm từ đó trước
   và sau khi bật hotword.
3. Sửa hotword trong Settings → Lưu → transcribe ngay (không unload/reload model thủ công) → xác
   nhận hotword mới có hiệu lực ngay.
4. Test với cả 3 family (ZipFormer, Gipformer, Sherpa VI 2025) — mỗi family có `bpe.model` riêng,
   phải tự sinh `bpe.vocab` riêng đúng cho từng family.

## Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| `sentencepiece-model` crate parse sai với `bpe.model` cụ thể của 1 trong 3 family (field optional khác giả định) | Test sinh vocab thật trên cả 3 family trước khi coi là xong; nếu lỗi, log rõ và fallback không-hotword thay vì crash |
| Cú pháp `:score` sherpa-onnx hiểu khác giả định (ví dụ dấu cách quanh `:`) | Đối chiếu trực tiếp với `hotword.txt` mẫu của app tham khảo — cùng cú pháp, cùng behavior kỳ vọng |
| Hotword rỗng nhưng vẫn gọi `create_stream_with_hotwords("")` gây lỗi/khác biệt hành vi | Guard rõ: hotword rỗng → dùng `create_stream()` như cũ |
| Người dùng nhập hotword rất dài (hàng nghìn dòng) làm chậm mỗi lần transcribe | Không giới hạn trong spec này — app tham khảo cũng không giới hạn; nếu sau này đo thấy chậm thật thì tối ưu (cache theo hash nội dung), không tối ưu sớm |
