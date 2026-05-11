// audio/transcription/engine.rs
//
// TranscriptionEngine and initialization logic for ZipFormer Vietnamese ASR.

use super::provider::TranscriptionProvider;
use super::zipformer_provider::ZipFormerProvider;
use log::{info, warn};
use std::sync::Arc;
use tauri::{AppHandle, Runtime};

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

/// Validate that the ZipFormer model is ready before recording starts
pub async fn validate_transcription_model_ready<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    info!("🔍 Validating ZipFormer Vietnamese ASR model...");

    if let Err(e) = crate::zipformer_engine::commands::zipformer_init().await {
        warn!("❌ Failed to initialize ZipFormer engine: {}", e);
        return Err(format!("Failed to initialize speech recognition: {}", e));
    }

    match crate::zipformer_engine::commands::zipformer_validate_model_ready(app.clone()).await {
        Ok(name) => {
            info!("✅ ZipFormer model ready: {}", name);
            Ok(())
        }
        Err(e) => {
            warn!("❌ ZipFormer model validation failed: {}", e);
            Err(e)
        }
    }
}

/// Get or initialize the ZipFormer transcription engine
pub async fn get_or_init_transcription_engine<R: Runtime>(
    _app: &AppHandle<R>,
) -> Result<TranscriptionEngine, String> {
    info!("🎤 Initializing ZipFormer transcription engine");

    let engine = crate::zipformer_engine::commands::get_engine_arc()?;

    if !engine.is_model_loaded().await {
        engine.load_model().await.map_err(|e| {
            format!("Failed to load ZipFormer model: {}", e)
        })?;
    }

    let provider = Arc::new(ZipFormerProvider::new(engine));
    Ok(TranscriptionEngine::Provider(provider))
}
