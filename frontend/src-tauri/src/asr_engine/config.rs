use crate::database::models::TranscriptSetting;
use crate::asr_engine::model_family::{ModelFamily, ModelVariant};

/// ASR processing path — live recording vs file import/retranscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrPath {
    Live,
    File,
}

/// Resolved ASR configuration for one path, with legacy column fallback.
#[derive(Debug, Clone)]
pub struct PathAsrConfig {
    pub family_id: String,
    pub variant: ModelVariant,
    pub decoding_method: String,
    pub num_active_paths: i32,
    pub max_segment_seconds: u32,
    pub rover_enabled: bool,
    pub rover_family_b: Option<String>,
    pub rover_variant_b: Option<String>,
}

impl PathAsrConfig {
    pub fn from_transcript_setting(row: &TranscriptSetting, path: AsrPath) -> Self {
        match path {
            AsrPath::Live => Self::resolve_live(row),
            AsrPath::File => Self::resolve_file(row),
        }
    }

    fn resolve_live(row: &TranscriptSetting) -> Self {
        let family_id = row
            .live_model
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| row.model.clone());
        let variant_str = row
            .live_asr_variant
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| row.asr_variant.clone());
        let decoding_method = row
            .live_decoding_method
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| row.decoding_method.clone());
        let num_active_paths = row
            .live_num_active_paths
            .unwrap_or(row.num_active_paths);
        let max_seg = row
            .live_max_segment_seconds
            .unwrap_or(row.max_segment_seconds);

        let family = ModelFamily::from_id(&family_id);
        let requested = ModelVariant::from_str(&variant_str);
        let variant = if family.available_variants().contains(&requested) {
            requested
        } else {
            family.available_variants()[0]
        };

        Self {
            family_id: family.id().to_string(),
            variant,
            decoding_method,
            num_active_paths,
            max_segment_seconds: crate::audio::common::clamp_max_segment_seconds(max_seg),
            rover_enabled: false,
            rover_family_b: None,
            rover_variant_b: None,
        }
    }

    fn resolve_file(row: &TranscriptSetting) -> Self {
        let family_id = row
            .file_model
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| row.model.clone());
        let variant_str = row
            .file_asr_variant
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| row.asr_variant.clone());
        let decoding_method = row
            .file_decoding_method
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| row.decoding_method.clone());
        let num_active_paths = row
            .file_num_active_paths
            .unwrap_or(row.num_active_paths);
        let max_seg = row
            .file_max_segment_seconds
            .unwrap_or(row.max_segment_seconds);
        let rover_enabled = row.file_rover_enabled.unwrap_or(row.rover_enabled);
        let rover_family_b = row
            .file_rover_family_b
            .clone()
            .or_else(|| row.rover_family_b.clone());
        let rover_variant_b = row
            .file_rover_variant_b
            .clone()
            .or_else(|| row.rover_variant_b.clone());

        let family = ModelFamily::from_id(&family_id);
        let family = if family.is_online_streaming() {
            ModelFamily::ZipFormer30M
        } else {
            family
        };
        let requested = ModelVariant::from_str(&variant_str);
        let variant = if family.available_variants().contains(&requested) {
            requested
        } else {
            family.available_variants()[0]
        };

        Self {
            family_id: family.id().to_string(),
            variant,
            decoding_method,
            num_active_paths,
            max_segment_seconds: crate::audio::common::clamp_max_segment_seconds(max_seg),
            rover_enabled,
            rover_family_b,
            rover_variant_b,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> TranscriptSetting {
        TranscriptSetting {
            id: "1".to_string(),
            provider: "asr".to_string(),
            model: "zipformer-vi-30m".to_string(),
            asr_variant: "int8".to_string(),
            decoding_method: "modified_beam_search".to_string(),
            num_active_paths: 15,
            max_segment_seconds: 25,
            rover_enabled: true,
            rover_family_b: Some("gipformer-65m-rnnt".to_string()),
            rover_variant_b: Some("int8".to_string()),
            hotwords: None,
            capu_cpu_threads: None,
            capu_punctuation_level: 7,
            capu_case_level: 3,
            live_model: Some("zipformer-vi-30m".to_string()),
            live_asr_variant: Some("int8".to_string()),
            live_decoding_method: Some("modified_beam_search".to_string()),
            live_num_active_paths: Some(15),
            live_max_segment_seconds: Some(20),
            file_model: Some("gipformer-65m-rnnt".to_string()),
            file_asr_variant: Some("int8".to_string()),
            file_decoding_method: Some("greedy_search".to_string()),
            file_num_active_paths: Some(10),
            file_max_segment_seconds: Some(30),
            file_rover_enabled: Some(true),
            file_rover_family_b: Some("sherpa-onnx-zipformer-vi-2025-04-20".to_string()),
            file_rover_variant_b: Some("full".to_string()),
            diarization_enabled: false,
            diarization_num_speakers: None,
        }
    }

    #[test]
    fn live_config_ignores_file_rover() {
        let row = sample_row();
        let cfg = PathAsrConfig::from_transcript_setting(&row, AsrPath::Live);
        assert!(!cfg.rover_enabled);
        assert_eq!(cfg.family_id, "zipformer-vi-30m");
        assert_eq!(cfg.max_segment_seconds, 20);
    }

    #[test]
    fn file_config_reads_file_columns() {
        let row = sample_row();
        let cfg = PathAsrConfig::from_transcript_setting(&row, AsrPath::File);
        assert!(cfg.rover_enabled);
        assert_eq!(cfg.family_id, "gipformer-65m-rnnt");
        assert_eq!(cfg.decoding_method, "greedy_search");
        assert_eq!(cfg.max_segment_seconds, 30);
    }

    #[test]
    fn file_config_rejects_streaming_family() {
        let mut row = sample_row();
        row.file_model = Some("zipformer-vi-30m-streaming".to_string());
        let cfg = PathAsrConfig::from_transcript_setting(&row, AsrPath::File);
        assert_eq!(cfg.family_id, "zipformer-vi-30m");
    }

    #[test]
    fn live_config_keeps_streaming_family() {
        let mut row = sample_row();
        row.live_model = Some("zipformer-vi-30m-streaming".to_string());
        row.live_asr_variant = Some("full".to_string());
        let cfg = PathAsrConfig::from_transcript_setting(&row, AsrPath::Live);
        assert_eq!(cfg.family_id, "zipformer-vi-30m-streaming");
        assert_eq!(cfg.variant, ModelVariant::Full);
    }
}
