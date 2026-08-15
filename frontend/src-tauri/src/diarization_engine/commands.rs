//! Tauri commands for Community-1 diarization model ready / vendor / init.

use super::engine::{DiarizationConfig, DiarizationEngine};
use crate::config::{
    DIARIZATION_EMB_BIAS_FILE, DIARIZATION_EMB_ENCODER_FILE, DIARIZATION_EMB_WEIGHT_FILE,
    DIARIZATION_PLDA_PREPARED_FILE, DIARIZATION_SEG_FILE, DIARIZATION_SUBDIR,
};
use log::{error, info};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Runtime};

pub(crate) static DIARIZATION_ENGINE: Mutex<Option<Arc<Mutex<DiarizationEngine>>>> =
    Mutex::new(None);

fn resolve_diarization_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("models").join(DIARIZATION_SUBDIR))
}

fn required_files() -> [&'static str; 5] {
    [
        DIARIZATION_SEG_FILE,
        DIARIZATION_EMB_ENCODER_FILE,
        DIARIZATION_EMB_WEIGHT_FILE,
        DIARIZATION_EMB_BIAS_FILE,
        DIARIZATION_PLDA_PREPARED_FILE,
    ]
}

fn files_ready(dir: &Path) -> bool {
    required_files().iter().all(|name| dir.join(name).exists())
}

#[tauri::command]
pub async fn diarization_get_models_directory<R: Runtime>(
    app: AppHandle<R>,
) -> Result<String, String> {
    resolve_diarization_dir(&app)
        .map(|d| d.to_string_lossy().to_string())
        .ok_or_else(|| "Could not resolve app data directory".to_string())
}

#[tauri::command]
pub async fn diarization_is_model_ready<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    let dir = resolve_diarization_dir(&app)
        .ok_or_else(|| "Could not resolve app data directory".to_string())?;
    Ok(files_ready(&dir))
}

/// Copy Community-1 assets into app data.
/// `onnx_source_dir`: folder with seg/encoder/weight/bias (e.g. test ASR `models/pyannote-onnx`)
/// `plda_source_dir`: folder containing `plda_prepared.npz` (e.g. `.../plda` or community-1 root)
#[tauri::command]
pub async fn diarization_vendor_models<R: Runtime>(
    app: AppHandle<R>,
    onnx_source_dir: String,
    plda_source_dir: Option<String>,
) -> Result<(), String> {
    let dest = resolve_diarization_dir(&app)
        .ok_or_else(|| "Could not resolve app data directory".to_string())?;
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dest.join("plda")).map_err(|e| e.to_string())?;

    let onnx = PathBuf::from(&onnx_source_dir);
    for name in [
        DIARIZATION_SEG_FILE,
        DIARIZATION_EMB_ENCODER_FILE,
        DIARIZATION_EMB_WEIGHT_FILE,
        DIARIZATION_EMB_BIAS_FILE,
    ] {
        let src = onnx.join(name);
        if !src.exists() {
            return Err(format!("missing {}", src.display()));
        }
        std::fs::copy(&src, dest.join(name)).map_err(|e| e.to_string())?;
    }

    let plda_src = plda_source_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| onnx.join("plda"));

    let prepared = if plda_src.join("plda_prepared.npz").exists() {
        plda_src.join("plda_prepared.npz")
    } else if plda_src.join("plda").join("plda_prepared.npz").exists() {
        plda_src.join("plda").join("plda_prepared.npz")
    } else {
        return Err(format!(
            "plda_prepared.npz not found under {}",
            plda_src.display()
        ));
    };
    std::fs::copy(&prepared, dest.join(DIARIZATION_PLDA_PREPARED_FILE))
        .map_err(|e| e.to_string())?;

    for name in ["plda.npz", "xvec_transform.npz"] {
        let s = if plda_src.join(name).exists() {
            plda_src.join(name)
        } else {
            plda_src.join("plda").join(name)
        };
        if s.exists() {
            let _ = std::fs::copy(&s, dest.join("plda").join(name));
        }
    }

    info!("Diarization models vendored to {}", dest.display());
    Ok(())
}

#[tauri::command]
pub async fn diarization_init<R: Runtime>(
    app: AppHandle<R>,
    num_speakers: Option<u32>,
    num_threads: Option<u32>,
) -> Result<(), String> {
    let dir = resolve_diarization_dir(&app)
        .ok_or_else(|| "Could not resolve app data directory".to_string())?;
    if !files_ready(&dir) {
        return Err("Diarization models not ready — vendor/copy them first".to_string());
    }

    let cfg = DiarizationConfig {
        num_speakers: num_speakers.map(|n| n as usize),
        num_threads: num_threads.unwrap_or(4) as usize,
        ..DiarizationConfig::default()
    };

    let engine = DiarizationEngine::load(&dir, cfg).map_err(|e| {
        error!("diarization_init failed: {e}");
        e.to_string()
    })?;

    let mut slot = DIARIZATION_ENGINE
        .lock()
        .map_err(|_| "diarization engine lock poisoned".to_string())?;
    *slot = Some(Arc::new(Mutex::new(engine)));
    info!("Diarization engine initialized");
    Ok(())
}

pub fn get_engine_arc() -> Option<Arc<Mutex<DiarizationEngine>>> {
    DIARIZATION_ENGINE.lock().ok().and_then(|g| g.clone())
}
