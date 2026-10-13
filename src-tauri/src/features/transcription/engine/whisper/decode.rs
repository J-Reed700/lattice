//! The decode loop: mel windows, language detection, temperature fallback,
//! and timestamped segments for already-decoded PCM.

use candle_core::{IndexOp, Tensor, D};
use candle_transformers::models::whisper::{self as whisper};

use super::{DecodeResult, LoadedWhisper};
use crate::application::ports::transcription_port::TranscriptSegment;
use crate::shared::error::AppError;

fn inference_error(e: candle_core::Error) -> AppError {
    AppError::Other(format!("whisper inference failed: {e}"))
}

pub(super) fn argmax(values: &[f32]) -> Result<u32, AppError> {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(index, _)| index as u32)
        .ok_or_else(|| AppError::InvalidData("empty logits".into()))
}

/// Cheap stand-in for gzip compression ratio: total bytes over distinct 8-byte
/// shingles. Repetition loops — whisper's classic failure mode — collapse the
/// shingle set and push the ratio well past the threshold.
pub(super) fn compression_ratio(text: &str) -> f64 {
    let bytes = text.as_bytes();
    if bytes.len() < 8 {
        return 1.0;
    }
    let distinct: std::collections::HashSet<&[u8]> = bytes.windows(8).collect();
    bytes.len() as f64 / distinct.len().max(1) as f64
}

/// Compute the mel spectrogram for one window of PCM as a `(1, n_mel, frames)`
/// tensor.
fn mel_window(
    loaded: &LoadedWhisper,
    pcm: &[f32],
    seek: usize,
    segment_frames: usize,
) -> Result<Tensor, AppError> {
    let start = seek * whisper::HOP_LENGTH;
    let end = (seek + segment_frames) * whisper::HOP_LENGTH;
    let samples = pcm
        .get(start..end.min(pcm.len()))
        .ok_or_else(|| AppError::InvalidData("audio window out of range".into()))?;

    let mel = whisper::audio::pcm_to_mel(&loaded.config, samples, &loaded.mel_filters);
    let n_mel = loaded.config.num_mel_bins;
    let frames = mel.len() / n_mel.max(1);

    // Whisper is trained on exactly 30 s (`N_FRAMES`) of mel and expects that
    // shape even when the audio is shorter — OpenAI's reference implementation
    // calls this `pad_or_trim`. `pcm_to_mel` already zero-pads past `N_FRAMES`,
    // so the full window is always available; narrowing to the *content* length
    // instead would hand the encoder a 7 s spectrogram for a 7 s file and make
    // it hallucinate trailing text with timestamps past the end of the audio.
    if frames < whisper::N_FRAMES {
        return Err(AppError::InvalidData(
            "mel spectrogram is shorter than a 30 s window".into(),
        ));
    }

    Tensor::from_vec(mel, (1, n_mel, frames), &loaded.device)
        .and_then(|tensor| tensor.narrow(2, 0, whisper::N_FRAMES))
        .map_err(inference_error)
}

/// Detect the spoken language from the first window.
fn detect_language(loaded: &mut LoadedWhisper, mel: &Tensor) -> Result<(u32, String), AppError> {
    let language_ids: Vec<u32> = loaded.language_tokens.iter().map(|(id, _)| *id).collect();
    let ids_tensor =
        Tensor::new(language_ids.as_slice(), &loaded.device).map_err(inference_error)?;

    loaded.model.reset_kv_cache();
    let audio = loaded
        .model
        .encoder
        .forward(mel, true)
        .map_err(inference_error)?;
    let tokens = Tensor::new(&[[loaded.sot]], &loaded.device).map_err(inference_error)?;
    let ys = loaded
        .model
        .decoder
        .forward(&tokens, &audio, true)
        .map_err(inference_error)?;

    let logits = loaded
        .model
        .decoder
        .final_linear(&ys.i(..1).map_err(inference_error)?)
        .and_then(|logits| logits.i(0)?.i(0))
        .and_then(|logits| logits.index_select(&ids_tensor, 0))
        .map_err(inference_error)?;
    let probs = candle_nn::ops::softmax(&logits, D::Minus1)
        .and_then(|probs| probs.to_vec1::<f32>())
        .map_err(inference_error)?;

    let best = argmax(&probs)? as usize;
    let (id, tag) = loaded
        .language_tokens
        .get(best)
        .ok_or_else(|| AppError::InvalidData("language probabilities out of range".into()))?;
    Ok((*id, tag.clone()))
}

/// Decode one mel window at one temperature.
fn decode_window(
    loaded: &mut LoadedWhisper,
    mel: &Tensor,
    temperature: f64,
    language_token: Option<u32>,
) -> Result<DecodeResult, AppError> {
    loaded.model.reset_kv_cache();
    let audio = loaded
        .model
        .encoder
        .forward(mel, true)
        .map_err(inference_error)?;

    let mut tokens = vec![loaded.sot];
    if let Some(language) = language_token {
        tokens.push(language);
    }
    tokens.push(loaded.transcribe);

    let mut sum_logprob = 0f64;
    let mut no_speech_prob = f64::NAN;
    let mut rng = rand::thread_rng();

    let max_steps = loaded.config.max_target_positions / 2;
    for step in 0..max_steps {
        let input = Tensor::new(tokens.as_slice(), &loaded.device)
            .and_then(|tensor| tensor.unsqueeze(0))
            .map_err(inference_error)?;

        let ys = loaded
            .model
            .decoder
            .forward(&input, &audio, step == 0)
            .map_err(inference_error)?;

        if step == 0 {
            let logits = loaded
                .model
                .decoder
                .final_linear(&ys.i(..1).map_err(inference_error)?)
                .and_then(|logits| logits.i(0)?.i(0))
                .map_err(inference_error)?;
            no_speech_prob = candle_nn::ops::softmax(&logits, 0)
                .and_then(|probs| probs.i(loaded.no_speech as usize)?.to_scalar::<f32>())
                .map_err(inference_error)? as f64;
        }

        let (_, seq_len, _) = ys.dims3().map_err(inference_error)?;
        let logits = loaded
            .model
            .decoder
            .final_linear(&ys.i((..1, seq_len - 1..)).map_err(inference_error)?)
            .and_then(|logits| logits.i(0)?.i(0))
            .and_then(|logits| logits.broadcast_add(&loaded.suppress))
            .map_err(inference_error)?;

        let next = if temperature > 0.0 {
            let probs = candle_nn::ops::softmax(
                &(&logits / temperature).map_err(inference_error)?,
                D::Minus1,
            )
            .and_then(|probs| probs.to_vec1::<f32>())
            .map_err(inference_error)?;

            match rand::distributions::WeightedIndex::new(&probs) {
                Ok(distribution) => {
                    use rand::distributions::Distribution;
                    distribution.sample(&mut rng) as u32
                }
                // All-zero or non-finite weights: fall back to greedy.
                Err(_) => argmax(&probs)?,
            }
        } else {
            let values = logits.to_vec1::<f32>().map_err(inference_error)?;
            argmax(&values)?
        };

        let probs = candle_nn::ops::softmax(&logits, D::Minus1)
            .and_then(|probs| probs.i(next as usize)?.to_scalar::<f32>())
            .map_err(inference_error)?;
        sum_logprob += (probs as f64).ln();

        tokens.push(next);

        if next == loaded.eot || tokens.len() > loaded.config.max_target_positions {
            break;
        }
    }

    let text = loaded
        .tokenizer
        .decode(&tokens, true)
        .map_err(|e| AppError::TokenizationError {
            reason: format!("could not decode whisper tokens: {e}"),
        })?;

    let avg_logprob = sum_logprob / tokens.len().max(1) as f64;

    Ok(DecodeResult {
        compression_ratio: compression_ratio(&text),
        tokens,
        avg_logprob,
        no_speech_prob,
    })
}

/// Walk the temperature schedule, accepting the first plausible decode.
///
/// This is what rescues whisper's repetition-loop failure mode.
fn decode_with_fallback(
    loaded: &mut LoadedWhisper,
    mel: &Tensor,
    language_token: Option<u32>,
) -> Result<DecodeResult, AppError> {
    let mut last: Option<DecodeResult> = None;

    for (index, temperature) in whisper::TEMPERATURES.iter().enumerate() {
        let result = decode_window(loaded, mel, *temperature, language_token)?;

        let plausible = result.avg_logprob >= whisper::LOGPROB_THRESHOLD
            && result.compression_ratio <= whisper::COMPRESSION_RATIO_THRESHOLD;
        if plausible || index == whisper::TEMPERATURES.len() - 1 {
            return Ok(result);
        }
        last = Some(result);
    }

    last.ok_or_else(|| AppError::InvalidData("whisper produced no decode".into()))
}

/// Split a decoded window into segments on its timestamp tokens.
fn segments_from_tokens(
    loaded: &LoadedWhisper,
    result: &DecodeResult,
    time_offset_ms: u64,
    window_end_ms: u64,
) -> Result<Vec<TranscriptSegment>, AppError> {
    let mut segments = Vec::new();
    let mut pending: Vec<u32> = Vec::new();
    let mut segment_start_ms: Option<u64> = None;

    let mut push = |start_ms: u64, end_ms: u64, tokens: &[u32]| -> Result<(), AppError> {
        if tokens.is_empty() {
            return Ok(());
        }
        // A short final window is padded out to 30 s, so whisper can emit
        // timestamps past the end of the recording. Clamp to the real content
        // end and drop anything that starts after the audio does.
        if start_ms >= window_end_ms {
            return Ok(());
        }
        let end_ms = end_ms.min(window_end_ms);
        let text =
            loaded
                .tokenizer
                .decode(tokens, true)
                .map_err(|e| AppError::TokenizationError {
                    reason: format!("could not decode whisper tokens: {e}"),
                })?;
        let text = text.trim();
        if !text.is_empty() {
            segments.push(TranscriptSegment {
                start_ms,
                end_ms: end_ms.max(start_ms),
                text: text.to_string(),
            });
        }
        Ok(())
    };

    for token in &result.tokens {
        let token = *token;
        if token == loaded.sot || token == loaded.eot {
            continue;
        }
        if token > loaded.no_timestamps {
            // Timestamp tokens count in 20 ms steps from the window start.
            let relative_ms = u64::from(token - loaded.no_timestamps - 1).saturating_mul(1000) / 50;
            let absolute_ms = time_offset_ms.saturating_add(relative_ms);

            match segment_start_ms {
                None => segment_start_ms = Some(absolute_ms),
                Some(start_ms) => {
                    push(start_ms, absolute_ms, &pending)?;
                    pending.clear();
                    segment_start_ms = Some(absolute_ms);
                }
            }
            continue;
        }
        pending.push(token);
    }

    // Flush trailing text that never got a closing timestamp.
    if !pending.is_empty() {
        let start_ms = segment_start_ms.unwrap_or(time_offset_ms);
        push(start_ms, window_end_ms, &pending)?;
    }

    Ok(segments)
}

/// Transcribe already-decoded PCM. Runs on a blocking thread.
pub(super) fn transcribe_pcm(
    loaded: &mut LoadedWhisper,
    pcm: &[f32],
) -> Result<(Vec<TranscriptSegment>, String), AppError> {
    let content_frames = pcm.len() / whisper::HOP_LENGTH;
    if content_frames == 0 {
        return Ok((Vec::new(), "en".to_string()));
    }

    // Language: detected once from the first window, then pinned for the file.
    let (language_token, language) = if loaded.language_tokens.is_empty() {
        (None, "en".to_string())
    } else {
        let first_frames = content_frames.min(whisper::N_FRAMES);
        let mel = mel_window(loaded, pcm, 0, first_frames)?;
        let (token, tag) = detect_language(loaded, &mel)?;
        (Some(token), tag)
    };

    let mut segments: Vec<TranscriptSegment> = Vec::new();
    let mut seek = 0usize;

    while seek < content_frames {
        let segment_frames = (content_frames - seek).min(whisper::N_FRAMES);
        let time_offset_ms = (seek * whisper::HOP_LENGTH * 1000 / whisper::SAMPLE_RATE) as u64;
        let window_end_ms =
            ((seek + segment_frames) * whisper::HOP_LENGTH * 1000 / whisper::SAMPLE_RATE) as u64;

        let mel = mel_window(loaded, pcm, seek, segment_frames)?;
        let result = decode_with_fallback(loaded, &mel, language_token)?;

        let is_silence = result.no_speech_prob > whisper::NO_SPEECH_THRESHOLD
            && result.avg_logprob < whisper::LOGPROB_THRESHOLD;

        if !is_silence {
            segments.extend(segments_from_tokens(
                loaded,
                &result,
                time_offset_ms,
                window_end_ms,
            )?);
        }

        seek += segment_frames;
    }

    Ok((segments, language))
}
