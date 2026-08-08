# ROVER Wiring (Phase C) — Settings, database, pipeline

## Vấn đề

Phase A (`rnnt_decoder`) và Phase B (`rover_engine::merge`/`RoverDecoder`) đã xong và **đã verify
thực tế trên audio thật**: `RoverDecoder` chạy 2 model song song, merge đúng (0/23 từ bất đồng trên
clip test — khớp hoàn toàn với việc cả 3 model đơn lẻ đã cho cùng kết quả ở Phase A). `RoverDecoder`
hiện là code chết — chưa có Settings UI, chưa có DB schema, chưa có call site nào trong app gọi tới.

Phase C: wiring `RoverDecoder` vào Settings, database, và 3 điểm gọi ASR hiện có (ghi âm trực tiếp,
import file, retranscribe).

## Ngoài phạm vi

- Expose `beam_size` thành setting cho người dùng chỉnh — giữ cố định = 4, lựa chọn có chủ đích của
  Meetily đánh đổi bớt độ rộng tìm kiếm lấy tốc độ. **Sửa 2026-08-07**: dòng này trước đây ghi nhầm
  4 là "mặc định app tham khảo dùng cho decoder này" — thực ra app tham khảo dùng
  `max_active_paths=8` cho ROVER; con số 4 ở đó là `cpu_threads` (số luồng CPU), một tham số khác bị
  nhầm lẫn. Đã đối chiếu lại trực tiếp `core/asr_engine.py`, benchmark cả 2 giá trị trên audio thật
  (4 → nhanh hơn đáng kể, 8 → khớp đúng "công sức tìm kiếm" của app tham khảo nhưng chậm hơn nhiều),
  và người dùng quyết định giữ 4 cho Meetily.
- Hiển thị `disagree` flag (từ nào bị ghi đè bởi model B) lên UI transcript — dữ liệu đã có sẵn
  trong `MergedWord`, nhưng chưa thiết kế UI cho việc này; có thể làm sau như một cải tiến riêng.
- Hợp nhất kiến trúc gọi ASR giữa `worker.rs` (đi qua `TranscriptionProvider` trait) và
  `import.rs`/`retranscription.rs` (gọi thẳng `asr_engine`) — đây là sự không nhất quán có từ trước
  Phase C, không phải việc ROVER cần sửa.
- GPU-specific tuning cho việc chạy 2 decoder song song.

## Thiết kế

### 1. Database

Thêm 3 cột vào `transcript_settings` (bảng 1 dòng hiện có, không tạo bảng mới):

| Cột | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `roverEnabled` | INTEGER (bool) | `0` | Bật/tắt chế độ ROVER |
| `roverFamilyB` | TEXT | `NULL` | Family ID của model B (phụ) |
| `roverVariantB` | TEXT | `NULL` | Biến thể (`int8`/`full`) của model B |

Khi `roverEnabled = 1`, cột `model`/`asrVariant` **đã có sẵn** được tái dùng làm "family A / variant
A" — không thêm cột riêng cho phía A, tránh trùng lặp dữ liệu.

Migration mới: `frontend/src-tauri/migrations/20260804000000_add_rover_config.sql`.

### 2. Backend Rust — `rover_engine::commands`

Không tạo lại logic tải model — tải file vẫn dùng nguyên `asr_engine::commands::asr_download_model`/
`asr_get_variant_status` gọi 2 lần (1 lần mỗi phía), vì tải file là thao tác generic theo
family+variant, đã có sẵn và đúng cho mọi family. `rover_engine::commands` chỉ thêm phần ROVER cần
mà `asr_engine` không có: giữ trạng thái **2 model cùng lúc** và gọi `RoverDecoder`.

```rust
pub(crate) static ROVER_ENGINE: Mutex<Option<Arc<tokio::sync::Mutex<RoverDecoder>>>> = ...;
```

Dùng `tokio::sync::Mutex` (không phải `RwLock` như `AsrEngine`) vì `RoverDecoder::decode` cần
`&mut self` — khác với `AsrEngine` vốn dùng `Arc<RwLock<Option<OfflineRecognizer>>>` vì sherpa-onnx's
`OfflineRecognizer` cho phép tạo nhiều `stream` mà không cần mutable access.

Commands:
- `rover_init` — khởi tạo static, tương tự `asr_init`.
- `rover_load_model(family_a, variant_a, family_b, variant_b)` — resolve đường dẫn file mỗi phía
  bằng `asr_engine::model_family::ModelFamily::variant_subdir`/`model_files` (dùng lại, không viết
  lại), gọi `RoverDecoder::load`.
- `rover_is_model_loaded`, `rover_get_current_config`.
- `rover_validate_model_ready` — đọc config đã lưu (`roverFamilyB`/`roverVariantB` + `model`/
  `asrVariant` hiện có làm phía A), kiểm tra file cả 2 phía tồn tại, load nếu chưa load hoặc cặp
  family/variant đã đổi.

`resolve_models_base_dir` trong `asr_engine/commands.rs` hiện là hàm private — đổi thành
`pub(crate)` để `rover_engine::commands` dùng lại, tránh 2 nơi tính đường dẫn base dir models khác
nhau có thể lệch nhau.

### 3. `RoverProvider` — cầu nối vào `TranscriptionProvider`

```rust
pub struct RoverProvider {
    decoder: Arc<tokio::sync::Mutex<RoverDecoder>>,
    family_a_id: String,
    family_b_id: String,
}
```

`TranscriptionProvider::transcribe(&self, ...)` yêu cầu `&self` (bất biến), nhưng
`RoverDecoder::decode` cần `&mut self` — giải quyết bằng khóa `tokio::sync::Mutex` bên trong
`RoverProvider::transcribe`. `RoverDecoder::decode` tự nó là hàm đồng bộ/chặn (spawn 2 OS thread qua
`std::thread::scope`) — bọc trong `tokio::task::block_in_place`, đúng pattern
`AsrEngine::transcribe_audio` đã dùng.

`TranscriptResult.confidence` (hiện luôn `None` cho ASR local) được set = trung bình
`word.confidence` trên các từ merge được — dữ liệu đã tính sẵn ở Phase A/B, tận dụng luôn.

### 4. Ba điểm gọi ASR — dispatch theo `roverEnabled`

| File | Cách gọi hiện tại | Thay đổi |
|---|---|---|
| `audio/transcription/engine.rs` (`get_or_init_transcription_engine`, `validate_transcription_model_ready`) | Luôn qua `AsrProvider` | Đọc config trước; nếu `roverEnabled` → build `RoverProvider`; else giữ nguyên. `worker.rs` (nơi gọi 2 hàm này) **không đổi gì**. |
| `audio/import.rs` (dòng ~512-517, ~584) | Gọi thẳng `asr_engine::commands::asr_init/asr_validate_model_ready/get_engine_arc` rồi `.transcribe_audio()` | Thêm nhánh `if rover_enabled { rover_engine::commands... } else { <code cũ>` }` |
| `audio/retranscription.rs` (dòng ~227-232, ~283) | Giống `import.rs` | Giống `import.rs` |

Không hợp nhất `import.rs`/`retranscription.rs` sang dùng `TranscriptionProvider` trait — đó là sự
khác biệt kiến trúc có từ trước, ngoài phạm vi spec này (xem "Ngoài phạm vi").

### 5. Frontend

`AsrModelManager.tsx`:
- Thêm toggle "Bật ROVER (kết hợp 2 model)".
- Khi bật: hiện 2 hàng chọn family+variant (tái dùng UI hiện có cho mỗi hàng) — "Model A (chính)"
  và "Model B (phụ)". Cả 2 phải `hasFiles = true` mới cho phép Lưu/Áp dụng.
- Khi bật: ẩn "Phương pháp giải mã" + "Số đường giải mã" (`decodingMethod`/`numActivePaths`) — đây
  là tham số riêng của sherpa-onnx, không áp dụng cho decoder tùy chỉnh của ROVER.
- Ghi chú ngắn: "ROVER dùng gấp đôi RAM/CPU so với 1 model — khuyến nghị dùng int8 cho cả 2 phía."
- Disable toàn bộ khi đang ghi âm (`isRecording`), giữ nguyên hành vi hiện tại.

`lib/asr.ts`: thêm `roverEnabled`, `roverFamilyB`, `roverVariantB` vào type config + hàm gọi
`api_save_transcript_config`.

### 6. Mặc định & an toàn

- `roverEnabled = false` mặc định — người dùng hiện tại không bị ảnh hưởng.
- ROVER yêu cầu tải + giữ 2 model trong RAM cùng lúc trong suốt thời gian bật — khác biệt tài nguyên
  rõ rệt so với chế độ 1 model, đã ghi chú trong UI.

## Kiểm thử

### Tự động (Rust)
- Unit test `rover_validate_model_ready`-style logic: đủ 2 bộ file → sẵn sàng; thiếu 1 trong 2 →
  lỗi rõ ràng liệt kê phía nào thiếu.

### Manual (bắt buộc trước merge)
1. Settings → bật ROVER → chọn Model A = ZipFormer 30M int8, Model B = Gipformer 65M int8 → tải cả
   2 (nếu chưa có) → Lưu.
2. Ghi âm ngắn → transcript xuất hiện, không lỗi, không treo UI trong lúc decode (dù chạy 2 model
   song song).
3. Import 1 file audio → transcript qua ROVER.
4. Retranscribe 1 meeting cũ → qua ROVER.
5. Tắt ROVER → chuyển về 1 model bình thường → xác nhận luồng cũ không bị ảnh hưởng.
6. Kiểm tra RAM tăng khi bật ROVER (Task Manager) — xác nhận đúng như ghi chú UI, không phải bug.

## Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| `RoverDecoder::decode` chặn tokio runtime nếu quên `block_in_place` | Code review kỹ điểm này ở cả `RoverProvider` lẫn `import.rs`/`retranscription.rs`; đối chiếu trực tiếp với cách `AsrEngine::transcribe_audio` đã làm |
| `resolve_models_base_dir` lệch giữa `asr_engine` và `rover_engine` nếu duplicate logic | Đổi thành `pub(crate)` và dùng lại, không viết 2 bản |
| Người dùng bật ROVER trên máy yếu, app chậm/treo | Ghi chú rõ trong UI trước khi bật; không tự động bật cho ai |
| `import.rs`/`retranscription.rs` thêm nhánh rover riêng biệt dễ lệch nhau theo thời gian | Cả 2 nhánh code gần như giống hệt nhau — nếu sau này cần sửa, sửa cả 2 nơi cùng lúc (grep `rover_enabled` trong 2 file) |
