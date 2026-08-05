use super::engine::AsrEngine;
use super::model_family::{ModelFamily, ModelVariant};
use log::{error, info};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

fn live_asr_thread_count() -> usize {
    let (physical_cores, _) = crate::capu_engine::cpu_topology::detect_cpu_topology();
    super::thread_budget::asr_thread_budget(physical_cores, super::thread_budget::DecodeConcurrency::SingleLive)
}

pub(crate) static ASR_ENGINE: Mutex<Option<Arc<AsrEngine>>> = Mutex::new(None);

pub(crate) fn resolve_models_base_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("models"))
}

pub fn resolve_bundled_hotwords_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    if let Ok(resource) = app.path().resource_dir() {
        let path = resource.join(crate::config::HOTWORDS_RESOURCE_FILE);
        if path.exists() {
            return Some(path);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(crate::config::HOTWORDS_RESOURCE_FILE);
    if dev.exists() {
        return Some(dev);
    }
    None
}

pub fn load_bundled_hotwords_raw<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    resolve_bundled_hotwords_path(app).and_then(|p| std::fs::read_to_string(p).ok())
}

async fn ensure_models_dir<R: Runtime>(engine: &AsrEngine, app: &AppHandle<R>) {
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
            let mut guard = ASR_ENGINE.lock().unwrap();
            if guard.is_none() {
                *guard = Some(Arc::new(AsrEngine::new()));
                info!("ASR engine initialized");
            }
            guard.as_ref().cloned().unwrap()
        };

        if let Some(dir) = resolve_models_base_dir(&app_clone) {
            engine.set_models_directory(dir.clone()).await;
            info!("ASR models base directory: {:?}", dir);
        }
    });
}

#[tauri::command]
pub async fn asr_init() -> Result<(), String> {
    let mut guard = ASR_ENGINE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(Arc::new(AsrEngine::new()));
        info!("ASR engine initialized (on-demand)");
    }
    Ok(())
}

#[tauri::command]
pub async fn asr_get_model_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<serde_json::Value, String> {
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;
    let status = engine.get_model_status().await;
    serde_json::to_value(status).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn asr_is_model_loaded() -> Result<bool, String> {
    let engine = get_engine()?;
    Ok(engine.is_model_loaded().await)
}

#[tauri::command]
pub async fn asr_get_models_directory<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
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
pub async fn asr_get_variant_status<R: Runtime>(
    app: AppHandle<R>,
    family: String,
    variant: String,
) -> Result<serde_json::Value, String> {
    asr_init().await?;
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;

    let f = ModelFamily::from_id(&family);
    let v = ModelVariant::from_str(&variant);
    let has_files = engine.are_variant_files_present(&f, &v).await;
    let current_family = engine.get_current_family().await;
    let current_variant = engine.get_current_variant().await;
    let is_loaded = engine.is_model_loaded().await && current_family == f && current_variant == v;

    Ok(serde_json::json!({
        "hasFiles": has_files,
        "isLoaded": is_loaded,
    }))
}

#[tauri::command]
pub async fn asr_download_model<R: Runtime>(
    app: AppHandle<R>,
    family: String,
    variant: String,
) -> Result<(), String> {
    asr_init().await?;
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;

    let f = ModelFamily::from_id(&family);
    let v = ModelVariant::from_str(&variant);
    let app_clone = app.clone();
    let engine_clone = engine.clone();

    tauri::async_runtime::spawn(async move {
        let f_for_load = f;
        let v_for_load = v;
        let cb = {
            let app = app_clone.clone();
            Box::new(move |progress: u8| {
                let _ = app.emit(
                    "asr-model-download-progress",
                    serde_json::json!({ "progress": progress }),
                );
            })
        };

        match engine_clone.download_model(f, v, Some(cb)).await {
            Ok(()) => {
                info!("ASR model download complete — loading model");
                let decoding = engine_clone.get_decoding_method().await;
                let paths = engine_clone.get_num_active_paths().await;
                if let Err(e) = engine_clone
                    .load_model(f_for_load, v_for_load, decoding, paths, live_asr_thread_count())
                    .await
                {
                    error!("ASR auto-load after download failed: {}", e);
                }
                let _ = app_clone.emit("asr-model-download-complete", ());
            }
            Err(e) => {
                error!("ASR model download failed: {}", e);
                let _ = app_clone.emit(
                    "asr-model-download-error",
                    serde_json::json!({ "error": e.to_string() }),
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn asr_load_model<R: Runtime>(
    app: AppHandle<R>,
    family: String,
    variant: String,
    decoding_method: String,
    num_active_paths: i32,
) -> Result<(), String> {
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;
    let f = ModelFamily::from_id(&family);
    let v = ModelVariant::from_str(&variant);
    engine
        .load_model(f, v, decoding_method, num_active_paths, live_asr_thread_count())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn asr_transcribe_audio(audio_data: Vec<f32>) -> Result<String, String> {
    let engine = get_engine()?;
    engine
        .transcribe_audio(audio_data)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn asr_validate_model_ready<R: Runtime>(
    app: AppHandle<R>,
    family: Option<String>,
    variant: Option<String>,
    decoding_method: Option<String>,
    num_active_paths: Option<i32>,
) -> Result<String, String> {
    asr_init().await?;
    let engine = get_engine()?;
    ensure_models_dir(&engine, &app).await;

    let (f, v, dm, paths) = if family.is_none() {
        if let Some(state) = app.try_state::<crate::state::AppState>() {
            let pool = state.db_manager.pool();
            let live_cfg =
                crate::database::repositories::setting::SettingsRepository::get_path_asr_config(
                    pool,
                    crate::asr_engine::config::AsrPath::Live,
                )
                .await;
            info!(
                "Using saved live ASR config: family={}, variant={}, decoding={}, paths={}",
                live_cfg.family_id,
                live_cfg.variant.as_str(),
                live_cfg.decoding_method,
                live_cfg.num_active_paths
            );
            let family = ModelFamily::from_id(&live_cfg.family_id);
            let v = live_cfg.variant;
            (
                family,
                v,
                live_cfg.decoding_method,
                live_cfg.num_active_paths,
            )
        } else {
            (
                ModelFamily::ZipFormer30M,
                ModelVariant::from_str("int8"),
                decoding_method.unwrap_or_else(|| "modified_beam_search".to_string()),
                num_active_paths.unwrap_or(15),
            )
        }
    } else {
        (
            ModelFamily::from_id(family.as_deref().unwrap_or("zipformer-vi-30m")),
            ModelVariant::from_str(variant.as_deref().unwrap_or("int8")),
            decoding_method.unwrap_or_else(|| "modified_beam_search".to_string()),
            num_active_paths.unwrap_or(15),
        )
    };

    if !engine.are_variant_files_present(&f, &v).await {
        return Err(
            "ASR model not downloaded. Please download it from Settings → Transcription."
                .to_string(),
        );
    }

    let current_family = engine.get_current_family().await;
    let current_variant = engine.get_current_variant().await;
    if !engine.is_model_loaded().await || current_family != f || current_variant != v {
        engine
            .load_model(f, v, dm, paths, live_asr_thread_count())
            .await
            .map_err(|e| e.to_string())?;
    }

    if let Some(state) = app.try_state::<crate::state::AppState>() {
        if let Ok(Some(config)) = crate::database::repositories::setting::SettingsRepository::get_transcript_config(
            state.db_manager.pool(),
        )
        .await
        {
            let bundled = load_bundled_hotwords_raw(&app);
            let text = crate::asr_engine::hotwords::effective_hotwords_text(
                config.hotwords.as_deref(),
                bundled.as_deref(),
            );
            engine.set_hotwords(text).await;
        }
    }

    Ok(f.id().to_string())
}

#[tauri::command]
pub async fn asr_get_current_config() -> Result<serde_json::Value, String> {
    let engine = get_engine()?;
    let family = engine.get_current_family().await;
    let variant = engine.get_current_variant().await;
    Ok(serde_json::json!({
        "family": family.id(),
        "variant": variant.as_str(),
        "decodingMethod": engine.get_decoding_method().await,
        "numActivePaths": engine.get_num_active_paths().await,
        "isLoaded": engine.is_model_loaded().await,
    }))
}

pub fn get_engine_arc() -> Result<Arc<AsrEngine>, String> {
    get_engine()
}

fn get_engine() -> Result<Arc<AsrEngine>, String> {
    let guard = ASR_ENGINE.lock().unwrap();
    guard
        .as_ref()
        .cloned()
        .ok_or_else(|| "ASR engine not initialized. Call asr_init first.".to_string())
}
