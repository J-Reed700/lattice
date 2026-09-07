//! Transcription engine: audio decoding, whisper inference, transcript rendering.

pub mod audio_decode;
pub mod transcript;
pub mod whisper;

pub use audio_decode::{decode_to_mono_16k, DecodedAudio};
pub use transcript::{
    format_timestamp, group_segments_into_windows, render_transcript, TranscriptWindow,
    TRANSCRIPT_WINDOW_SECS,
};
pub use whisper::{WhisperTranscriptionService, MAX_AUDIO_SECS};
