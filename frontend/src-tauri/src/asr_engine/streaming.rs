use super::model_family::{ModelFamily, ModelVariant};
use anyhow::{anyhow, Result};
use log::info;
use sherpa_onnx::{
    OnlineRecognizer, OnlineRecognizerConfig, OnlineTransducerModelConfig,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

pub(crate) static STREAMING_ENGINE: Mutex<Option<Arc<StreamingEngine>>> = Mutex::new(None);

pub struct StreamingEngine {
    recognizer: RwLock<Option<OnlineRecognizer>>,
    current_family: RwLock<Option<ModelFamily>>,
    current_variant: RwLock<Option<ModelVariant>>,
    hotwords_text: RwLock<String>,
}

impl StreamingEngine {
    fn new() -> Self {
        Self {
            recognizer: RwLock::new(None),
            current_family: RwLock::new(None),
            current_variant: RwLock::new(None),
            hotwords_text: RwLock::new(String::new()),
        }
    }

    pub async fn is_loaded(&self) -> bool {
        self.recognizer.read().await.is_some()
    }

    pub async fn is_loaded_as(&self, family: ModelFamily, variant: ModelVariant) -> bool {
        if !self.is_loaded().await {
            return false;
        }
        *self.current_family.read().await == Some(family)
            && *self.current_variant.read().await == Some(variant)
    }

    pub async fn set_hotwords(&self, text: String) {
        *self.hotwords_text.write().await = crate::asr_engine::hotwords::filter_hotwords_text(&text);
    }

    pub async fn hotwords(&self) -> String {
        self.hotwords_text.read().await.clone()
    }

    pub async fn unload(&self) {
        *self.recognizer.write().await = None;
        *self.current_family.write().await = None;
        *self.current_variant.write().await = None;
        info!("Streaming ASR model unloaded");
    }

    pub fn recognizer(&self) -> &RwLock<Option<OnlineRecognizer>> {
        &self.recognizer
    }

    pub async fn load_model(
        &self,
        family: ModelFamily,
        variant: ModelVariant,
        decoding_method: String,
        num_active_paths: i32,
        num_threads: usize,
        models_base: &Path,
        resource_dir: Option<&Path>,
    ) -> Result<()> {
        if !family.is_online_streaming() {
            return Err(anyhow!("load_model on StreamingEngine requires a streaming family"));
        }
        if !family.available_variants().contains(&variant) {
            return Err(anyhow!(
                "{} does not support variant '{}'",
                family.id(),
                variant.as_str()
            ));
        }

        let dir = models_base.join(family.variant_subdir(variant));
        ensure_bundled_tokens(&dir, resource_dir)?;

        let missing: Vec<&str> = family
            .model_files(variant)
            .iter()
            .filter(|&&f| !dir.join(f).exists())
            .copied()
            .collect();
        if !missing.is_empty() {
            return Err(anyhow!("Missing streaming model files: {:?}", missing));
        }

        let encoder = dir.join(family.encoder_file(variant)).to_string_lossy().to_string();
        let decoder = dir.join(family.decoder_file(variant)).to_string_lossy().to_string();
        let joiner = dir.join(family.joiner_file(variant)).to_string_lossy().to_string();
        let tokens = dir.join(family.token_file()).to_string_lossy().to_string();

        let mut config = OnlineRecognizerConfig::default();
        config.model_config.transducer = OnlineTransducerModelConfig {
            encoder: Some(encoder),
            decoder: Some(decoder),
            joiner: Some(joiner),
        };
        config.model_config.tokens = Some(tokens);
        config.model_config.num_threads = num_threads.max(1) as i32;
        config.enable_endpoint = true;
        config.rule1_min_trailing_silence = 3.0;
        config.rule2_min_trailing_silence = 2.0;
        config.rule3_min_utterance_length = 20.0;

        if decoding_method == "modified_beam_search" {
            config.decoding_method = Some("modified_beam_search".to_string());
            config.max_active_paths = num_active_paths.max(1);
        } else {
            config.decoding_method = Some("greedy_search".to_string());
        }

        let bpe_model_path = dir.join(family.bpe_file());
        match crate::asr_engine::hotwords::ensure_bpe_vocab(&bpe_model_path) {
            Some(bpe_vocab_path) => {
                config.model_config.modeling_unit = Some("bpe".to_string());
                config.model_config.bpe_vocab = Some(bpe_vocab_path.to_string_lossy().to_string());
            }
            None => {
                log::warn!("Hotwords disabled for streaming model — failed to prepare bpe.vocab");
            }
        }

        info!(
            "Loading streaming ASR (family: {}, decoding: {:?}, paths: {}, threads: {})",
            family.id(),
            config.decoding_method,
            config.max_active_paths,
            config.model_config.num_threads
        );

        let recognizer = tokio::task::block_in_place(|| OnlineRecognizer::create(&config))
            .ok_or_else(|| anyhow!("Failed to create OnlineRecognizer — check streaming model files"))?;

        *self.recognizer.write().await = Some(recognizer);
        *self.current_family.write().await = Some(family);
        *self.current_variant.write().await = Some(variant);
        info!("Streaming ASR model loaded (OnlineRecognizer, no VAD)");
        Ok(())
    }
}

pub fn get_or_init_streaming_engine() -> Arc<StreamingEngine> {
    let mut guard = STREAMING_ENGINE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(Arc::new(StreamingEngine::new()));
    }
    guard.as_ref().cloned().unwrap()
}

pub fn streaming_engine_if_init() -> Option<Arc<StreamingEngine>> {
    STREAMING_ENGINE.lock().unwrap().clone()
}

pub fn resolve_bundled_tokens_path(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let name = crate::config::ZIPFORMER_STREAMING_TOKENS_RESOURCE;
    if let Some(dir) = resource_dir {
        let direct = dir.join(name);
        if direct.exists() {
            return Some(direct);
        }
        let nested = dir.join("resources").join(name);
        if nested.exists() {
            return Some(nested);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(name);
    if dev.exists() {
        Some(dev)
    } else {
        None
    }
}

pub fn ensure_bundled_tokens(model_dir: &Path, resource_dir: Option<&Path>) -> Result<()> {
    let dest = model_dir.join(crate::config::ZIPFORMER_STREAMING_TOKENS);
    if dest.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(model_dir)?;
    let src = resolve_bundled_tokens_path(resource_dir)
        .ok_or_else(|| anyhow!("Bundled zipformer-streaming-tokens.txt not found"))?;
    std::fs::copy(&src, &dest)?;
    info!("Copied bundled streaming tokens to {:?}", dest);
    Ok(())
}
