use crate::asr_engine::model_family::{ModelFamily, ModelVariant};
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use log::info;
use serde::{Deserialize, Serialize};
use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig,
    OfflineTransducerModelConfig,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ModelStatus {
    NotLoaded,
    Downloading(u8),
    Ready,
    Error(String),
}

pub struct AsrEngine {
    recognizer: Arc<RwLock<Option<OfflineRecognizer>>>,
    model_status: Arc<RwLock<ModelStatus>>,
    models_base_dir: Arc<RwLock<PathBuf>>,
    current_family: Arc<RwLock<ModelFamily>>,
    current_variant: Arc<RwLock<ModelVariant>>,
    decoding_method: Arc<RwLock<String>>,
    num_active_paths: Arc<RwLock<i32>>,
    hotwords_text: Arc<RwLock<String>>,
}

impl AsrEngine {
    pub fn new() -> Self {
        Self {
            recognizer: Arc::new(RwLock::new(None)),
            model_status: Arc::new(RwLock::new(ModelStatus::NotLoaded)),
            models_base_dir: Arc::new(RwLock::new(PathBuf::new())),
            current_family: Arc::new(RwLock::new(ModelFamily::ZipFormer30M)),
            current_variant: Arc::new(RwLock::new(ModelVariant::Int8)),
            decoding_method: Arc::new(RwLock::new("modified_beam_search".to_string())),
            num_active_paths: Arc::new(RwLock::new(15)),
            hotwords_text: Arc::new(RwLock::new(String::new())),
        }
    }

    pub async fn set_models_directory(&self, path: PathBuf) {
        *self.models_base_dir.write().await = path;
    }

    pub async fn get_models_directory(&self) -> PathBuf {
        self.models_base_dir.read().await.clone()
    }

    pub async fn get_model_status(&self) -> ModelStatus {
        self.model_status.read().await.clone()
    }

    pub async fn is_model_loaded(&self) -> bool {
        self.recognizer.read().await.is_some()
    }

    pub async fn get_current_family(&self) -> ModelFamily {
        *self.current_family.read().await
    }

    pub async fn get_current_variant(&self) -> ModelVariant {
        *self.current_variant.read().await
    }

    pub async fn get_decoding_method(&self) -> String {
        self.decoding_method.read().await.clone()
    }

    pub async fn get_num_active_paths(&self) -> i32 {
        *self.num_active_paths.read().await
    }

    /// Sets the hotword text used on every subsequent `transcribe_audio` call. Takes
    /// effect immediately — no model reload needed, since sherpa-onnx accepts hotwords
    /// per transcribe call via `create_stream_with_hotwords`, not baked into the loaded
    /// recognizer. `text` is filtered (comments/blank lines stripped) before storing.
    pub async fn set_hotwords(&self, text: String) {
        *self.hotwords_text.write().await = crate::asr_engine::hotwords::filter_hotwords_text(&text);
    }

    pub async fn get_hotwords(&self) -> String {
        self.hotwords_text.read().await.clone()
    }

    pub async fn get_current_model(&self) -> Option<String> {
        if self.is_model_loaded().await {
            Some(self.current_family.read().await.id().to_string())
        } else {
            None
        }
    }

    fn variant_dir(&self, base: &PathBuf, family: &ModelFamily, variant: &ModelVariant) -> PathBuf {
        base.join(family.variant_subdir(*variant))
    }

    pub async fn are_variant_files_present(
        &self,
        family: &ModelFamily,
        variant: &ModelVariant,
    ) -> bool {
        let base = self.models_base_dir.read().await.clone();
        if base == PathBuf::new() {
            return false;
        }
        let dir = self.variant_dir(&base, family, variant);
        family
            .model_files(*variant)
            .iter()
            .all(|f| dir.join(f).exists())
    }

    pub async fn are_model_files_present(&self) -> bool {
        let family = *self.current_family.read().await;
        let variant = *self.current_variant.read().await;
        self.are_variant_files_present(&family, &variant).await
    }

    pub async fn download_model(
        &self,
        family: ModelFamily,
        variant: ModelVariant,
        progress_callback: Option<Box<dyn Fn(u8) + Send>>,
    ) -> Result<()> {
        if !family.available_variants().contains(&variant) {
            return Err(anyhow!(
                "{} does not support variant '{}' (available: {:?})",
                family.id(),
                variant.as_str(),
                family.available_variants()
            ));
        }

        let base = self.models_base_dir.read().await.clone();
        if base == PathBuf::new() {
            return Err(anyhow!("Models directory not set"));
        }
        let dir = self.variant_dir(&base, &family, &variant);
        tokio::fs::create_dir_all(&dir).await?;

        *self.model_status.write().await = ModelStatus::Downloading(0);

        let files = family.model_files(variant);
        let total_bytes = family.total_size_bytes(variant);

        let file_sizes: Vec<u64> = {
            let enc_size = family.encoder_size_bytes(variant);
            if family.is_online_streaming() {
                vec![enc_size, 2_500_000, 2_000_000, 268_000, 25_000]
            } else {
                vec![enc_size, 1_310_000, 1_030_000, 268_000, 50_000]
            }
        };

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|e| anyhow!("Failed to build HTTP client: {}", e))?;

        let mut bytes_downloaded: u64 = 0;
        let mut last_stream_reported: u8 = 0;
        let hf_url = family.hf_url(variant);

        for (idx, filename) in files.iter().enumerate() {
            let dest = dir.join(filename);
            let tmp = dir.join(format!("{}.tmp", filename));

            // HF streaming repo has no tokens.txt — copy the bundled vocab after download.
            if family.is_online_streaming() && *filename == family.token_file() {
                info!("Skipping HuggingFace download for bundled {}", filename);
                bytes_downloaded += file_sizes.get(idx).copied().unwrap_or(0);
                continue;
            }

            if dest.exists() {
                info!("Skipping already downloaded: {}", filename);
                bytes_downloaded += file_sizes.get(idx).copied().unwrap_or(0);
                let progress = ((bytes_downloaded * 100) / total_bytes.max(1)).min(99) as u8;
                *self.model_status.write().await = ModelStatus::Downloading(progress);
                if let Some(ref cb) = progress_callback {
                    cb(progress);
                    last_stream_reported = last_stream_reported.max(progress);
                }
                continue;
            }

            let url = format!("{}/{}", hf_url, filename);
            info!(
                "Downloading [{}/{}]: {} ({:.1} MB)",
                idx + 1,
                files.len(),
                filename,
                file_sizes.get(idx).copied().unwrap_or(0) as f64 / 1_000_000.0
            );

            let response = client
                .get(&url)
                .send()
                .await
                .map_err(|e| anyhow!("Failed to connect for {}: {}", filename, e))?;

            if !response.status().is_success() {
                let err = format!("HTTP {} for {}", response.status(), filename);
                *self.model_status.write().await = ModelStatus::Error(err.clone());
                return Err(anyhow!("{}", err));
            }

            let mut stream = response.bytes_stream();
            let mut file = tokio::fs::File::create(&tmp)
                .await
                .map_err(|e| anyhow!("Cannot create {}: {}", filename, e))?;

            let mut file_bytes: u64 = 0;

            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|e| anyhow!("Download error for {}: {}", filename, e))?;
                file.write_all(&chunk)
                    .await
                    .map_err(|e| anyhow!("Write error for {}: {}", filename, e))?;
                file_bytes += chunk.len() as u64;

                let cumulative = bytes_downloaded.saturating_add(file_bytes);
                let overall = ((cumulative * 100) / total_bytes.max(1)).min(99) as u8;
                if overall > last_stream_reported + 2 {
                    last_stream_reported = overall;
                    *self.model_status.write().await = ModelStatus::Downloading(overall);
                    if let Some(ref cb) = progress_callback {
                        cb(overall);
                    }
                }
            }

            file.flush()
                .await
                .map_err(|e| anyhow!("Flush error for {}: {}", filename, e))?;
            drop(file);

            tokio::fs::rename(&tmp, &dest)
                .await
                .map_err(|e| anyhow!("Failed to finalise {}: {}", filename, e))?;

            bytes_downloaded += file_bytes;
            let progress = ((bytes_downloaded * 100) / total_bytes.max(1)).min(99) as u8;
            *self.model_status.write().await = ModelStatus::Downloading(progress);
            if let Some(ref cb) = progress_callback {
                cb(progress);
                last_stream_reported = last_stream_reported.max(progress);
            }
            info!(
                "Downloaded: {} ({:.2} MB)",
                filename,
                file_bytes as f64 / 1_000_000.0
            );
        }

        *self.model_status.write().await = ModelStatus::Downloading(100);
        if let Some(ref cb) = progress_callback {
            cb(100);
        }
        if family.is_online_streaming() {
            crate::asr_engine::streaming::ensure_bundled_tokens(&dir, None)?;
        }

        info!(
            "All ASR model files downloaded successfully (family: {}, variant: {})",
            family.id(),
            variant.as_str()
        );
        Ok(())
    }

    pub async fn load_model(
        &self,
        family: ModelFamily,
        variant: ModelVariant,
        decoding_method: String,
        num_active_paths: i32,
        num_threads: usize,
    ) -> Result<()> {
        if family.is_online_streaming() {
            return Err(anyhow!(
                "Streaming family must be loaded via OnlineRecognizer, not OfflineRecognizer"
            ));
        }

        if !family.available_variants().contains(&variant) {
            return Err(anyhow!(
                "{} does not support variant '{}' (available: {:?})",
                family.id(),
                variant.as_str(),
                family.available_variants()
            ));
        }

        if self.is_model_loaded().await {
            let loaded_family = *self.current_family.read().await;
            let loaded_variant = *self.current_variant.read().await;
            if loaded_family != family || loaded_variant != variant {
                self.unload_model().await;
            }
        }

        let base = self.models_base_dir.read().await.clone();
        let dir = self.variant_dir(&base, &family, &variant);

        if !self.are_variant_files_present(&family, &variant).await {
            let missing: Vec<&str> = family
                .model_files(variant)
                .iter()
                .filter(|&&f| !dir.join(f).exists())
                .copied()
                .collect();
            let err = format!("Missing model files: {:?}", missing);
            *self.model_status.write().await = ModelStatus::Error(err.clone());
            return Err(anyhow!(err));
        }

        info!(
            "Loading ASR model (family: {}, variant: {}, decoding: {}, paths: {})",
            family.id(),
            variant.as_str(),
            decoding_method,
            num_active_paths
        );

        let encoder = dir
            .join(family.encoder_file(variant))
            .to_string_lossy()
            .to_string();
        let decoder = dir
            .join(family.decoder_file(variant))
            .to_string_lossy()
            .to_string();
        let joiner = dir
            .join(family.joiner_file(variant))
            .to_string_lossy()
            .to_string();

        let token_path = dir.join(family.token_file());
        let tokens = if token_path.exists() {
            token_path
        } else if family == ModelFamily::Gipformer65M {
            let fallback = dir.join(crate::config::GIPFORMER_VOCAB_FALLBACK);
            if fallback.exists() {
                log::warn!("Gipformer: tokens.txt missing, falling back to config.json");
                fallback
            } else {
                return Err(anyhow!("Missing token file: {}", family.token_file()));
            }
        } else {
            return Err(anyhow!("Missing token file: {}", family.token_file()));
        }
        .to_string_lossy()
        .to_string();

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.transducer = OfflineTransducerModelConfig {
            encoder: Some(encoder),
            decoder: Some(decoder),
            joiner: Some(joiner),
        };
        config.model_config.tokens = Some(tokens);
        config.model_config.num_threads = num_threads.max(1) as i32;

        if decoding_method == "modified_beam_search" {
            config.decoding_method = Some("modified_beam_search".to_string());
            config.max_active_paths = num_active_paths;
        }

        // Hotword support: sherpa-onnx needs a text bpe.vocab (piece+score) alongside the
        // binary bpe.model to tokenize hotword phrases internally. Best-effort — a failure
        // here disables hotwords for this model without blocking the load itself.
        let bpe_model_path = dir.join(family.bpe_file());
        match crate::asr_engine::hotwords::ensure_bpe_vocab(&bpe_model_path) {
            Some(bpe_vocab_path) => {
                config.model_config.modeling_unit = Some("bpe".to_string());
                config.model_config.bpe_vocab = Some(bpe_vocab_path.to_string_lossy().to_string());
            }
            None => {
                log::warn!("Hotwords disabled for {} — failed to prepare bpe.vocab", family.id());
            }
        }

        info!(
            "ASR config — family: {}, encoder: {}, decoding: {:?}, max_active_paths: {}",
            family.id(),
            config
                .model_config
                .transducer
                .encoder
                .as_deref()
                .unwrap_or("?"),
            config.decoding_method,
            config.max_active_paths,
        );

        let recognizer = tokio::task::block_in_place(|| OfflineRecognizer::create(&config))
            .ok_or_else(|| anyhow!("Failed to create ASR recognizer — check model files"))?;

        *self.recognizer.write().await = Some(recognizer);
        *self.model_status.write().await = ModelStatus::Ready;
        *self.current_family.write().await = family;
        *self.current_variant.write().await = variant;
        *self.decoding_method.write().await = decoding_method;
        *self.num_active_paths.write().await = num_active_paths;

        info!("ASR model loaded (offline RNNT)");
        Ok(())
    }

    pub async fn unload_model(&self) {
        *self.recognizer.write().await = None;
        *self.model_status.write().await = ModelStatus::NotLoaded;
        info!("ASR model unloaded, native memory released");
    }

    pub async fn transcribe_audio(&self, audio: Vec<f32>) -> Result<String> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let guard = self.recognizer.read().await;
        let recognizer = guard
            .as_ref()
            .ok_or_else(|| anyhow!("ASR model not loaded"))?;
        let hotwords = self.hotwords_text.read().await.clone();

        let text = tokio::task::block_in_place(|| {
            let stream = if hotwords.is_empty() {
                recognizer.create_stream()
            } else {
                recognizer.create_stream_with_hotwords(&hotwords)
            };
            stream.accept_waveform(16000, &audio);
            recognizer.decode(&stream);

            stream
                .get_result()
                .map(|r| r.text.trim().to_string())
                .unwrap_or_default()
        });

        if !text.is_empty() {
            info!("ASR transcribed: {}", text);
        }

        Ok(text)
    }
}

impl Default for AsrEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unload_model_clears_recognizer_and_status() {
        let engine = AsrEngine::new();
        *engine.model_status.write().await = ModelStatus::Ready;

        assert!(!matches!(
            *engine.model_status.read().await,
            ModelStatus::NotLoaded
        ));

        engine.unload_model().await;

        assert!(engine.recognizer.read().await.is_none());
        assert!(matches!(
            *engine.model_status.read().await,
            ModelStatus::NotLoaded
        ));
    }

    #[tokio::test]
    async fn test_load_model_rejects_unsupported_variant() {
        let engine = AsrEngine::new();
        let result = engine
            .load_model(
                ModelFamily::SherpaZipformerVi2025,
                ModelVariant::Int8,
                "modified_beam_search".to_string(),
                15,
                2,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not support variant"));
    }

    #[tokio::test]
    async fn test_load_model_rejects_streaming_family() {
        let engine = AsrEngine::new();
        let result = engine
            .load_model(
                ModelFamily::ZipFormer30MStreaming,
                ModelVariant::Full,
                "modified_beam_search".to_string(),
                8,
                2,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("OnlineRecognizer"));
    }
}
