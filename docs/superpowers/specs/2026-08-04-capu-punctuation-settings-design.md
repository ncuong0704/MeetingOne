# Cài đặt CAPU: Số luồng CPU, Mức độ thêm dấu, Mức độ tự viết hoa

## Vấn đề

Spec trước ([2026-07-31-vietnamese-capu-punctuation-design.md](2026-07-31-vietnamese-capu-punctuation-design.md))
đã port model ViBERT-CAPU (GECToR/seq2labels) sang Rust thuần và cố tình để ngoài phạm vi phần UI:
"Không thêm UI chọn bật/tắt tính năng trong spec này". Tính năng hiện đã chạy end-to-end
(`post_asr::process_asr_text` gọi `CapuEngine::restore_punctuation` cho cả live lẫn
retranscription), nhưng:

- `CapuEngine::infer_once`
  ([capu_engine.rs:39-108](../../../frontend/src-tauri/src/capu_engine/capu_engine.rs)) chỉ argmax
  logits thô — **không có cơ chế điều chỉnh độ mạnh/yếu** của việc thêm dấu câu hay viết hoa.
- `CapuEngine::load` build ONNX session bằng `Session::builder()` mặc định — **không cấu hình số
  luồng CPU**.
- Không có setting nào trong 3 mục này được lưu hay hiển thị trên UI.

App tham khảo (`C:\Users\HP\Desktop\test ASR`, PyQt6 + sherpa-onnx + cùng model `vibert-capu`) đã có
sẵn đúng 3 setting này và đã được người dùng thực tế kiểm chứng. Spec này port hành vi đó sang Rust.

## Nghiên cứu: app tham khảo làm gì

Đọc trực tiếp `tab_file.py`, `core/gec_model.py`, `core/config.py`, `core/punctuation_restorer_improved.py`:

### 1. Số luồng CPU (`tab_file.py:523-553`)

```python
from core.config import ALLOWED_THREADS, DEFAULT_THREADS  # = physical cores, physical cores
self.slider_threads = QSlider(...)
self.slider_threads.setRange(1, ALLOWED_THREADS)
self.slider_threads.setValue(DEFAULT_THREADS)
```

`ALLOWED_THREADS`/`DEFAULT_THREADS` đến từ `_detect_cpu_topology()`
(`core/config.py:110-179`): dùng `psutil.cpu_count(logical=False)` cho physical core, có nhánh
riêng phát hiện VM (`wmic computersystem get model` trên Windows) vì trên VM
`physical == logical` (không có HT thật). Giá trị này set `intra_op_num_threads` cho ONNX Runtime
qua `compute_ort_threads()` (`core/config.py:182-219`) — có kèm bench thực đo (6C/12T Intel) cho
thấy dùng đúng physical core là tối ưu cho encoder ASR *và* cho punctuation model, dùng full logical
(HT) làm **chậm hơn** do over-subscription.

### 2. Mức độ thêm dấu (`tab_file.py:555-585`, giá trị dùng ở `tab_file.py:1545-1559`)

Slider `1..10`, mặc định `7`. `get_config()`:

```python
confidence = 0.5 - (slider_val - 1) * (1.3 / 9)
bypass_restorer = (slider_val == 1)
```

`confidence` được cộng vào xác suất (sau softmax) của nhãn `$KEEP` trước khi lấy argmax
(`core/gec_model.py:499-500`: `all_class_probs[:, :, self.noop_index] += self.confidence`).
Slider càng cao → `confidence` càng âm → xác suất "không làm gì" càng bị đè xuống → model thêm dấu
câu tích cực hơn. Ở `slider_val == 1`, ứng dụng **bỏ qua hoàn toàn** việc gọi model (giữ nguyên text
thô) thay vì chạy model với confidence dương rất lớn.

Nhãn hiển thị cạnh slider lấy từ dict cố định, không nội suy giữa các mốc
(`tab_file.py:1066-1069`):
```python
labels = {1: "Rất ít", 3: "Ít", 5: "Vừa", 7: "Nhiều", 10: "Rất nhiều"}
label = labels.get(value, str(value))  # giá trị không có trong dict → hiện số thô
```

### 3. Mức độ tự viết hoa (`tab_file.py:587-617`)

Slider `1..10`, mặc định `3`. `get_config()`:

```python
case_confidence = -1.5 + (case_val - 1) * (2.0 / 9)
```

Cộng vào xác suất của **mọi** nhãn bắt đầu bằng `$TRANSFORM_CASE_` (`core/gec_model.py:502-504`),
với `case_indices` được quét 1 lần từ vocab lúc load model (`core/gec_model.py:108-112`). Cùng dict
nhãn hiển thị `{1: "Rất ít", 3: "Ít", 5: "Vừa", 7: "Nhiều", 10: "Rất nhiều"}`.

### Khác biệt kiến trúc quan trọng — vì sao không copy y nguyên

App tham khảo tạo **session ONNX mới cho mỗi lần xử lý file** (`ImprovedPunctuationRestorer.__init__`
nhận `confidence`/`case_confidence`/`prefer_int8` làm tham số constructor). Ở Meetily, `CapuEngine`
là **singleton sống suốt vòng đời app** (`CAPU_ENGINE: Mutex<Option<Arc<Mutex<CapuEngine>>>>` trong
[commands.rs](../../../frontend/src-tauri/src/capu_engine/commands.rs)), load 1 lần lúc khởi động.
Hệ quả:
- `confidence`/`case_confidence` (bias tính từ 2 slider mức độ) **có thể set trực tiếp lên engine đang
  chạy**, không cần rebuild session — áp dụng từ lần inference tiếp theo.
- Số luồng CPU **là tham số của `Session::builder()`**, chỉ có hiệu lực lúc tạo session → đổi giá trị
  này bắt buộc phải rebuild (unload + load lại) `CapuEngine`.

## Ngoài phạm vi

- Không đổi số luồng của `asr_engine` (đã hardcode `num_threads = 2` ở
  [engine.rs:352](../../../frontend/src-tauri/src/asr_engine/engine.rs)) — app tham khảo dùng 1
  slider chung cho cả ASR + CAPU + diarization vì kiến trúc per-job của nó cho phép; Meetily tách
  CAPU thành singleton riêng nên setting này chỉ áp dụng cho CAPU. Đổi số luồng ASR là việc khác, để
  spec riêng nếu cần.
- Không thêm setting `Live` riêng khác `File` như app tham khảo (`[FileSettings]`/`[LiveSettings]`
  trong `config.ini`) — `post_asr::process_asr_text` đã dùng chung 1 `CapuEngine` cho mọi đường
  (live, retranscription, import), nên chỉ cần 1 bộ setting áp dụng toàn app.
- Không thêm cơ chế "tối ưu tự động" (auto-tune theo benchmark phần cứng như comment bench trong
  `compute_ort_threads`) — chỉ port slider thủ công, người dùng tự chọn.
- Không đổi ngưỡng `detect_logits`/gate probability đã có sẵn trong `infer_once` (nếu có) — spec này
  chỉ thêm bias theo 2 slider mức độ, không đụng tới cơ chế gate khác của GECToR.

## Thiết kế

### 1. `CapuEngine` — bias theo mức độ (Rust: `capu_engine.rs`)

`infer_once` hiện lấy `max_by` trực tiếp trên `logits_data` thô (dòng 90-95). Thêm bước softmax
trước khi so sánh, và cộng bias đúng công thức app tham khảo:

```rust
pub struct CapuEngine {
    session: Session,
    tokenizer: CapuTokenizer,
    labels: Vec<Action>,
    case_label_indices: Vec<usize>,   // quét 1 lần lúc load(), các Action::TransformCase*
    punctuation_confidence: f32,      // mặc định ứng với level=7
    case_confidence: f32,             // mặc định ứng với level=3
}

impl CapuEngine {
    pub fn set_punctuation_level(&mut self, level: u8) {
        let level = level.clamp(1, 10) as f32;
        self.punctuation_confidence = 0.5 - (level - 1.0) * (1.3 / 9.0);
    }
    pub fn set_case_level(&mut self, level: u8) {
        let level = level.clamp(1, 10) as f32;
        self.case_confidence = -1.5 + (level - 1.0) * (2.0 / 9.0);
    }
}
```

Trong `infer_once`, sau khi có `row_logits` cho mỗi từ: softmax → cộng `punctuation_confidence` vào
xác suất của `Action::Keep` → cộng `case_confidence` vào xác suất của mọi index trong
`case_label_indices` → `max_by` trên mảng đã cộng bias (thay vì trên logits thô).
`case_label_indices` xác định bằng cách so `self.labels[i]` với các biến thể case đã có trong
`vocabulary.rs` (`TransformCaseCapital`, `TransformCaseUpper`, `TransformCaseLower`,
`TransformCaseCapital1`, `TransformCaseUpperMinus1` — xem enum `Action` hiện có, không đoán tên mới).

### 2. Bypass ở mức thấp nhất (`post_asr.rs`)

`process_asr_text` nhận thêm tham số mức độ (đọc từ state cache, xem mục 5), kiểm tra trước khi gọi
engine:

```rust
pub fn process_asr_text(raw: &str, capu_trailing: &mut Vec<String>) -> String {
    let lowered = raw.to_lowercase();
    let after_itn = crate::itn_engine::engine::inverse_normalize_or_pass(&lowered);

    if current_punctuation_level() == 1 {
        return after_itn; // giống hệt bypass_restorer của app tham khảo
    }
    match crate::capu_engine::commands::get_engine_arc() { /* như cũ */ }
}
```

### 3. Số luồng CPU lúc load (`capu_engine.rs` + `commands.rs`)

`CapuEngine::load` nhận thêm `threads: usize`:

```rust
let session = Session::builder()?
    .with_intra_threads(threads)?
    .commit_from_file(model_path_str)?;
```

(API `with_intra_threads` xác nhận có thật trong `ort 2.0.0-rc.10`, xem
`session/builder/impl_options.rs:51` của crate đã vendor.)

### 4. Phát hiện physical core (mới: hàm trong `capu_engine` hoặc `hardware_detector.rs`)

`sysinfo = "0.32"` đã có trong `Cargo.toml` nhưng chưa dùng ở đâu trong `src/` — không cần thêm
dependency:

```rust
pub fn detect_cpu_topology() -> (usize, usize) {
    let mut sys = sysinfo::System::new();
    sys.refresh_cpu_all();
    let logical = sys.cpus().len().max(1);
    let physical = sysinfo::System::physical_core_count().unwrap_or((logical / 2).max(1));
    (physical, logical)
}
```

`sysinfo::System::physical_core_count()` đọc topology thật của OS (Windows: qua
`GetLogicalProcessorInformation`), nên tự động đúng trên VM (vCPU thường báo physical == logical) mà
không cần tự viết heuristic `wmic model` như app tham khảo.

Tauri command mới `capu_get_cpu_topology` trả `{ physicalCores, logicalThreads }` để frontend set
`max`/mặc định cho slider mà không hardcode.

### 5. Database & apply-on-save orchestration

Thêm 3 cột vào `transcript_settings` (cùng bảng đang chứa `roverEnabled`, `hotwords`, ...):

```sql
ALTER TABLE transcript_settings ADD COLUMN capuCpuThreads INTEGER;
ALTER TABLE transcript_settings ADD COLUMN capuPunctuationLevel INTEGER;
ALTER TABLE transcript_settings ADD COLUMN capuCaseLevel INTEGER;
```

- `capuCpuThreads` NULL = "auto" → resolve bằng `detect_cpu_topology().0` (physical cores) khi đọc,
  giống cách `max_segment_seconds` NULL fallback về `DEFAULT_MAX_SEGMENT_SECONDS` hiện tại
  ([setting.rs:217-222](../../../frontend/src-tauri/src/database/repositories/setting.rs)).
- `capuPunctuationLevel` mặc định `7`, `capuCaseLevel` mặc định `3` — khớp app tham khảo.

Thread qua `TranscriptSetting` (model), `SettingsRepository::{get,save}_transcript_config`,
`TranscriptConfig` (`api.rs`), `api_get_transcript_config`/`api_save_transcript_config` — thêm 3
tham số optional theo đúng pattern đã có cho `roverEnabled`/`hotwords`
([api.rs:445-459](../../../frontend/src-tauri/src/api/api.rs)).

Command mới `capu_apply_settings(threads, punctuation_level, case_level)`:
1. Luôn gọi `engine.set_punctuation_level(...)` + `engine.set_case_level(...)` trên engine đang chạy
   (nếu đã load) — không cần rebuild, có hiệu lực ngay.
2. Chỉ rebuild session (unload `CAPU_ENGINE`, gọi lại `CapuEngine::load` với `threads` mới) **nếu**
   `threads` khác số luồng đang dùng (lưu số luồng hiện tại làm field/state, ví dụ
   `CapuEngine.loaded_threads: usize`, so sánh trước khi rebuild). Nếu rebuild thất bại (ví dụ file
   model bị xoá) → giữ nguyên engine cũ đang chạy, trả lỗi cho frontend — không phá engine đang hoạt
   động tốt.

Frontend gọi command này ngay sau `api_save_transcript_config` trong `handleSave()`, cùng 1 lần bấm
"Lưu cấu hình" — giống cách `AsrModelManager.tsx` hiện đã gọi `AsrAPI.loadModel(...)` ngay sau khi
lưu config ASR ([AsrModelManager.tsx:320-327](../../../frontend/src/components/AsrModelManager.tsx)).

### 6. Frontend

`CapuAPI` mới trong [lib/asr.ts](../../../frontend/src/lib/asr.ts) (cùng file với `AsrAPI`/`RoverAPI`
vì dùng chung command `api_save_transcript_config`):

```ts
export const CapuAPI = {
  getCpuTopology: (): Promise<{ physicalCores: number; logicalThreads: number }> =>
    invoke('capu_get_cpu_topology'),
  applySettings: (threads: number, punctuationLevel: number, caseLevel: number): Promise<void> =>
    invoke('capu_apply_settings', { threads, punctuationLevel, caseLevel }),
};
```

`AsrModelManager.tsx` thêm state (`capuThreads`, `capuPunctuationLevel`, `capuCaseLevel`,
`physicalCores`), load trong `loadSavedConfig()` hiện có, `physicalCores` fetch 1 lần lúc mount qua
`CapuAPI.getCpuTopology()`. Thêm section mới sau khối "Từ khóa ưu tiên" (hotwords), trước nút Lưu,
theo đúng pattern slider đã dùng cho `numActivePaths`/`maxSegmentSeconds`
([AsrModelManager.tsx:556-610](../../../frontend/src/components/AsrModelManager.tsx)): label + badge
giá trị dạng mono + `<input type="range">` `accent-blue-500` + caption min/max.

Nhãn mức độ hiển thị cạnh mỗi slider mức độ dùng đúng bảng tra cứu **theo giá trị chính xác** như app
tham khảo (không nội suy các mốc ở giữa):

```ts
const LEVEL_LABELS: Record<number, string> = { 1: 'Rất ít', 3: 'Ít', 5: 'Vừa', 7: 'Nhiều', 10: 'Rất nhiều' };
const levelLabel = (v: number) => LEVEL_LABELS[v] ?? String(v);
```

3 control mới:
- **Số luồng CPU** — range `1..physicalCores`, mặc định = `physicalCores`. Ghi chú: đổi giá trị này
  sẽ reload lại engine CAPU khi lưu (mất khoảng 1-2s).
- **Mức độ thêm dấu** — range `1..10`, mặc định `7`. Ghi chú: mức 1 tắt hoàn toàn việc thêm dấu câu.
- **Mức độ tự viết hoa** — range `1..10`, mặc định `3`.

`handleSave()` thêm 3 field vào lời gọi `invoke('api_save_transcript_config', ...)` hiện có, sau đó
gọi `CapuAPI.applySettings(...)`. Lỗi từ `applySettings` (ví dụ rebuild thất bại) hiển thị qua
`saveMessage` như các lỗi lưu khác đã có.

## Kiểm thử

### Tự động (Rust)
- Unit test `set_punctuation_level`/`set_case_level`: kiểm tra công thức đúng tại các mốc biên
  (level=1, level=10) và mốc mặc định (7, 3) khớp giá trị đã tính tay từ công thức app tham khảo.
- Unit test bias trong `infer_once` bằng fixture logits giả (không cần model ONNX thật): dựng mảng
  logits cố định, kiểm tra bias làm đổi lựa chọn argmax đúng như kỳ vọng ở level cực đoan (1 và 10).
- Unit test `detect_cpu_topology()`: chỉ assert `physical >= 1 && physical <= logical` (không assert
  giá trị cụ thể vì phụ thuộc máy chạy CI).

### Manual (bắt buộc trước merge)
1. Kéo "Mức độ thêm dấu" xuống 1, lưu, ghi âm một câu tiếng Việt → xác nhận transcript ra hoàn toàn
   không có dấu câu (bypass hoạt động).
2. Kéo lên 10, lưu, ghi âm cùng câu → xác nhận thêm dấu câu tích cực hơn rõ rệt so với mức mặc định 7.
3. Test tương tự cho "Mức độ tự viết hoa" ở 1 và 10 — so sánh tần suất viết hoa.
4. Đổi "Số luồng CPU" sang giá trị khác mặc định, lưu → xác nhận app không treo/crash trong lúc
   reload, và CAPU vẫn hoạt động đúng ngay sau đó (không cần khởi động lại app).
5. Đổi số luồng CPU **trong lúc đang ghi âm** (nếu UI cho phép) → xác nhận không làm gãy phiên ghi âm
   đang chạy, hoặc UI khoá control này khi `isRecording` giống các control ASR khác đã làm.
6. Retranscription (re-transcribe 1 meeting cũ) dùng đúng setting hiện tại — xác nhận mức độ đã lưu
   ảnh hưởng cả đường retranscription, không chỉ live.

## Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| Thêm bước softmax vào `infer_once` làm đổi kết quả decode ở mức mặc định so với hành vi argmax-trên-logits-thô hiện tại (dù không có bias, softmax không đổi thứ hạng argmax nên về lý thuyết kết quả giữ nguyên khi bias=0) | Test hồi quy: so kết quả `restore_punctuation` trên vài câu mẫu trước/sau khi thêm softmax với level mặc định (7/3) — phải giống hệt bias=0 case; nếu không giống, có bug ở bước cộng bias, không phải ở softmax |
| Rebuild session (đổi số luồng CPU) xảy ra đúng lúc một request inference khác đang chạy trên `Arc<Mutex<CapuEngine>>` | `Mutex` đã serialize truy cập — rebuild chỉ là swap nội dung bên trong lock, request đang chờ lock sẽ thấy engine mới ngay khi lấy được lock; không cần cơ chế đồng bộ thêm |
| `case_label_indices` tính sai nếu tên biến thể `Action::TransformCase*` trong `vocabulary.rs` không khớp giả định lúc viết spec | Đọc trực tiếp enum `Action` hiện có trong `vocabulary.rs` lúc code, không đoán tên field |
| Người dùng đặt số luồng CPU cao hơn physical core thật (nếu FE gửi giá trị ngoài dải do bug UI) | Clamp `threads` về `1..=physicalCores` ở tầng Rust command `capu_apply_settings`, độc lập với giới hạn slider ở FE — không tin dữ liệu từ frontend |
| Migration thêm cột cho DB đã có dữ liệu người dùng cũ (không có 3 cột mới) | Dùng `ALTER TABLE ... ADD COLUMN` (không phải tạo lại bảng) — cột mới NULL-able, `get_transcript_config` đọc NULL → áp mặc định, không cần backfill |
