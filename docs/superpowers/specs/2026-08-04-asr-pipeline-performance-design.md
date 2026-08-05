# Tối ưu hiệu năng pipeline âm thanh → transcript (tách luồng Live / File)

## Vấn đề

Người dùng đo được app tham khảo (`C:\Users\HP\Desktop\test ASR`, PyQt6 + sherpa-onnx/onnxruntime,
cùng họ model Zipformer RNN-T + CAPU `vibert-capu` với Meetily) xử lý âm thanh thành transcript/markdown
nhanh hơn Meetily khoảng **8-9 lần**. Khảo sát cả 2 codebase (agent Explore, xem lịch sử hội thoại) tìm
ra 2 nhóm nguyên nhân độc lập:

**Luồng ghi âm trực tiếp** (`transcription/worker.rs`):
- `NUM_WORKERS = 1` ([worker.rs:76](../../../frontend/src-tauri/src/audio/transcription/worker.rs#L76))
  — mỗi đoạn VAD xử lý tuần tự tuyệt đối.
- CAPU chạy đồng bộ, chặn ngay trong worker, ngay sau ASR
  ([worker.rs:236-239](../../../frontend/src-tauri/src/audio/transcription/worker.rs#L236-L239) gọi
  [post_asr.rs](../../../frontend/src-tauri/src/audio/post_asr.rs)) — 2 lần nghẽn CPU nối tiếp trên
  cùng 1 thread, mỗi đoạn.
- ASR hardcode `num_threads = 2`
  ([asr_engine/engine.rs:352](../../../frontend/src-tauri/src/asr_engine/engine.rs#L352)) bất kể máy có
  bao nhiêu core, trong khi CAPU mặc định dùng hết core vật lý — 2 session ONNX tranh nhau tài nguyên
  không đồng bộ.
- ROVER (`rnnt_decoder/sessions.rs`) **không set `intra_threads`** cho cả 3 session (encoder/
  decoder/joiner) của mỗi trong 2 decoder chạy song song qua `std::thread::scope` — dễ oversubscribe
  CPU nghiêm trọng hơn cả đường ASR đơn.

**Luồng file** (`import.rs` — import file có sẵn — và `retranscription.rs` — transcribe lại 1 cuộc
họp): cả hai gần như trùng lặp logic 100%, xử lý tuần tự từng đoạn VAD một trong 1 vòng `for`, gọi
`process_asr_text` (CAPU) riêng cho từng đoạn — không tận dụng song song hoá dù đây là batch job không
có ràng buộc real-time.

## Nghiên cứu: app tham khảo làm gì

Đọc trực tiếp `streaming_asr.py`, `streaming_asr_online.py`, `core/asr_engine.py`, `core/config.py`,
`core/hardware_accel.py`, `core/punctuation_restorer_improved.py`, `core/gec_model.py`, `tab_live.py`.

**Live (`tab_live.py` + `streaming_asr_online.py`)**: dùng model Zipformer bản streaming riêng (fp16,
export chunk-64/left-128), giải mã tăng dần qua `sherpa_onnx.OnlineRecognizer` trong lúc mic đang thu —
đến lúc người dùng bấm dừng, gần như toàn bộ audio đã transcribe xong, chỉ còn đuôi ~0.3-1s. Đây là lý
do lớn nhất khiến app "nhanh gần như tức thì" ở chế độ live.

**File (`core/asr_engine.py:2141-2397`)**:
- VAD cắt khoảng lặng, ghép các đoạn có tiếng thành buffer liên tục, chia lại thành khối 30s có overlap
  3s tại điểm im lặng gần nhất, merge lại bằng alignment ở điểm overlap.
- Khi ≥4 khối và ≥4 core vật lý: chia khối chẵn/lẻ cho **2 thread xử lý song song, mỗi thread có
  recognizer + ngân sách thread riêng** — benchmark đo được ~1.7x tổng thời gian.
- Punctuation (`punctuation_restorer_improved.py:38-41`, `gec_model.py:374-399`): xử lý **toàn bộ
  document trong 1 lượt**, chia chunk 56 token/overlap 16, batch qua ONNX theo `mini_batch_size=32` (áp
  dụng batch-dim thật ở tầng ONNX — Meetily **không** làm phần này, xem "Ngoài phạm vi").

**Session tuning chung** (`config.py:182-219`): `intra_op_num_threads` = số core **vật lý** (đã
benchmark, dùng logical/HT làm chậm hơn do oversubscription — cùng phát hiện đã áp dụng cho CAPU trong
spec [2026-08-04-capu-punctuation-settings-design.md](2026-08-04-capu-punctuation-settings-design.md)).

Spec này port 2 kỹ thuật cốt lõi — **tách CAPU khỏi đường nghẽn chính (chạy nền/song song)** và **song
song hoá đường file** — sang kiến trúc Rust hiện có của Meetily, không copy nguyên xi kiến trúc Python
vì khác nhau về threading model (OS thread thật + `RwLock`/`Mutex` vs GIL + multiprocessing-style).

## Ngoài phạm vi

- **GPU acceleration** (CUDA/DirectML) — cả sherpa-onnx lẫn `ort` hiện chỉ build CPU; wiring GPU cần
  test trên nhiều phần cứng, không hợp phạm vi "nhanh, rủi ro thấp" của spec này.
- **Đổi model ASR sang bản streaming thật** (kiểu `OnlineRecognizer` chunk-64 của app tham khảo) — giữ
  nguyên `OfflineRecognizer` hiện tại của Meetily; đổi model là thay đổi kiến trúc lớn hơn nhiều, để
  đánh giá riêng nếu các tối ưu trong spec này chưa đủ.
- **Batch-dim thật cho CAPU** (kiểu `mini_batch_size=32` của app tham khảo — nhiều chuỗi trong 1 lệnh
  ONNX `session.run`) — `CapuEngine::infer_once`
  ([capu_engine.rs:148-219](../../../frontend/src-tauri/src/capu_engine/capu_engine.rs#L148-L219)) hiện
  chỉ hỗ trợ batch size 1. Spec này chỉ **giảm số lần gọi** (gộp nhiều đoạn thành 1 chuỗi dài hơn/lần
  gọi), không sửa `infer_once` để nhận nhiều chuỗi cùng lúc — việc đó cần xây dựng tensor có batch-dim
  và logic pad/unpad, rủi ro cao hơn, để sau nếu cần thêm tốc độ.
- **Nhiều ASR worker song song cho luồng LIVE** — đã cân nhắc và quyết định giữ `NUM_WORKERS = 1` để
  tránh phải thêm cơ chế sắp xếp lại thứ tự khi emit `transcript-update` theo thời gian thực. Chỉ luồng
  FILE (không có ràng buộc live-order) mới song song hoá ASR trong spec này.
- **UI slider cho số luồng ASR** — tự động theo CPU topology, không thêm control trên Settings (khác
  CAPU đã có slider từ spec trước).
- Sửa bug `$MERGE_SPACE` trùng từ ngữ cảnh đã ghi chú sẵn trong
  [capu_engine.rs:268-291](../../../frontend/src-tauri/src/capu_engine/capu_engine.rs#L268-L291) — không
  thuộc phạm vi hiệu năng. Việc gộp lô lớn hơn (mục A.2/B.3 dưới đây) thực ra **giảm** tần suất gặp bug
  này vì ít điểm nối ngữ cảnh hơn trên cùng một lượng audio.
- `retranscription.rs`/`import.rs` giữ nguyên phần đọc file/VAD/lưu DB hiện có — chỉ thay phần vòng lặp
  transcribe-từng-đoạn bằng hàm dùng chung mới (mục B).

## Thiết kế

### Nguyên tắc chung: ngân sách thread theo số đường giải mã đồng thời

Thêm hàm dùng chung mới, ví dụ `asr_engine::thread_budget::asr_thread_budget` (tái dùng
`capu_engine::cpu_topology::detect_cpu_topology()` đã có sẵn, không cần code phát hiện CPU mới):

```rust
pub enum DecodeConcurrency {
    SingleLive,       // luồng live, 1 model ASR
    RoverLive,        // luồng live, ROVER = 2 decoder chạy song song
    SingleFileWorker, // luồng file, 1 trong 2 file-worker song song
    RoverFileWorker,  // luồng file, 1 trong 2 file-worker, mỗi worker tự chạy ROVER (2 decoder)
}

pub fn asr_thread_budget(physical_cores: usize, concurrency: DecodeConcurrency) -> usize {
    use DecodeConcurrency::*;
    match concurrency {
        SingleLive => physical_cores.clamp(2, 4),
        RoverLive => (physical_cores.clamp(2, 4) / 2).max(1),
        SingleFileWorker => (physical_cores / 2).max(1),
        RoverFileWorker => (physical_cores / 4).max(1),
    }
}
```

Lý do các con số: `SingleLive` chừa chỗ cho CAPU background stage (mục A) chạy nền cùng lúc — không lấy
hết core như trước (2 hardcode) nhưng cũng không lấy hết máy. `RoverLive`/`RoverFileWorker` chia đôi vì
2 decoder chạy thật sự song song (`std::thread::scope`), không phải tuần tự.

### A. Luồng Live (`transcription/worker.rs`)

```
VAD segment ──▶ Stage 1: ASR worker (giữ 1 worker, num_threads theo asr_thread_budget)
                   │ ITN → emit transcript-update NGAY (text thô, UI live không đổi)
                   │ push PendingSegment vào CAPU_QUEUE (mpsc channel mới)
                   ▼
             Stage 2: CAPU background task (task riêng, chạy song song lúc ghi âm)
                   │ CapuBatcher gom tới ~200 từ HOẶC quá ~5s chưa flush
                   │ → 1 lần gọi restore_punctuation() cho cả lô
                   ▼
             Thay N đoạn thô trong lô bằng 1 đoạn transcript đã có dấu câu
```

#### A.1 — `CapuBatcher`: module dùng chung mới (live lẫn file đều gọi)

Đặt tại `frontend/src-tauri/src/capu_engine/batch.rs`, không phụ thuộc Tauri/tokio (chỉ logic thuần) để
cả Stage 2 (async, có debounce) lẫn luồng file (đồng bộ, không debounce) đều dùng được:

```rust
pub struct PendingSegment {
    pub source_ids: Vec<u64>,   // sequence_id (live) hoặc index (file) của (các) đoạn VAD gốc
    pub raw_text: String,       // text sau ITN, trước CAPU
    pub audio_start_time: f64,
    pub audio_end_time: f64,
}

pub struct FinalizedSegment {
    pub text: String,               // output CAPU cho cả lô
    pub audio_start_time: f64,      // = start của PendingSegment đầu lô
    pub audio_end_time: f64,        // = end của PendingSegment cuối lô
    pub source_ids: Vec<u64>,       // hợp tất cả source_ids trong lô — để bên gọi biết thay thế gì
}

pub struct CapuBatcher {
    pending: Vec<PendingSegment>,
    pending_word_count: usize,
    trailing_context: Vec<String>,
}

impl CapuBatcher {
    pub fn new() -> Self { ... }

    /// Thêm 1 đoạn thô. Nếu tổng số từ đang chờ vượt `word_budget`, tự flush và trả về lô đã xử lý.
    pub fn push(
        &mut self,
        engine: &mut CapuEngine,
        seg: PendingSegment,
        word_budget: usize,
    ) -> Result<Option<FinalizedSegment>> { ... }

    /// Ép xử lý ngay phần đang chờ (dùng lúc debounce timeout, hoặc cuối input). None nếu rỗng.
    pub fn flush(&mut self, engine: &mut CapuEngine) -> Result<Option<FinalizedSegment>> { ... }
}
```

Bên trong `push`/`flush`, khi đủ điều kiện flush: nối `raw_text` của các `PendingSegment` đang chờ bằng
khoảng trắng, gọi **nguyên vẹn** `engine.restore_punctuation(&self.trailing_context, &joined_text)` đã
có sẵn ([capu_engine.rs:292-345](../../../frontend/src-tauri/src/capu_engine/capu_engine.rs#L292-L345))
— không sửa hàm này. `word_budget` đề xuất mặc định **200 từ** (an toàn dưới `CAPU_MAX_SEQ_LEN = 512`
token kể cả sau khi tách subword, còn đủ margin so với mức app tham khảo test ở chunk 56 token).

#### A.2 — Stage 2: task nền trong `transcription/worker.rs`

Task `tokio::spawn` riêng, nhận từ channel `mpsc::unbounded_channel::<PendingSegment>()` (đặt tên
`CAPU_QUEUE` trong code), vòng lặp dùng `tokio::select!` giữa "có message mới" và "hết hạn debounce
timer 5s kể từ item đang chờ đầu tiên chưa flush" — cả 2 nhánh đều gọi `CapuBatcher::push`/`flush` (chạy
trong `tokio::task::block_in_place` vì đây vẫn là inference ONNX đồng bộ). Khi có `FinalizedSegment` trả
về: gọi hàm mới trong `recording_manager`/`recording_saver` (mục A.3) để thay thế các đoạn thô đã lưu,
KHÔNG emit lại `transcript-update` cho UI (đúng yêu cầu "chỉ cần đúng khi xong họp" — UI live tiếp tục
hiện bản thô từ Stage 1, transcript lưu trữ được sửa ngầm).

Task nhận `Arc<Mutex<CapuEngine>>` giống cách `post_asr.rs` đang lấy qua
`capu_engine::commands::get_engine_arc()` — không đổi cách quản lý vòng đời engine hiện có.

#### A.3 — Cập nhật transcript đã lưu

Thêm hàm trong `recording_saver.rs`/`recording_manager.rs`, ví dụ
`replace_transcript_segments(source_ids: &[u64], finalized: FinalizedSegment)`: xoá các segment đã lưu
có `sequence_id` nằm trong `source_ids`, chèn 1 segment mới ở đúng vị trí (giữ thứ tự thời gian) với
text/mốc thời gian từ `finalized`. Đây là thay đổi **cấu trúc lưu trữ**: transcript cuối cùng sẽ có ít
đoạn hơn nhưng dài hơn (dạng đoạn văn) so với hiện tại (1 đoạn/1 VAD segment) — đã xác nhận với người
dùng đây là hành vi mong muốn.

#### A.4 — `stop_recording` đợi Stage 2 flush nốt

Trong [recording_commands.rs](../../../frontend/src-tauri/src/audio/recording_commands.rs), ngay sau
đoạn đợi `transcription_task` (Stage 1) hoàn tất (~dòng 719-773 hiện tại), thêm bước đợi Stage 2: đóng
`CAPU_QUEUE` (drop sender), await handle của Stage 2 task — task tự `flush()` phần còn dở trước khi
thoát vòng lặp. Vì Stage 2 đã chạy song song suốt buổi ghi, phần tồn đọng lúc này thường rất nhỏ (tối đa
1 lô ~200 từ hoặc ~5s cuối) — khác hẳn việc phải chờ toàn bộ CAPU cho cả cuộc họp sau khi dừng.

#### A.5 — ASR thread tuning

[asr_engine/engine.rs:352](../../../frontend/src-tauri/src/asr_engine/engine.rs#L352): `load_model`
nhận thêm tham số `num_threads: usize` (giống cách `CapuEngine::load` đã nhận `threads` ở spec trước),
thay `config.model_config.num_threads = 2` bằng `config.model_config.num_threads = num_threads`. Nơi gọi
`load_model` cho luồng live (trong `asr_engine::commands`) tính giá trị qua
`asr_thread_budget(detect_cpu_topology().0, DecodeConcurrency::SingleLive)`.

#### A.6 — ROVER thread tuning

[rnnt_decoder/sessions.rs](../../../frontend/src-tauri/src/rnnt_decoder/sessions.rs): `load_session`
nhận thêm `threads: usize`, thêm `.with_intra_threads(threads)?` trước `.commit_from_file(...)` (pattern
giống hệt `capu_engine.rs:88-93`) cho cả 3 session (encoder/decoder/joiner). `RnntSessions::load` và
`RnntDecoder::load` ([rnnt_decoder/engine.rs:37-43](../../../frontend/src-tauri/src/rnnt_decoder/engine.rs#L37-L43))
nhận thêm cùng tham số, truyền xuống. `RoverDecoder::load`
([rover_engine/engine.rs:20-30](../../../frontend/src-tauri/src/rover_engine/engine.rs#L20-L30)) nhận
`threads_per_decoder: usize`, truyền cho cả `decoder_a`/`decoder_b`. Nơi khởi tạo ROVER cho luồng live
(`rover_engine::commands::rover_init`) tính qua
`asr_thread_budget(detect_cpu_topology().0, DecodeConcurrency::RoverLive)`.

### B. Luồng File (`import.rs` + `retranscription.rs`)

#### B.1 — Hàm batch-transcribe dùng chung mới

Thêm module mới, ví dụ `frontend/src-tauri/src/audio/batch_transcribe.rs`:

```rust
pub struct BatchTranscribeConfig {
    pub family: ModelFamily,
    pub variant: ModelVariant,
    pub decoding_method: String,
    pub num_active_paths: i32,
    pub rover: Option<(...)>,   // Some nếu ROVER đang bật, chứa đường dẫn 2 family
    pub models_base_dir: PathBuf,
}

pub async fn batch_transcribe(
    segments: Vec<SpeechSegment>,
    config: BatchTranscribeConfig,
    progress: impl Fn(usize, usize) + Send + 'static, // (đã xong, tổng số) để emit progress
) -> Result<Vec<TranscriptSegment>> { ... }
```

Cả `import.rs::run_import` và `retranscription.rs::run_retranscription` gọi hàm này ngay sau
`expand_segments_at_silence`, thay cho vòng `for` hiện có của mỗi file; phần đọc/copy file, VAD, lưu DB,
ghi `transcripts.json`/`metadata.json` **giữ nguyên riêng của từng file** như hiện tại.

#### B.2 — 2 ASR worker song song, mỗi worker 1 instance model riêng

Bên trong `batch_transcribe`: nếu `segments.len() >= 4 && detect_cpu_topology().0 >= 4`, chia
`segments` xen kẽ chẵn/lẻ cho 2 nhánh; mỗi nhánh **load một `AsrEngine::new()` độc lập** (gọi
`set_models_directory` + `load_model(family, variant, decoding_method, num_active_paths, num_threads)`
với `num_threads = asr_thread_budget(physical_cores, DecodeConcurrency::SingleFileWorker)`), chạy trên 1
`tokio::task::spawn_blocking` riêng. Lý do dùng model riêng thay vì chia sẻ 1
`Arc<AsrEngine>` giữa 2 task: mirror đúng cách app tham khảo đã kiểm chứng trong production, tránh phải
tự xác minh sherpa-onnx-rs có an toàn cho gọi `decode()` đồng thời từ 2 thread trên cùng 1
`OfflineRecognizer` hay không — đánh đổi thêm ~1 bản sao model int8 (vài chục MB, chỉ tồn tại trong lúc
batch job chạy) để lấy sự an toàn đã được chứng minh. Nếu không đủ điều kiện song song (ít đoạn/máy yếu)
→ rơi về đúng 1 worker tuần tự như hiện tại, dùng `AsrEngine` singleton toàn cục sẵn có (không load thêm
bản sao).

Với ROVER bật: mỗi nhánh trong 2 nhánh trên tự load 1 `RoverDecoder` riêng (2 decoder/nhánh × 2 nhánh =
4 decoder đồng thời), thread mỗi decoder = `asr_thread_budget(physical_cores,
DecodeConcurrency::RoverFileWorker)`.

Sau khi cả 2 nhánh xong, merge kết quả theo **chỉ số gốc trong `segments`** (không phải theo thời điểm
hoàn thành) trước khi sang bước CAPU — đơn giản vì đây là danh sách tĩnh, không phải luồng phát trực
tiếp cần giữ thứ tự lúc emit.

#### B.3 — Batch CAPU cho toàn bộ transcript

Sau khi có danh sách text thô đã merge đúng thứ tự: lặp qua bằng **cùng `CapuBatcher`** ở mục A.1 (gọi
`push` cho từng đoạn theo thứ tự, `flush` cuối cùng khi hết danh sách) — không cần debounce timer vì
đây là danh sách hoàn chỉnh, chỉ cần ngưỡng từ. Kết quả là danh sách `FinalizedSegment` — chuyển thẳng
thành `Vec<TranscriptSegment>` để lưu DB, thay cho việc gọi `process_asr_text` riêng lẻ từng đoạn trong
vòng `for` như hiện tại ở cả `import.rs` và `retranscription.rs`.

#### B.4 — Cập nhật `import.rs`/`retranscription.rs`

Cả 2 file thay đoạn code từ "Best-effort CAPU init" tới hết vòng `for` xử lý transcribe (import.rs:568-640,
retranscription.rs:279-338) bằng 1 lời gọi `batch_transcribe(...)`. Phần trước đó (decode/resample/VAD/
`expand_segments_at_silence`) và sau đó (tạo `TranscriptSegment`, lưu DB, ghi file JSON) giữ nguyên.

## Kiểm thử

### Tự động (Rust)
- `CapuBatcher`: unit test gom lô theo ngưỡng từ (đủ ngưỡng → flush đúng 1 lần với đúng nội dung nối),
  test flush cuối khi chưa đủ ngưỡng, test `source_ids`/mốc thời gian của `FinalizedSegment` đúng =
  start đoạn đầu / end đoạn cuối trong lô.
- `asr_thread_budget`: test từng nhánh `DecodeConcurrency` tại các mốc core biên (1, 2, 4, 8, 16) —
  luôn `>= 1`, các nhánh Rover luôn `<=` nhánh Single tương ứng.
- Test merge kết quả 2 file-worker song song đúng thứ tự gốc (dùng dữ liệu giả, không cần model thật).
- Test fallback: `CapuBatcher::push`/`flush` khi `restore_punctuation` lỗi — xác nhận trả lỗi rõ ràng,
  bên gọi (Stage 2 / `batch_transcribe`) fallback về text ITN thô cho lô đó, không panic, không chặn các
  lô tiếp theo.

### Manual (bắt buộc trước merge)
1. Ghi âm 1 đoạn ngắn (~1 phút) và 1 đoạn dài (~15-20 phút) — xác nhận UI live vẫn hiện text ngay khi
   nói (chưa có dấu câu), transcript sau khi dừng có dấu câu đúng, và số đoạn transcript giảm/gộp lại
   như thiết kế.
2. Đo thời gian từ lúc bấm dừng tới lúc transcript "final" sẵn sàng, so với trước khi sửa (kỳ vọng giảm
   rõ rệt, phần lớn CAPU đã chạy nền trong lúc ghi).
3. Bật ROVER, lặp lại bước 1-2 — xác nhận không treo máy do oversubscribe thread, thời gian vẫn cải
   thiện so với trước.
4. Import 1 file audio dài (≥10 phút) trên máy ≥4 core — xác nhận 2 worker chạy song song (log), kết
   quả transcript đúng thứ tự thời gian, so thời gian xử lý trước/sau.
5. Retranscription 1 cuộc họp cũ — xác nhận hành vi giống import (dùng chung `batch_transcribe`).
6. Máy/file không đủ điều kiện song song (< 4 đoạn hoặc < 4 core) — xác nhận rơi về 1 worker tuần tự,
   không lỗi.

## Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| Gộp nhiều đoạn VAD thành 1 `FinalizedSegment` làm mất độ chi tiết click-to-seek theo câu | Đã xác nhận với người dùng đây là đánh đổi chấp nhận được; mốc thời gian lô vẫn đúng (đầu lô → cuối lô), chỉ thô hơn, không sai |
| CAPU Stage 2 (nền) và Stage 1 (ASR) cùng tranh CPU với CAPU vốn đã mặc định dùng hết core vật lý | Nêu rõ là đánh đổi chấp nhận được (mục "Nguyên tắc chung"); có thể tinh chỉnh `word_budget`/ngân sách ASR sau khi đo thực tế nếu cần |
| 2 file-worker mỗi worker load riêng 1 model ASR — tốn thêm bộ nhớ tạm thời trong lúc batch job chạy | Model int8 30-65M tham số chỉ vài chục MB; chỉ tồn tại trong thời gian xử lý file, giải phóng ngay sau (theo đúng cơ chế `unload_engine_after_batch` đã có) |
| `stop_recording` phải đợi Stage 2 flush — nếu Stage 2 task panic/treo, `stop_recording` có thể chờ vô hạn | Áp dụng cùng timeout 10 phút đã có cho Stage 1 ([recording_commands.rs:752-753](../../../frontend/src-tauri/src/audio/recording_commands.rs#L752-L753)) cho bước đợi Stage 2 |
| Giả định sherpa-onnx-rs không an toàn cho gọi `decode()` đồng thời trên 1 recognizer có thể sai (dẫn đến tốn tài nguyên load 2 model không cần thiết) | Chấp nhận đánh đổi để ưu tiên an toàn/rủi ro thấp theo yêu cầu; có thể xác minh và đơn giản hoá (dùng chung 1 `Arc<AsrEngine>` với `RwLock` đọc đồng thời) ở lần sau nếu cần |
| Đổi `AsrEngine::load_model`/`RnntSessions::load`/`RnntDecoder::load`/`RoverDecoder::load` thêm tham số — vỡ các lời gọi hiện có (test, call site khác) | Cập nhật toàn bộ call site trong cùng lần sửa (đã liệt kê ở mục A.5/A.6); `cargo check` sẽ báo lỗi biên dịch nếu bỏ sót nơi nào |
