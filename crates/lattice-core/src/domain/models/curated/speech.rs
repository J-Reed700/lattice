//! The curated read-aloud model: Kokoro-82M with its British female voices
//! and the British English G2P that feeds it.
//!
//! Every file is pinned to a repository revision and its SHA-256 (the Hugging
//! Face tree API's `lfs.oid`, or the digest of the pinned bytes for the small
//! files stored outside LFS). They download under fixed local names into one
//! directory, which the speech engine reads:
//!
//!     ~/.cache/lattice/models/<id>/{kokoro-v1_0.json, kokoro-v1_0.pth,
//!         misaki-*.json, misaki-g2p-gb.*, voice-<voice>.bin}
//!
//! The voices are the onnx-community float32 packs, byte-identical to the
//! tensors in hexgrad's `.pt` files. All of it is Apache-2.0.

use crate::domain::models::metadata::ModelFileMetadata;
use crate::domain::models::selection::{
    ModelCategory, ModelFormat, ModelMetadata, PerformanceTier,
};

pub const SPEECH_MODEL_ID: &str = "kokoro-82m-british";
pub const SPEECH_CONFIG_FILE: &str = "kokoro-v1_0.json";
pub const SPEECH_WEIGHTS_FILE: &str = "kokoro-v1_0.pth";
pub const SPEECH_LEXICON_GOLD_FILE: &str = "misaki-gb_gold.json";
pub const SPEECH_LEXICON_SILVER_FILE: &str = "misaki-gb_silver.json";
pub const SPEECH_G2P_CONFIG_FILE: &str = "misaki-g2p-gb.json";
pub const SPEECH_G2P_WEIGHTS_FILE: &str = "misaki-g2p-gb.safetensors";

const KOKORO: &str =
    "https://huggingface.co/hexgrad/Kokoro-82M/resolve/f3ff3571791e39611d31c381e3a41a3af07b4987";
const KOKORO_ONNX: &str = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231";
const MISAKI: &str =
    "https://huggingface.co/datasets/hexgrad/misaki/resolve/7a6093fd11124188077f2dc16397cc465466d59a";
const MISAKI_G2P: &str = "https://huggingface.co/PeterReid/graphemes_to_phonemes_en_gb/resolve/d8357d5067fa26a5c34134d6bbcf4bbf000c0ac8";

/// A Kokoro voice pack: 510 style rows of 256 floats.
#[derive(Debug, Clone, Copy)]
pub struct CuratedVoice {
    pub id: &'static str,
    pub name: &'static str,
    pub sha256: &'static str,
}

const VOICE_BYTES: u64 = 522_240;

/// British English female voices, best graded first (hexgrad's `VOICES.md`:
/// Emma B-, Isabella C, Alice and Lily D).
pub const SPEECH_VOICES: &[CuratedVoice] = &[
    CuratedVoice {
        id: "bf_emma",
        name: "Emma",
        sha256: "669fe0647f9dd04fcab92f1439a40eeb4c8b4ab1f82e4996fe3d918ce4a63b73",
    },
    CuratedVoice {
        id: "bf_isabella",
        name: "Isabella",
        sha256: "3754352c4aaa46d17f27654ab7518d65b62ad6163a0f55a5f4330c2da2c4e94f",
    },
    CuratedVoice {
        id: "bf_alice",
        name: "Alice",
        sha256: "08afa6ba24da61ea5e8efa139e5aadc938d83f0a6da5a900adaf763ac1da5573",
    },
    CuratedVoice {
        id: "bf_lily",
        name: "Lily",
        sha256: "5e0ee32ebe64a467124976b14e69590746f1c4ce41a12b587a50c862edfea335",
    },
];

pub const DEFAULT_SPEECH_VOICE: &str = "bf_emma";

/// The local file a voice downloads to.
pub fn speech_voice_file(voice: &str) -> String {
    format!("voice-{voice}.bin")
}

fn file(local: &str, url: String, bytes: u64, sha256: &str) -> ModelFileMetadata {
    ModelFileMetadata::with_checksum(local.into(), url, bytes, sha256.into())
}

/// The read-aloud model, as a catalog entry.
pub fn get_curated_speech_models() -> Vec<ModelMetadata> {
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
            SPEECH_LEXICON_GOLD_FILE,
            format!("{MISAKI}/gb_gold.json"),
            2_841_062,
            "090cdecbe6bda9516a2cdc21563d62def0b91f5b55ea74fe6ef729eef682785f",
        ),
        file(
            SPEECH_LEXICON_SILVER_FILE,
            format!("{MISAKI}/gb_silver.json"),
            3_677_750,
            "cb8655f5e614e6306146e0da1bf93522ac542ebd39dcd9892afcf51c9aa52e1a",
        ),
        file(
            SPEECH_G2P_CONFIG_FILE,
            format!("{MISAKI_G2P}/config.json"),
            1_246,
            "e4f248e6af0cfb6cb54aea6ff0168d16b6f3cdabed438c487cde4ab0815d1bac",
        ),
        file(
            SPEECH_G2P_WEIGHTS_FILE,
            format!("{MISAKI_G2P}/model.safetensors"),
            3_011_692,
            "4994f474bb6f4584076a4e98189caaec11aa3f773a0d82d5bc263a03dd07e703",
        ),
    ];
    files.extend(SPEECH_VOICES.iter().map(|voice| {
        file(
            &speech_voice_file(voice.id),
            format!("{KOKORO_ONNX}/voices/{}.bin", voice.id),
            VOICE_BYTES,
            voice.sha256,
        )
    }));
    let total_size_bytes = files.iter().map(|f| f.size_bytes).sum();
    vec![ModelMetadata {
        id: SPEECH_MODEL_ID.into(),
        name: "Kokoro 82M (British voices)".into(),
        category: ModelCategory::Speech,
        description: "Reads chat answers and lessons aloud on this computer, in four \
                      British English voices."
            .into(),
        size_gb: total_size_bytes as f64 / 1_000_000_000.0,
        minimum_ram_gb: 2.0,
        recommended_ram_gb: 4.0,
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
    }]
}
