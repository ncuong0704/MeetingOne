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

const HF_BASE_URL: &str =
    "https://huggingface.co/hynt/Zipformer-30M-RNNT-6000h/resolve/main";

const MODEL_FILES: &[&str] = &[
    crate::config::ZIPFORMER_ENCODER,
    crate::config::ZIPFORMER_DECODER,
    crate::config::ZIPFORMER_JOINER,
    crate::config::ZIPFORMER_BPE,
    crate::config::ZIPFORMER_VOCAB,
];

// Approximate sizes in bytes: encoder, decoder, joiner, bpe.model, config.json(vocab)
const FILE_SIZES: &[u64] = &[29_000_000, 1_310_000, 1_030_000, 268_000, 50_000];

pub struct ZipFormerEngine {
    recognizer: Arc<RwLock<Option<OfflineRecognizer>>>,
    model_status: Arc<RwLock<ModelStatus>>,
    models_dir: Arc<RwLock<PathBuf>>,
}

impl ZipFormerEngine {
    pub fn new() -> Self {
        Self {
            recognizer: Arc::new(RwLock::new(None)),
            model_status: Arc::new(RwLock::new(ModelStatus::NotLoaded)),
            models_dir: Arc::new(RwLock::new(PathBuf::new())),
        }
    }

    pub async fn set_models_directory(&self, path: PathBuf) {
        *self.models_dir.write().await = path;
    }

    pub async fn get_models_directory(&self) -> PathBuf {
        self.models_dir.read().await.clone()
    }

    pub async fn get_model_status(&self) -> ModelStatus {
        self.model_status.read().await.clone()
    }

    pub async fn is_model_loaded(&self) -> bool {
        self.recognizer.read().await.is_some()
    }

    pub async fn get_current_model(&self) -> Option<String> {
        if self.is_model_loaded().await {
            Some(crate::config::ZIPFORMER_MODEL_NAME.to_string())
        } else {
            None
        }
    }

    pub async fn are_model_files_present(&self) -> bool {
        let dir = self.models_dir.read().await.clone();
        if dir == PathBuf::new() {
            return false;
        }
        MODEL_FILES.iter().all(|f| dir.join(f).exists())
    }

    pub async fn download_model(
        &self,
        progress_callback: Option<Box<dyn Fn(u8) + Send>>,
    ) -> Result<()> {
        let dir = self.models_dir.read().await.clone();
        if dir == PathBuf::new() {
            return Err(anyhow!("Models directory not set"));
        }
        tokio::fs::create_dir_all(&dir).await?;

        *self.model_status.write().await = ModelStatus::Downloading(0);

        let total_bytes: u64 = FILE_SIZES.iter().sum();
        let total_files = MODEL_FILES.len();

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| anyhow!("Failed to build HTTP client: {}", e))?;

        let mut bytes_downloaded: u64 = 0;
        let mut last_stream_reported: u8 = 0;

        for (idx, filename) in MODEL_FILES.iter().enumerate() {
            let dest = dir.join(filename);
            let tmp = dir.join(format!("{}.tmp", filename));

            if dest.exists() {
                info!("Skipping already downloaded: {}", filename);
                bytes_downloaded += FILE_SIZES.get(idx).copied().unwrap_or(0);
                let progress = ((bytes_downloaded * 100) / total_bytes.max(1)).min(99) as u8;
                *self.model_status.write().await = ModelStatus::Downloading(progress);
                if let Some(ref cb) = progress_callback {
                    cb(progress);
                    last_stream_reported = last_stream_reported.max(progress);
                }
                continue;
            }

            let url = format!("{}/{}", HF_BASE_URL, filename);
            info!(
                "Downloading [{}/{}]: {} ({:.1} MB)",
                idx + 1,
                total_files,
                filename,
                FILE_SIZES.get(idx).copied().unwrap_or(0) as f64 / 1_000_000.0
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

                // Monotonic %: (completed bytes + current partial) / estimated total — not per-file slices,
                // which would jump backward when switching files.
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
        info!("All ZipFormer model files downloaded successfully");
        Ok(())
    }

    /// Load the ZipFormer RNNT model (offline/batch mode — works with non-streaming ONNX)
    pub async fn load_model(&self) -> Result<()> {
        let dir = self.models_dir.read().await.clone();

        if !self.are_model_files_present().await {
            let missing: Vec<&str> = MODEL_FILES
                .iter()
                .filter(|&&f| !dir.join(f).exists())
                .copied()
                .collect();
            let err = format!("Missing model files: {:?}", missing);
            *self.model_status.write().await = ModelStatus::Error(err.clone());
            return Err(anyhow!(err));
        }

        info!("Loading ZipFormer Vietnamese ASR model (offline mode)...");

        let encoder = dir
            .join(crate::config::ZIPFORMER_ENCODER)
            .to_string_lossy()
            .to_string();
        let decoder = dir
            .join(crate::config::ZIPFORMER_DECODER)
            .to_string_lossy()
            .to_string();
        let joiner = dir
            .join(crate::config::ZIPFORMER_JOINER)
            .to_string_lossy()
            .to_string();
        // config.json is the vocabulary file (plain-text "token id" per line)
        let tokens = dir
            .join(crate::config::ZIPFORMER_VOCAB)
            .to_string_lossy()
            .to_string();
        let bpe_vocab = dir
            .join(crate::config::ZIPFORMER_BPE)
            .to_string_lossy()
            .to_string();

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.transducer = OfflineTransducerModelConfig {
            encoder: Some(encoder),
            decoder: Some(decoder),
            joiner: Some(joiner),
        };
        config.model_config.tokens = Some(tokens);
        config.model_config.bpe_vocab = Some(bpe_vocab);
        config.model_config.num_threads = 2;

        info!(
            "ZipFormer config — encoder: {}, tokens: {}",
            config
                .model_config
                .transducer
                .encoder
                .as_deref()
                .unwrap_or("?"),
            config.model_config.tokens.as_deref().unwrap_or("?")
        );

        let recognizer = OfflineRecognizer::create(&config)
            .ok_or_else(|| anyhow!("Failed to create ZipFormer recognizer — check model files"))?;

        *self.recognizer.write().await = Some(recognizer);
        *self.model_status.write().await = ModelStatus::Ready;

        info!("ZipFormer Vietnamese ASR model loaded (offline RNNT)");
        Ok(())
    }

    /// Transcribe a complete audio buffer (16 kHz f32 mono PCM).
    /// Uses offline batch inference — call once per VAD segment.
    pub async fn transcribe_audio(&self, audio: Vec<f32>) -> Result<String> {
        let guard = self.recognizer.read().await;
        let recognizer = guard
            .as_ref()
            .ok_or_else(|| anyhow!("ZipFormer model not loaded"))?;

        if audio.is_empty() {
            return Ok(String::new());
        }

        let stream = recognizer.create_stream();
        stream.accept_waveform(16000, &audio);
        recognizer.decode(&stream);

        let text = stream
            .get_result()
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default();

        if !text.is_empty() {
            info!("ZipFormer transcribed: {}", text);
        }

        Ok(text)
    }
}

impl Default for ZipFormerEngine {
    fn default() -> Self {
        Self::new()
    }
}
