/// Supported audio file extensions for import and retranscription.
///
/// Includes native Symphonia formats (MP4, M4A, WAV, MP3, FLAC, OGG, AAC)
/// and FFmpeg-backed formats (MKV, WebM, WMA).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp4", "m4a", "wav", "mp3", "flac", "ogg", "aac", "mkv", "webm", "wma",
];
// Queue budgets for capture callbacks, streaming frames, VAD segments and saver.
pub const CAPTURE_QUEUE_CAPACITY: usize = 4096;
pub const STREAMING_ASR_QUEUE_CAPACITY: usize = 4096;
pub const OFFLINE_ASR_QUEUE_CAPACITY: usize = 8;
pub const RECORDING_QUEUE_CAPACITY: usize = 512;
