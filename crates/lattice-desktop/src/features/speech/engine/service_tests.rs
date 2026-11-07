#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::PathBuf;

use lattice_core::domain::models::curated::{find_speech_voice, SPEECH_VOICES};
use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice_core::domain::models::metadata::ModelType;
use lattice_core::domain::repositories::DownloadedModelRepository;
use lattice_core::shared::error::Result;

use super::*;

/// The `models` table holding the entries this install has downloaded.
struct Installed(HashMap<String, DownloadedModel>);

impl Installed {
    fn with(entries: &[(&str, &str)]) -> Arc<dyn DownloadedModelRepository> {
        let rows = entries.iter().map(|(id, dir)| {
            let row = DownloadedModel::new_with_catalog(
                format!("row-{id}"),
                (*id).into(),
                (*id).into(),
                ModelLocation::LocalFile {
                    path: PathBuf::from(dir).join("first-file"),
                },
                100,
                "kokoro".into(),
                None,
                |_| Some(ModelType::Speech),
            )
            .unwrap();
            ((*id).to_string(), row)
        });
        Arc::new(Self(rows.collect()))
    }
}

#[async_trait::async_trait]
impl DownloadedModelRepository for Installed {
    async fn delete(&self, _model_id: &str) -> Result<()> {
        unreachable!("speech never deletes")
    }
    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<DownloadedModel>> {
        Ok(self.0.get(model_id).cloned())
    }
    async fn list(&self) -> Result<Vec<DownloadedModel>> {
        unreachable!("speech reads one row")
    }
    async fn delete_by_model_id_if_not_active(
        &self,
        _model_id: &str,
        _active_model_id: &str,
    ) -> Result<bool> {
        unreachable!("speech never deletes")
    }
    async fn find_by_id(&self, _id: &str) -> Result<Option<DownloadedModel>> {
        unreachable!("speech reads by model id")
    }
    async fn save(&self, _model: &DownloadedModel) -> Result<()> {
        unreachable!("speech never writes")
    }
    async fn is_downloaded(&self, model_id: &str) -> Result<bool> {
        Ok(self.0.contains_key(model_id))
    }
    async fn has_any_embedding_model(&self) -> Result<bool> {
        unreachable!("speech reads one row")
    }
    async fn delete_if_not_active(&self, _id: &str) -> Result<Option<DownloadedModel>> {
        unreachable!("speech never deletes")
    }
}

fn speech(entries: &[(&str, &str)]) -> KokoroSpeech {
    KokoroSpeech::new(
        Installed::with(entries),
        Arc::new(BackgroundTasks::default()),
    )
}

async fn dirs(speech: &KokoroSpeech, voice: &str) -> std::result::Result<VoiceDirs, AppError> {
    speech
        .dirs_for(find_speech_voice(voice).expect("a curated voice"))
        .await
}

fn at(voice: &str, reader: &str) -> VoiceDirs {
    VoiceDirs {
        voice: voice.into(),
        reader: reader.into(),
    }
}

#[tokio::test]
async fn an_install_with_only_the_first_download_keeps_its_four_voices() {
    let speech = speech(&[(SPEECH_MODEL_ID, "first")]);
    assert_eq!(speech.model_dir().await.unwrap(), Some("first".into()));
    assert_eq!(speech.voice_pack_dir().await.unwrap(), None);
    for voice in ["bf_emma", "bf_isabella", "bf_alice", "bf_lily"] {
        assert_eq!(dirs(&speech, voice).await.unwrap(), at("first", "first"));
    }
}

#[tokio::test]
async fn a_voice_in_the_pack_says_it_is_not_downloaded_until_the_pack_is() {
    let speech = speech(&[(SPEECH_MODEL_ID, "first")]);
    for (voice, name) in [("af_heart", "Heart"), ("bm_george", "George")] {
        let error = dirs(&speech, voice).await.unwrap_err();
        assert!(
            matches!(&error, AppError::ServiceNotAvailable(message) if message.contains(name)),
            "{error}"
        );
    }
}

#[tokio::test]
async fn a_voice_in_the_pack_reads_in_its_own_accent() {
    let speech = speech(&[(SPEECH_MODEL_ID, "first"), (SPEECH_VOICE_PACK_ID, "pack")]);
    assert_eq!(dirs(&speech, "af_heart").await.unwrap(), at("pack", "pack"));
    assert_eq!(dirs(&speech, "am_adam").await.unwrap(), at("pack", "pack"));
    // A British man's file is in the pack; the British reader is not.
    assert_eq!(
        dirs(&speech, "bm_george").await.unwrap(),
        at("pack", "first")
    );
    assert_eq!(
        dirs(&speech, "bf_emma").await.unwrap(),
        at("first", "first")
    );
}

#[test]
#[ignore = "needs the downloaded speech bundle in LATTICE_KOKORO_DIR"]
fn the_first_download_alone_loads_and_speaks() {
    let dir = PathBuf::from(std::env::var_os("LATTICE_KOKORO_DIR").expect("LATTICE_KOKORO_DIR"));
    let loaded = Loaded::load(&dir).unwrap();
    let emma = find_speech_voice("bf_emma").unwrap();
    let dirs = VoiceDirs {
        voice: dir.clone(),
        reader: dir,
    };
    assert!(!loaded.speak("Hello.", emma, &dirs, 1.0).unwrap().is_empty());
}

/// Where the real downloads are, for the tests that speak.
struct Downloads {
    first: PathBuf,
    pack: PathBuf,
}

impl Downloads {
    fn from_env() -> Self {
        let dir = |name: &str| PathBuf::from(std::env::var_os(name).expect(name));
        Self {
            first: dir("LATTICE_KOKORO_DIR"),
            pack: dir("LATTICE_KOKORO_PACK_DIR"),
        }
    }

    fn of(&self, model_id: &str) -> PathBuf {
        if model_id == SPEECH_MODEL_ID {
            self.first.clone()
        } else {
            self.pack.clone()
        }
    }

    fn dirs(&self, voice: &CuratedVoice) -> VoiceDirs {
        VoiceDirs {
            voice: self.of(voice.model_id()),
            reader: self.of(voice.accent().model_id()),
        }
    }
}

#[test]
#[ignore = "needs the speech bundle and voice pack in LATTICE_KOKORO_DIR and LATTICE_KOKORO_PACK_DIR"]
fn every_voice_loads_and_the_pack_speaks_in_both_accents() {
    let downloads = Downloads::from_env();
    let loaded = Loaded::load(&downloads.first).unwrap();
    for voice in SPEECH_VOICES {
        loaded
            .voice(voice, &downloads.of(voice.model_id()))
            .unwrap();
    }
    for id in ["af_heart", "am_michael", "bm_george"] {
        let voice = find_speech_voice(id).unwrap();
        let spoken = loaded
            .speak("Hello there.", voice, &downloads.dirs(voice), 1.0)
            .unwrap();
        assert!(!spoken.is_empty(), "{id}");
    }
}

/// Holds what `transcribes_a_real_recording` listens for, so Whisper can
/// check the files as well as an ear.
const LISTENING_TEXT: &str = "The quick brown fox jumps over the lazy dog. The water is better \
                              than the butter, and I waited for the letter.";

#[test]
#[ignore = "needs both downloads, and LATTICE_KOKORO_WAV_DIR to write to"]
fn writes_a_wav_in_each_accent_for_a_listening_check() {
    let downloads = Downloads::from_env();
    let out =
        PathBuf::from(std::env::var_os("LATTICE_KOKORO_WAV_DIR").expect("LATTICE_KOKORO_WAV_DIR"));
    std::fs::create_dir_all(&out).unwrap();
    let loaded = Loaded::load(&downloads.first).unwrap();
    for id in ["af_heart", "am_michael", "bf_emma", "bm_george"] {
        let voice = find_speech_voice(id).unwrap();
        let spoken = loaded
            .speak(LISTENING_TEXT, voice, &downloads.dirs(voice), 1.0)
            .unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut wav = hound::WavWriter::create(out.join(format!("{id}.wav")), spec).unwrap();
        for sample in spoken {
            wav.write_sample((sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
                .unwrap();
        }
        wav.finalize().unwrap();
    }
}
