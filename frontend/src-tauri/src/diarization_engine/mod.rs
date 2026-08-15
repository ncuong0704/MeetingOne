//! Senko CAM++ speaker diarization (file import / retranscription).

mod align;
mod clustering;
pub mod commands;
mod embedding;
mod engine;

pub use align::{resegment_by_speaker_turns, speaker_color_for_index, AlignedPiece, SpeakerTurn};
pub use engine::{DiarizationConfig, DiarizationEngine};
