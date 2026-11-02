//! Hear a recording made in the app.

use std::sync::Arc;

use base64::Engine;

use crate::features::transcription::dto::TranscribedSpeechDto;
use crate::features::transcription::engine::audio_decode::TARGET_SAMPLE_RATE;
use lattice_core::application::ports::TranscriptionPort;
use lattice_core::shared::error::{AppError, Result};

/// Longest recording one call takes; the mic ends an utterance well before this.
const MAX_SPEECH_SECS: usize = 5 * 60;

/// `pcm` is mono 16-bit little-endian samples, base64, as the renderer sends them.
fn decode_pcm(pcm: &str) -> Result<Vec<f32>> {
    let max_bytes = MAX_SPEECH_SECS * TARGET_SAMPLE_RATE as usize * 2;
    if pcm.len() / 4 * 3 > max_bytes {
        return Err(AppError::InvalidInput(format!(
            "A recording can be at most {} minutes.",
            MAX_SPEECH_SECS / 60
        )));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(pcm)
        .map_err(|e| AppError::InvalidInput(format!("The recording is not base64: {e}")))?;
    if bytes.len() % 2 != 0 {
        return Err(AppError::InvalidInput(
            "The recording is not 16-bit audio.".to_string(),
        ));
    }
    Ok(bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|sample| f32::from(i16::from_le_bytes(*sample)) / 32_768.0)
        .collect())
}

/// Turns speech into text for a chat's voice conversation.
pub struct TranscribeSpeechUseCase {
    transcription: Arc<dyn TranscriptionPort>,
}

impl TranscribeSpeechUseCase {
    /// Build the use case.
    pub fn new(transcription: Arc<dyn TranscriptionPort>) -> Self {
        Self { transcription }
    }

    /// What was said in `pcm`; the text is empty when it was not speech.
    pub async fn execute(&self, pcm: &str) -> Result<TranscribedSpeechDto> {
        let samples = decode_pcm(pcm)?;
        if !self.transcription.is_ready().await? {
            return Err(AppError::ServiceNotAvailable(
                "No transcription model is downloaded.".to_string(),
            ));
        }

        let transcript = self.transcription.transcribe_samples(samples).await?;
        let text = transcript
            .segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        Ok(TranscribedSpeechDto {
            text,
            language: transcript.language,
            duration_ms: transcript.duration_ms,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lattice_core::application::ports::transcription_port::{Transcript, TranscriptSegment};
    use std::path::Path;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeWhisper {
        missing: bool,
        heard: Mutex<Option<Vec<f32>>>,
    }

    fn segment(text: &str) -> TranscriptSegment {
        TranscriptSegment {
            start_ms: 0,
            end_ms: 1_000,
            text: text.to_string(),
        }
    }

    #[async_trait]
    impl TranscriptionPort for FakeWhisper {
        async fn transcribe(&self, _audio: &Path) -> Result<Transcript> {
            panic!("a recording never reaches the file path");
        }

        async fn transcribe_samples(&self, samples: Vec<f32>) -> Result<Transcript> {
            *self.heard.lock().unwrap() = Some(samples);
            Ok(Transcript {
                segments: vec![segment("Hello there."), segment("How are you?")],
                language: "en".to_string(),
                duration_ms: 2_000,
            })
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(!self.missing)
        }

        async fn active_model_name(&self) -> Result<Option<String>> {
            Ok(None)
        }
    }

    fn encode(samples: &[i16]) -> String {
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn decodes_little_endian_pcm_to_full_scale_floats() {
        assert_eq!(
            decode_pcm(&encode(&[0, 16_384, -16_384, i16::MIN])).unwrap(),
            vec![0.0, 0.5, -0.5, -1.0]
        );
    }

    #[test]
    fn refuses_what_is_not_a_recording() {
        assert!(matches!(
            decode_pcm("not base64!"),
            Err(AppError::InvalidInput(_))
        ));
        assert!(matches!(
            decode_pcm(&base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3])),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn refuses_a_recording_past_the_limit_before_decoding_it() {
        let samples = MAX_SPEECH_SECS * TARGET_SAMPLE_RATE as usize + 1;
        let too_long = "A".repeat((samples * 2).div_ceil(3) * 4);
        assert!(matches!(
            decode_pcm(&too_long),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn joins_what_was_heard_into_one_message() {
        let whisper = Arc::new(FakeWhisper::default());
        let port = Arc::clone(&whisper) as Arc<dyn TranscriptionPort>;

        let spoken = TranscribeSpeechUseCase::new(port)
            .execute(&encode(&[16_384]))
            .await
            .unwrap();

        assert_eq!(spoken.text, "Hello there. How are you?");
        assert_eq!(spoken.language, "en");
        assert_eq!(spoken.duration_ms, 2_000);
        assert_eq!(whisper.heard.lock().unwrap().as_deref(), Some(&[0.5][..]));
    }

    #[tokio::test]
    async fn says_so_when_no_model_is_downloaded() {
        let port = Arc::new(FakeWhisper {
            missing: true,
            ..FakeWhisper::default()
        }) as Arc<dyn TranscriptionPort>;

        let error = TranscribeSpeechUseCase::new(port)
            .execute(&encode(&[0]))
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::ServiceNotAvailable(_)));
    }
}
