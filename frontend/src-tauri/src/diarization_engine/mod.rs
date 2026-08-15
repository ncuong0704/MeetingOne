//! Offline speaker diarization — Community-1 Pure ORT (file import).

pub mod align;

pub use align::{align_speakers_to_segments, speaker_color_for_index, SpeakerTurn};
