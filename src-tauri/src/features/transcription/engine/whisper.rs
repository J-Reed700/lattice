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
use candle_core::{Device, Tensor};
use candle_transformers::models::whisper::{quantized_model, Config};
use tokenizers::Tokenizer;
use tracing::info;

use crate::features::transcription::engine::audio_decode::decode_to_mono_16k;
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use lattice_core::application::ports::transcription_port::{Transcript, TranscriptionPort};
use lattice_core::shared::error::AppError;
use lattice_core::shared::runtime::background::BackgroundTasks;
use lattice_core::shared::runtime::with_autorelease_pool;

mod decode;
mod load;

use decode::transcribe_pcm;
use load::load_whisper;

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
    /// Where the idle-unload timer runs, so shutdown stops it.
    background: Arc<BackgroundTasks>,
}

impl WhisperTranscriptionService {
    /// Build the service. Cheap: no file I/O, no model load.
    pub fn new(models: DownloadedModelRepository, background: Arc<BackgroundTasks>) -> Self {
        Self {
            models,
            background,
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

        for meta in lattice_core::domain::models::curated::get_curated_transcription_models() {
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
        let cancel = self.background.token();

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
        if self.background.spawn(task).is_none() {
            self.idle_timer_active.store(false, Ordering::Release);
        }
    }
}

/// The 80-bin mel filterbank, vendored from candle
/// (`candle-examples/examples/whisper/melfilters.bytes`, MIT).
///
/// This is a constant of the whisper architecture, identical for every 80-mel
/// model — not a per-model artifact. Bundling it means the catalog does not
/// have to ship the same 64 KB table alongside every entry, and a model whose
/// repo omits it still loads.
const BUNDLED_MEL_FILTERS_80: &[u8] = include_bytes!("melfilters.bytes");

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
mod tests;
