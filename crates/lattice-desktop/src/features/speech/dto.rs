//! Speech DTOs exposed over Tauri IPC.

use serde::{Deserialize, Serialize};

/// The English a voice speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SpeechAccentDto {
    American,
    British,
}

/// How a voice's speaker is described in Kokoro's voice list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SpeechGenderDto {
    Female,
    Male,
}

/// One of the voices Kokoro can speak in.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechVoiceDto {
    /// Kokoro's voice id, e.g. `bf_emma`.
    pub id: String,
    pub name: String,
    pub accent: SpeechAccentDto,
    pub gender: SpeechGenderDto,
    /// hexgrad's overall grade for the voice's training data, `A` to `F+`.
    pub grade: String,
    /// True when everything this voice needs is on disk.
    pub downloaded: bool,
}

/// The second download: the voices and accents the first one leaves out.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechVoicePackDto {
    /// True when the pack is downloaded and complete.
    pub ready: bool,
    /// The catalog id to download.
    pub model_id: String,
    pub model_name: String,
    /// Total bytes of the download.
    pub download_bytes: f64,
}

/// Whether read-aloud can speak now, and what downloading it involves.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatusDto {
    /// True when the voice model is downloaded and complete.
    pub model_ready: bool,
    /// The catalog id to download.
    pub model_id: String,
    pub model_name: String,
    /// Total bytes of the download.
    pub download_bytes: f64,
    pub license: String,
    pub voice_pack: SpeechVoicePackDto,
    pub voices: Vec<SpeechVoiceDto>,
    pub default_voice: String,
}

/// Speech for one piece of text.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechClipDto {
    pub sample_rate: u32,
    /// Mono 16-bit little-endian PCM, base64.
    pub pcm: String,
    pub duration_ms: f64,
}
