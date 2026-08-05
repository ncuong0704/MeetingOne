use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MeetingModel {
    pub id: String,
    pub title: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub folder_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct DateTimeUtc(pub DateTime<Utc>);

impl From<NaiveDateTime> for DateTimeUtc {
    fn from(naive: NaiveDateTime) -> Self {
        DateTimeUtc(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MeetingDocument {
    pub id: String,
    pub meeting_id: String,
    pub filename: String,
    pub extracted_text: String,
    pub char_count: i64,
    pub created_at: DateTimeUtc,
}

// Renamed from TranscriptSegment to Transcript to match the table name
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Transcript {
    pub id: String,
    pub meeting_id: String,
    pub transcript: String,
    pub timestamp: String,
    pub summary: Option<String>,
    pub action_items: Option<String>,
    pub key_points: Option<String>,
    // Recording-relative timestamps for audio-transcript synchronization
    pub audio_start_time: Option<f64>,
    pub audio_end_time: Option<f64>,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SummaryProcess {
    pub meeting_id: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub error: Option<String>,
    pub result: Option<String>, // JSON
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub chunk_count: i64,
    pub processing_time: f64,
    pub metadata: Option<String>, // JSON
    pub result_backup: Option<String>, // Backup of result before regeneration
    pub result_backup_timestamp: Option<chrono::DateTime<chrono::Utc>>, // When backup was created
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TranscriptChunk {
    pub meeting_id: String,
    pub meeting_name: Option<String>,
    pub transcript_text: String,
    pub model: String,
    pub model_name: String,
    pub chunk_size: Option<i64>,
    pub overlap: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Setting {
    pub id: String,
    pub provider: String,
    pub model: String,
    #[sqlx(rename = "openaiApiKey")]
    #[serde(rename = "openaiApiKey")]
    pub openai_api_key: Option<String>,
    #[sqlx(rename = "anthropicApiKey")]
    #[serde(rename = "anthropicApiKey")]
    pub anthropic_api_key: Option<String>,
    #[sqlx(rename = "openRouterApiKey")]
    #[serde(rename = "openRouterApiKey")]
    pub open_router_api_key: Option<String>,
    /// Custom OpenAI-compatible endpoint configuration stored as JSON
    #[sqlx(rename = "customOpenAIConfig")]
    #[serde(rename = "customOpenAIConfig")]
    pub custom_openai_config: Option<String>,
    /// Per-provider fallback model list stored as JSON map
    #[sqlx(rename = "fallbackModels")]
    #[serde(rename = "fallbackModels")]
    pub fallback_models: Option<String>,
    /// Custom LLM prompts stored as JSON (PromptConfig). NULL = use built-in defaults.
    #[sqlx(rename = "promptSettings")]
    #[serde(rename = "promptSettings")]
    pub prompt_settings: Option<String>,
    /// ID of the default template to use for summary generation. NULL = use app default.
    #[sqlx(rename = "defaultTemplate")]
    #[serde(rename = "defaultTemplate")]
    pub default_template: Option<String>,
}

impl Setting {
    /// Parse the custom OpenAI config from JSON string
    pub fn get_custom_openai_config(&self) -> Option<crate::summary::CustomOpenAIConfig> {
        self.custom_openai_config.as_ref().and_then(|json| {
            serde_json::from_str(json).ok()
        })
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TranscriptSetting {
    pub id: String,
    pub provider: String,
    pub model: String,
    #[sqlx(rename = "asrVariant")]
    #[serde(rename = "asrVariant")]
    pub asr_variant: String,
    #[sqlx(rename = "decodingMethod")]
    #[serde(rename = "decodingMethod")]
    pub decoding_method: String,
    #[sqlx(rename = "numActivePaths")]
    #[serde(rename = "numActivePaths")]
    pub num_active_paths: i32,
    #[sqlx(rename = "maxSegmentSeconds")]
    #[serde(rename = "maxSegmentSeconds")]
    pub max_segment_seconds: i32,
    #[sqlx(rename = "roverEnabled")]
    #[serde(rename = "roverEnabled")]
    pub rover_enabled: bool,
    #[sqlx(rename = "roverFamilyB")]
    #[serde(rename = "roverFamilyB")]
    pub rover_family_b: Option<String>,
    #[sqlx(rename = "roverVariantB")]
    #[serde(rename = "roverVariantB")]
    pub rover_variant_b: Option<String>,
    pub hotwords: Option<String>,
    #[sqlx(rename = "capuCpuThreads")]
    #[serde(rename = "capuCpuThreads")]
    pub capu_cpu_threads: Option<i32>,
    #[sqlx(rename = "capuPunctuationLevel")]
    #[serde(rename = "capuPunctuationLevel")]
    pub capu_punctuation_level: i32,
    #[sqlx(rename = "capuCaseLevel")]
    #[serde(rename = "capuCaseLevel")]
    pub capu_case_level: i32,
    #[sqlx(rename = "liveModel")]
    #[serde(rename = "liveModel")]
    pub live_model: Option<String>,
    #[sqlx(rename = "liveAsrVariant")]
    #[serde(rename = "liveAsrVariant")]
    pub live_asr_variant: Option<String>,
    #[sqlx(rename = "liveDecodingMethod")]
    #[serde(rename = "liveDecodingMethod")]
    pub live_decoding_method: Option<String>,
    #[sqlx(rename = "liveNumActivePaths")]
    #[serde(rename = "liveNumActivePaths")]
    pub live_num_active_paths: Option<i32>,
    #[sqlx(rename = "liveMaxSegmentSeconds")]
    #[serde(rename = "liveMaxSegmentSeconds")]
    pub live_max_segment_seconds: Option<i32>,
    #[sqlx(rename = "fileModel")]
    #[serde(rename = "fileModel")]
    pub file_model: Option<String>,
    #[sqlx(rename = "fileAsrVariant")]
    #[serde(rename = "fileAsrVariant")]
    pub file_asr_variant: Option<String>,
    #[sqlx(rename = "fileDecodingMethod")]
    #[serde(rename = "fileDecodingMethod")]
    pub file_decoding_method: Option<String>,
    #[sqlx(rename = "fileNumActivePaths")]
    #[serde(rename = "fileNumActivePaths")]
    pub file_num_active_paths: Option<i32>,
    #[sqlx(rename = "fileMaxSegmentSeconds")]
    #[serde(rename = "fileMaxSegmentSeconds")]
    pub file_max_segment_seconds: Option<i32>,
    #[sqlx(rename = "fileRoverEnabled")]
    #[serde(rename = "fileRoverEnabled")]
    pub file_rover_enabled: Option<bool>,
    #[sqlx(rename = "fileRoverFamilyB")]
    #[serde(rename = "fileRoverFamilyB")]
    pub file_rover_family_b: Option<String>,
    #[sqlx(rename = "fileRoverVariantB")]
    #[serde(rename = "fileRoverVariantB")]
    pub file_rover_variant_b: Option<String>,
}
