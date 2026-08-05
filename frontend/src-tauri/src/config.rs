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
pub const CAPU_BATCH_WORD_BUDGET: usize = 200;

/// Vietnamese ITN — bundled FAR resources (no download)
pub const ITN_RESOURCE_SUBDIR: &str = "itn-vi";
pub const ITN_CLASSIFY_FAR: &str = "tokenize_and_classify.far";
pub const ITN_VERBALIZE_FAR: &str = "verbalize.far";

/// Bundled default hotwords list (tên riêng / thuật ngữ chuyên ngành)
pub const HOTWORDS_RESOURCE_FILE: &str = "hotwords.txt";
