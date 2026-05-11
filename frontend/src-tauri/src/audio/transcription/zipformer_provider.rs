use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use std::sync::Arc;

pub struct ZipFormerProvider {
    engine: Arc<crate::zipformer_engine::ZipFormerEngine>,
}

impl ZipFormerProvider {
    pub fn new(engine: Arc<crate::zipformer_engine::ZipFormerEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl TranscriptionProvider for ZipFormerProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        _language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if !self.engine.is_model_loaded().await {
            return Err(TranscriptionError::ModelNotLoaded);
        }

        let text = self
            .engine
            .transcribe_audio(audio)
            .await
            .map_err(|e| TranscriptionError::EngineFailed(e.to_string()))?;

        Ok(TranscriptResult {
            text,
            confidence: None,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        self.engine.is_model_loaded().await
    }

    async fn get_current_model(&self) -> Option<String> {
        self.engine.get_current_model().await
    }

    fn provider_name(&self) -> &'static str {
        "zipformer-vi"
    }
}
