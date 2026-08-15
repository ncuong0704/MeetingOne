# Senko CAM++ Diarization Implementation Plan

> **For agentic workers:** Execute task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Replace MeetingOne Community-1 Pure ORT diarization with Senko CAM++ (test ASR `senko_campp_optimized` algorithm, energy VAD, no community-1 models).

**Status:** Implemented 2026-08-15. Community-1 modules (`plda.rs`, `segmentation.rs`) removed. Engine is CAM++ 192-d + energy VAD + spectral.

**Architecture:** Keep UI/settings/align. Replace `embedding`/`engine`/`commands`. Delete `plda`/`segmentation`. Spectral clustering + CAM++ 192-d ONNX.

**Tech Stack:** Rust `ort`, `ndarray`, `kaldi-native-fbank` (povey). Spec: `docs/superpowers/specs/2026-08-15-speaker-diarization-senko-campp-design.md`.

**Reference:** `C:\Users\HP\Desktop\test ASR\core\speaker_diarization_senko_campp.py` and `_optimized.py`.

---

## File map

| Path | Role |
|---|---|
| `frontend/src-tauri/src/config.rs` | CAM++ constants |
| `frontend/src-tauri/src/diarization_engine/clustering.rs` | NEW — spectral + post |
| `frontend/src-tauri/src/diarization_engine/embedding.rs` | REPLACE — CAM++ |
| `frontend/src-tauri/src/diarization_engine/engine.rs` | REPLACE — orchestrator |
| `frontend/src-tauri/src/diarization_engine/commands.rs` | Vendor 1 ONNX |
| `frontend/src-tauri/src/diarization_engine/mod.rs` | Drop plda/segmentation |
| `frontend/src-tauri/src/audio/import.rs` | Remove debug logs; keep resegment |
| DELETE | `plda.rs`, `segmentation.rs` |

---

### Task 1: clustering.rs (TDD)

- [x] Cosine + spectral 2-blob test, merge_by_cos, post-process turns.

### Task 2: CAM++ embedding + engine

- [x] Fbank povey, ONNX `feats`/`embs`, energy VAD, 1.5s/0.6s windows, batch 32.

### Task 3: commands + config; delete community files

- [x] `DIARIZATION_SUBDIR = "diarization-senko-campp"`, file `campplus_cn_en_common_200k.onnx` size 28_283_928.

### Task 4: Strip debug instrumentation

- [x] `import.rs` `agent_dbg`; `engine.rs` agent log.

### Task 5: Tests + vendor + launch app
