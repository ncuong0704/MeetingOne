// audio/transcription/engine.rs
//
// TranscriptionEngine and initialization logic for Vietnamese ASR.

use super::asr_provider::AsrProvider;
use super::gemini_key::{needs_local_asr, resolve_stt_api_key};
use super::provider::TranscriptionProvider;
use crate::asr_engine::config::AsrPath;
use crate::database::repositories::setting::SettingsRepository;
use log::{info, warn};
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime};

async fn live_asr_config<R: Runtime>(app: &AppHandle<R>) -> Option<crate::asr_engine::config::PathAsrConfig> {
    let Some(app_state) = app.try_state::<crate::state::AppState>() else {
        return None;
    };
    Some(
        crate::database::repositories::setting::SettingsRepository::get_path_asr_config(
            app_state.db_manager.pool(),
            AsrPath::Live,
        )
        .await,
    )
}

// ============================================================================
// TRANSCRIPTION ENGINE ENUM
// ============================================================================

pub enum TranscriptionEngine {
    Provider(Arc<dyn TranscriptionProvider>),
}

impl TranscriptionEngine {
    pub async fn is_model_loaded(&self) -> bool {
        match self {
            Self::Provider(p) => p.is_model_loaded().await,
        }
    }

    pub async fn get_current_model(&self) -> Option<String> {
        match self {
            Self::Provider(p) => p.get_current_model().await,
        }
    }

    pub fn provider_name(&self) -> &str {
        match self {
            Self::Provider(p) => p.provider_name(),
        }
    }
}

// ============================================================================
// MODEL VALIDATION AND INITIALIZATION
// ============================================================================

/// Validate that the ASR model is ready before recording starts
pub async fn validate_transcription_model_ready<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    let Some(app_state) = app.try_state::<crate::state::AppState>() else {
        return Err("App state not available".to_string());
    };
    let pool = app_state.db_manager.pool();
    let provider = SettingsRepository::get_stt_provider(pool, AsrPath::Live).await;
    if !needs_local_asr(provider) {
        info!("Gemini live STT: checking API key (skip local ASR init)");
        resolve_stt_api_key(pool).await?;
        return Ok(());
    }

    info!("🔍 Validating Vietnamese ASR model (live path)...");

    let live_cfg = live_asr_config(app)
        .await
        .ok_or_else(|| "App state not available".to_string())?;

    if let Err(e) = crate::asr_engine::commands::asr_init().await {
        warn!("❌ Failed to initialize ASR engine: {}", e);
        return Err(format!("Failed to initialize speech recognition: {}", e));
    }

    match crate::asr_engine::commands::asr_validate_model_ready(
        app.clone(),
        Some(live_cfg.family_id.clone()),
        Some(live_cfg.variant.as_str().to_string()),
        Some(live_cfg.decoding_method.clone()),
        Some(live_cfg.num_active_paths),
    )
    .await
    {
        Ok(name) => {
            info!("✅ ASR model ready: {}", name);
            Ok(())
        }
        Err(e) => {
            warn!("❌ ASR model validation failed: {}", e);
            Err(e)
        }
    }
}

/// Get or initialize the ASR transcription engine
pub async fn get_or_init_transcription_engine<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<TranscriptionEngine, String> {
    info!("🎤 Initializing ASR transcription engine (live path)");

    let live_cfg = live_asr_config(app)
        .await
        .ok_or_else(|| "App state not available".to_string())?;

    crate::asr_engine::commands::asr_validate_model_ready(
        app.clone(),
        Some(live_cfg.family_id.clone()),
        Some(live_cfg.variant.as_str().to_string()),
        Some(live_cfg.decoding_method.clone()),
        Some(live_cfg.num_active_paths),
    )
    .await?;

    let engine = crate::asr_engine::commands::get_engine_arc()?;
    let provider = Arc::new(AsrProvider::new(engine));
    Ok(TranscriptionEngine::Provider(provider))
}
