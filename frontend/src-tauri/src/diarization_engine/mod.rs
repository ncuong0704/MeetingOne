//! Offline speaker diarization — Community-1 Pure ORT (file import).

pub mod align;
pub mod plda;

pub use align::{align_speakers_to_segments, speaker_color_for_index, SpeakerTurn};
pub use plda::{load_plda, plda_transform, vbx_cluster, vbx_hard_labels, xvec_transform, PldaData};
