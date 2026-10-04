//! On-device whisper transcription via candle.
//!
//! Model presence and location come from `DownloadedModelRepository`. Nothing
//! is loaded until the first `transcribe` call, and the model is dropped after
//! [`TRANSCRIPTION_IDLE_TTL`] of inactivity.
//!
//! Inference runs on `spawn_blocking` behind a semaphore of one: whisper is the
//! heaviest thing this process runs, and Metal can crash on parallel kernel
//! dispatch from multiple threads.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use candle_core::{Device, IndexOp, Tensor, D};
use candle_transformers::models::whisper::{self as whisper, quantized_model, Config};
use candle_transformers::quantized_var_builder::VarBuilder;
use tokenizers::Tokenizer;
use tracing::info;

use crate::application::ports::transcription_port::{
    Transcript, TranscriptSegment, TranscriptionPort,
};
use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
use crate::features::transcription::engine::audio_decode::decode_to_mono_16k;
use crate::shared::error::AppError;
use crate::shared::runtime::with_autorelease_pool;

/// How long a loaded model is kept resident after the last transcription.
pub const TRANSCRIPTION_IDLE_TTL: Duration = Duration::from_secs(300);

/// Longest recording accepted, in seconds.
pub const MAX_AUDIO_SECS: u64 = 2 * 60 * 60;

fn idle_expired(last_used: Instant, now: Instant, idle_ttl: Duration) -> bool {
    now.saturating_duration_since(last_used) >= idle_ttl
}

fn claim_idle_timer(active: &AtomicBool) -> bool {
    active
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

fn try_take_loaded(
    loaded: &parking_lot::Mutex<Option<LoadedWhisper>>,
) -> Option<Option<LoadedWhisper>> {
    Some(loaded.try_lock()?.take())
}

async fn wait_for_idle_permit(
    permits: Arc<tokio::sync::Semaphore>,
    last_used: Arc<parking_lot::Mutex<Instant>>,
    cancel: tokio_util::sync::CancellationToken,
    idle_ttl: Duration,
) -> Option<tokio::sync::OwnedSemaphorePermit> {
    loop {
        let deadline = *last_used.lock() + idle_ttl;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return None,
            _ = tokio::time::sleep_until(deadline.into()) => {}
        }
        let permit = tokio::select! {
            biased;
            _ = cancel.cancelled() => return None,
            permit = Arc::clone(&permits).acquire_owned() => permit.ok()?,
        };
        if idle_expired(*last_used.lock(), Instant::now(), idle_ttl) {
            return Some(permit);
        }
        drop(permit);
    }
}

/// Mel bin count of the base/small models this engine supports.
const SUPPORTED_MEL_BINS: usize = 80;

/// Number of mel-filter coefficients per bin in `melfilters.bytes`.
const MEL_FILTER_WIDTH: usize = 201;

/// Where a downloaded transcription model lives.
#[derive(Debug, Clone)]
struct ResolvedModel {
    model_id: String,
    name: String,
    dir: PathBuf,
}

/// A loaded whisper model plus the token ids the decode loop needs.
struct LoadedWhisper {
    model_id: String,
    model: quantized_model::Whisper,
    tokenizer: Tokenizer,
    config: Config,
    mel_filters: Vec<f32>,
    device: Device,
    sot: u32,
    eot: u32,
    transcribe: u32,
    no_timestamps: u32,
    no_speech: u32,
    suppress: Tensor,
    /// `(token id, language tag)` pairs, empty for English-only models.
    language_tokens: Vec<(u32, String)>,
}

/// One decoding attempt over a single 30 s mel window.
struct DecodeResult {
    tokens: Vec<u32>,
    avg_logprob: f64,
    no_speech_prob: f64,
    compression_ratio: f64,
}

/// Whisper-backed implementation of [`TranscriptionPort`].
pub struct WhisperTranscriptionService {
    models: DownloadedModelRepository,
    /// Admission control: exactly one transcription at a time, waiting in
    /// async-land rather than occupying blocking-pool threads.
    permits: Arc<tokio::sync::Semaphore>,
    loaded: Arc<parking_lot::Mutex<Option<LoadedWhisper>>>,
    last_used: Arc<parking_lot::Mutex<Instant>>,
    idle_timer_active: Arc<AtomicBool>,
}

impl WhisperTranscriptionService {
    /// Build the service. Cheap: no file I/O, no model load.
    pub fn new(models: DownloadedModelRepository) -> Self {
        Self {
            models,
            permits: Arc::new(tokio::sync::Semaphore::new(1)),
            loaded: Arc::new(parking_lot::Mutex::new(None)),
            last_used: Arc::new(parking_lot::Mutex::new(Instant::now())),
            idle_timer_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The transcription model = the most recently downloaded of our curated
    /// transcription entries.
    ///
    /// Repository Barrier: the repository answers both "do we have one?" and
    /// "where is it?" — never a filesystem walk. `is_downloaded` is checked
    /// first because `find_by_model_id` errors for an in-flight row whose
    /// `storage_path` is still NULL.
    async fn resolve_model(&self) -> Result<Option<ResolvedModel>, AppError> {
        let mut best: Option<(chrono::DateTime<chrono::Utc>, ResolvedModel)> = None;

        for meta in crate::domain::models::curated::get_curated_transcription_models() {
            if !self.models.is_downloaded(&meta.id).await? {
                continue;
            }
            let Some(downloaded) = self.models.find_by_model_id(&meta.id).await? else {
                continue;
            };
            let Some(dir) = downloaded.location().enclosing_dir() else {
                continue;
            };

            let resolved = ResolvedModel {
                model_id: meta.id.clone(),
                name: downloaded.model_name().to_string(),
                dir,
            };
            let downloaded_at = *downloaded.downloaded_at();

            match &best {
                Some((at, _)) if *at >= downloaded_at => {}
                _ => best = Some((downloaded_at, resolved)),
            }
        }

        Ok(best.map(|(_, model)| model))
    }

    /// Keep one supervised timer alive while the model is loaded. It waits for
    /// the inference permit before inspecting the model, so it never blocks an
    /// async executor thread on the synchronous model mutex.
    fn schedule_idle_unload(&self) {
        if !claim_idle_timer(&self.idle_timer_active) {
            return;
        }
        let loaded = Arc::clone(&self.loaded);
        let last_used = Arc::clone(&self.last_used);
        let permits = Arc::clone(&self.permits);
        let idle_timer_active = Arc::clone(&self.idle_timer_active);
        let cancel = crate::shared::runtime::background::cancellation_token();

        let task = async move {
            loop {
                let Some(permit) = wait_for_idle_permit(
                    Arc::clone(&permits),
                    Arc::clone(&last_used),
                    cancel.clone(),
                    TRANSCRIPTION_IDLE_TTL,
                )
                .await
                else {
                    idle_timer_active.store(false, Ordering::Release);
                    return;
                };
                // The semaphore guarantees inference is not holding this lock;
                // try_lock keeps this task nonblocking even if another short
                // model-management operation owns it.
                let model = match try_take_loaded(&loaded) {
                    None => {
                        drop(permit);
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    }
                    Some(None) => {
                        // Inference may have failed while loading the model.
                        // There is nothing to unload, and the next request
                        // will start a fresh timer after it obtains the permit.
                        idle_timer_active.store(false, Ordering::Release);
                        drop(permit);
                        return;
                    }
                    Some(Some(model)) => model,
                };
                // Tensor destruction can be expensive; keep it off the async
                // executor just like model loading and inference.
                idle_timer_active.store(false, Ordering::Release);
                let _ = tokio::task::spawn_blocking(move || {
                    info!(
                        model_id = %model.model_id,
                        "transcription model unloaded after idle"
                    );
                    drop(model);
                })
                .await;
                drop(permit);
                return;
            }
        };
        if crate::shared::runtime::background::spawn(task).is_none() {
            self.idle_timer_active.store(false, Ordering::Release);
        }
    }
}

fn read_json_config(dir: &Path, model_id: &str) -> Result<Config, AppError> {
    let path = dir.join("config.json");
    let bytes = std::fs::read(&path).map_err(|e| {
        AppError::ModelLoadFailed(format!(
            "Transcription model '{model_id}' is incomplete: config.json is missing ({e})"
        ))
    })?;
    serde_json::from_slice::<Config>(&bytes).map_err(|e| {
        AppError::ModelLoadFailed(format!(
            "Transcription model '{model_id}' has an unreadable config.json: {e}"
        ))
    })
}

/// The 80-bin mel filterbank, vendored from candle
/// (`candle-examples/examples/whisper/melfilters.bytes`, MIT).
///
/// This is a constant of the whisper architecture, identical for every 80-mel
/// model — not a per-model artifact. Bundling it means the catalog does not
/// have to ship the same 64 KB table alongside every entry, and a model whose
/// repo omits it still loads.
const BUNDLED_MEL_FILTERS_80: &[u8] = include_bytes!("melfilters.bytes");

fn read_mel_filters(dir: &Path, model_id: &str, num_mel_bins: usize) -> Result<Vec<f32>, AppError> {
    // A model directory may ship its own table (a future 128-mel model must);
    // otherwise fall back to the bundled 80-bin filterbank.
    let path = dir.join("melfilters.bytes");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) if num_mel_bins == SUPPORTED_MEL_BINS => BUNDLED_MEL_FILTERS_80.to_vec(),
        Err(e) => {
            return Err(AppError::ModelLoadFailed(format!(
                "Transcription model '{model_id}' is incomplete: melfilters.bytes is missing ({e})"
            )))
        }
    };

    let filters: Vec<f32> = bytes
        .chunks_exact(4)
        .filter_map(|chunk| <[u8; 4]>::try_from(chunk).ok().map(f32::from_le_bytes))
        .collect();

    let expected = num_mel_bins * MEL_FILTER_WIDTH;
    if filters.len() != expected {
        return Err(AppError::InvalidData(format!(
            "Transcription model '{model_id}' has {} mel filter values, expected {expected}",
            filters.len()
        )));
    }

    Ok(filters)
}

fn token_id(tokenizer: &Tokenizer, token: &str, model_id: &str) -> Result<u32, AppError> {
    tokenizer.token_to_id(token).ok_or_else(|| {
        AppError::ModelLoadFailed(format!(
            "Transcription model '{model_id}' has no '{token}' token in its tokenizer"
        ))
    })
}

/// Collect the `<|xx|>` language tokens from the tokenizer vocabulary.
///
/// Deriving them from the vocab avoids embedding a 99-language table that would
/// drift from whatever model the user downloaded. Task/control tokens
/// (`<|translate|>`, `<|nospeech|>`, …) are longer than three characters and
/// timestamp tokens contain digits, so a 2–3 lowercase-letter tag selects
/// exactly the language set.
fn collect_language_tokens(tokenizer: &Tokenizer) -> Vec<(u32, String)> {
    let mut tokens: Vec<(u32, String)> = tokenizer
        .get_vocab(true)
        .into_iter()
        .filter_map(|(token, id)| {
            let tag = token.strip_prefix("<|")?.strip_suffix("|>")?;
            let is_language_tag =
                (2..=3).contains(&tag.len()) && tag.chars().all(|c| c.is_ascii_lowercase());
            is_language_tag.then(|| (id, tag.to_string()))
        })
        .collect();
    tokens.sort_by_key(|(id, _)| *id);
    tokens
}

/// Use the same platform policy as embeddings: Metal on Apple Silicon,
/// CPU on Intel Macs, with recoverable accelerator initialization.
fn best_device() -> Device {
    crate::infrastructure::ml::compute_device::best_available_compute_device("transcription")
}

fn load_whisper(resolved: &ResolvedModel) -> Result<LoadedWhisper, AppError> {
    let model_id = resolved.model_id.as_str();
    let dir = resolved.dir.as_path();

    let config = read_json_config(dir, model_id)?;
    if config.num_mel_bins != SUPPORTED_MEL_BINS {
        return Err(AppError::ModelLoadFailed(format!(
            "Transcription model '{model_id}' uses {} mel bins; Lattice supports {SUPPORTED_MEL_BINS}-bin whisper models (base, small).",
            config.num_mel_bins
        )));
    }

    let mel_filters = read_mel_filters(dir, model_id, config.num_mel_bins)?;

    let tokenizer_path = dir.join("tokenizer.json");
    let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
        AppError::TokenizationError {
            reason: format!(
                "Transcription model '{model_id}' is incomplete: tokenizer.json could not be read ({e})"
            ),
        }
    })?;

    let weights_path = dir.join("model.gguf");
    if !weights_path.is_file() {
        return Err(AppError::ModelLoadFailed(format!(
            "Transcription model '{model_id}' is incomplete: model.gguf is missing"
        )));
    }

    let device = best_device();
    let var_builder = VarBuilder::from_gguf(&weights_path, &device)
        .map_err(|e| AppError::ModelLoadFailed(format!("whisper weights failed to load: {e}")))?;
    let model = quantized_model::Whisper::load(&var_builder, config.clone())
        .map_err(|e| AppError::ModelLoadFailed(format!("whisper weights failed to load: {e}")))?;

    let sot = token_id(&tokenizer, whisper::SOT_TOKEN, model_id)?;
    let eot = token_id(&tokenizer, whisper::EOT_TOKEN, model_id)?;
    let transcribe = token_id(&tokenizer, whisper::TRANSCRIBE_TOKEN, model_id)?;
    let no_timestamps = token_id(&tokenizer, whisper::NO_TIMESTAMPS_TOKEN, model_id)?;
    let no_speech = whisper::NO_SPEECH_TOKENS
        .iter()
        .find_map(|token| tokenizer.token_to_id(token))
        .ok_or_else(|| {
            AppError::ModelLoadFailed(format!(
                "Transcription model '{model_id}' has no no-speech token in its tokenizer"
            ))
        })?;

    // Suppressed logits, built once. `no_timestamps` is suppressed because we
    // always decode *with* timestamps — the timestamps are the whole point.
    let suppress: Vec<f32> = (0..config.vocab_size as u32)
        .map(|token| {
            if config.suppress_tokens.contains(&token) || token == no_timestamps {
                f32::NEG_INFINITY
            } else {
                0.0
            }
        })
        .collect();
    let suppress = Tensor::new(suppress.as_slice(), &device)
        .map_err(|e| AppError::ModelLoadFailed(format!("whisper weights failed to load: {e}")))?;

    let language_tokens = collect_language_tokens(&tokenizer);

    info!(
        model_id,
        device = ?device,
        multilingual = !language_tokens.is_empty(),
        "transcription model loaded"
    );

    Ok(LoadedWhisper {
        model_id: resolved.model_id.clone(),
        model,
        tokenizer,
        config,
        mel_filters,
        device,
        sot,
        eot,
        transcribe,
        no_timestamps,
        no_speech,
        suppress,
        language_tokens,
    })
}

fn inference_error(e: candle_core::Error) -> AppError {
    AppError::Other(format!("whisper inference failed: {e}"))
}

fn argmax(values: &[f32]) -> Result<u32, AppError> {
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
fn compression_ratio(text: &str) -> f64 {
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
fn transcribe_pcm(
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

#[async_trait]
impl TranscriptionPort for WhisperTranscriptionService {
    async fn transcribe(&self, audio: &Path) -> Result<Transcript, AppError> {
        let Some(resolved) = self.resolve_model().await? else {
            return Err(AppError::ServiceNotAvailable(
                "No transcription model is downloaded.".to_string(),
            ));
        };

        let permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| AppError::InternalError(format!("transcription queue closed: {e}")))?;

        let path = audio.to_path_buf();
        let loaded_slot = Arc::clone(&self.loaded);
        let last_used = Arc::clone(&self.last_used);
        self.schedule_idle_unload();

        let outcome = tokio::task::spawn_blocking(move || -> Result<Transcript, AppError> {
            // The blocking child owns the permit. Dropping the async caller
            // cannot release inference admission while this closure still
            // holds the model lock.
            let _permit = permit;
            with_autorelease_pool(move || -> Result<Transcript, AppError> {
                let decoded = decode_to_mono_16k(&path, MAX_AUDIO_SECS)?;

                let mut guard = loaded_slot.lock();
                let needs_load = guard
                    .as_ref()
                    .is_none_or(|loaded| loaded.model_id != resolved.model_id);
                if needs_load {
                    *guard = Some(load_whisper(&resolved)?);
                }
                let loaded = guard.as_mut().ok_or_else(|| {
                    AppError::ModelLoadFailed("transcription model was not loaded".to_string())
                })?;

                let (segments, language) = transcribe_pcm(loaded, &decoded.samples)?;
                *last_used.lock() = Instant::now();

                Ok(Transcript {
                    segments,
                    language,
                    duration_ms: decoded.duration_ms,
                })
            })
        })
        .await
        .map_err(|e| AppError::InternalError(format!("transcription task failed: {e}")))?;

        outcome
    }

    async fn is_ready(&self) -> Result<bool, AppError> {
        Ok(self.resolve_model().await?.is_some())
    }

    async fn active_model_name(&self) -> Result<Option<String>, AppError> {
        Ok(self.resolve_model().await?.map(|model| model.name))
    }
}

#[cfg(test)]
mod tests_e2e;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use super::*;

    #[test]
    fn only_one_idle_unload_timer_can_be_active() {
        let active = Arc::new(AtomicBool::new(false));
        let barrier = Arc::new(std::sync::Barrier::new(16));
        let workers: Vec<_> = (0..16)
            .map(|_| {
                let active = Arc::clone(&active);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    claim_idle_timer(&active)
                })
            })
            .collect();
        let claims = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|claimed| *claimed)
            .count();

        assert_eq!(claims, 1);
        active.store(false, Ordering::Release);
        assert!(claim_idle_timer(&active));
    }

    #[test]
    fn idle_deadline_recheck_keeps_a_recently_used_model_loaded() {
        let last_used = Instant::now();

        assert!(!idle_expired(
            last_used,
            last_used + TRANSCRIPTION_IDLE_TTL - Duration::from_millis(1),
            TRANSCRIPTION_IDLE_TTL,
        ));
        assert!(idle_expired(
            last_used,
            last_used + TRANSCRIPTION_IDLE_TTL,
            TRANSCRIPTION_IDLE_TTL,
        ));
    }

    #[tokio::test]
    async fn blocking_inference_keeps_its_permit_if_its_async_caller_is_dropped() {
        let permits = Arc::new(tokio::sync::Semaphore::new(1));
        let permit = Arc::clone(&permits).acquire_owned().await.unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let child = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _ = started_tx.send(());
            let _ = release_rx.recv();
        });

        started_rx.await.unwrap();
        child.abort();
        assert!(Arc::clone(&permits).try_acquire_owned().is_err());
        release_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Ok(permit) = Arc::clone(&permits).try_acquire_owned() {
                    drop(permit);
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn stale_idle_timer_rechecks_use_after_waiting_for_inference_gate() {
        let idle_ttl = Duration::from_millis(100);
        let permits = Arc::new(tokio::sync::Semaphore::new(1));
        let inference_permit = Arc::clone(&permits).acquire_owned().await.unwrap();
        let last_used = Arc::new(parking_lot::Mutex::new(
            Instant::now() - idle_ttl + Duration::from_millis(30),
        ));
        let resident_model = Arc::new(parking_lot::Mutex::new(Some(())));
        let cancel = tokio_util::sync::CancellationToken::new();
        let timer = tokio::spawn(wait_for_idle_permit(
            Arc::clone(&permits),
            Arc::clone(&last_used),
            cancel,
            idle_ttl,
        ));

        // The timer's first deadline passes while inference still owns the
        // gate. Inference uses the model again before releasing that gate.
        tokio::time::sleep(Duration::from_millis(50)).await;
        *last_used.lock() = Instant::now();
        drop(inference_permit);

        tokio::pin!(timer);
        assert!(tokio::time::timeout(Duration::from_millis(20), &mut timer)
            .await
            .is_err());
        assert!(resident_model.lock().is_some());
        let idle_permit = tokio::time::timeout(Duration::from_secs(1), &mut timer)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(resident_model.lock().take().is_some());
        drop(idle_permit);
    }

    #[test]
    fn compression_ratio_flags_repetition_loops() {
        let healthy = "The quarterly numbers were up eleven percent across enterprise renewals.";
        let looping = "thank you thank you thank you thank you thank you thank you thank you";

        assert!(compression_ratio(healthy) < whisper::COMPRESSION_RATIO_THRESHOLD);
        assert!(compression_ratio(looping) > whisper::COMPRESSION_RATIO_THRESHOLD);
    }

    #[test]
    fn argmax_picks_the_largest_value() {
        assert_eq!(argmax(&[0.1, 0.9, 0.4]).unwrap(), 1);
        assert!(argmax(&[]).is_err());
    }
}
