# DNSMOS Audio Quality Scoring — Rust thuần, post-recording

## Vấn đề

Người dùng có thể ghi âm bằng mic kém/xa/nhiễu nền mà không biết cho đến khi đọc transcript
thấy sai nhiều. App tham chiếu ([sherpa-vietnamese-asr](https://github.com/welcomyou/sherpa-vietnamese-asr))
dùng model DNSMOS (Microsoft, `sig_bak_ovr.onnx`) để chấm điểm chất lượng audio (1-5) theo 3 trục:
**SIG** (giọng nói), **BAK** (tạp âm nền), **OVRL** (tổng thể) — không cần reference audio sạch để so sánh.

Nguồn: `core/audio_analyzer.py` (class `AudioAnalyzer`), dùng trong `tab_file.py` (chấm điểm sau khi
xử lý file — hiện trong finish dialog) và `tab_live.py` (nút "test mic" độc lập trước khi ghi).

## Quyết định đã chốt (brainstorming)

| Câu hỏi | Lựa chọn |
|---|---|
| Thời điểm chạy | **Post-recording only** — không chạy liên tục trong lúc ghi (đúng như app gốc: `tab_file.py` chấm sau khi xử lý xong, `tab_live.py` chỉ có nút test mic riêng, không chấm liên tục khi đang ghi) |
| Phạm vi | Bao gồm cả nút "Test microphone" on-demand (dùng chung analyzer, input là sample ngắn thay vì file đã lưu) |
| Model | Bundle sẵn trong app (Tauri resource), không cần UI tải model — file chỉ 1.1MB |

## Ngoài phạm vi

- Không chấm điểm real-time liên tục trong lúc đang ghi âm (thêm tải vào hot path ASR).
- Không port `quality_result_dialog.py` UI y hệt — chỉ hiển thị điểm trong Meeting Details bằng UI hiện có của app.
- Không port toàn bộ `audio_analyzer.py` (VAD wrapper riêng, RMS analysis, calibration...) — chỉ phần DNSMOS.
  VAD dùng lại `ContinuousVadProcessor` đã có trong [vad.rs](../../../frontend/src-tauri/src/audio/vad.rs).
- Không toggle bật/tắt trong Settings ở spec này (giống cách ITN đã làm — luôn bật, đơn giản hoá).
- Không chặn hoặc cảnh báo chặn việc bắt đầu ghi âm dựa trên điểm số — chỉ hiển thị thông tin.

## Kiến trúc tổng quan

```
Recording stops (recording_manager::stop_recording)
    ↓
recording_saver.stop_and_save() → file_path
    ↓ (background task, không block stop_recording trả về)
audio_quality::analyzer::analyze_file(file_path)
    ↓
decode → resample 16kHz (dùng lại resampler có sẵn) → ContinuousVadProcessor (VAD segments)
    ↓
với mỗi segment: audio_quality::model::score(segment) → {sig, bak, ovrl} (sliding window 9.01s)
    ↓
average across segments → điểm cuối cho cả recording
    ↓
lưu vào bảng meetings (dnsmos_sig, dnsmos_bak, dnsmos_ovrl, dnsmos_computed_at)
    ↓
emit event "audio-quality-ready" { meeting_id, sig, bak, ovrl }
    ↓
Frontend: Meeting Details hiển thị điểm; toast cảnh báo nếu ovrl < 2.5
```

Nhánh phụ — **Test microphone** (nút riêng trước khi ghi):
```
User bấm "Test microphone"
    ↓
Capture mẫu ngắn (~10s) từ device đã chọn
    ↓
audio_quality::analyzer::analyze_samples(pcm) → {sig, bak, ovrl}  (cùng hàm lõi, không qua VAD/file)
    ↓
Trả kết quả trực tiếp cho frontend (không lưu DB) → hiển thị toast/dialog nhỏ
```

## 1. Model DNSMOS

| Thuộc tính | Giá trị |
|---|---|
| File | `sig_bak_ovr.onnx` (~1.1MB) |
| Nguồn | [microsoft/DNS-Challenge](https://github.com/microsoft/DNS-Challenge/raw/master/DNSMOS/DNSMOS/sig_bak_ovr.onnx) |
| Input | float32, shape `(1, 144160)` — đúng 9.01s @ 16kHz, **không** peak-normalize |
| Output | `[SIG_raw, BAK_raw, OVRL_raw]` |
| Post-processing | polynomial fit cố định (xem dưới), clip [1.0, 5.0] |

Polynomial (copy chính xác từ `audio_analyzer.py`, KHÔNG thay đổi hệ số):

```rust
// p_sig(x) = -0.08397278*x^2 + 1.22083953*x + 0.0052439
// p_bak(x) = -0.13166888*x^2 + 1.60915514*x - 0.39604546
// p_ovr(x) = -0.06766283*x^2 + 1.11546468*x + 0.04602535
fn poly(coeffs: [f32; 3], x: f32) -> f32 {
    coeffs[0] * x * x + coeffs[1] * x + coeffs[2]
}
```

**Lưu trong app:** `frontend/src-tauri/resources/dnsmos/sig_bak_ovr.onnx` (bundle Tauri resource,
giống pattern `resources/itn-vi/`). Pin SHA256 khi tải về lúc implement (source đã có sẵn:
`269fbebdb513aa23cddfbb593542ecc540284a91849ac50516870e1ac78f6edd` — verify lại khi download).

## 2. Module `audio_quality/`

```
audio_quality/
├── mod.rs
├── model.rs        # DnsmosModel: load ONNX session, score_window(samples: &[f32]) -> Scores
├── analyzer.rs      # analyze_file(path) / analyze_samples(pcm) -> Scores (sliding window + VAD + average)
└── commands.rs      # get_meeting_audio_quality, test_microphone_quality
```

### `DnsmosModel::score_window(samples: &[f32; 144160]) -> Result<Scores>`

Port 1:1 từ `compute_dnsmos()`:
1. Pad/truncate về đúng 144160 samples (zero-pad nếu ngắn hơn).
2. `session.run()` với input `(1, 144160)`.
3. Áp polynomial → `Scores { sig, bak, ovrl }`, clip [1.0, 5.0].

### `analyzer::analyze_file(path: &Path) -> Result<Scores>`

Port từ `compute_dnsmos_average()` + phần segment-based trong `analyze_quality()`:
1. Decode file đã lưu (dùng lại [decoder.rs](../../../frontend/src-tauri/src/audio/decoder.rs)).
2. Resample về 16kHz nếu cần (dùng lại resampler có trong `vad.rs`).
3. Chạy `ContinuousVadProcessor` → lấy các `SpeechSegment`.
4. Với mỗi segment: sliding window 144160-sample (không overlap, giống Python — window cuối pad zero) → `score_window` mỗi window → trung bình trong segment.
5. Trung bình các segment → điểm cuối.
6. Nếu VAD không tìm thấy segment nào (matching Python fallback) → chấm trực tiếp trên toàn bộ audio đã decode (một hoặc nhiều window 144160-sample nối tiếp).

### `analyzer::analyze_samples(pcm: &[f32]) -> Result<Scores>`

Dùng cho "Test microphone" — bỏ qua bước file/VAD, chấm trực tiếp mẫu ngắn đã capture (đã ở 16kHz
từ device capture, hoặc resample nếu cần).

### Lazy load & lifecycle

Giống `CapuEngine`/`ItnEngine` — load model một lần (lazy, lần đầu cần dùng), giữ trong
`Arc<Mutex<Option<DnsmosModel>>>` ở app state. Load lỗi (thiếu file, ONNX lỗi) → log `warn!`,
mọi lệnh liên quan trả `Ok(None)` / bỏ qua, **không** làm fail recording hay import.

## 3. Điểm tích hợp

| File | Thay đổi |
|---|---|
| `audio/recording_manager.rs` | Sau `recording_saver.stop_and_save()` trả `Some(file_path)` trong `stop_recording` và `save_recording_only`: spawn background task gọi `audio_quality::analyze_and_store(meeting_id, file_path, app)` |
| `audio/import.rs` | Sau khi import file audio xong: gọi tương tự (mọi đường audio vào app đều được chấm điểm) |
| `audio/retranscription.rs` | Idem — retranscribe dùng lại audio đã có, chấm lại nếu chưa có điểm hoặc audio đổi |
| `database/repositories/meeting.rs` | Thêm hàm `update_audio_quality(meeting_id, sig, bak, ovrl)` |
| `database/models.rs` | `MeetingModel` thêm 4 field: `dnsmos_sig`, `dnsmos_bak`, `dnsmos_ovrl: Option<f64>`, `dnsmos_computed_at: Option<DateTimeUtc>` |
| `lib.rs` | `pub mod audio_quality;`, đăng ký commands `get_meeting_audio_quality`, `test_microphone_quality` |
| `config.rs` | Đường dẫn resource + hằng số ngưỡng (`DNSMOS_READY_THRESHOLD: f32 = 2.5`) |

Background task **không** block đường trả về của `stop_recording`/`save_recording_only` — chạy
`tauri::async_runtime::spawn`, lỗi chỉ log, không propagate lên UI như một failure của việc ghi âm.

## 4. Database

Migration mới `frontend/src-tauri/migrations/20260804000000_add_audio_quality.sql`:

```sql
ALTER TABLE meetings ADD COLUMN dnsmos_sig REAL;
ALTER TABLE meetings ADD COLUMN dnsmos_bak REAL;
ALTER TABLE meetings ADD COLUMN dnsmos_ovrl REAL;
ALTER TABLE meetings ADD COLUMN dnsmos_computed_at TEXT;
```

Tất cả nullable — meeting cũ, hoặc meeting mà việc chấm điểm thất bại, đều có giá trị NULL và UI ẩn
phần hiển thị điểm khi NULL.

## 5. Frontend

| File | Vai trò |
|---|---|
| `frontend/src/lib/audioQuality.ts` | Wrapper `invoke('get_meeting_audio_quality', ...)`, `invoke('test_microphone_quality', ...)` |
| `frontend/src/components/MeetingDetails/...` | Thêm phần hiển thị SIG/BAK/OVRL (chỉ hiện khi có dữ liệu) |
| Recording-start flow (`useRecordingStart.ts` / mic-selection UI) | Nút "Test microphone" → gọi `test_microphone_quality`, hiện kết quả qua toast (pattern giống `DownloadProgressToast.tsx`) |
| Event listener | Lắng nghe `audio-quality-ready`, cập nhật Meeting Details nếu đang mở, toast cảnh báo khi `ovrl < 2.5` |

## 6. Kiểm thử

### Tự động (Rust)

- Unit test polynomial fit: input raw scores đã biết từ Python reference → so sánh output khớp trong sai số nhỏ (giống cách ITN port test case từ `test.py`).
- Integration test (`#[ignore]`, cần model file): load `sig_bak_ovr.onnx` bundled, chấm một WAV sample cố định, assert điểm nằm trong khoảng hợp lý đã biết trước (ví dụ ghi âm sạch → OVRL > 3.0).

### Manual E2E

1. Ghi âm một đoạn ngắn với mic bình thường → dừng ghi → mở Meeting Details → thấy điểm SIG/BAK/OVRL.
2. Bấm "Test microphone" trước khi ghi → thấy kết quả ngay không cần ghi âm đầy đủ.
3. Ghi âm với mic cố tình để xa/nhiễu → điểm OVRL thấp hơn, toast cảnh báo xuất hiện.
4. Import file audio có sẵn → vẫn được chấm điểm.

## 7. Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| ONNX input shape/tên input khác giữa `ort` version hiện tại và Python `onnxruntime` | Verify bằng `session.inputs()` lúc implement (giống cách CAPU đã làm), không giả định tên cố định |
| Background scoring chạy lâu trên máy yếu, chồng nhiều task nếu ghi âm liên tiếp | Giới hạn 1 task DNSMOS chạy đồng thời (queue hoặc semaphore) — chi tiết hoá ở writing-plans |
| VAD không tìm thấy speech (audio toàn im lặng) | Fallback chấm trực tiếp trên raw audio, giống Python |
| SHA256 model không khớp lúc bundle | Verify hash khi tải về, fail loudly ở build/dev time, không silent lúc runtime |

## 8. Attribution

Cập nhật `docs/THIRD_PARTY.md` thêm mục DNSMOS:
[microsoft/DNS-Challenge](https://github.com/microsoft/DNS-Challenge) — model `sig_bak_ovr.onnx`,
polynomial fit coefficients dùng nguyên bản.

## 9. Thứ tự triển khai (cho writing-plans)

1. **Task 0:** Tải + verify SHA256 model, bundle vào `resources/dnsmos/`, spike load ONNX session + kiểm tra input/output shape thực tế qua `ort`
2. **Task 1:** `audio_quality::model` — `DnsmosModel::score_window` + polynomial, unit test khớp Python reference
3. **Task 2:** `audio_quality::analyzer` — `analyze_file` (decode + resample + VAD + sliding window + average)
4. **Task 3:** Migration DB + `meeting.rs` repository + `MeetingModel` fields
5. **Task 4:** Tích hợp `recording_manager.rs` (background task, event emit), `import.rs`, `retranscription.rs`
6. **Task 5:** `analyzer::analyze_samples` + `test_microphone_quality` command
7. **Task 6:** Frontend — Meeting Details hiển thị điểm, event listener, toast cảnh báo
8. **Task 7:** Frontend — nút "Test microphone" trong recording-start flow
9. **Task 8:** Manual E2E theo mục 6 + cập nhật `docs/THIRD_PARTY.md`
