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
    let file = crate::config::HOTWORDS_RESOURCE_FILE;
    let subdir = crate::config::USER_DEFAULTS_SUBDIR;
    if let Ok(resource) = app.path().resource_dir() {
        for path in [
            resource.join(subdir).join(file),
            resource.join("resources").join(subdir).join(file),
            resource.join(file),
        ] {
            if path.exists() {
                return Some(path);
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(subdir)
        .join(file);
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
    let has_files = if f.is_online_streaming() && !has_files {
        let base = engine.get_models_directory().await;
        let dir = base.join(f.variant_subdir(v));
        let resource = app.path().resource_dir().ok();
        let _ = crate::asr_engine::streaming::ensure_bundled_tokens(&dir, resource.as_deref());
        engine.are_variant_files_present(&f, &v).await
    } else {
        has_files
    };

    let is_loaded = if f.is_online_streaming() {
        crate::asr_engine::streaming::get_or_init_streaming_engine()
            .is_loaded_as(f, v)
            .await
    } else {
        let current_family = engine.get_current_family().await;
        let current_variant = engine.get_current_variant().await;
        engine.is_model_loaded().await && current_family == f && current_variant == v
    };

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
                let resource = app_clone.path().resource_dir().ok();
                if f_for_load.is_online_streaming() {
                    let dir = engine_clone
                        .get_models_directory()
                        .await
                        .join(f_for_load.variant_subdir(v_for_load));
                    if let Err(e) = crate::asr_engine::streaming::ensure_bundled_tokens(
                        &dir,
                        resource.as_deref(),
                    ) {
                        error!("Failed to copy streaming tokens: {}", e);
                    }
                }
                let decoding = engine_clone.get_decoding_method().await;
                let paths = engine_clone.get_num_active_paths().await;
                if let Err(e) = load_family(
                    &engine_clone,
                    &app_clone,
                    f_for_load,
                    v_for_load,
                    decoding,
                    paths,
                )
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
    load_family(&engine, &app, f, v, decoding_method, num_active_paths).await
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
        if f.is_online_streaming() {
            let dir = engine.get_models_directory().await.join(f.variant_subdir(v));
            let resource = app.path().resource_dir().ok();
            let _ = crate::asr_engine::streaming::ensure_bundled_tokens(&dir, resource.as_deref());
        }
        if !engine.are_variant_files_present(&f, &v).await {
            return Err(
                "ASR model not downloaded. Please download it from Settings → Transcription."
                    .to_string(),
            );
        }
    }

    let current_matches = if f.is_online_streaming() {
        crate::asr_engine::streaming::get_or_init_streaming_engine()
            .is_loaded_as(f, v)
            .await
    } else {
        let current_family = engine.get_current_family().await;
        let current_variant = engine.get_current_variant().await;
        engine.is_model_loaded().await && current_family == f && current_variant == v
    };

    if !current_matches {
        load_family(&engine, &app, f, v, dm, paths)
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
            engine.set_hotwords(text.clone()).await;
            crate::asr_engine::streaming::get_or_init_streaming_engine()
                .set_hotwords(text)
                .await;
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

async fn load_family<R: Runtime>(
    engine: &AsrEngine,
    app: &AppHandle<R>,
    family: ModelFamily,
    variant: ModelVariant,
    decoding_method: String,
    num_active_paths: i32,
) -> Result<(), String> {
    let threads = live_asr_thread_count();
    if family.is_online_streaming() {
        engine.unload_model().await;
        let streaming = crate::asr_engine::streaming::get_or_init_streaming_engine();
        let base = engine.get_models_directory().await;
        let resource = app.path().resource_dir().ok();
        streaming
            .load_model(
                family,
                variant,
                decoding_method,
                num_active_paths,
                threads,
                &base,
                resource.as_deref(),
            )
            .await
            .map_err(|e| e.to_string())
    } else {
        if let Some(streaming) = crate::asr_engine::streaming::streaming_engine_if_init() {
            streaming.unload().await;
        }
        engine
            .load_model(family, variant, decoding_method, num_active_paths, threads)
            .await
            .map_err(|e| e.to_string())
    }
}

fn get_engine() -> Result<Arc<AsrEngine>, String> {
    let guard = ASR_ENGINE.lock().unwrap();
    guard
        .as_ref()
        .cloned()
        .ok_or_else(|| "ASR engine not initialized. Call asr_init first.".to_string())
}
