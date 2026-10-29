//! Speech DTOs exposed over Tauri IPC.

use serde::{Deserialize, Serialize};

/// One of the voices the downloaded model speaks in.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechVoiceDto {
    /// Kokoro's voice id, e.g. `bf_emma`.
    pub id: String,
    pub name: String,
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
