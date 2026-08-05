# Vietnamese ITN (Inverse Text Normalization) — Rust thuần, luôn bật

## Vấn đề

ASR (ZipFormer / Gipformer) xuất text dạng **nói** — chữ viết bằng chữ cái, không chuẩn hóa số/đơn vị/ngày giờ.
Ví dụ: `một phẩy hai mét` thay vì `1,2 m`, `năm triệu đồng` thay vì `5.000.000₫`.

CAPU chỉ xử lý **dấu câu + viết hoa**, không chuyển số/đơn vị. Cần thêm bước **ITN** trước CAPU.

Repo tham chiếu:
[vieetj731/Vietnamese-Inverse-Text-Normalization](https://github.com/vieetj731/Vietnamese-Inverse-Text-Normalization)
(dùng Pynini FST — `tokenize_and_classify.far` + `verbalize.far`).

## Quyết định đã chốt (brainstorming)

| Câu hỏi | Lựa chọn |
|---|---|
| Runtime | **B** — Rust thuần (không Python subprocess) |
| Bật/tắt | **A** — Luôn bật, bundle file `.far` trong app, không UI Settings |

## Ngoài phạm vi

- Không Python/Pynini runtime trên máy user.
- Không UI tải model ITN (file nhỏ, bundle sẵn).
- Không toggle bật/tắt ITN trong spec này.
- Không thay đổi logic CAPU — chỉ thêm bước trước nó.
- Không port toàn bộ `nemo_text_processing/` — chỉ dùng 2 file `.far` đã build sẵn.

## Kiến trúc tổng quan

```
Segment audio (VAD)
    ↓
ASR.transcribe_audio() → text thô (thường UPPERCASE, không dấu câu)
    ↓
to_lowercase()                    ← ASR output giống CAPU
    ↓
itn_engine::inverse_normalize()   ← classify FST → verbalize FST
    ↓
capu_engine::restore_punctuation()
    ↓
TranscriptUpdate → UI / DB
```

**Thứ tự bắt buộc:** ITN trước CAPU — CAPU được train trên text có số/dấu chuẩn; ITN chuẩn hóa
entity trước khi model dự đoán dấu câu.

## 1. Tài nguyên ITN

Copy từ repo gốc (commit pin khi implement):

| File | Vai trò |
|---|---|
| `far/classify/tokenize_and_classify.far` | Tokenize + classify entity (số, ngày, tiền...) |
| `far/verbalize/verbalize.far` | Verbalize → dạng viết |

**Lưu trong app:** `frontend/src-tauri/resources/itn-vi/` (bundle Tauri resource, không `%APPDATA%`).

Load lúc runtime qua `app.path().resource_dir()` → `resources/itn-vi/...`.

## 2. Module `itn_engine/`

```
itn_engine/
├── mod.rs
├── engine.rs       # ItnEngine: load FST, inverse_normalize()
└── commands.rs     # itn_init, itn_is_ready (optional — cho health check)
```

### `ItnEngine::inverse_normalize(text: &str) -> Result<String>`

Port logic từ `inverse_normalize.py`:

```python
token = top_rewrite(s, classifier)
return top_rewrite(token, verbalizer)
```

Rust tương đương:
1. Compose input với classifier FST → intermediate token string
2. Compose intermediate với verbalizer FST → output string
3. Nếu rewrite thất bại → trả về input gốc (fallback, không panic)

**Stateless per segment** — không cần trailing context (khác CAPU).

### Khởi tạo

- `init_on_startup()` trong `lib.rs` — load 2 FST một lần khi app start
- Lỗi load FAR → log `error!`, `ItnEngine` = disabled; pipeline fallback bỏ qua ITN

## 3. Thư viện Rust (spike Task 0)

| Ưu tiên | Crate | Ghi chú |
|---|---|---|
| 1 | [`arcweight`](https://docs.rs/arcweight) | Có `FarReader` / `open_far()` — đọc `.far` trực tiếp |
| 2 | [`rustfst`](https://github.com/garvys-org/rustfst) | Compose + shortest path; **không** đọc FAR natively |

**Spike Task 0 (bắt buộc trước implement):**

1. Thêm `arcweight` vào `Cargo.toml`
2. Load `tokenize_and_classify.far` + `verbalize.far` từ repo ITN
3. Chạy 5 câu từ `test.py` (cardinal, decimal, money, time, date)
4. So sánh output với Python `inverse_normalize.py`

**Nếu spike FAIL** (FAR Pynini không tương thích):
- Plan B: dev machine chạy `farextract` (OpenFST CLI) → ship 2 file `.fst` thay `.far`
- `build.rs` hoặc script `scripts/extract-itn-fst.sh` — chỉ dev, không runtime

## 4. Điểm tích hợp

| File | Thay đổi |
|---|---|
| `audio/transcription/worker.rs` | Sau ASR, trước CAPU: `itn_engine::inverse_normalize(&transcript)` |
| `audio/import.rs` | Idem, trong vòng segment |
| `audio/retranscription.rs` | Idem |
| `lib.rs` | `pub mod itn_engine`, `itn_engine::commands::init_on_startup()` |

**Helper chung** (tránh duplicate 3 chỗ):

```rust
// itn_engine/post_asr.rs hoặc inline trong engine.rs
pub fn apply_itn_then_capu(
    raw: &str,
    capu_trailing: &mut Vec<String>,
) -> String {
    let lowered = raw.to_lowercase();
    let itn_out = crate::itn_engine::inverse_normalize_or_pass(&lowered);
    // ... capu restore_punctuation on itn_out ...
}
```

Hoặc tách `apply_itn(text) -> String` và gọi CAPU riêng — giữ mirror pattern hiện tại.

### Fallback (giống CAPU)

```rust
match itn_engine::inverse_normalize(&text) {
    Ok(s) => s,
    Err(e) => {
        warn!("ITN failed: {}, using pre-ITN text", e);
        text.clone()
    }
}
```

ITN disabled (FAR load fail) → `inverse_normalize_or_pass` trả input unchanged.

## 5. Config (`config.rs`)

```rust
pub const ITN_RESOURCE_SUBDIR: &str = "itn-vi";
pub const ITN_CLASSIFY_FAR: &str = "tokenize_and_classify.far";
pub const ITN_VERBALIZE_FAR: &str = "verbalize.far";
```

## 6. Tauri resources

`tauri.conf.json` — thêm `resources/itn-vi/**` vào bundle (pattern giống FFmpeg/models nếu có).

Windows production: FAR nằm cạnh binary trong `resources/`.

## 7. Kiểm thử

### Tự động (Rust)

Port test cases từ `test.py` của repo gốc:

| Nhóm | Ví dụ input → output |
|---|---|
| cardinal | `âm hai` → `-2`, `một trăm` → `100` |
| decimal | `một phẩy hai ba` → `1,23` |
| money | `một nghìn đồng` → `1.000₫` |
| time | `hai giờ rưỡi` → `02h30` |
| date | `ngày mồng chín tháng tám` → `ngày 09/08` |

File: `itn_engine/engine.rs` → `#[cfg(test)]` + `#[ignore]` integration test cần FAR files.

### Manual E2E

1. Ghi âm câu có số: "năm triệu đồng", "một phẩy hai mét"
2. Transcript hiển thị `5.000.000₫`, `1,2 m` (sau ITN) + dấu câu (sau CAPU)

## 8. Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| FAR Pynini ≠ OpenFST FAR | Task 0 spike; fallback extract `.fst` |
| `arcweight` chưa mature | Pin version; wrap errors; fallback pass-through |
| ASR UPPERCASE làm ITN sai | `to_lowercase()` trước ITN (đã xác nhận test.py dùng lowercase) |
| ITN + CAPU latency | FST nhẹ (~ms/segment); benchmark trong Task 8 |
| License repo ITN | Ghi attribution trong README/LICENSE app (verify license file repo) |

## 9. Attribution

Repo [Vietnamese-Inverse-Text-Normalization](https://github.com/vieetj731/Vietnamese-Inverse-Text-Normalization)
— thêm credit trong `docs/` hoặc About screen (license TBD khi implement).

## 10. Thứ tự triển khai (cho writing-plans)

1. **Task 0:** Spike `arcweight` + FAR load + 5 test câu
2. **Task 1:** `itn_engine` module + `inverse_normalize`
3. **Task 2:** Bundle resources + `tauri.conf.json`
4. **Task 3:** Tích hợp worker / import / retranscription
5. **Task 4:** Unit tests port từ `test.py`
6. **Task 5:** Manual E2E + attribution
