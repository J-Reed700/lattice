//! Read-aloud on this computer: the downloaded Kokoro bundle, loaded on the
//! first request and dropped after [`SPEECH_IDLE_TTL`] without one. A reader
//! or a voice loads the first time it is asked for, so an install is only
//! ever asked for the files of the voice it speaks in.
//!
//! Synthesis runs on `spawn_blocking`, one request at a time. The loaded
//! model is shared as an `Arc`, so unloading never pulls it from under a
//! request that is still speaking.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use candle_core::Device;
use tracing::info;

use super::g2p::G2p;
use super::kokoro::{Kokoro, SeededNoise, Voice, MAX_PHONEMES, SAMPLE_RATE};
use lattice_core::domain::models::curated::{
    speech_voice_file, CuratedVoice, SpeechAccent, SPEECH_CONFIG_FILE, SPEECH_MODEL_ID,
    SPEECH_VOICE_PACK_ID, SPEECH_WEIGHTS_FILE,
};
use lattice_core::domain::repositories::DownloadedModelRepository;
use lattice_core::shared::error::AppError;
use lattice_core::shared::runtime::background::BackgroundTasks;

/// How long the model stays loaded after the last sentence it spoke.
pub const SPEECH_IDLE_TTL: Duration = Duration::from_secs(300);

/// Speech at Kokoro's sample rate.
pub struct Clip {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

struct Loaded {
    dir: PathBuf,
    kokoro: Kokoro,
    readers: parking_lot::Mutex<HashMap<SpeechAccent, Arc<G2p>>>,
    voices: parking_lot::Mutex<HashMap<&'static str, Arc<Voice>>>,
}

/// Where the files of one voice are.
#[derive(Debug, PartialEq, Eq)]
struct VoiceDirs {
    voice: PathBuf,
    reader: PathBuf,
}

fn load_failed(e: impl std::fmt::Display) -> AppError {
    AppError::ModelLoadFailed(format!("The read-aloud voice could not load: {e}"))
}

impl Loaded {
    fn load(dir: &Path) -> Result<Self, AppError> {
        let started = Instant::now();
        let kokoro = Kokoro::load(
            &dir.join(SPEECH_CONFIG_FILE),
            &dir.join(SPEECH_WEIGHTS_FILE),
            &Device::Cpu,
        )
        .map_err(load_failed)?;
        info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            "read-aloud voice loaded"
        );
        Ok(Self {
            dir: dir.to_path_buf(),
            kokoro,
            readers: parking_lot::Mutex::new(HashMap::new()),
            voices: parking_lot::Mutex::new(HashMap::new()),
        })
    }

    fn reader(&self, accent: SpeechAccent, dir: &Path) -> Result<Arc<G2p>, AppError> {
        let mut readers = self.readers.lock();
        if let Some(reader) = readers.get(&accent) {
            return Ok(Arc::clone(reader));
        }
        let reader = Arc::new(G2p::load(accent, dir).map_err(load_failed)?);
        readers.insert(accent, Arc::clone(&reader));
        Ok(reader)
    }

    fn voice(&self, voice: &CuratedVoice, dir: &Path) -> Result<Arc<Voice>, AppError> {
        let mut voices = self.voices.lock();
        if let Some(loaded) = voices.get(voice.id) {
            return Ok(Arc::clone(loaded));
        }
        let bytes = std::fs::read(dir.join(speech_voice_file(voice.id))).map_err(load_failed)?;
        let loaded = Arc::new(Voice::from_bytes(&bytes).map_err(load_failed)?);
        voices.insert(voice.id, Arc::clone(&loaded));
        Ok(loaded)
    }

    fn speak(
        &self,
        text: &str,
        voice: &CuratedVoice,
        dirs: &VoiceDirs,
        speed: f32,
    ) -> Result<Vec<f32>, AppError> {
        let phonemes = self.reader(voice.accent(), &dirs.reader)?.phonemize(text);
        let voice = self.voice(voice, &dirs.voice)?;
        let mut noise = SeededNoise::new({
            let mut hasher = DefaultHasher::new();
            phonemes.hash(&mut hasher);
            hasher.finish()
        });
        let mut samples = Vec::new();
        for piece in split_phonemes(&phonemes, MAX_PHONEMES) {
            samples.extend(
                self.kokoro
                    .synthesize(&piece, &voice, speed, &mut noise)
                    .map_err(|e| AppError::Other(format!("Reading aloud failed: {e}")))?,
            );
        }
        Ok(samples)
    }
}

/// Phoneme pieces no longer than `max` characters, broken after the last
/// sentence or clause punctuation that fits, else the last space.
pub(crate) fn split_phonemes(phonemes: &str, max: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut rest: Vec<char> = phonemes.trim().chars().collect();
    while rest.len() > max {
        let window = rest.get(..=max).unwrap_or(&rest);
        let after = |set: &str| {
            window
                .iter()
                .rposition(|c| set.contains(*c))
                .filter(|&at| at + 1 < window.len())
                .map(|at| at + 1)
        };
        let cut = after(".!?…")
            .or_else(|| after(";:—"))
            .or_else(|| after(","))
            .or_else(|| window.iter().rposition(|c| *c == ' ').filter(|&at| at > 0))
            .unwrap_or(max);
        let tail = rest.split_off(cut);
        pieces.push(rest.iter().collect::<String>().trim().to_string());
        rest = tail;
    }
    let last: String = rest.iter().collect::<String>().trim().to_string();
    if !last.is_empty() {
        pieces.push(last);
    }
    pieces.retain(|p| !p.is_empty());
    pieces
}

/// The read-aloud engine.
pub struct KokoroSpeech {
    models: Arc<dyn DownloadedModelRepository>,
    loaded: Arc<tokio::sync::Mutex<Option<Arc<Loaded>>>>,
    speaking: Arc<tokio::sync::Semaphore>,
    last_used: Arc<parking_lot::Mutex<Instant>>,
    background: Arc<BackgroundTasks>,
}

impl KokoroSpeech {
    /// Cheap: no file I/O and no model load.
    pub fn new(
        models: Arc<dyn DownloadedModelRepository>,
        background: Arc<BackgroundTasks>,
    ) -> Self {
        Self {
            models,
            loaded: Arc::new(tokio::sync::Mutex::new(None)),
            speaking: Arc::new(tokio::sync::Semaphore::new(1)),
            last_used: Arc::new(parking_lot::Mutex::new(Instant::now())),
            background,
        }
    }

    async fn entry_dir(&self, model_id: &str) -> Result<Option<PathBuf>, AppError> {
        if !self.models.is_downloaded(model_id).await? {
            return Ok(None);
        }
        Ok(self
            .models
            .find_by_model_id(model_id)
            .await?
            .and_then(|model| model.location().enclosing_dir()))
    }

    /// Where the downloaded bundle is, when it is complete on disk.
    pub async fn model_dir(&self) -> Result<Option<PathBuf>, AppError> {
        self.entry_dir(SPEECH_MODEL_ID).await
    }

    /// Where the voice pack is, when it is complete on disk.
    pub async fn voice_pack_dir(&self) -> Result<Option<PathBuf>, AppError> {
        self.entry_dir(SPEECH_VOICE_PACK_ID).await
    }

    async fn dirs_for(&self, voice: &CuratedVoice) -> Result<VoiceDirs, AppError> {
        let dir = |model_id: &'static str| async move {
            self.entry_dir(model_id).await?.ok_or_else(|| {
                AppError::ServiceNotAvailable(format!(
                    "{} is in the voices that are not downloaded. Download them in Settings, \
                     Read aloud.",
                    voice.name
                ))
            })
        };
        Ok(VoiceDirs {
            voice: dir(voice.model_id()).await?,
            reader: dir(voice.accent().model_id()).await?,
        })
    }

    async fn model(&self) -> Result<Arc<Loaded>, AppError> {
        let dir = self.model_dir().await?.ok_or_else(|| {
            AppError::ServiceNotAvailable("The read-aloud voices are not downloaded.".to_string())
        })?;
        let mut slot = self.loaded.lock().await;
        if let Some(loaded) = slot.as_ref().filter(|loaded| loaded.dir == dir) {
            return Ok(Arc::clone(loaded));
        }
        let loaded = tokio::task::spawn_blocking(move || Loaded::load(&dir))
            .await
            .map_err(|e| AppError::InternalError(format!("loading the voice failed: {e}")))??;
        let loaded = Arc::new(loaded);
        let first_load = slot.replace(Arc::clone(&loaded)).is_none();
        drop(slot);
        if first_load {
            self.unload_when_idle();
        }
        Ok(loaded)
    }

    fn unload_when_idle(&self) {
        let loaded = Arc::clone(&self.loaded);
        let last_used = Arc::clone(&self.last_used);
        let cancel = self.background.token();
        self.background.spawn(async move {
            loop {
                let deadline = *last_used.lock() + SPEECH_IDLE_TTL;
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep_until(deadline.into()) => {}
                }
                if last_used.lock().elapsed() < SPEECH_IDLE_TTL {
                    continue;
                }
                if let Some(model) = loaded.lock().await.take() {
                    let _ = tokio::task::spawn_blocking(move || drop(model)).await;
                    info!("read-aloud voice unloaded after idle");
                }
                return;
            }
        });
    }

    /// `text` read in `voice` at `speed` (1.0 is natural pace).
    pub async fn synthesize(
        &self,
        text: String,
        voice: &'static CuratedVoice,
        speed: f32,
    ) -> Result<Clip, AppError> {
        let _turn = self
            .speaking
            .acquire()
            .await
            .map_err(|e| AppError::InternalError(format!("read-aloud queue closed: {e}")))?;
        let dirs = self.dirs_for(voice).await?;
        let model = self.model().await?;
        *self.last_used.lock() = Instant::now();
        let samples = tokio::task::spawn_blocking(move || model.speak(&text, voice, &dirs, speed))
            .await
            .map_err(|e| AppError::InternalError(format!("reading aloud failed: {e}")))??;
        *self.last_used.lock() = Instant::now();
        Ok(Clip {
            samples,
            sample_rate: SAMPLE_RATE,
        })
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
