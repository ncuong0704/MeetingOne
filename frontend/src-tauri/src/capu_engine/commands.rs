use super::CapuEngine;
use crate::config::{
    CAPU_DTAGS_FILE, CAPU_DTAGS_SIZE_BYTES, CAPU_HF_URL, CAPU_LABELS_FILE, CAPU_LABELS_SIZE_BYTES,
    CAPU_MODEL_FILE, CAPU_MODEL_SIZE_BYTES, CAPU_SUBDIR, CAPU_VOCAB_FILE, CAPU_VOCAB_SIZE_BYTES,
};
use futures_util::StreamExt;
use log::{error, info};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::io::AsyncWriteExt;

pub(crate) static CAPU_ENGINE: Mutex<Option<Arc<Mutex<CapuEngine>>>> = Mutex::new(None);

fn resolve_capu_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("models").join(CAPU_SUBDIR))
}

fn capu_files() -> [(&'static str, u64); 4] {
    [
        (CAPU_MODEL_FILE, CAPU_MODEL_SIZE_BYTES),
        (CAPU_VOCAB_FILE, CAPU_VOCAB_SIZE_BYTES),
        (CAPU_LABELS_FILE, CAPU_LABELS_SIZE_BYTES),
        (CAPU_DTAGS_FILE, CAPU_DTAGS_SIZE_BYTES),
    ]
}

#[tauri::command]
pub async fn capu_get_models_directory<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    resolve_capu_dir(&app)
        .map(|d| d.to_string_lossy().to_string())
        .ok_or_else(|| "Could not resolve app data directory".to_string())
}

#[tauri::command]
pub async fn capu_is_model_downloaded<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    let dir = resolve_capu_dir(&app).ok_or_else(|| "Could not resolve app data directory".to_string())?;
    Ok(capu_files()
        .iter()
        .all(|(name, _)| dir.join(name).exists()))
}

#[tauri::command]
pub async fn capu_download_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = resolve_capu_dir(&app).ok_or_else(|| "Could not resolve app data directory".to_string())?;
    let app_clone = app.clone();

    tauri::async_runtime::spawn(async move {
        match download_capu_files(&dir, &app_clone).await {
            Ok(()) => {
                info!("CAPU model download complete");
                let _ = app_clone.emit("capu-model-download-complete", ());
            }
            Err(e) => {
                error!("CAPU model download failed: {}", e);
                let _ = app_clone.emit(
                    "capu-model-download-error",
                    serde_json::json!({ "error": e.to_string() }),
                );
            }
        }
    });

    Ok(())
}

async fn download_capu_files<R: Runtime>(dir: &PathBuf, app: &AppHandle<R>) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    tokio::fs::create_dir_all(dir.join("vocabulary")).await?;

    let files = capu_files();
    let total_bytes: u64 = files.iter().map(|(_, size)| size).sum();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .build()?;

    let mut bytes_downloaded: u64 = 0;
    let mut last_reported: u8 = 0;

    for (filename, size) in files.iter() {
        let dest = dir.join(filename);
        if dest.exists() {
            bytes_downloaded += size;
            continue;
        }
        let tmp = dir.join(format!("{}.tmp", filename.replace('/', "_")));
        let url = format!("{}/{}", CAPU_HF_URL, filename);

        let response = client.get(&url).send().await?;
        if !response.status().is_success() {
            anyhow::bail!("HTTP {} for {}", response.status(), filename);
        }

        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&tmp).await?;
        let mut file_bytes: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            file_bytes += chunk.len() as u64;

            let cumulative = bytes_downloaded.saturating_add(file_bytes);
            let overall = ((cumulative * 100) / total_bytes.max(1)).min(99) as u8;
            if overall > last_reported {
                last_reported = overall;
                let _ = app.emit(
                    "capu-model-download-progress",
                    serde_json::json!({ "progress": overall }),
                );
            }
        }
        file.flush().await?;
        drop(file);
        tokio::fs::rename(&tmp, &dest).await?;
        bytes_downloaded += file_bytes;
    }

    let _ = app.emit(
        "capu-model-download-progress",
        serde_json::json!({ "progress": 100 }),
    );
    Ok(())
}

/// Resolves `(threads, punctuation_level, case_level)` for the CAPU engine: reads the saved
/// `TranscriptSetting` row if the app's DB state is already available (`app.try_state` —
/// this may run before database setup completes at startup, matching the same best-effort
/// pattern `asr_load_model` already uses to read hotwords), falling back to physical-core
/// count / level 7 / level 3 otherwise. `capu_cpu_threads` is clamped to this machine's
/// physical core count — never trust a stored value blindly, hardware can differ across
/// runs (e.g. a DB copied from a different machine).
async fn resolve_capu_settings<R: Runtime>(app: &AppHandle<R>) -> (usize, u8, u8) {
    let (physical_cores, _) = crate::capu_engine::cpu_topology::detect_cpu_topology();

    if let Some(state) = app.try_state::<crate::state::AppState>() {
        if let Ok(Some(config)) =
            crate::database::repositories::setting::SettingsRepository::get_transcript_config(
                state.db_manager.pool(),
            )
            .await
        {
            let threads = config
                .capu_cpu_threads
                .filter(|&t| t > 0)
                .map(|t| (t as usize).min(physical_cores))
                .unwrap_or(physical_cores);
            let punct = config.capu_punctuation_level.clamp(1, 10) as u8;
            let case = config.capu_case_level.clamp(1, 10) as u8;
            return (threads, punct, case);
        }
    }

    (physical_cores, 7, 3)
}

#[tauri::command]
pub async fn capu_init<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = resolve_capu_dir(&app).ok_or_else(|| "Could not resolve app data directory".to_string())?;

    {
        let guard = CAPU_ENGINE.lock().unwrap();
        if guard.is_some() {
            return Ok(());
        }
    }

    let model_path = dir.join(CAPU_MODEL_FILE);
    let vocab_path = dir.join(CAPU_VOCAB_FILE);
    let labels_path = dir.join(CAPU_LABELS_FILE);

    if !model_path.exists() || !vocab_path.exists() || !labels_path.exists() {
        return Err("CAPU model files are missing.".to_string());
    }

    let (threads, punctuation_level, case_level) = resolve_capu_settings(&app).await;

    let engine = CapuEngine::load(
        &model_path,
        &vocab_path,
        &labels_path,
        threads,
        punctuation_level,
        case_level,
    )
    .map_err(|e| e.to_string())?;

    let mut guard = CAPU_ENGINE.lock().unwrap();
    *guard = Some(Arc::new(Mutex::new(engine)));
    info!(
        "CAPU engine initialized ({} threads, punctuation level {}, case level {})",
        threads, punctuation_level, case_level
    );
    Ok(())
}

/// Best-effort: applies new settings to the already-loaded CAPU engine (if any) when
/// `api_save_transcript_config` saves. Punctuation/case levels update immediately, no
/// rebuild needed. The ONNX session is only rebuilt (unload + reload) when `threads` is set
/// and differs from what's currently loaded — and if that reload fails (e.g. model files
/// were deleted), the previous working engine is left untouched rather than torn down.
pub(crate) async fn apply_settings_after_save<R: Runtime>(
    app: AppHandle<R>,
    threads: Option<i32>,
    punctuation_level: u8,
    case_level: u8,
) {
    let engine_arc = match get_engine_arc() {
        Some(e) => e,
        None => return, // not loaded yet — capu_init will pick up saved settings next time it runs
    };

    let (physical_cores, _) = crate::capu_engine::cpu_topology::detect_cpu_topology();
    let requested_threads = threads
        .filter(|&t| t > 0)
        .map(|t| (t as usize).min(physical_cores));

    let mut reload_threads: Option<usize> = None;
    {
        let mut engine = engine_arc.lock().unwrap();
        engine.set_punctuation_level(punctuation_level);
        engine.set_case_level(case_level);
        if let Some(t) = requested_threads {
            if t != engine.threads() {
                reload_threads = Some(t);
            }
        }
    }

    let threads = match reload_threads {
        Some(t) => t,
        None => return,
    };

    let dir = match resolve_capu_dir(&app) {
        Some(d) => d,
        None => return,
    };
    let model_path = dir.join(CAPU_MODEL_FILE);
    let vocab_path = dir.join(CAPU_VOCAB_FILE);
    let labels_path = dir.join(CAPU_LABELS_FILE);

    match CapuEngine::load(
        &model_path,
        &vocab_path,
        &labels_path,
        threads,
        punctuation_level,
        case_level,
    ) {
        Ok(new_engine) => {
            let mut guard = CAPU_ENGINE.lock().unwrap();
            *guard = Some(Arc::new(Mutex::new(new_engine)));
            info!("CAPU engine reloaded with {} threads", threads);
        }
        Err(e) => {
            error!(
                "Failed to reload CAPU engine with {} threads: {} — keeping previous engine",
                threads, e
            );
        }
    }
}

#[derive(serde::Serialize)]
pub struct CpuTopology {
    #[serde(rename = "physicalCores")]
    pub physical_cores: usize,
    #[serde(rename = "logicalThreads")]
    pub logical_threads: usize,
}

#[tauri::command]
pub async fn capu_get_cpu_topology() -> Result<CpuTopology, String> {
    let (physical_cores, logical_threads) = crate::capu_engine::cpu_topology::detect_cpu_topology();
    Ok(CpuTopology {
        physical_cores,
        logical_threads,
    })
}

/// Used internally by `worker.rs` and `retranscription.rs` — not a Tauri command.
pub(crate) fn get_engine_arc() -> Option<Arc<Mutex<CapuEngine>>> {
    CAPU_ENGINE.lock().unwrap().as_ref().cloned()
}

/// Download CAPU on first launch if needed, then load the engine in the background.
pub fn init_on_startup<R: Runtime>(app: &AppHandle<R>) {
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let dir = match resolve_capu_dir(&app_clone) {
            Some(d) => d,
            None => {
                log::warn!("CAPU startup: could not resolve app data directory");
                return;
            }
        };

        match capu_is_model_downloaded(app_clone.clone()).await {
            Ok(true) => {}
            Ok(false) => {
                info!("CAPU model not present — downloading (~110 MB) in background...");
                if let Err(e) = download_capu_files(&dir, &app_clone).await {
                    log::warn!("CAPU background download failed: {}", e);
                    return;
                }
                info!("CAPU model download complete");
            }
            Err(e) => {
                log::warn!("CAPU startup check failed: {}", e);
                return;
            }
        }

        if let Err(e) = capu_init(app_clone).await {
            log::warn!("CAPU init on startup failed: {}", e);
        }
    });
}
