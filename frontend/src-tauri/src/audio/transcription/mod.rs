// audio/transcription/mod.rs
//
// Transcription module: Vietnamese ASR provider.

pub mod asr_provider;
pub mod engine;
pub mod provider;
pub mod worker;

pub use asr_provider::AsrProvider;
pub use engine::{
    get_or_init_transcription_engine, validate_transcription_model_ready, TranscriptionEngine,
};
pub use provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
pub use worker::{reset_speech_detected_flag, start_transcription_task, TranscriptFinalized, TranscriptUpdate};
