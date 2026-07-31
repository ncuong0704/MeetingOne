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
