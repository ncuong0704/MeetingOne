# Vietnamese ITN Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Vietnamese Inverse Text Normalization (ITN) as a always-on Rust step between ASR and CAPU, converting spoken-form text to written form (e.g. `một phẩy hai ba` → `1,23`) using bundled Pynini/OpenFST `.far` files from [vieetj731/Vietnamese-Inverse-Text-Normalization](https://github.com/vieetj731/Vietnamese-Inverse-Text-Normalization).

**Architecture:** New `itn_engine` module loads two FST archives at startup via `arcweight`, exposes `inverse_normalize_or_pass(text)`. Pipeline becomes `ASR → lowercase → ITN → CAPU`. Shared helper `post_asr.rs` avoids duplicating logic in worker/import/retranscription.

**Tech Stack:** Rust, `arcweight` (FAR reader + compose + shortest_path), Tauri 2.x resources bundling.

**Reference spec:** `docs/superpowers/specs/2026-08-03-vietnamese-itn-design.md`

---

## Before you start: key facts

- Python reference (`inverse_normalize.py`):
  ```python
  token = top_rewrite(s, classifier)
  return top_rewrite(token, verbalizer)
  ```
- ASR output is UPPERCASE → **must** `to_lowercase()` before ITN (test.py uses lowercase).
- ITN is **stateless** per segment (no trailing context).
- ITN failure must **never** block transcript — pass through to CAPU.
- `rustfst` alone cannot read `.far` — use **`arcweight::io::open_far`**.
- If FAR load fails at spike → extract `.fst` with OpenFST `farextract` on dev machine, ship `.fst` instead (document in Task 0).

---

### Task 0: Spike — load FAR + rewrite one sentence

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`
- Create: `frontend/src-tauri/resources/itn-vi/.gitkeep` (placeholder)
- Create: `frontend/src-tauri/scripts/fetch-itn-far.ps1`
- Create: `frontend/src-tauri/src/itn_engine/spike.rs` (temporary, `#[cfg(test)]` only)

- [ ] **Step 1: Download FAR files**

Create `frontend/src-tauri/scripts/fetch-itn-far.ps1`:

```powershell
$base = "https://github.com/vieetj731/Vietnamese-Inverse-Text-Normalization/raw/master"
$dest = Join-Path $PSScriptRoot "..\resources\itn-vi"
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Invoke-WebRequest "$base/far/classify/tokenize_and_classify.far" -OutFile "$dest\tokenize_and_classify.far"
Invoke-WebRequest "$base/far/verbalize/verbalize.far" -OutFile "$dest\verbalize.far"
Write-Host "ITN FAR files saved to $dest"
```

Run: `cd frontend/src-tauri && powershell -ExecutionPolicy Bypass -File scripts/fetch-itn-far.ps1`

- [ ] **Step 2: Add dependency**

In `frontend/src-tauri/Cargo.toml` dependencies block:

```toml
# Vietnamese ITN (inverse text normalization) via OpenFST FAR files
arcweight = "0.3"
```

Run: `cd frontend/src-tauri && cargo check`
Expected: resolves (may take time on first fetch).

- [ ] **Step 3: Spike test — load FAR and rewrite**

Create `frontend/src-tauri/src/itn_engine/spike.rs` (add `mod spike;` under `#[cfg(test)]` in mod.rs later, or inline in engine tests):

```rust
#[cfg(test)]
mod spike {
    use arcweight::io::open_far;
    use arcweight::prelude::*;
    use arcweight::algorithms::{compose_default, shortest_path, decode_linear_fst_output, ShortestPathConfig};
    use std::path::PathBuf;

    fn resource_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/itn-vi")
    }

    fn linear_fst_from_utf8(s: &str) -> arcweight::fst::VectorFst<TropicalWeight> {
        // Build char-level linear acceptor: each char is one arc ilabel==olabel
        // Use arcweight::fst helpers or manual state chain.
        // See arcweight docs: fst! macro or VectorFst::add_arc loop.
        todo!("implement char-level linear FST — see arcweight examples")
    }

    fn top_rewrite(input: &str, transducer: &impl Fst<TropicalWeight>) -> anyhow::Result<String> {
        let input_fst = linear_fst_from_utf8(input);
        let composed = compose_default(&input_fst, transducer)?;
        let path = shortest_path(&composed, ShortestPathConfig::default())?;
        decode_linear_fst_output(&path).map_err(|e| anyhow::anyhow!("{e}"))
    }

    fn load_fst_from_far(far_path: &PathBuf, entry_name: &str) -> anyhow::Result<arcweight::fst::VectorFst<TropicalWeight>> {
        let mut reader = open_far(far_path)?;
        let names = reader.list();
        let name = names.iter().find(|n| n.contains(entry_name) || **n == entry_name)
            .or_else(|| names.first())
            .ok_or_else(|| anyhow::anyhow!("empty FAR: {:?}", far_path))?;
        reader.get(name)?.ok_or_else(|| anyhow::anyhow!("FST {} not in FAR", name))
    }

    #[test]
    #[ignore] // run manually: cargo test itn_spike -- --ignored --nocapture
    fn itn_spike_decimal() {
        let dir = resource_dir();
        let classify = load_fst_from_far(&dir.join("tokenize_and_classify.far"), "tokenize").unwrap();
        let verbalize = load_fst_from_far(&dir.join("verbalize.far"), "verbalize").unwrap();

        let input = "một phẩy hai ba";
        let token = top_rewrite(input, &classify).unwrap();
        let output = top_rewrite(&token, &verbalize).unwrap();

        eprintln!("ITN: {:?} -> {:?} -> {:?}", input, token, output);
        assert_eq!(output, "1,23");
    }
}
```

**Spike success criteria:**
- `open_far` loads both files without error
- `một phẩy hai ba` → `1,23`
- At least 3/5 categories from `test.py` pass (decimal, money, cardinal)

**If spike FAILS on FAR format:**
1. Install OpenFST, run `farextract` on both `.far` files
2. Commit `tokenize_and_classify.fst` + `verbalize.fst` to `resources/itn-vi/`
3. Load with `VectorFst::read()` from rustfst/arcweight instead of `open_far`
4. Document fallback in `itn_engine/engine.rs` comment

Run: `cd frontend/src-tauri && cargo test itn_spike -- --ignored --nocapture`

---

### Task 1: `itn_engine` module

**Files:**
- Create: `frontend/src-tauri/src/itn_engine/mod.rs`
- Create: `frontend/src-tauri/src/itn_engine/engine.rs`
- Create: `frontend/src-tauri/src/itn_engine/rewrite.rs` (linear_fst + top_rewrite helpers from spike)
- Create: `frontend/src-tauri/src/itn_engine/commands.rs`
- Modify: `frontend/src-tauri/src/config.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Add config constants**

In `config.rs` after CAPU block:

```rust
/// Vietnamese ITN — bundled FAR resources (no download)
pub const ITN_RESOURCE_SUBDIR: &str = "itn-vi";
pub const ITN_CLASSIFY_FAR: &str = "tokenize_and_classify.far";
pub const ITN_VERBALIZE_FAR: &str = "verbalize.far";
```

- [ ] **Step 2: Create `rewrite.rs`**

Move working `linear_fst_from_utf8` + `top_rewrite` from spike into `rewrite.rs` (production code, not `todo!`).

- [ ] **Step 3: Create `engine.rs`**

```rust
pub struct ItnEngine {
    classifier: VectorFst<TropicalWeight>,
    verbalizer: VectorFst<TropicalWeight>,
}

impl ItnEngine {
    pub fn load_from_dir(dir: &Path) -> Result<Self> { /* open_far both files */ }

    pub fn inverse_normalize(&self, text: &str) -> Result<String> {
        let token = top_rewrite(text, &self.classifier)?;
        top_rewrite(&token, &self.verbalizer)
    }
}

static ITN_ENGINE: Mutex<Option<ItnEngine>> = Mutex::new(None);

pub fn inverse_normalize_or_pass(text: &str) -> String {
    let guard = ITN_ENGINE.lock().unwrap();
    match guard.as_ref() {
        Some(engine) => match engine.inverse_normalize(text) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("ITN rewrite failed: {}, passing through", e);
                text.to_string()
            }
        },
        None => text.to_string(),
    }
}
```

- [ ] **Step 4: Create `commands.rs`**

```rust
pub fn init_on_startup<R: Runtime>(app: &AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let path = resolve_itn_dir(&app);
        match ItnEngine::load_from_dir(&path) {
            Ok(engine) => {
                *ITN_ENGINE.lock().unwrap() = Some(engine);
                info!("ITN engine initialized from {:?}", path);
            }
            Err(e) => error!("ITN engine failed to load (ITN disabled): {}", e),
        }
    });
}

fn resolve_itn_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    // Production: app.path().resource_dir()?.join(ITN_RESOURCE_SUBDIR)
    // Dev fallback: CARGO_MANIFEST_DIR/resources/itn-vi
}
```

`#[tauri::command] pub async fn itn_is_ready() -> Result<bool, String>` — optional health check.

- [ ] **Step 5: Register in `lib.rs`**

```rust
pub mod itn_engine;
// in setup:
itn_engine::commands::init_on_startup(&_app.handle());
// invoke_handler: itn_engine::commands::itn_is_ready,
```

- [ ] **Step 6: Verify**

Run: `cd frontend/src-tauri && cargo check`

---

### Task 2: Bundle resources in Tauri

**Files:**
- Modify: `frontend/src-tauri/tauri.conf.json`
- Ensure: `frontend/src-tauri/resources/itn-vi/*.far` (from fetch script)

- [ ] **Step 1: Add resources to bundle**

In `tauri.conf.json` → `bundle.resources`, append:

```json
"resources/itn-vi/*"
```

- [ ] **Step 2: Verify dev + production paths**

`resolve_itn_dir` logic:

```rust
fn resolve_itn_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    if let Ok(resource) = app.path().resource_dir() {
        let bundled = resource.join(crate::config::ITN_RESOURCE_SUBDIR);
        if bundled.join(crate::config::ITN_CLASSIFY_FAR).exists() {
            return bundled;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(crate::config::ITN_RESOURCE_SUBDIR)
}
```

- [ ] **Step 3: Add FAR files to git**

After `fetch-itn-far.ps1`, commit `resources/itn-vi/*.far` (or document LFS if large).

Run: `cd frontend/src-tauri && cargo run` — log should show `ITN engine initialized`.

---

### Task 3: Pipeline integration

**Files:**
- Create: `frontend/src-tauri/src/audio/post_asr.rs`
- Modify: `frontend/src-tauri/src/audio/mod.rs`
- Modify: `frontend/src-tauri/src/audio/transcription/worker.rs`
- Modify: `frontend/src-tauri/src/audio/import.rs`
- Modify: `frontend/src-tauri/src/audio/retranscription.rs`

- [ ] **Step 1: Create `post_asr.rs`**

```rust
/// Apply ITN then CAPU to raw ASR text. Falls back gracefully on any failure.
pub fn process_asr_text(
    raw: &str,
    capu_trailing: &mut Vec<String>,
) -> String {
    let lowered = raw.to_lowercase();
    let after_itn = crate::itn_engine::engine::inverse_normalize_or_pass(&lowered);

    match crate::capu_engine::commands::get_engine_arc() {
        Some(engine_arc) => {
            let mut engine = engine_arc.lock().unwrap();
            match engine.restore_punctuation(capu_trailing, &after_itn) {
                Ok((restored, next_context)) => {
                    *capu_trailing = next_context;
                    restored
                }
                Err(e) => {
                    log::warn!("CAPU failed after ITN: {}", e);
                    after_itn
                }
            }
        }
        None => after_itn,
    }
}
```

- [ ] **Step 2: Update `worker.rs`**

Replace inline CAPU block (~lines 235–265) with:

```rust
let mut trailing = CAPU_TRAILING_CONTEXT.lock().unwrap().clone();
let punctuated_text = crate::audio::post_asr::process_asr_text(&transcript, &mut trailing);
*CAPU_TRAILING_CONTEXT.lock().unwrap() = trailing;
```

- [ ] **Step 3: Update `import.rs`**

Inside segment loop, before CAPU:

```rust
let lowered = text.to_lowercase();
let after_itn = crate::itn_engine::engine::inverse_normalize_or_pass(&lowered);
// then CAPU on after_itn instead of text
```

Or call `process_asr_text(&text, &mut capu_trailing_context)` directly.

- [ ] **Step 4: Update `retranscription.rs`**

Same pattern as import.rs.

- [ ] **Step 5: Verify compile**

Run: `cd frontend/src-tauri && cargo check`

---

### Task 4: Port tests from `test.py`

**Files:**
- Modify: `frontend/src-tauri/src/itn_engine/engine.rs` (test module)

- [ ] **Step 1: Add integration tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> ItnEngine {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/itn-vi");
        ItnEngine::load_from_dir(&dir).expect("run fetch-itn-far.ps1 first")
    }

    #[test]
    #[ignore]
    fn test_decimal() {
        let e = engine();
        assert_eq!(e.inverse_normalize("một phẩy hai ba").unwrap(), "1,23");
        assert_eq!(e.inverse_normalize("không phẩy một").unwrap(), "0,1");
    }

    #[test]
    #[ignore]
    fn test_money() {
        let e = engine();
        assert_eq!(e.inverse_normalize("một nghìn đồng").unwrap(), "1.000₫");
    }

    #[test]
    #[ignore]
    fn test_cardinal() {
        let e = engine();
        assert_eq!(e.inverse_normalize("âm hai").unwrap(), "-2");
        assert_eq!(e.inverse_normalize("một trăm").unwrap(), "100");
    }

    #[test]
    #[ignore]
    fn test_time() {
        let e = engine();
        assert_eq!(e.inverse_normalize("hai giờ rưỡi").unwrap(), "02h30");
    }

    #[test]
    #[ignore]
    fn test_date() {
        let e = engine();
        assert_eq!(e.inverse_normalize("ngày mồng chín tháng tám").unwrap(), "ngày 09/08");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd frontend/src-tauri && cargo test itn_engine -- --ignored --nocapture`
Expected: all PASS after Task 0 spike patterns are correct.

---

### Task 5: Attribution + manual E2E

**Files:**
- Modify: `frontend/README.md` or create `docs/THIRD_PARTY.md` (one paragraph only)

- [ ] **Step 1: Add attribution**

```markdown
## Third-party: Vietnamese ITN

Inverse text normalization rules from
[vieetj731/Vietnamese-Inverse-Text-Normalization](https://github.com/vieetj731/Vietnamese-Inverse-Text-Normalization).
Bundled as OpenFST archive files (`tokenize_and_classify.far`, `verbalize.far`).
```

- [ ] **Step 2: Manual E2E checklist**

1. Restart app — log: `ITN engine initialized`
2. Ghi âm hoặc nhập file có câu: "một nghìn đồng", "năm phẩy năm triệu"
3. Transcript shows `1.000₫`, `5.500.000` (numbers normalized)
4. CAPU still adds punctuation on top
5. Disable test: rename FAR file → app still transcribes (ITN pass-through, CAPU works)

- [ ] **Step 3: Remove temporary spike module**

Delete `itn_engine/spike.rs` if merged into `rewrite.rs`.

---

## Spec coverage checklist

| Spec requirement | Task |
|---|---|
| Rust only, no Python | Task 0–1 (`arcweight`) |
| Always on, bundled FAR | Task 2 |
| ITN before CAPU | Task 3 (`post_asr.rs`) |
| lowercase before ITN | Task 3 |
| Fallback on failure | Task 1 `inverse_normalize_or_pass` |
| worker/import/retranscription | Task 3 |
| Tests from test.py | Task 4 |
| Attribution | Task 5 |

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-03-vietnamese-itn.md`. Two execution options:

**1. Subagent-Driven (recommended)** — fresh subagent per task, review between tasks

**2. Inline Execution** — implement in this session with checkpoints

Which approach?
