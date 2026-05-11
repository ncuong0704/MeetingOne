/// Application configuration constants — ZipFormer Vietnamese ASR

pub const ZIPFORMER_MODEL_NAME: &str = "zipformer-vi-30m";
pub const ZIPFORMER_HF_REPO: &str = "hynt/Zipformer-30M-RNNT-6000h";

// Int8-quantized ONNX model files (~30 MB total)
pub const ZIPFORMER_ENCODER: &str = "encoder-epoch-20-avg-10.int8.onnx";
pub const ZIPFORMER_DECODER: &str = "decoder-epoch-20-avg-10.int8.onnx";
pub const ZIPFORMER_JOINER: &str = "joiner-epoch-20-avg-10.int8.onnx";

// Vocabulary file (plain text "token id" per line, named config.json in the repo)
pub const ZIPFORMER_VOCAB: &str = "config.json";

// BPE tokenizer model
pub const ZIPFORMER_BPE: &str = "bpe.model";
