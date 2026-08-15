//! Tauri commands for Senko CAM++ diarization model ready / vendor / init.

use super::engine::{DiarizationConfig, DiarizationEngine};
use crate::config::{DIARIZATION_CAMP_FILE, DIARIZATION_SUBDIR};
use log::{error, info, warn};
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

fn files_ready(dir: &Path) -> bool {
    let p = dir.join(DIARIZATION_CAMP_FILE);
    p.exists() && p.is_file()
}

fn known_campplus_sources() -> Vec<PathBuf> {
    let mut srcs = Vec::new();
    if let Ok(p) = std::env::var("MEETINGONE_CAMPPLUS_ONNX") {
        srcs.push(PathBuf::from(p));
    }
    srcs.push(PathBuf::from(
        r"C:\Users\HP\Desktop\test ASR\models\campp-3dspeaker\campplus_cn_en_common_200k.onnx",
    ));
    srcs.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("diarization-fixture")
            .join(DIARIZATION_CAMP_FILE),
    );
    srcs
}

fn copy_campplus_if_needed(dest_dir: &Path) -> Result<(), String> {
    if files_ready(dest_dir) {
        return Ok(());
    }
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    for src in known_campplus_sources() {
        if src.is_file() {
            std::fs::copy(&src, dest_dir.join(DIARIZATION_CAMP_FILE)).map_err(|e| e.to_string())?;
            info!(
                "Diarization CAM++ copied from {} to {}",
                src.display(),
                dest_dir.display()
            );
            return Ok(());
        }
    }
    Err("CAM++ ONNX not found — set MEETINGONE_CAMPPLUS_ONNX or vendor campplus_cn_en_common_200k.onnx".to_string())
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
    if files_ready(&dir) {
        return Ok(true);
    }
    match copy_campplus_if_needed(&dir) {
        Ok(()) => Ok(true),
        Err(e) => {
            warn!("{e}");
            Ok(false)
        }
    }
}

/// Copy CAM++ ONNX into app data.
/// `onnx_source_dir`: folder containing `campplus_cn_en_common_200k.onnx`, or the file itself.
/// `plda_source_dir`: unused (kept so older frontend invoke signatures still compile).
#[tauri::command]
pub async fn diarization_vendor_models<R: Runtime>(
    app: AppHandle<R>,
    onnx_source_dir: String,
    _plda_source_dir: Option<String>,
) -> Result<(), String> {
    let dest = resolve_diarization_dir(&app)
        .ok_or_else(|| "Could not resolve app data directory".to_string())?;
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    let src_dir = PathBuf::from(&onnx_source_dir);
    let src = if src_dir.is_file() {
        src_dir
    } else {
        src_dir.join(DIARIZATION_CAMP_FILE)
    };
    if !src.exists() {
        return Err(format!("missing {}", src.display()));
    }
    std::fs::copy(&src, dest.join(DIARIZATION_CAMP_FILE)).map_err(|e| e.to_string())?;
    info!("Diarization CAM++ vendored to {}", dest.display());
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
    copy_campplus_if_needed(&dir)?;
    if !files_ready(&dir) {
        return Err("Diarization models not ready — vendor CAM++ ONNX first".to_string());
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
    info!("Senko CAM++ diarization engine initialized");
    Ok(())
}

pub fn get_engine_arc() -> Option<Arc<Mutex<DiarizationEngine>>> {
    DIARIZATION_ENGINE.lock().ok().and_then(|g| g.clone())
}

pub fn init_on_startup<R: Runtime>(app: &AppHandle<R>) {
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(dir) = resolve_diarization_dir(&app_clone) else {
            warn!("Diarization startup: could not resolve app data directory");
            return;
        };
        if files_ready(&dir) {
            return;
        }
        match copy_campplus_if_needed(&dir) {
            Ok(()) => info!("Diarization CAM++ model ready at {}", dir.display()),
            Err(e) => warn!("Diarization CAM++ not auto-vendored: {e}"),
        }
    });
}
