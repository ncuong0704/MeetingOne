//! Offline speaker diarization — Community-1 Pure ORT (file import).

pub mod align;
pub mod commands;
pub mod embedding;
pub mod engine;
pub mod plda;
pub mod segmentation;

pub use align::{
    align_speakers_to_segments, resegment_by_speaker_turns, speaker_color_for_index, AlignedPiece,
    SpeakerTurn,
};
pub use engine::{DiarizationConfig, DiarizationEngine};
pub use plda::{load_plda, plda_transform, vbx_cluster, vbx_hard_labels, xvec_transform, PldaData};
