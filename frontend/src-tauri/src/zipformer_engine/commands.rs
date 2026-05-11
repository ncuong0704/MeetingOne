use super::zipformer_engine::ZipFormerEngine;
use log::{error, info};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub(crate) static ZIPFORMER_ENGINE: Mutex<Option<Arc<ZipFormerEngine>>> = Mutex::new(None);

/// Compute the models directory path from the app's data directory.
fn resolve_models_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("models").join("zipformer"))
}

/// Ensure the engine's models directory is set. Called lazily before any operation that needs it.
async fn ensure_models_dir<R: Runtime>(engine: &ZipFormerEngine, app: &AppHandle<R>) {
    if engine.get_models_directory().await == PathBuf::new() {
        if let Some(dir) = resolve_models_dir(app) {
            engine.set_models_directory(dir).await;
        }
    }
}

/// Called from lib.rs setup — initialises engine and sets the models directory.
pub fn init_on_startup<R: Runtime>(app: &AppHandle<R>) {
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        // Init engine (drop guard before any await)
        let engine = {
            let mut guard = ZIPFORMER_ENGINE.lock().unwrap();
            if guard.is_none() {
                *guard = Some(Arc::new(ZipFormerEngine::new()));
                info!("ZipFormer engine initialized");
            }
            guard.as_ref().cloned().unwrap()
        }; // guard dropped here

        if let Some(dir) = resolve_models_dir(&app_clone) {
            engine.set_models_directory(dir.clone()).await;
            info!("ZipFormer models directory: {:?}", dir);
        }
    });
}

#[tauri::command]
pub async fn zipformer_init() -> Result<(), String> {
    let mut guard = ZIPFORMER_ENGINE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(Arc::new(ZipFormerEngine::new()));
        info!("ZipFormer engine initialized (on-demand)");
    }
    Ok(())
}

#[tauri::command]
pub async fn zipformer_get_model_status<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;
    let status = engine.get_model_status().await;
    serde_json::to_value(status).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn zipformer_is_model_loaded() -> Result<bool, String> {
    let engine = get_engine()?;
    Ok(engine.is_model_loaded().await)
}

#[tauri::command]
pub async fn zipformer_get_models_directory<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    // Always compute path from AppHandle — safe to call before engine init
    if let Some(dir) = resolve_models_dir(&app) {
        return Ok(dir.to_string_lossy().to_string());
    }
    // Fallback to engine if available
    if let Ok(engine) = get_engine() {
        ensure_models_dir(&engine, &app).await;
        return Ok(engine.get_models_directory().await.to_string_lossy().to_string());
    }
    Ok(String::new())
}

#[tauri::command]
pub async fn zipformer_download_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    zipformer_init().await?;
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;

    let app_clone = app.clone();
    let engine_clone = engine.clone();

    tauri::async_runtime::spawn(async move {
        let cb = {
            let app = app_clone.clone();
            Box::new(move |progress: u8| {
                let _ = app.emit(
                    "zipformer-model-download-progress",
                    serde_json::json!({ "progress": progress }),
                );
            })
        };

        match engine_clone.download_model(Some(cb)).await {
            Ok(()) => {
                info!("ZipFormer model download complete");
                let _ = app_clone.emit("zipformer-model-download-complete", ());
            }
            Err(e) => {
                error!("ZipFormer model download failed: {}", e);
                let _ = app_clone.emit(
                    "zipformer-model-download-error",
                    serde_json::json!({ "error": e.to_string() }),
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn zipformer_load_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;
    engine.load_model().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn zipformer_transcribe_audio(audio_data: Vec<f32>) -> Result<String, String> {
    let engine = get_engine()?;
    engine.transcribe_audio(audio_data).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn zipformer_validate_model_ready<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    zipformer_init().await?;
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;

    if !engine.are_model_files_present().await {
        return Err(
            "ZipFormer model not downloaded. Please download it from Settings → Transcription."
                .to_string(),
        );
    }

    if !engine.is_model_loaded().await {
        engine.load_model().await.map_err(|e| e.to_string())?;
    }

    Ok(crate::config::ZIPFORMER_MODEL_NAME.to_string())
}

pub(crate) fn get_engine_arc() -> Result<Arc<ZipFormerEngine>, String> {
    get_engine()
}

fn get_engine() -> Result<Arc<ZipFormerEngine>, String> {
    let guard = ZIPFORMER_ENGINE.lock().unwrap();
    guard
        .as_ref()
        .cloned()
        .ok_or_else(|| "ZipFormer engine not initialized. Call zipformer_init first.".to_string())
}
