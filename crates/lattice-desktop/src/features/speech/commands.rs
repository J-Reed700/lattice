//! Speech command implementations.
//!
//! # Commands (2 total)
//!
//! - `get_speech_status` — whether the read-aloud voices are downloaded
//! - `synthesize_speech` — one piece of text as audio

use base64::Engine;

use crate::features::speech::dto::{SpeechClipDto, SpeechStatusDto, SpeechVoiceDto};
use crate::interfaces::di::Container;
use lattice_core::domain::models::curated::{
    get_curated_speech_models, DEFAULT_SPEECH_VOICE, SPEECH_MODEL_ID, SPEECH_VOICES,
};
use lattice_core::shared::error::{AppError, Result};

/// Longest text one call reads; the renderer sends a sentence or two.
const MAX_TEXT_CHARS: usize = 4_000;
const SPEEDS: std::ops::RangeInclusive<f32> = 0.5..=2.0;

pub async fn get_speech_status_impl(container: &Container) -> Result<SpeechStatusDto> {
    let entry = get_curated_speech_models()
        .into_iter()
        .find(|model| model.id == SPEECH_MODEL_ID)
        .ok_or_else(|| AppError::InternalError("The speech model is not in the catalog.".into()))?;
    Ok(SpeechStatusDto {
        model_ready: container.speech().model_dir().await?.is_some(),
        model_id: entry.id,
        model_name: entry.name,
        download_bytes: entry.total_size_bytes as f64,
        license: entry.license,
        voices: SPEECH_VOICES
            .iter()
            .map(|voice| SpeechVoiceDto {
                id: voice.id.to_string(),
                name: voice.name.to_string(),
            })
            .collect(),
        default_voice: DEFAULT_SPEECH_VOICE.to_string(),
    })
}

/// Reads `text` aloud; the samples go back as 16-bit PCM, which halves what
/// crosses IPC and is all a browser audio buffer keeps of them anyway.
pub async fn synthesize_speech_impl(
    container: &Container,
    text: String,
    voice: String,
    speed: f32,
) -> Result<SpeechClipDto> {
    if !SPEECH_VOICES.iter().any(|v| v.id == voice) {
        return Err(AppError::InvalidInput(format!("Unknown voice: {voice}")));
    }
    if !SPEEDS.contains(&speed) {
        return Err(AppError::InvalidInput(format!(
            "Speed must be between {} and {}.",
            SPEEDS.start(),
            SPEEDS.end()
        )));
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(AppError::InvalidInput(format!(
            "Read at most {MAX_TEXT_CHARS} characters at a time."
        )));
    }
    let clip = container.speech().synthesize(text, voice, speed).await?;
    let pcm: Vec<u8> = clip
        .samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16).to_le_bytes())
        .collect();
    Ok(SpeechClipDto {
        sample_rate: clip.sample_rate,
        duration_ms: clip.samples.len() as f64 * 1000.0 / f64::from(clip.sample_rate),
        pcm: base64::engine::general_purpose::STANDARD.encode(pcm),
    })
}
