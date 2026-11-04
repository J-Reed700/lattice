//! Speech command implementations.
//!
//! # Commands (2 total)
//!
//! - `get_speech_status` — whether the read-aloud voices are downloaded
//! - `synthesize_speech` — one piece of text as audio

use base64::Engine;

use crate::features::speech::dto::{
    SpeechAccentDto, SpeechClipDto, SpeechGenderDto, SpeechStatusDto, SpeechVoiceDto,
    SpeechVoicePackDto,
};
use crate::interfaces::di::Container;
use lattice_core::domain::models::curated::{
    find_speech_voice, get_curated_speech_models, CuratedVoice, SpeechAccent, SpeechGender,
    DEFAULT_SPEECH_VOICE, SPEECH_MODEL_ID, SPEECH_VOICES, SPEECH_VOICE_PACK_ID,
};
use lattice_core::domain::models::selection::ModelMetadata;
use lattice_core::shared::error::{AppError, Result};

/// Longest text one call reads; the renderer sends a sentence or two.
const MAX_TEXT_CHARS: usize = 4_000;
const SPEEDS: std::ops::RangeInclusive<f32> = 0.5..=2.0;

fn catalog_entry(id: &str) -> Result<ModelMetadata> {
    get_curated_speech_models()
        .into_iter()
        .find(|model| model.id == id)
        .ok_or_else(|| AppError::InternalError(format!("{id} is not in the catalog.")))
}

fn voice_dto(voice: &CuratedVoice, model_ready: bool, pack_ready: bool) -> SpeechVoiceDto {
    SpeechVoiceDto {
        id: voice.id.to_string(),
        name: voice.name.to_string(),
        accent: match voice.accent() {
            SpeechAccent::American => SpeechAccentDto::American,
            SpeechAccent::British => SpeechAccentDto::British,
        },
        gender: match voice.gender() {
            SpeechGender::Female => SpeechGenderDto::Female,
            SpeechGender::Male => SpeechGenderDto::Male,
        },
        grade: voice.grade.to_string(),
        downloaded: model_ready && (voice.model_id() == SPEECH_MODEL_ID || pack_ready),
    }
}

fn speech_status(model_ready: bool, pack_ready: bool) -> Result<SpeechStatusDto> {
    let model = catalog_entry(SPEECH_MODEL_ID)?;
    let pack = catalog_entry(SPEECH_VOICE_PACK_ID)?;
    Ok(SpeechStatusDto {
        model_ready,
        model_id: model.id,
        model_name: model.name,
        download_bytes: model.total_size_bytes as f64,
        license: model.license,
        voice_pack: SpeechVoicePackDto {
            ready: pack_ready,
            model_id: pack.id,
            model_name: pack.name,
            download_bytes: pack.total_size_bytes as f64,
        },
        voices: SPEECH_VOICES
            .iter()
            .map(|voice| voice_dto(voice, model_ready, pack_ready))
            .collect(),
        default_voice: DEFAULT_SPEECH_VOICE.to_string(),
    })
}

pub async fn get_speech_status_impl(container: &Container) -> Result<SpeechStatusDto> {
    let speech = container.speech();
    speech_status(
        speech.model_dir().await?.is_some(),
        speech.voice_pack_dir().await?.is_some(),
    )
}

/// Reads `text` aloud; the samples go back as 16-bit PCM, which halves what
/// crosses IPC and is all a browser audio buffer keeps of them anyway.
pub async fn synthesize_speech_impl(
    container: &Container,
    text: String,
    voice: String,
    speed: f32,
) -> Result<SpeechClipDto> {
    let voice = find_speech_voice(&voice)
        .ok_or_else(|| AppError::InvalidInput(format!("Unknown voice: {voice}")))?;
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;

    fn downloaded(status: &SpeechStatusDto) -> Vec<&str> {
        status
            .voices
            .iter()
            .filter(|voice| voice.downloaded)
            .map(|voice| voice.id.as_str())
            .collect()
    }

    #[test]
    fn nothing_speaks_before_the_first_download() {
        let status = speech_status(false, false).unwrap();
        assert!(downloaded(&status).is_empty());
        assert_eq!(status.voices.len(), 28);
        // The pack alone has no model to speak with.
        assert!(downloaded(&speech_status(false, true).unwrap()).is_empty());
    }

    #[test]
    fn an_install_without_the_pack_keeps_its_voices_and_is_told_the_size_of_the_rest() {
        let status = speech_status(true, false).unwrap();
        assert!(status.model_ready);
        assert_eq!(status.model_id, "kokoro-82m-british");
        assert_eq!(status.default_voice, "bf_emma");
        assert_eq!(
            downloaded(&status),
            ["bf_emma", "bf_isabella", "bf_alice", "bf_lily"]
        );
        assert!(!status.voice_pack.ready);
        assert_eq!(status.voice_pack.model_id, SPEECH_VOICE_PACK_ID);
        assert!(status.voice_pack.download_bytes < 25_000_000.0);
    }

    #[test]
    fn every_voice_speaks_with_both_downloads() {
        let status = speech_status(true, true).unwrap();
        assert!(status.voice_pack.ready);
        assert_eq!(downloaded(&status).len(), 28);
    }

    #[test]
    fn a_voice_says_its_accent_gender_and_grade() {
        let status = speech_status(true, true).unwrap();
        let heart = status.voices.iter().find(|v| v.id == "af_heart").unwrap();
        assert_eq!(heart.name, "Heart");
        assert_eq!(heart.accent, SpeechAccentDto::American);
        assert_eq!(heart.gender, SpeechGenderDto::Female);
        assert_eq!(heart.grade, "A");
        let george = status.voices.iter().find(|v| v.id == "bm_george").unwrap();
        assert_eq!(george.accent, SpeechAccentDto::British);
        assert_eq!(george.gender, SpeechGenderDto::Male);
    }
}
