use super::zipformer_engine::{ModelVariant, ZipFormerEngine};
use log::{error, info};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub(crate) static ZIPFORMER_ENGINE: Mutex<Option<Arc<ZipFormerEngine>>> = Mutex::new(None);

/// Compute the base models directory (without variant subdir).
fn resolve_models_base_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("models"))
}

async fn ensure_models_dir<R: Runtime>(engine: &ZipFormerEngine, app: &AppHandle<R>) {
    if engine.get_models_directory().await == PathBuf::new() {
        if let Some(dir) = resolve_models_base_dir(app) {
            engine.set_models_directory(dir).await;
        }
    }
}

pub fn init_on_startup<R: Runtime>(app: &AppHandle<R>) {
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let engine = {
            let mut guard = ZIPFORMER_ENGINE.lock().unwrap();
            if guard.is_none() {
                *guard = Some(Arc::new(ZipFormerEngine::new()));
                info!("ZipFormer engine initialized");
            }
            guard.as_ref().cloned().unwrap()
        };

        if let Some(dir) = resolve_models_base_dir(&app_clone) {
            engine.set_models_directory(dir.clone()).await;
            info!("ZipFormer models base directory: {:?}", dir);
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
pub async fn zipformer_get_model_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<serde_json::Value, String> {
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
pub async fn zipformer_get_models_directory<R: Runtime>(
    app: AppHandle<R>,
) -> Result<String, String> {
    if let Some(dir) = resolve_models_base_dir(&app) {
        return Ok(dir.to_string_lossy().to_string());
    }
    if let Ok(engine) = get_engine() {
        ensure_models_dir(&engine, &app).await;
        return Ok(engine
            .get_models_directory()
            .await
            .to_string_lossy()
            .to_string());
    }
    Ok(String::new())
}

#[tauri::command]
pub async fn zipformer_get_variant_status<R: Runtime>(
    app: AppHandle<R>,
    variant: String,
) -> Result<serde_json::Value, String> {
    zipformer_init().await?;
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;

    let v = ModelVariant::from_str(&variant);
    let has_files = engine.are_variant_files_present(&v).await;
    let current = engine.get_current_variant().await;
    let is_loaded = engine.is_model_loaded().await && current == v;

    Ok(serde_json::json!({
        "hasFiles": has_files,
        "isLoaded": is_loaded,
    }))
}

#[tauri::command]
pub async fn zipformer_download_model<R: Runtime>(
    app: AppHandle<R>,
    variant: String,
) -> Result<(), String> {
    zipformer_init().await?;
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;

    let v = ModelVariant::from_str(&variant);
    let app_clone = app.clone();
    let engine_clone = engine.clone();

    tauri::async_runtime::spawn(async move {
        let v_for_load = v.clone();
        let cb = {
            let app = app_clone.clone();
            Box::new(move |progress: u8| {
                let _ = app.emit(
                    "zipformer-model-download-progress",
                    serde_json::json!({ "progress": progress }),
                );
            })
        };

        match engine_clone.download_model(v, Some(cb)).await {
            Ok(()) => {
                info!("ZipFormer model download complete — loading model");
                let decoding = engine_clone.get_decoding_method().await;
                let paths = engine_clone.get_num_active_paths().await;
                if let Err(e) = engine_clone.load_model(v_for_load, decoding, paths).await {
                    error!("ZipFormer auto-load after download failed: {}", e);
                }
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
pub async fn zipformer_load_model<R: Runtime>(
    app: AppHandle<R>,
    variant: String,
    decoding_method: String,
    num_active_paths: i32,
) -> Result<(), String> {
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;
    let v = ModelVariant::from_str(&variant);
    engine
        .load_model(v, decoding_method, num_active_paths)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn zipformer_transcribe_audio(audio_data: Vec<f32>) -> Result<String, String> {
    let engine = get_engine()?;
    engine
        .transcribe_audio(audio_data)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn zipformer_validate_model_ready<R: Runtime>(
    app: AppHandle<R>,
    variant: Option<String>,
    decoding_method: Option<String>,
    num_active_paths: Option<i32>,
) -> Result<String, String> {
    zipformer_init().await?;
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;

    // When called from the recording pipeline (variant=None), read the user's saved config
    // from DB instead of hardcoding "int8". This ensures the selected variant persists
    // across sessions.
    let (v, dm, paths) = if variant.is_none() {
        if let Some(state) = app.try_state::<crate::state::AppState>() {
            let pool = state.db_manager.pool();
            match crate::database::repositories::setting::SettingsRepository::get_transcript_config(pool).await {
                Ok(Some(config)) => {
                    info!(
                        "📖 Using saved ZipFormer config: variant={}, decoding={}, paths={}",
                        config.zipformer_variant, config.decoding_method, config.num_active_paths
                    );
                    (
                        ModelVariant::from_str(&config.zipformer_variant),
                        config.decoding_method,
                        config.num_active_paths,
                    )
                }
                _ => (
                    ModelVariant::from_str("int8"),
                    decoding_method.unwrap_or_else(|| "modified_beam_search".to_string()),
                    num_active_paths.unwrap_or(15),
                ),
            }
        } else {
            (
                ModelVariant::from_str("int8"),
                decoding_method.unwrap_or_else(|| "modified_beam_search".to_string()),
                num_active_paths.unwrap_or(15),
            )
        }
    } else {
        (
            ModelVariant::from_str(variant.as_deref().unwrap_or("int8")),
            decoding_method.unwrap_or_else(|| "modified_beam_search".to_string()),
            num_active_paths.unwrap_or(15),
        )
    };

    if !engine.are_variant_files_present(&v).await {
        return Err(
            "ZipFormer model not downloaded. Please download it from Settings → Transcription."
                .to_string(),
        );
    }

    if !engine.is_model_loaded().await || engine.get_current_variant().await != v {
        engine
            .load_model(v, dm, paths)
            .await
            .map_err(|e| e.to_string())?;
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
