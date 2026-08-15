/// Application configuration constants — ZipFormer Vietnamese ASR

pub const ZIPFORMER_MODEL_NAME: &str = "zipformer-vi-30m";

// Model variant identifiers
pub const ZIPFORMER_VARIANT_INT8: &str = "int8";
pub const ZIPFORMER_VARIANT_FULL: &str = "full";

// Int8-quantized model (~32 MB)
pub const ZIPFORMER_INT8_HF_URL: &str =
    "https://huggingface.co/hynt/Zipformer-30M-RNNT-6000h/resolve/main";
pub const ZIPFORMER_INT8_SUBDIR: &str = "zipformer-vi-int8";
pub const ZIPFORMER_INT8_ENCODER: &str = "encoder-epoch-20-avg-10.int8.onnx";
pub const ZIPFORMER_INT8_DECODER: &str = "decoder-epoch-20-avg-10.int8.onnx";
pub const ZIPFORMER_INT8_JOINER: &str = "joiner-epoch-20-avg-10.int8.onnx";
pub const ZIPFORMER_INT8_SIZE_BYTES: u64 = 32_000_000;

// Full-precision model (~100 MB)
pub const ZIPFORMER_FULL_HF_URL: &str =
    "https://huggingface.co/hynt/Zipformer-30M-RNNT-6000h/resolve/main";
pub const ZIPFORMER_FULL_SUBDIR: &str = "zipformer-vi-full";
pub const ZIPFORMER_FULL_ENCODER: &str = "encoder-epoch-20-avg-10.onnx";
pub const ZIPFORMER_FULL_DECODER: &str = "decoder-epoch-20-avg-10.onnx";
pub const ZIPFORMER_FULL_JOINER: &str = "joiner-epoch-20-avg-10.onnx";
pub const ZIPFORMER_FULL_SIZE_BYTES: u64 = 100_000_000;

// Shared files (same for both models)
pub const ZIPFORMER_BPE: &str = "bpe.model";
pub const ZIPFORMER_VOCAB: &str = "config.json";

/// Application configuration constants — Gipformer 65M Vietnamese ASR

pub const GIPFORMER_MODEL_NAME: &str = "gipformer-65m-rnnt";

pub const GIPFORMER_INT8_HF_URL: &str =
    "https://huggingface.co/g-group-ai-lab/gipformer-65M-rnnt/resolve/main";
pub const GIPFORMER_INT8_SUBDIR: &str = "gipformer-vi-int8";
pub const GIPFORMER_INT8_ENCODER: &str = "encoder-epoch-35-avg-6.int8.onnx";
pub const GIPFORMER_INT8_DECODER: &str = "decoder-epoch-35-avg-6.int8.onnx";
pub const GIPFORMER_INT8_JOINER: &str = "joiner-epoch-35-avg-6.int8.onnx";
pub const GIPFORMER_INT8_SIZE_BYTES: u64 = 71_000_000;

pub const GIPFORMER_FULL_HF_URL: &str =
    "https://huggingface.co/g-group-ai-lab/gipformer-65M-rnnt/resolve/main";
pub const GIPFORMER_FULL_SUBDIR: &str = "gipformer-vi-full";
pub const GIPFORMER_FULL_ENCODER: &str = "encoder-epoch-35-avg-6.onnx";
pub const GIPFORMER_FULL_DECODER: &str = "decoder-epoch-35-avg-6.onnx";
pub const GIPFORMER_FULL_JOINER: &str = "joiner-epoch-35-avg-6.onnx";
pub const GIPFORMER_FULL_SIZE_BYTES: u64 = 261_000_000;

pub const GIPFORMER_BPE: &str = "bpe.model";
pub const GIPFORMER_TOKENS: &str = "tokens.txt";
pub const GIPFORMER_VOCAB_FALLBACK: &str = "config.json";

/// Application configuration constants — Sherpa-ONNX Zipformer VI 2025 ASR (full precision only)

pub const SHERPA_VI_2025_MODEL_NAME: &str = "sherpa-onnx-zipformer-vi-2025-04-20";

pub const SHERPA_VI_2025_HF_URL: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20/resolve/main";
pub const SHERPA_VI_2025_SUBDIR: &str = "sherpa-vi-2025-full";
pub const SHERPA_VI_2025_ENCODER: &str = "encoder-epoch-12-avg-8.onnx";
pub const SHERPA_VI_2025_DECODER: &str = "decoder-epoch-12-avg-8.onnx";
pub const SHERPA_VI_2025_JOINER: &str = "joiner-epoch-12-avg-8.onnx";
pub const SHERPA_VI_2025_SIZE_BYTES: u64 = 261_000_000;

pub const SHERPA_VI_2025_BPE: &str = "bpe.model";
pub const SHERPA_VI_2025_TOKENS: &str = "tokens.txt";

/// Live streaming ZipFormer (chunk-64) — test ASR OnlineRecognizer path
pub const ZIPFORMER_STREAMING_MODEL_NAME: &str = "zipformer-vi-30m-streaming";
pub const ZIPFORMER_STREAMING_HF_URL: &str =
    "https://huggingface.co/hynt/Zipformer-30M-RNNT-Streaming-6000h/resolve/main";
pub const ZIPFORMER_STREAMING_SUBDIR: &str = "zipformer-vi-streaming";
pub const ZIPFORMER_STREAMING_ENCODER: &str =
    "encoder-epoch-31-avg-11-chunk-64-left-128.fp16.onnx";
pub const ZIPFORMER_STREAMING_DECODER: &str =
    "decoder-epoch-31-avg-11-chunk-64-left-128.fp16.onnx";
pub const ZIPFORMER_STREAMING_JOINER: &str =
    "joiner-epoch-31-avg-11-chunk-64-left-128.fp16.onnx";
pub const ZIPFORMER_STREAMING_SIZE_BYTES: u64 = 51_000_000;
pub const ZIPFORMER_STREAMING_BPE: &str = "bpe.model";
pub const ZIPFORMER_STREAMING_TOKENS: &str = "tokens.txt";
pub const ZIPFORMER_STREAMING_TOKENS_RESOURCE: &str = "zipformer-streaming-tokens.txt";
pub const ZIPFORMER_STREAMING_MAX_UTTERANCE_SECS: f64 = 12.0;

/// Application configuration constants — CAPU Vietnamese punctuation restoration

pub const CAPU_MODEL_NAME: &str = "vibert-capu-vi";
pub const CAPU_SUBDIR: &str = "capu-vi";

pub const CAPU_HF_URL: &str = "https://huggingface.co/welcomyou/vibert-capu-onnx/resolve/main";

pub const CAPU_MODEL_FILE: &str = "vibert-capu.int8.onnx";
pub const CAPU_VOCAB_FILE: &str = "vocab.txt";
pub const CAPU_LABELS_FILE: &str = "vocabulary/labels.txt";
pub const CAPU_DTAGS_FILE: &str = "vocabulary/d_tags.txt";

// Approximate sizes, used only for the download progress bar
pub const CAPU_MODEL_SIZE_BYTES: u64 = 110_000_000;
pub const CAPU_VOCAB_SIZE_BYTES: u64 = 500_000;
pub const CAPU_LABELS_SIZE_BYTES: u64 = 300;
pub const CAPU_DTAGS_SIZE_BYTES: u64 = 100;

pub const CAPU_MAX_SEQ_LEN: usize = 512;
pub const CAPU_MAX_ITERATIONS: usize = 3;
pub const CAPU_TRAILING_CONTEXT_WORDS: usize = 15;
/// Matches the reference app's own `chunk_size=56` (tokens, vs. words here — close enough
/// given ~1 subword/word in Vietnamese). This is a real quality bound, not just a tuning
/// knob: CAPU is a BERT-style self-attention model, and empirically (real ROVER transcript,
/// same text/pause-hints, budget varied 200/56/40/30) it gets measurably more
/// conservative — favoring `$KEEP` over `$APPEND_.` even at genuine >=1s pauses — as the
/// sequence it's asked to judge in one call gets longer. 200 (chosen purely to minimize
/// CAPU's fixed per-call overhead) produced 10 periods over 786 words; 56 produced 16 with
/// no chunk-boundary artifacts. Shrinking further (40/30) pushed periods higher still (21,
/// 45) but by forcing spurious sentence breaks right at arbitrary chunk-cut points instead
/// of real pauses — 56 is the sweet spot between the two failure modes.
pub const CAPU_BATCH_WORD_BUDGET: usize = 56;

/// Vietnamese ITN — bundled FAR resources (no download)
pub const ITN_RESOURCE_SUBDIR: &str = "itn-vi";
pub const ITN_CLASSIFY_FAR: &str = "tokenize_and_classify.far";
pub const ITN_VERBALIZE_FAR: &str = "verbalize.far";

/// Bundled default hotwords list (tên riêng / thuật ngữ chuyên ngành)
pub const HOTWORDS_RESOURCE_FILE: &str = "hotwords.txt";

/// Speaker diarization — Community-1 Pure ORT (matches test ASR `community1_pure_ort`)
pub const DIARIZATION_SUBDIR: &str = "diarization-community1";
pub const DIARIZATION_SEG_FILE: &str = "segmentation-community-1.onnx";
pub const DIARIZATION_SEG_SIZE_BYTES: u64 = 5_916_375;
pub const DIARIZATION_EMB_ENCODER_FILE: &str = "embedding_encoder.onnx";
pub const DIARIZATION_EMB_ENCODER_SIZE_BYTES: u64 = 21_306_024;
pub const DIARIZATION_EMB_WEIGHT_FILE: &str = "resnet_seg_1_weight.npy";
pub const DIARIZATION_EMB_WEIGHT_SIZE_BYTES: u64 = 5_243_008;
pub const DIARIZATION_EMB_BIAS_FILE: &str = "resnet_seg_1_bias.npy";
pub const DIARIZATION_EMB_BIAS_SIZE_BYTES: u64 = 1_152;
pub const DIARIZATION_PLDA_PREPARED_FILE: &str = "plda/plda_prepared.npz";
pub const DIARIZATION_PLDA_PREPARED_SIZE_BYTES: u64 = 268_226;

pub const DIARIZATION_SAMPLE_RATE: u32 = 16_000;
pub const DIARIZATION_CHUNK_DURATION_SEC: f64 = 10.0;
pub const DIARIZATION_CHUNK_STEP_SEC: f64 = 1.0;
pub const DIARIZATION_DEFAULT_THRESHOLD: f64 = 0.6;
pub const DIARIZATION_DEFAULT_FA: f64 = 0.07;
pub const DIARIZATION_DEFAULT_FB: f64 = 0.8;
