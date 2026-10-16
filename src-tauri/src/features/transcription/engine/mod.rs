//! Transcription engine: audio decoding, whisper inference, transcript rendering.

pub mod audio_decode;
pub mod whisper;

pub use audio_decode::{decode_to_mono_16k, DecodedAudio};
pub use whisper::{WhisperTranscriptionService, MAX_AUDIO_SECS};
