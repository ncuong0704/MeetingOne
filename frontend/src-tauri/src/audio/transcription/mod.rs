// audio/transcription/mod.rs
//
// Transcription module: Vietnamese ASR provider.

pub mod asr_provider;
pub mod engine;
pub mod live_speaker;
pub mod provider;
pub mod streaming_worker;
pub mod worker;

pub use asr_provider::AsrProvider;
pub use engine::{
    get_or_init_transcription_engine, validate_transcription_model_ready, TranscriptionEngine,
};
pub use provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
pub use streaming_worker::start_streaming_task;
pub use worker::{reset_speech_detected_flag, start_transcription_task, TranscriptFinalized, TranscriptUpdate};
