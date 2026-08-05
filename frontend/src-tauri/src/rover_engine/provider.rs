use crate::audio::transcription::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use crate::rover_engine::engine::RoverDecoder;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

pub struct RoverProvider {
    decoder: Arc<TokioMutex<RoverDecoder>>,
    family_a_id: String,
    family_b_id: String,
}

impl RoverProvider {
    pub fn new(
        decoder: Arc<TokioMutex<RoverDecoder>>,
        family_a_id: String,
        family_b_id: String,
    ) -> Self {
        Self {
            decoder,
            family_a_id,
            family_b_id,
        }
    }
}

#[async_trait]
impl TranscriptionProvider for RoverProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        _language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        let decoder = self.decoder.clone();
        let result = tokio::task::block_in_place(move || {
            let mut guard = decoder.blocking_lock();
            guard.decode(&audio, 16000.0)
        })
        .map_err(|e| TranscriptionError::EngineFailed(e.to_string()))?;

        let confidence = if result.words.is_empty() {
            None
        } else {
            Some(
                result.words.iter().map(|w| w.word.confidence).sum::<f32>()
                    / result.words.len() as f32,
            )
        };

        Ok(TranscriptResult {
            text: result.text,
            confidence,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        true
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(format!("rover:{}+{}", self.family_a_id, self.family_b_id))
    }

    fn provider_name(&self) -> &'static str {
        "rover-vi"
    }
}
