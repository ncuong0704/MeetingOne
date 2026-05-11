// audio/transcription/mod.rs
//
// Transcription module: ZipFormer Vietnamese ASR provider.

pub mod engine;
pub mod provider;
pub mod worker;
pub mod zipformer_provider;

pub use engine::{
    get_or_init_transcription_engine, validate_transcription_model_ready, TranscriptionEngine,
};
pub use provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
pub use worker::{reset_speech_detected_flag, start_transcription_task, TranscriptUpdate};
pub use zipformer_provider::ZipFormerProvider;
