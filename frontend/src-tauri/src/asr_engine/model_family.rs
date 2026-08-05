use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelFamily {
    ZipFormer30M,
    Gipformer65M,
    SherpaZipformerVi2025,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelVariant {
    #[default]
    Int8,
    Full,
}

impl ModelFamily {
    pub fn from_id(s: &str) -> Self {
        match s {
            crate::config::GIPFORMER_MODEL_NAME => ModelFamily::Gipformer65M,
            crate::config::SHERPA_VI_2025_MODEL_NAME => ModelFamily::SherpaZipformerVi2025,
            _ => ModelFamily::ZipFormer30M,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => crate::config::ZIPFORMER_MODEL_NAME,
            ModelFamily::Gipformer65M => crate::config::GIPFORMER_MODEL_NAME,
            ModelFamily::SherpaZipformerVi2025 => crate::config::SHERPA_VI_2025_MODEL_NAME,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => "ZipFormer 30M",
            ModelFamily::Gipformer65M => "Gipformer 65M",
            ModelFamily::SherpaZipformerVi2025 => "Sherpa-ONNX Zipformer VI (2025)",
        }
    }

    /// Which `ModelVariant`s this family actually ships. `SherpaZipformerVi2025` has no
    /// int8 build upstream — callers (UI, engine) must check this before using `Int8`
    /// with that family; the per-variant lookup functions below panic on that combination.
    pub fn available_variants(self) -> &'static [ModelVariant] {
        match self {
            ModelFamily::ZipFormer30M => &[ModelVariant::Int8, ModelVariant::Full],
            ModelFamily::Gipformer65M => &[ModelVariant::Int8, ModelVariant::Full],
            ModelFamily::SherpaZipformerVi2025 => &[ModelVariant::Full],
        }
    }

    pub fn variant_subdir(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_SUBDIR,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_SUBDIR,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_SUBDIR,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_SUBDIR,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_SUBDIR,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn hf_url(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_HF_URL,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_HF_URL,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_HF_URL,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_HF_URL,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_HF_URL,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn encoder_file(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_ENCODER,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_ENCODER,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_ENCODER,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_ENCODER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_ENCODER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn decoder_file(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_DECODER,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_DECODER,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_DECODER,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_DECODER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_DECODER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn joiner_file(self, variant: ModelVariant) -> &'static str {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_JOINER,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_JOINER,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_JOINER,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_JOINER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_JOINER,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn bpe_file(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => crate::config::ZIPFORMER_BPE,
            ModelFamily::Gipformer65M => crate::config::GIPFORMER_BPE,
            ModelFamily::SherpaZipformerVi2025 => crate::config::SHERPA_VI_2025_BPE,
        }
    }

    pub fn token_file(self) -> &'static str {
        match self {
            ModelFamily::ZipFormer30M => crate::config::ZIPFORMER_VOCAB,
            ModelFamily::Gipformer65M => crate::config::GIPFORMER_TOKENS,
            ModelFamily::SherpaZipformerVi2025 => crate::config::SHERPA_VI_2025_TOKENS,
        }
    }

    pub fn encoder_size_bytes(self, variant: ModelVariant) -> u64 {
        match (self, variant) {
            (ModelFamily::ZipFormer30M, ModelVariant::Int8) => crate::config::ZIPFORMER_INT8_SIZE_BYTES,
            (ModelFamily::ZipFormer30M, ModelVariant::Full) => crate::config::ZIPFORMER_FULL_SIZE_BYTES,
            (ModelFamily::Gipformer65M, ModelVariant::Int8) => crate::config::GIPFORMER_INT8_SIZE_BYTES,
            (ModelFamily::Gipformer65M, ModelVariant::Full) => crate::config::GIPFORMER_FULL_SIZE_BYTES,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Full) => crate::config::SHERPA_VI_2025_SIZE_BYTES,
            (ModelFamily::SherpaZipformerVi2025, ModelVariant::Int8) => {
                unreachable!("SherpaZipformerVi2025 has no int8 variant — check available_variants() first")
            }
        }
    }

    pub fn model_files(self, variant: ModelVariant) -> [&'static str; 5] {
        [
            self.encoder_file(variant),
            self.decoder_file(variant),
            self.joiner_file(variant),
            self.bpe_file(),
            self.token_file(),
        ]
    }

    pub fn total_size_bytes(self, variant: ModelVariant) -> u64 {
        let shared: u64 = 268_000 + 50_000 + 1_310_000;
        self.encoder_size_bytes(variant) + shared
    }
}

impl ModelVariant {
    pub fn from_str(s: &str) -> Self {
        match s {
            "full" => ModelVariant::Full,
            _ => ModelVariant::Int8,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ModelVariant::Int8 => crate::config::ZIPFORMER_VARIANT_INT8,
            ModelVariant::Full => crate::config::ZIPFORMER_VARIANT_FULL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zipformer30m_int8_files_match_existing_layout() {
        let files = ModelFamily::ZipFormer30M.model_files(ModelVariant::Int8);
        assert_eq!(files[0], "encoder-epoch-20-avg-10.int8.onnx");
        assert_eq!(files[4], "config.json");
        assert_eq!(
            ModelFamily::ZipFormer30M.variant_subdir(ModelVariant::Int8),
            "zipformer-vi-int8"
        );
    }

    #[test]
    fn gipformer_int8_uses_separate_subdir_and_tokens() {
        let files = ModelFamily::Gipformer65M.model_files(ModelVariant::Int8);
        assert_eq!(files[0], "encoder-epoch-35-avg-6.int8.onnx");
        assert_eq!(files[4], "tokens.txt");
        assert_eq!(
            ModelFamily::Gipformer65M.variant_subdir(ModelVariant::Int8),
            "gipformer-vi-int8"
        );
        assert_ne!(
            ModelFamily::Gipformer65M.variant_subdir(ModelVariant::Int8),
            ModelFamily::ZipFormer30M.variant_subdir(ModelVariant::Int8)
        );
    }

    #[test]
    fn from_id_roundtrip() {
        assert_eq!(
            ModelFamily::from_id("gipformer-65m-rnnt"),
            ModelFamily::Gipformer65M
        );
        assert_eq!(
            ModelFamily::from_id("zipformer-vi-30m"),
            ModelFamily::ZipFormer30M
        );
    }

    #[test]
    fn sherpa_vi_2025_full_files_and_subdir() {
        let files = ModelFamily::SherpaZipformerVi2025.model_files(ModelVariant::Full);
        assert_eq!(files[0], "encoder-epoch-12-avg-8.onnx");
        assert_eq!(files[1], "decoder-epoch-12-avg-8.onnx");
        assert_eq!(files[2], "joiner-epoch-12-avg-8.onnx");
        assert_eq!(files[3], "bpe.model");
        assert_eq!(files[4], "tokens.txt");
        assert_eq!(
            ModelFamily::SherpaZipformerVi2025.variant_subdir(ModelVariant::Full),
            "sherpa-vi-2025-full"
        );
    }

    #[test]
    fn sherpa_vi_2025_has_full_variant_only() {
        assert_eq!(
            ModelFamily::SherpaZipformerVi2025.available_variants(),
            &[ModelVariant::Full]
        );
        assert_eq!(
            ModelFamily::ZipFormer30M.available_variants(),
            &[ModelVariant::Int8, ModelVariant::Full]
        );
        assert_eq!(
            ModelFamily::Gipformer65M.available_variants(),
            &[ModelVariant::Int8, ModelVariant::Full]
        );
    }

    #[test]
    fn from_id_includes_sherpa_vi_2025() {
        assert_eq!(
            ModelFamily::from_id("sherpa-onnx-zipformer-vi-2025-04-20"),
            ModelFamily::SherpaZipformerVi2025
        );
        assert_eq!(ModelFamily::SherpaZipformerVi2025.id(), "sherpa-onnx-zipformer-vi-2025-04-20");
    }
}
