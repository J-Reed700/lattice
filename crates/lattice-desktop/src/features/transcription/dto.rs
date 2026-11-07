//! Transcription DTOs exposed over Tauri IPC.

use serde::{Deserialize, Serialize};

use lattice_core::application::ports::transcription_port::Transcript;

/// One contiguous run of transcribed speech.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegmentDto {
    /// Offset of the first word from the start of the recording.
    pub start_ms: u64,
    /// Offset of the last word from the start of the recording.
    pub end_ms: u64,
    /// The spoken text.
    pub text: String,
}

/// The transcript of one audio file.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptDto {
    /// Absolute path of the transcribed file.
    pub file_path: String,
    /// Language tag reported by the model.
    pub language: String,
    /// Decoded audio duration.
    pub duration_ms: u64,
    /// Segments in playback order.
    pub segments: Vec<TranscriptSegmentDto>,
    /// The rendered document text, with `[m:ss–m:ss]` window markers.
    pub text: String,
}

/// What was said in a recording made in the app.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TranscribedSpeechDto {
    /// The words, empty when the recording was not speech.
    pub text: String,
    /// Language tag reported by the model.
    pub language: String,
    /// Length of the recording.
    pub duration_ms: u64,
}

/// Whether transcription is available right now.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionStatusDto {
    /// True when a transcription model is downloaded.
    pub model_ready: bool,
    /// Display name of the model that would be used.
    pub model_name: Option<String>,
    /// What to download for a voice conversation.
    pub voice_model: VoiceModelDto,
}

/// The transcription model offered for a voice conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceModelDto {
    /// Catalog id, to start the download with.
    pub model_id: String,
    /// Display name.
    pub model_name: String,
    /// Size of the download.
    pub download_bytes: f64,
    /// Licence of the weights.
    pub license: String,
}

impl TranscriptDto {
    /// Build the DTO from a domain transcript plus its rendered text.
    pub fn from_transcript(file_path: String, transcript: &Transcript, text: String) -> Self {
        Self {
            file_path,
            language: transcript.language.clone(),
            duration_ms: transcript.duration_ms,
            segments: transcript
                .segments
                .iter()
                .map(|segment| TranscriptSegmentDto {
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    text: segment.text.clone(),
                })
                .collect(),
            text,
        }
    }
}
