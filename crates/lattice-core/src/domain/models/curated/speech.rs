//! The curated read-aloud models: Kokoro-82M, every English voice hexgrad
//! lists in `VOICES.md`, and the G2P that reads each accent.
//!
//! Two catalog entries, because the download pipeline fetches an entry whole
//! and treats one with a missing file as not downloaded:
//!
//! - [`SPEECH_MODEL_ID`] is the model, the British G2P and the four British
//!   women. Its id and files stay as first shipped, so an install that has it
//!   keeps working and never fetches the 327 MB weights again; the name is
//!   only ever seen by the database and the directory on disk.
//! - [`SPEECH_VOICE_PACK_ID`] is everything added since: the other 24 voices
//!   and the American G2P. It speaks only with the first entry beside it.
//!
//! Every file is pinned to a repository revision and its SHA-256 (the Hugging
//! Face tree API's `lfs.oid`, or the digest of the pinned bytes for the small
//! files stored outside LFS). They download under fixed local names, one
//! directory per entry, which the speech engine reads:
//!
//!     ~/.cache/lattice/models/<id>/{kokoro-v1_0.json, kokoro-v1_0.pth,
//!         misaki-*.json, misaki-g2p-*.*, voice-<voice>.bin}
//!
//! The voices are the onnx-community float32 packs, byte-identical to the
//! tensors in hexgrad's `.pt` files. All of it is Apache-2.0.

use crate::domain::models::metadata::ModelFileMetadata;
use crate::domain::models::selection::{
    ModelCategory, ModelFormat, ModelMetadata, PerformanceTier,
};

pub const SPEECH_MODEL_ID: &str = "kokoro-82m-british";
pub const SPEECH_VOICE_PACK_ID: &str = "kokoro-82m-more-voices";
pub const SPEECH_CONFIG_FILE: &str = "kokoro-v1_0.json";
pub const SPEECH_WEIGHTS_FILE: &str = "kokoro-v1_0.pth";

const KOKORO: &str =
    "https://huggingface.co/hexgrad/Kokoro-82M/resolve/f3ff3571791e39611d31c381e3a41a3af07b4987";
const KOKORO_ONNX: &str = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231";
const MISAKI: &str =
    "https://huggingface.co/datasets/hexgrad/misaki/resolve/7a6093fd11124188077f2dc16397cc465466d59a";
const MISAKI_G2P_GB: &str = "https://huggingface.co/PeterReid/graphemes_to_phonemes_en_gb/resolve/d8357d5067fa26a5c34134d6bbcf4bbf000c0ac8";
const MISAKI_G2P_US: &str = "https://huggingface.co/PeterReid/graphemes_to_phonemes_en_us/resolve/a5631b285d18d59483c32c0c3379cb9fac924f4b";

/// The English a voice was trained on, which its text must be read in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpeechAccent {
    American,
    British,
}

/// The local files that read an accent: misaki's lexicons and its fallback.
#[derive(Debug, Clone, Copy)]
pub struct SpeechReaderFiles {
    pub gold: &'static str,
    pub silver: &'static str,
    pub g2p_config: &'static str,
    pub g2p_weights: &'static str,
}

impl SpeechAccent {
    pub fn reader_files(self) -> SpeechReaderFiles {
        match self {
            Self::American => SpeechReaderFiles {
                gold: "misaki-us_gold.json",
                silver: "misaki-us_silver.json",
                g2p_config: "misaki-g2p-us.json",
                g2p_weights: "misaki-g2p-us.safetensors",
            },
            Self::British => SpeechReaderFiles {
                gold: "misaki-gb_gold.json",
                silver: "misaki-gb_silver.json",
                g2p_config: "misaki-g2p-gb.json",
                g2p_weights: "misaki-g2p-gb.safetensors",
            },
        }
    }

    /// The catalog entry that holds [`Self::reader_files`].
    pub fn model_id(self) -> &'static str {
        match self {
            Self::American => SPEECH_VOICE_PACK_ID,
            Self::British => SPEECH_MODEL_ID,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechGender {
    Female,
    Male,
}

/// A Kokoro voice pack: 510 style rows of 256 floats.
#[derive(Debug, Clone, Copy)]
pub struct CuratedVoice {
    pub id: &'static str,
    pub name: &'static str,
    /// The overall grade in `VOICES.md`, hexgrad's estimate of the quality
    /// and quantity of the voice's training data.
    pub grade: &'static str,
    pub sha256: &'static str,
}

impl CuratedVoice {
    /// Kokoro's voice ids lead with the language, then the gender: `bf_emma`.
    pub fn accent(&self) -> SpeechAccent {
        if self.id.starts_with('a') {
            SpeechAccent::American
        } else {
            SpeechAccent::British
        }
    }

    pub fn gender(&self) -> SpeechGender {
        if self.id.get(1..2) == Some("m") {
            SpeechGender::Male
        } else {
            SpeechGender::Female
        }
    }

    /// The catalog entry that holds this voice's file.
    pub fn model_id(&self) -> &'static str {
        if self.id.starts_with("bf_") {
            SPEECH_MODEL_ID
        } else {
            SPEECH_VOICE_PACK_ID
        }
    }
}

const fn voice(
    id: &'static str,
    name: &'static str,
    grade: &'static str,
    sha256: &'static str,
) -> CuratedVoice {
    CuratedVoice {
        id,
        name,
        grade,
        sha256,
    }
}

const VOICE_BYTES: u64 = 522_240;

/// Every English voice in `VOICES.md`, British then American, best graded
/// first.
#[rustfmt::skip]
pub const SPEECH_VOICES: &[CuratedVoice] = &[
    voice("bf_emma", "Emma", "B-", "669fe0647f9dd04fcab92f1439a40eeb4c8b4ab1f82e4996fe3d918ce4a63b73"),
    voice("bf_isabella", "Isabella", "C", "3754352c4aaa46d17f27654ab7518d65b62ad6163a0f55a5f4330c2da2c4e94f"),
    voice("bm_fable", "Fable", "C", "f889083196807b4adb15e9204252165f503b8d33d3982e681c52443c49d798f1"),
    voice("bm_george", "George", "C", "c4b235a4c1f2cd3b939fed08b899ce9385638b763f7b73a59616c4fc9bd6c9bc"),
    voice("bm_lewis", "Lewis", "D+", "b8f671cef828c30e66fdf0b0756a76bba58f6bb3398cbbf27058642acbcedb97"),
    voice("bf_alice", "Alice", "D", "08afa6ba24da61ea5e8efa139e5aadc938d83f0a6da5a900adaf763ac1da5573"),
    voice("bf_lily", "Lily", "D", "5e0ee32ebe64a467124976b14e69590746f1c4ce41a12b587a50c862edfea335"),
    voice("bm_daniel", "Daniel", "D", "6b3194bbceffb746733cbc22c8f593dd44e401a71d53895a2dca891bc595a1e8"),
    voice("af_heart", "Heart", "A", "d583ccff3cdca2f7fae535cb998ac07e9fcb90f09737b9a41fa2734ec44a8f0b"),
    voice("af_bella", "Bella", "A-", "f69d836209b78eb8c66e75e3cda491e26ea838a3674257e9d4e5703cbaf55c8b"),
    voice("af_nicole", "Nicole", "B-", "cd2191ab31b914ed7b318416b0e4440fdf392ddad9106a060819aa600a64f59a"),
    voice("af_aoede", "Aoede", "C+", "4a004c33430762e2461eedb2013fad808ef4ab3121f5300f554476caf58d8361"),
    voice("af_kore", "Kore", "C+", "9be5221b6a941c04b561959b8ff0b06e809444dcc4ab7e75a7b23606f691819e"),
    voice("af_sarah", "Sarah", "C+", "4409fbc125afabacc615d94db5398d847006a737b0247d6892b7a9a0007a2f0a"),
    voice("am_fenrir", "Fenrir", "C+", "c27989f741f7ee34d273a39d8a595cc0837d35f5ced9a29b7cc162614616df43"),
    voice("am_michael", "Michael", "C+", "1d1f21dd8da39c30705cd4c75d039d265e9bc4a2a93ed09bc9e1b1225eb95ba1"),
    voice("am_puck", "Puck", "C+", "fcf73c989033e9233e0b98713eca600c8c74dcc1614b37009d5450ff4a2274a0"),
    voice("af_alloy", "Alloy", "C", "c4a6b876047fd7fb472edf4ebd63cfac7c3b958a7cae7c106e8f038ca6308c45"),
    voice("af_nova", "Nova", "C", "18778272caa0d0eebaea251c35fd635f038434f9eee5e691d02a174bd328414f"),
    voice("af_sky", "Sky", "C-", "4435255c9744f3f31659e0d714ab7689bf65d9e77ec1cce060f083912614f0b9"),
    voice("af_jessica", "Jessica", "D", "a240a5e3c15b43563d6e923bdca8ef5613a23471d9b77653694012435df23bd8"),
    voice("af_river", "River", "D", "00a2bcf82b1d86e8f19902ede58c65ccf6c0e43b44b7d74fad54e5d8933c9c30"),
    voice("am_echo", "Echo", "D", "3968b92c3c4cd1c4416dbded36c13eaa388a90d5788d02a13e4d781f5f8cf3c3"),
    voice("am_eric", "Eric", "D", "e8b5be17edd1e3636901ce7598baafe2dc8dd8ff707a0c23bf9e461add7e2832"),
    voice("am_liam", "Liam", "D", "52403be32fd047c6a44517cb0bcd6b134f2a18baa73e70ef41651e0eab921ade"),
    voice("am_onyx", "Onyx", "D", "da5d135b424164916d75a68ffb4c2abce3d7d5ccc82dd1ee6cf447ce286145e6"),
    voice("am_santa", "Santa", "D-", "61150cf726ab6c5ed7a99f90a304f91f5a72c00c592e89ec94e5df11c319227a"),
    voice("am_adam", "Adam", "F+", "162b035ed91cfc48b6046982184c645f72edcdd1b82843347f605d7bf7b15716"),
];

pub const DEFAULT_SPEECH_VOICE: &str = "bf_emma";

/// The voice with this id.
pub fn find_speech_voice(id: &str) -> Option<&'static CuratedVoice> {
    SPEECH_VOICES.iter().find(|voice| voice.id == id)
}

/// The local file a voice downloads to.
pub fn speech_voice_file(voice: &str) -> String {
    format!("voice-{voice}.bin")
}

fn file(local: &str, url: String, bytes: u64, sha256: &str) -> ModelFileMetadata {
    ModelFileMetadata::with_checksum(local.into(), url, bytes, sha256.into())
}

/// The voice files one catalog entry holds.
fn voice_files(model_id: &str) -> impl Iterator<Item = ModelFileMetadata> + '_ {
    SPEECH_VOICES
        .iter()
        .filter(move |voice| voice.model_id() == model_id)
        .map(|voice| {
            file(
                &speech_voice_file(voice.id),
                format!("{KOKORO_ONNX}/voices/{}.bin", voice.id),
                VOICE_BYTES,
                voice.sha256,
            )
        })
}

fn entry(id: &str, name: &str, description: &str, files: Vec<ModelFileMetadata>) -> ModelMetadata {
    let total_size_bytes = files.iter().map(|f| f.size_bytes).sum();
    ModelMetadata {
        id: id.into(),
        name: name.into(),
        category: ModelCategory::Speech,
        description: description.into(),
        size_gb: total_size_bytes as f64 / 1_000_000_000.0,
        context_length: 512,
        performance_tier: PerformanceTier::Fast,
        supported_quantizations: vec!["F32".into()],
        capabilities: vec!["text-to-speech".into()],
        download_url: Some("https://huggingface.co/hexgrad/Kokoro-82M".into()),
        license: "Apache-2.0".into(),
        requires_auth: false,
        model_id: Some("hexgrad/Kokoro-82M".into()),
        default_filename: None,
        files,
        total_size_bytes,
        embedding_dimensions: None,
        embedding_compatibility: None,
        format: ModelFormat::Safetensors,
    }
}

fn model_entry() -> ModelMetadata {
    let british = SpeechAccent::British.reader_files();
    let mut files = vec![
        file(
            SPEECH_CONFIG_FILE,
            format!("{KOKORO}/config.json"),
            2_351,
            "5abb01e2403b072bf03d04fde160443e209d7a0dad49a423be15196b9b43c17f",
        ),
        file(
            SPEECH_WEIGHTS_FILE,
            format!("{KOKORO}/kokoro-v1_0.pth"),
            327_212_226,
            "496dba118d1a58f5f3db2efc88dbdc216e0483fc89fe6e47ee1f2c53f18ad1e4",
        ),
        file(
            british.gold,
            format!("{MISAKI}/gb_gold.json"),
            2_841_062,
            "090cdecbe6bda9516a2cdc21563d62def0b91f5b55ea74fe6ef729eef682785f",
        ),
        file(
            british.silver,
            format!("{MISAKI}/gb_silver.json"),
            3_677_750,
            "cb8655f5e614e6306146e0da1bf93522ac542ebd39dcd9892afcf51c9aa52e1a",
        ),
        file(
            british.g2p_config,
            format!("{MISAKI_G2P_GB}/config.json"),
            1_246,
            "e4f248e6af0cfb6cb54aea6ff0168d16b6f3cdabed438c487cde4ab0815d1bac",
        ),
        file(
            british.g2p_weights,
            format!("{MISAKI_G2P_GB}/model.safetensors"),
            3_011_692,
            "4994f474bb6f4584076a4e98189caaec11aa3f773a0d82d5bc263a03dd07e703",
        ),
    ];
    files.extend(voice_files(SPEECH_MODEL_ID));
    entry(
        SPEECH_MODEL_ID,
        "Kokoro 82M",
        "Reads chat answers and lessons aloud on this computer, in four British English voices.",
        files,
    )
}

fn voice_pack_entry() -> ModelMetadata {
    let american = SpeechAccent::American.reader_files();
    let mut files = vec![
        file(
            american.gold,
            format!("{MISAKI}/us_gold.json"),
            3_001_196,
            "8507f89840f0813b10cf584740942f58e9cc9ad3660e24088b442ab0a6b126be",
        ),
        file(
            american.silver,
            format!("{MISAKI}/us_silver.json"),
            3_105_352,
            "ea0e1abca0c9b18fb0d3402034633a337154a3153e9a9f49f97d668c908e140c",
        ),
        file(
            american.g2p_config,
            format!("{MISAKI_G2P_US}/config.json"),
            1_257,
            "8deb3537fb29c63cd9f20d75515ae06e4c92f1b6db0703a2d45bca95b33a53a4",
        ),
        file(
            american.g2p_weights,
            format!("{MISAKI_G2P_US}/model.safetensors"),
            3_011_692,
            "dc4a02e62d4fcb4bb4097ecf00db89b8e1a12a549a52ab6adfbba220b80a55c5",
        ),
    ];
    files.extend(voice_files(SPEECH_VOICE_PACK_ID));
    entry(
        SPEECH_VOICE_PACK_ID,
        "Kokoro 82M: more voices",
        "Adds the American English voices and the British men's voices to Kokoro 82M.",
        files,
    )
}

/// The read-aloud models, as catalog entries.
pub fn get_curated_speech_models() -> Vec<ModelMetadata> {
    vec![model_entry(), voice_pack_entry()]
}
