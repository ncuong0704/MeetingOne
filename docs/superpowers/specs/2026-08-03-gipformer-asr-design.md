# Thêm Gipformer 65M RNNT làm model ASR thứ hai

## Vấn đề

Meetily hiện chỉ hỗ trợ **một** model nhận dạng giọng nói tiếng Việt:
[hynt/Zipformer-30M-RNNT-6000h](https://huggingface.co/hynt/Zipformer-30M-RNNT-6000h) (~30M params,
int8 ~32 MB). Engine nằm trong module `zipformer_engine`, gọi sherpa-onnx `OfflineRecognizer`
([zipformer_engine.rs](../../../frontend/src-tauri/src/zipformer_engine/zipformer_engine.rs)).

Người dùng muốn thêm
[g-group-ai-lab/gipformer-65M-rnnt](https://huggingface.co/g-group-ai-lab/gipformer-65M-rnnt)
(~65M params, kiến trúc Zipformer Transducer RNNT) làm **lựa chọn thứ hai** trong Cài đặt → Nhận dạng.
Theo benchmark công bố trên HuggingFace, Gipformer chính xác hơn đáng kể ở domain call center và
nhiều benchmark khác, đổi lại model lớn hơn và chậm hơn.

## Quyết định đã chốt (brainstorming)

| Câu hỏi | Lựa chọn |
|---|---|
| Cách tích hợp | **A** — Thêm làm model thứ hai, giữ ZipFormer 30M |
| Model mặc định (user mới) | **A** — ZipFormer 30M int8 |
| UI Cài đặt | **A** — Một bộ chọn thống nhất: family + variant + decoding |
| Hướng kỹ thuật | **Hướng 1** — Refactor `zipformer_engine` → `asr_engine` thống nhất |

## Ngoài phạm vi

- Không thêm streaming ASR (vẫn offline/batch như hiện tại).
- Không thay đổi pipeline CAPU — CAPU chạy sau ASR, độc lập với family.
- Không migrate/xóa model ZipFormer đã tải của user hiện tại.
- Không thêm model ASR thứ ba trong spec này (chỉ chuẩn bị kiến trúc dễ mở rộng).
- Không benchmark WER tự động trong CI — chỉ manual smoke test.

## Kiến trúc tổng quan

```
Settings UI (AsrModelManager)
    │  family + variant + decoding
    ▼
asr_engine/commands.rs          ← 1 global engine (Arc<Mutex<...>>)
    ▼
asr_engine/engine.rs            ← ModelFamily × ModelVariant → paths, HF URLs
    ▼
sherpa-onnx OfflineRecognizer    ← OfflineTransducerModelConfig (giữ nguyên)
    ▼
audio/import.rs, worker.rs, retranscription.rs
    ▼
capu_engine (không đổi)
```

**Ràng buộc runtime:** Chỉ **một** model ASR được load trong RAM tại một thời điểm. Đổi family
hoặc variant → `unload_model()` → `load_model(family, variant, ...)`.

## 1. Hai model family

### ZipFormer 30M (hiện có — giữ nguyên hành vi)

| Thuộc tính | Giá trị |
|---|---|
| ID | `zipformer-vi-30m` |
| HF repo | `hynt/Zipformer-30M-RNNT-6000h` |
| int8 subdir | `zipformer-vi-int8` |
| full subdir | `zipformer-vi-full` |
| Encoder int8 | `encoder-epoch-20-avg-10.int8.onnx` (~32 MB) |
| Decoder int8 | `decoder-epoch-20-avg-10.int8.onnx` |
| Joiner int8 | `joiner-epoch-20-avg-10.int8.onnx` |
| Encoder full | `encoder-epoch-20-avg-10.onnx` (~100 MB) |
| Token file | `config.json` |
| Shared | `bpe.model` |

### Gipformer 65M (mới)

| Thuộc tính | Giá trị |
|---|---|
| ID | `gipformer-65m-rnnt` |
| HF repo | `g-group-ai-lab/gipformer-65M-rnnt` |
| int8 subdir | `gipformer-vi-int8` |
| full subdir | `gipformer-vi-full` |
| Encoder int8 | `encoder-epoch-35-avg-6.int8.onnx` (~71 MB) |
| Decoder int8 | `decoder-epoch-35-avg-6.int8.onnx` (~1.3 MB) |
| Joiner int8 | `joiner-epoch-35-avg-6.int8.onnx` (~1 MB) |
| Encoder full | `encoder-epoch-35-avg-6.onnx` (~261 MB) |
| Decoder full | `decoder-epoch-35-avg-6.onnx` (~5 MB) |
| Joiner full | `joiner-epoch-35-avg-6.onnx` (~4 MB) |
| Token file | `tokens.txt` (khác ZipFormer — xem mục 4) |
| Shared | `bpe.model` |

**Tổng dung lượng ước tính (int8):** ~75 MB. **Full:** ~335 MB.

Repo HF còn có `epoch-35-avg-6.pt`, `epoch-999.pt` — **không tải** (chỉ cần ONNX).

## 2. Lưu trữ file trên đĩa

Giữ pattern hiện tại: thư mục gốc `%APPDATA%/com.meetingone.app/models/`, mỗi
family+variant một subdir phẳng (không nest thêm cấp family — tránh phá đường dẫn user đã tải):

```
models/
├── zipformer-vi-int8/     ← đã có, không đổi
├── zipformer-vi-full/     ← đã có, không đổi
├── gipformer-vi-int8/     ← mới
└── gipformer-vi-full/     ← mới
```

`ModelFamily` + `ModelVariant` → `subdir()` trả về một trong bốn đường dẫn trên.

## 3. Refactor backend Rust

### 3.1 Đổi tên module

| Cũ | Mới |
|---|---|
| `zipformer_engine/` | `asr_engine/` |
| `ZipFormerEngine` | `AsrEngine` |
| `ZIPFORMER_ENGINE` static | `ASR_ENGINE` static |
| `zipformer_*` commands | `asr_*` commands |

**Backward compatibility:** Không giữ alias `zipformer_*` — cập nhật toàn bộ call site frontend
và Rust trong cùng một PR để tránh hai API song song.

### 3.2 Types mới (`asr_engine/engine.rs`)

```rust
pub enum ModelFamily {
    ZipFormer30M,   // id: "zipformer-vi-30m"
    Gipformer65M,   // id: "gipformer-65m-rnnt"
}

pub enum ModelVariant {
    Int8,
    Full,
}
```

`ModelFamily` cung cấp:
- `id() -> &'static str`
- `label() -> &'static str` (hiển thị UI)
- `hf_base_url(variant) -> &'static str`
- `encoder/decoder/joiner/token/bpe filenames(variant)`
- `total_size_bytes(variant)` (cho progress bar)
- `model_files(variant) -> [&str; 5]` — 5 file bắt buộc

Engine state thêm `current_family: Arc<RwLock<ModelFamily>>`.

`variant_dir(base, family, variant)` → `base.join(family.variant_subdir(variant))`.

### 3.3 Tauri commands (`asr_engine/commands.rs`)

| Command | Tham số | Ghi chú |
|---|---|---|
| `asr_init` | — | Giữ logic `init_on_startup` |
| `asr_get_model_status` | — | |
| `asr_is_model_loaded` | — | |
| `asr_get_models_directory` | — | |
| `asr_download_model` | `family`, `variant` | Emit `asr-model-download-progress` |
| `asr_load_model` | `family`, `variant`, `decodingMethod`, `numActivePaths` | |
| `asr_transcribe_audio` | `audioData` | |
| `asr_validate_model_ready` | `family`, `variant`, ... | |
| `asr_get_variant_status` | `family`, `variant` | `hasFiles`, `isLoaded` |
| `asr_get_current_config` | — | family + variant + decoding đang load |

Events đổi tên: `asr-model-download-progress`, `asr-model-download-complete`,
`asr-model-download-error`.

### 3.4 `config.rs`

Gom constants ZipFormer hiện có + thêm block `GIPFORMER_*` tương tự. Hoặc struct
`AsrModelSpec` per family — tránh 40 hằng số rời.

### 3.5 Call sites cần cập nhật

| File | Thay đổi |
|---|---|
| `lib.rs` | `mod asr_engine`, register `asr_*` commands |
| `audio/import.rs` | `asr_init`, `get_engine_arc`, load theo saved config |
| `audio/transcription/worker.rs` | transcribe qua ASR engine |
| `audio/retranscription.rs` | idem |
| `audio/recording_commands.rs` | validate model ready trước ghi âm |
| `api/api.rs` | transcript config API |
| `database/models.rs` | thêm/đổi field family |
| `database/repositories/setting.rs` | save/load family |

### 3.6 Load model — khác biệt token file

ZipFormer:
```rust
config.model_config.tokens = Some(dir.join("config.json"));
```

Gipformer (dự kiến — **verify lúc implement** bằng smoke test):
```rust
config.model_config.tokens = Some(dir.join("tokens.txt"));
```

Cả hai: `bpe_vocab` **không set** (giữ comment hiện tại — `bpe.model` là SentencePiece binary).

Nếu `tokens.txt` không load được, thử `config.json` làm fallback và ghi log warn.

## 4. Database migration

File mới: `frontend/src-tauri/migrations/20260803000000_add_asr_family.sql`

```sql
-- Đổi provider legacy và đổi tên cột variant
UPDATE transcript_settings SET provider = 'asr' WHERE provider = 'zipformer';
ALTER TABLE transcript_settings RENAME COLUMN zipformerVariant TO asrVariant;
```

`transcript_settings` sau migration:

| Cột | Mặc định | Ý nghĩa |
|---|---|---|
| `provider` | `'asr'` | Đổi từ `'zipformer'` trong migration; chỉ còn một provider local ASR |
| `model` | `'zipformer-vi-30m'` | Family ID (`zipformer-vi-30m` / `gipformer-65m-rnnt`) |
| `asrVariant` | `'int8'` | `int8` / `full` |
| `decodingMethod` | `'modified_beam_search'` | không đổi |
| `numActivePaths` | `15` | không đổi |

Rust `TranscriptSetting` struct: `zipformer_variant` → `asr_variant`.

`save_transcript_config(...)` thêm/thay param `model` (family id) — đã có cột `model`.

## 5. Frontend

### 5.1 File mới / đổi tên

| Cũ | Mới |
|---|---|
| `lib/zipformer.ts` | `lib/asr.ts` |
| `components/ZipFormerModelManager.tsx` | `components/AsrModelManager.tsx` |
| `constants/modelDefaults.ts` | thêm `GIPFORMER_MODEL_ID`, default vẫn ZipFormer |

### 5.2 `AsrModelManager` UI

Trong card "Nhận dạng giọng nói tiếng Việt" (`TranscriptSettings.tsx`):

1. **Model ASR** (select):
   - `ZipFormer 30M` — `hynt/Zipformer-30M-RNNT-6000h` (~32 MB int8)
   - `Gipformer 65M` — `g-group-ai-lab/gipformer-65M-rnnt` (~75 MB int8)
2. **Biến thể** (select): `int8` | `full`
3. **Decoding** + **Active paths** — giữ như hiện tại
4. Trạng thái tải/load per family+variant
5. Nút **Tải model** / **Áp dụng**

Khi đổi family trong dropdown → refresh variant status của family mới (không unload model
đang chạy cho đến khi user bấm Áp dụng).

### 5.3 Hooks & guards

| File | Thay đổi |
|---|---|
| `hooks/useRecordingStart.ts` | `asr_*` API, check model theo saved family |
| `contexts/ConfigContext.tsx` | load/save `model` family id |
| `components/Sidebar/index.tsx` | default model id |
| `components/MeetingDetails/RetranscribeDialog.tsx` | text "ZipFormer" → "ASR" hoặc hiện tên family |

### 5.4 TypeScript types

```typescript
export type AsrModelFamily = 'zipformer-vi-30m' | 'gipformer-65m-rnnt';
export type ModelVariant = 'int8' | 'full';
```

`TranscriptModelProps.provider`: đổi thành `'asr'` (cùng migration DB).

## 6. Luồng người dùng

### User mới
1. Mở app → default config: `zipformer-vi-30m` / `int8`
2. Ghi âm lần đầu → prompt tải ZipFormer int8 (behavior hiện tại)

### Chuyển sang Gipformer
1. Cài đặt → Nhận dạng → chọn Gipformer 65M → int8 → Tải model
2. Bấm Áp dụng → unload ZipFormer → load Gipformer
3. Ghi âm / nhập file → transcribe bằng Gipformer → CAPU

### Đang ghi âm
- UI Settings disable đổi model (check `isRecording` từ context hiện có).

## 7. Xử lý lỗi

| Tình huống | Hành vi |
|---|---|
| Model chưa tải | Chặn ghi âm/nhập file, hiện dialog dẫn tới Settings |
| Download HF fail | Emit `asr-model-download-error`, giữ file `.tmp` partial có thể retry |
| Load Gipformer fail | `ModelStatus::Error(msg)`, không auto-fallback sang ZipFormer |
| Missing files | Message liệt kê file thiếu (giữ format hiện tại) |
| Đổi model khi đang ghi | Disable UI, không gọi `asr_load_model` |

## 8. Kiểm thử

### Tự động (Rust)
- Unit test `ModelFamily::model_files()` — đủ 5 file, URL HF đúng format
- Unit test `variant_dir()` — path không đụng `zipformer-vi-int8` cũ khi family Gipformer

### Manual (bắt buộc trước merge)
1. User cũ có ZipFormer int8 → upgrade → vẫn ghi âm được không cần tải lại
2. Tải Gipformer int8 → transcribe file audio mẫu → có text
3. CAPU thêm dấu câu sau Gipformer transcribe
4. Chuyển ZipFormer ↔ Gipformer qua Settings
5. Retranscribe meeting với Gipformer
6. Full variant Gipformer (nếu đủ dung lượng đĩa)

## 9. Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| `tokens.txt` vs `config.json` cho Gipformer | Smoke test sớm trong Task 1; fallback + log |
| Refactor rename gây miss call site | `cargo check` + grep `zipformer_` toàn repo |
| Model 65M chậm trên CPU | UI ghi chú "cần máy mạnh hơn"; default vẫn ZipFormer 30M |
| Download 335 MB full variant | Progress bar + size label rõ trong UI |

## 10. Thứ tự triển khai đề xuất (cho writing-plans)

1. **Task 1:** Constants + `ModelFamily`/`ModelVariant` + refactor rename module
2. **Task 2:** Download/load Gipformer int8, smoke test transcribe
3. **Task 3:** DB migration + API save/load family
4. **Task 4:** Frontend `AsrModelManager` + rename API
5. **Task 5:** Cập nhật call sites (import, worker, retranscription, recording)
6. **Task 6:** Gipformer full variant + manual E2E
