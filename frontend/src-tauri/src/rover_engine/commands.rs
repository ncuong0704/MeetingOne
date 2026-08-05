use crate::asr_engine::model_family::{ModelFamily, ModelVariant};
use crate::rover_engine::engine::RoverDecoder;
use log::info;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::Mutex as TokioMutex;

pub(crate) static ROVER_ENGINE: Mutex<Option<Arc<TokioMutex<RoverDecoder>>>> = Mutex::new(None);
type RoverConfigTuple = (ModelFamily, ModelVariant, ModelFamily, ModelVariant);
pub(crate) static ROVER_CONFIG: Mutex<Option<RoverConfigTuple>> = Mutex::new(None);

fn family_variant_dir(base: &PathBuf, family: ModelFamily, variant: ModelVariant) -> PathBuf {
    base.join(family.variant_subdir(variant))
}

pub(crate) fn family_paths(
    base: &PathBuf,
    family: ModelFamily,
    variant: ModelVariant,
) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let dir = family_variant_dir(base, family, variant);
    (
        dir.join(family.encoder_file(variant)),
        dir.join(family.decoder_file(variant)),
        dir.join(family.joiner_file(variant)),
        dir.join(family.token_file()),
    )
}

fn files_present(base: &PathBuf, family: ModelFamily, variant: ModelVariant) -> bool {
    let dir = family_variant_dir(base, family, variant);
    family
        .model_files(variant)
        .iter()
        .all(|f| dir.join(f).exists())
}

#[tauri::command]
pub async fn rover_init() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn rover_load_model<R: Runtime>(
    app: AppHandle<R>,
    family_a: String,
    variant_a: String,
    family_b: String,
    variant_b: String,
) -> Result<(), String> {
    let base = crate::asr_engine::commands::resolve_models_base_dir(&app)
        .ok_or_else(|| "Cannot resolve models directory".to_string())?;

    let fa = ModelFamily::from_id(&family_a);
    let va = ModelVariant::from_str(&variant_a);
    let fb = ModelFamily::from_id(&family_b);
    let vb = ModelVariant::from_str(&variant_b);

    if !fa.available_variants().contains(&va) {
        return Err(format!(
            "{} does not support variant '{}'",
            fa.id(),
            va.as_str()
        ));
    }
    if !fb.available_variants().contains(&vb) {
        return Err(format!(
            "{} does not support variant '{}'",
            fb.id(),
            vb.as_str()
        ));
    }

    if !files_present(&base, fa, va) {
        return Err(format!("Missing model files for {} ({})", fa.id(), va.as_str()));
    }
    if !files_present(&base, fb, vb) {
        return Err(format!("Missing model files for {} ({})", fb.id(), vb.as_str()));
    }

    let (enc_a, dec_a, joi_a, tok_a) = family_paths(&base, fa, va);
    let (enc_b, dec_b, joi_b, tok_b) = family_paths(&base, fb, vb);

    let threads_per_decoder = {
        let (physical_cores, _) = crate::capu_engine::cpu_topology::detect_cpu_topology();
        crate::asr_engine::thread_budget::asr_thread_budget(
            physical_cores,
            crate::asr_engine::thread_budget::DecodeConcurrency::RoverLive,
        )
    };

    let decoder = tokio::task::block_in_place(|| {
        RoverDecoder::load(
            (&enc_a, &dec_a, &joi_a, &tok_a),
            (&enc_b, &dec_b, &joi_b, &tok_b),
            4,
            threads_per_decoder,
        )
    })
    .map_err(|e| format!("Failed to load ROVER models: {}", e))?;

    *ROVER_ENGINE.lock().unwrap() = Some(Arc::new(TokioMutex::new(decoder)));
    *ROVER_CONFIG.lock().unwrap() = Some((fa, va, fb, vb));
    info!(
        "ROVER models loaded: {} ({}) + {} ({})",
        fa.id(),
        va.as_str(),
        fb.id(),
        vb.as_str()
    );
    Ok(())
}

#[tauri::command]
pub async fn rover_is_model_loaded() -> Result<bool, String> {
    Ok(ROVER_ENGINE.lock().unwrap().is_some())
}

#[tauri::command]
pub async fn rover_get_current_config() -> Result<serde_json::Value, String> {
    let cfg = *ROVER_CONFIG.lock().unwrap();
    match cfg {
        Some((fa, va, fb, vb)) => Ok(serde_json::json!({
            "familyA": fa.id(),
            "variantA": va.as_str(),
            "familyB": fb.id(),
            "variantB": vb.as_str(),
            "isLoaded": true,
        })),
        None => Ok(serde_json::json!({ "isLoaded": false })),
    }
}

pub fn get_engine_arc() -> Result<Arc<TokioMutex<RoverDecoder>>, String> {
    ROVER_ENGINE
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .ok_or_else(|| "ROVER engine not loaded. Call rover_load_model first.".to_string())
}

#[tauri::command]
pub async fn rover_validate_model_ready<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let app_state = app
        .try_state::<crate::state::AppState>()
        .ok_or_else(|| "App state not available".to_string())?;
    let config = crate::database::repositories::setting::SettingsRepository::get_transcript_config(
        app_state.db_manager.pool(),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "No transcript config found".to_string())?;

    if !config.rover_enabled {
        return Err("ROVER is not enabled in settings".to_string());
    }
    let family_b = config
        .rover_family_b
        .ok_or_else(|| "ROVER family B not configured".to_string())?;
    let variant_b = config.rover_variant_b.unwrap_or_else(|| "int8".to_string());

    let fa = ModelFamily::from_id(&config.model);
    let va = ModelVariant::from_str(&config.asr_variant);
    let fb = ModelFamily::from_id(&family_b);
    let vb = ModelVariant::from_str(&variant_b);

    let base = crate::asr_engine::commands::resolve_models_base_dir(&app)
        .ok_or_else(|| "Cannot resolve models directory".to_string())?;
    if !files_present(&base, fa, va) {
        return Err(format!("Missing model files for {} ({})", fa.id(), va.as_str()));
    }
    if !files_present(&base, fb, vb) {
        return Err(format!("Missing model files for {} ({})", fb.id(), vb.as_str()));
    }

    let needs_reload = {
        let loaded = ROVER_ENGINE.lock().unwrap().is_some();
        let current = *ROVER_CONFIG.lock().unwrap();
        !loaded || current != Some((fa, va, fb, vb))
    };

    if needs_reload {
        rover_load_model(
            app.clone(),
            fa.id().to_string(),
            va.as_str().to_string(),
            fb.id().to_string(),
            vb.as_str().to_string(),
        )
        .await?;
    }

    Ok(format!("{}+{}", fa.id(), fb.id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_present_is_false_when_any_required_file_is_missing() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let base = dir.path().to_path_buf();
        let family = ModelFamily::ZipFormer30M;
        let variant = ModelVariant::Int8;
        let subdir = base.join(family.variant_subdir(variant));
        std::fs::create_dir_all(&subdir).expect("create subdir");

        let files = family.model_files(variant);
        for f in &files[..files.len() - 1] {
            std::fs::write(subdir.join(f), b"stub").expect("write stub file");
        }

        assert!(!files_present(&base, family, variant));
    }

    #[test]
    fn files_present_is_true_when_all_required_files_exist() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let base = dir.path().to_path_buf();
        let family = ModelFamily::ZipFormer30M;
        let variant = ModelVariant::Int8;
        let subdir = base.join(family.variant_subdir(variant));
        std::fs::create_dir_all(&subdir).expect("create subdir");

        for f in family.model_files(variant) {
            std::fs::write(subdir.join(f), b"stub").expect("write stub file");
        }

        assert!(files_present(&base, family, variant));
    }
}
