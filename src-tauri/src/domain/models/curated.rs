//! # Curated Model Catalog
//!
//! Hardcoded catalog of recommended AI models across categories:
//! - LLMs: GGUF models optimized for local inference
//! - Embedding models: Semantic search optimized
//! - OCR models: Vision-language models for text extraction
//! - Transcription models: Whisper for on-device speech-to-text
//!
//! ## Design
//!
//! This module provides a curated list of models that work well with Lattice.
//! Models are selected based on:
//! - Size/performance tradeoff
//! - License compatibility
//! - Community adoption
//! - Quantization availability
//!
//! ## Example
//!
//! ```rust
//! use lattice::domain::models::curated::get_curated_llm_models;
//!
//! let llms = get_curated_llm_models();
//! for model in llms {
//!     println!("{}: {} GB", model.name, model.size_gb);
//! }
//! ```

use crate::domain::models::selection::ModelFormat;
use crate::domain::models::selection::{ModelCategory, ModelMetadata, PerformanceTier};
use std::path::Path;

/// Capability tag for a chat model whose GGUF chat template carries a native
/// tool-call format that llama.cpp parses under `--jinja`.
///
/// Every chat entry carries it. Qwen 3.5, 3.6 and 3.8 templates emit
/// `<tool_call>` JSON; Gemma 4's emit `<|tool_call>call:name{…}<tool_call|>`,
/// which the pinned llama-server (v0.6.0) parses with its own Gemma 4 chat
/// parser.
pub const TOOL_CALLING_CAPABILITY: &str = "tool-calling";

/// Whether a downloaded chat-model file is a catalog entry tagged for tool
/// calling. Matched on the file name, which is the entry's `default_filename`
/// for every catalog download.
///
/// `false` for anything else, including another quantisation of a tagged
/// model fetched from the Hugging Face browser: an unknown file has made no
/// promise about its template, and silence reads as "no tools".
pub fn chat_model_file_supports_tool_calling(file_name: &str) -> bool {
    let file_name = file_name.trim();
    CURATED_CHAT_MODELS.iter().any(|model| {
        model.file.eq_ignore_ascii_case(file_name)
            && model.capabilities.contains(&TOOL_CALLING_CAPABILITY)
    })
}

/// Whether the chat model at `model_path` is the catalog build meant to be
/// served with multi-token-prediction speculative decoding.
///
/// Matched on the download layout, `…/models/<catalog id>/<file>`, not on the
/// file name alone: the MTP build and the plain 9B publish the same file name
/// (`Qwen3.5-9B-Q4_K_M.gguf`) from different repositories, and only the
/// directory a catalog download lands in tells them apart.
///
/// Not "does the GGUF carry MTP layers": Qwen 3.8 27B's GGUF does too
/// (`qwen35.nextn_predict_layers = 1`), but nothing has measured MTP on this
/// app yet, so only the entry built for that experiment opts in.
pub fn chat_model_uses_mtp_draft(model_path: &Path) -> bool {
    let Some(file_name) = model_path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(directory) = model_path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
    else {
        return false;
    };
    CURATED_CHAT_MODELS.iter().any(|model| {
        model.mtp_draft
            && model.id.eq_ignore_ascii_case(directory)
            && model.file.eq_ignore_ascii_case(file_name)
    })
}

/// The catalog entry for one file of one Hugging Face repository, if the
/// catalog vouches for that exact file.
///
/// The catalog browser lists repository files with sizes from Hugging Face and
/// memory guessed from the file size. Where a file is a catalog entry, the
/// entry's own figures (KV cache from the GGUF header, a pinned SHA-256) and
/// its id replace the guess, so choosing it from the version list is the same
/// download first run makes.
pub fn curated_chat_model_for_file(repo: &str, file: &str) -> Option<ModelMetadata> {
    CURATED_CHAT_MODELS
        .iter()
        .find(|model| model.repo.eq_ignore_ascii_case(repo) && model.file == file)
        .map(CuratedChatModel::metadata)
}

/// Catalog chat entries a catalog search for `query` should show.
///
/// Every whitespace-separated term has to appear in the entry's name,
/// repository or id. An empty query, and the broad `gguf` query the catalog
/// overview runs, match every entry.
pub fn curated_chat_models_matching(query: &str) -> Vec<ModelMetadata> {
    let query = query.trim().to_lowercase();
    let terms: Vec<&str> = query.split_whitespace().collect();
    let browse_all = terms.is_empty() || terms == ["gguf"];
    CURATED_CHAT_MODELS
        .iter()
        .filter(|model| {
            let haystack = format!("{} {} {}", model.name, model.repo, model.id).to_lowercase();
            browse_all || terms.iter().all(|term| haystack.contains(term))
        })
        .map(CuratedChatModel::metadata)
        .collect()
}

/// One chat model the catalog vouches for: a single GGUF file, pinned by the
/// size and SHA-256 Hugging Face reports for it, so the download is verified
/// rather than trusted.
///
/// Sizes and digests come from `https://huggingface.co/api/models/<repo>/tree/main`
/// (`size`, `lfs.oid`; a HEAD of the resolve URL returns the same pair as
/// `x-linked-size` / `x-linked-etag`). Context lengths and KV shapes come
/// from each file's own GGUF header. A publisher that re-uploads a file keeps
/// its name and changes its digest; the download then fails verification
/// instead of installing something nobody checked, and the entry needs
/// re-reading.
#[derive(Debug, Clone, Copy)]
pub struct CuratedChatModel {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Hugging Face repository, `publisher/name`.
    pub repo: &'static str,
    /// The GGUF file inside `repo`.
    pub file: &'static str,
    pub size_bytes: u64,
    /// Lower-case hex SHA-256 of `file`.
    pub sha256: &'static str,
    pub quantization: &'static str,
    /// The window the model was trained for (`<arch>.context_length`).
    pub context_length: u32,
    kv: KvFootprint,
    pub performance_tier: PerformanceTier,
    pub capabilities: &'static [&'static str],
    pub license: &'static str,
    /// The GGUF carries multi-token-prediction layers and this entry exists to
    /// serve them: launch with MTP speculative decoding
    /// (`--spec-type draft-mtp --spec-draft-n-max N`), which also requires a
    /// single slot (`-np 1`). Read it through [`chat_model_uses_mtp_draft`].
    pub mtp_draft: bool,
}

/// What a model's KV cache costs on top of its weights, from its GGUF header,
/// at f16 (llama.cpp's default cache type).
///
/// Counted the way llama.cpp allocates it rather than as if every layer
/// attended to the whole window: Qwen 3.5/3.6/3.8 give one layer in four full
/// attention (`full_attention_interval = 4`; the rest keep a fixed recurrent
/// state), and Gemma 4 gives one in six, the others attending to a sliding
/// window whose cache stops growing at the window.
#[derive(Debug, Clone, Copy)]
struct KvFootprint {
    /// Key and value bytes per token across the layers that attend to the
    /// whole window: `layers * kv_heads * (key_len + value_len) * 2`.
    per_token_bytes: u64,
    /// The sliding-window layers' cache: `layers * kv_heads * (key_len +
    /// value_len) * 2 * (window + 512-token batch)`.
    windowed_bytes: u64,
}

/// Compute buffers, the recurrent state of hybrid layers and the server
/// itself. llama.cpp's output buffer alone is `vocab * batch * 4` bytes, about
/// half a gigabyte for these 250K-token vocabularies at the default batch.
const RUNTIME_OVERHEAD_BYTES: u64 = 1_000_000_000;

/// The window behind `minimum_ram_gb`: the sidecar's default context when it
/// cannot size one from the hardware (`DEFAULT_GPU_CONTEXT_SIZE`), and room
/// for retrieved passages plus a conversation.
const MINIMUM_WINDOW_TOKENS: u64 = 8_192;

/// The window behind `recommended_ram_gb`: the largest one the sidecar's
/// automatic sizing chooses (`AUTO_MAX_CONTEXT_SIZE`).
const COMFORTABLE_WINDOW_TOKENS: u64 = 32_768;

/// Memory the model holds while serving `window_tokens` of context, in GB
/// (10^9 bytes, as Hugging Face sizes are), rounded up to a tenth.
///
/// This is the figure the catalog shows as "Est. memory" and the one
/// `computeModelFit` compares with 70% of the machine's RAM.
fn memory_estimate_gb(weights_bytes: u64, kv: KvFootprint, window_tokens: u64) -> f64 {
    let bytes = weights_bytes
        + kv.windowed_bytes
        + kv.per_token_bytes * window_tokens
        + RUNTIME_OVERHEAD_BYTES;
    (bytes as f64 / 100_000_000.0).ceil() / 10.0
}

impl CuratedChatModel {
    fn metadata(&self) -> ModelMetadata {
        let repo_url = format!("https://huggingface.co/{}", self.repo);
        let file_url = format!("{repo_url}/resolve/main/{}", self.file);
        ModelMetadata {
            id: self.id.into(),
            name: self.name.into(),
            category: ModelCategory::LLM,
            description: self.description.into(),
            size_gb: self.size_bytes as f64 / 1_000_000_000.0,
            minimum_ram_gb: memory_estimate_gb(self.size_bytes, self.kv, MINIMUM_WINDOW_TOKENS),
            recommended_ram_gb: memory_estimate_gb(
                self.size_bytes,
                self.kv,
                COMFORTABLE_WINDOW_TOKENS,
            ),
            context_length: self.context_length,
            performance_tier: self.performance_tier,
            supported_quantizations: vec![self.quantization.into()],
            capabilities: self.capabilities.iter().map(|&c| c.into()).collect(),
            download_url: Some(repo_url),
            license: self.license.into(),
            requires_auth: false,
            model_id: Some(self.repo.into()),
            default_filename: Some(self.file.into()),
            // One listed file routes the download through the path that
            // passes the digest to the download manager, which hashes the
            // bytes as they arrive and rejects a mismatch.
            files: vec![super::metadata::ModelFileMetadata::with_checksum(
                self.file.into(),
                file_url,
                self.size_bytes,
                self.sha256.into(),
            )],
            total_size_bytes: self.size_bytes,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        }
    }
}

const QWEN_CAPABILITIES: &[&str] = &[
    "chat",
    "code",
    "reasoning",
    "multilingual",
    "long-context",
    TOOL_CALLING_CAPABILITY,
];

const GEMMA_CAPABILITIES: &[&str] = &[
    "chat",
    "code",
    "reasoning",
    "multilingual",
    "long-context",
    TOOL_CALLING_CAPABILITY,
];

/// Qwen 3.5 2B/4B/9B: 24 or 32 blocks, a quarter of them full attention
/// (6 or 8), 2 or 4 KV heads, 256-wide keys and values.
const QWEN35_2B_KV: KvFootprint = KvFootprint {
    per_token_bytes: 6 * 2 * 512 * 2,
    windowed_bytes: 0,
};
const QWEN35_4B_9B_KV: KvFootprint = KvFootprint {
    per_token_bytes: 8 * 4 * 512 * 2,
    windowed_bytes: 0,
};

/// The chat catalog, smallest working set first.
///
/// Every entry is current-generation (Qwen 3.5 or later, Gemma 4), so the
/// tool loop, citation and grounding prompts all meet a template that knows
/// tools. Older families were removed rather than hidden: a pre-release
/// catalog has no installs to keep working.
///
/// Thinking defaults below are read from each GGUF's embedded template
/// (`tokenizer.chat_template`), which is what llama-server renders. They
/// disagree between entries, so a request has to say `enable_thinking`
/// explicitly rather than rely on the default.
const CURATED_CHAT_MODELS: &[CuratedChatModel] = &[
    // Template: thinking off unless `enable_thinking: true`. The 2B model card
    // warns it is more prone to thinking loops, so it stays the floor for
    // machines that cannot hold the 4B.
    CuratedChatModel {
        id: "qwen3.5-2b-q4_k_m",
        name: "Qwen 3.5 2B",
        description: "The smallest Qwen 3.5, for machines with less than 8 GB of memory. Answers from your library; weaker at long, multi-step questions.",
        repo: "unsloth/Qwen3.5-2B-GGUF",
        file: "Qwen3.5-2B-Q4_K_M.gguf",
        size_bytes: 1_280_835_840,
        sha256: "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
        quantization: "Q4_K_M",
        context_length: 262_144,
        kv: QWEN35_2B_KV,
        performance_tier: PerformanceTier::Fast,
        capabilities: QWEN_CAPABILITIES,
        license: "Apache-2.0",
        mtp_draft: false,
    },
    // Template: thinking off unless `enable_thinking: true`.
    CuratedChatModel {
        id: "qwen3.5-4b-q4_k_m",
        name: "Qwen 3.5 4B",
        description: "A compact Qwen 3.5 that follows instructions and cites sources well for its size. The chat model for 8 GB machines, and a quick utility model on larger ones.",
        repo: "unsloth/Qwen3.5-4B-GGUF",
        file: "Qwen3.5-4B-Q4_K_M.gguf",
        size_bytes: 2_740_937_888,
        sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
        quantization: "Q4_K_M",
        context_length: 262_144,
        kv: QWEN35_4B_9B_KV,
        performance_tier: PerformanceTier::Balanced,
        capabilities: QWEN_CAPABILITIES,
        license: "Apache-2.0",
        mtp_draft: false,
    },
    // Gemma 4 E4B: 42 blocks, the last 18 reusing earlier layers' cache; of
    // the 24 that keep one, 4 are global (2 KV heads, 512-wide) and 20 slide
    // over 512 tokens (256-wide). Template: thinking off unless
    // `enable_thinking: true`.
    CuratedChatModel {
        id: "gemma-4-e4b-it-q4_k_m",
        name: "Gemma 4 E4B",
        description: "Google's efficient Gemma 4, with about 4B parameters active per token. A fast alternative to Qwen 3.5 9B on 12 to 16 GB machines, with native tool calls.",
        repo: "unsloth/gemma-4-E4B-it-GGUF",
        file: "gemma-4-E4B-it-Q4_K_M.gguf",
        size_bytes: 4_977_171_584,
        sha256: "85a896a047553e842f25297ee5b031d64ff30147d9c4af17b1e4b394cd1fab87",
        quantization: "Q4_K_M",
        context_length: 131_072,
        kv: KvFootprint {
            per_token_bytes: 4 * 2 * 1024 * 2,
            windowed_bytes: 20 * 2 * 512 * 2 * (512 + 512),
        },
        performance_tier: PerformanceTier::Balanced,
        capabilities: GEMMA_CAPABILITIES,
        license: "Apache-2.0",
        mtp_draft: false,
    },
    // Template: thinking off unless `enable_thinking: true`; a system
    // message anywhere but first raises a template error.
    CuratedChatModel {
        id: "qwen3.5-9b-q4_k_m",
        name: "Qwen 3.5 9B",
        description: "The recommended chat model where memory allows: strong reasoning, tool use and grounding in retrieved passages.",
        repo: "unsloth/Qwen3.5-9B-GGUF",
        file: "Qwen3.5-9B-Q4_K_M.gguf",
        size_bytes: 5_680_522_464,
        sha256: "03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8",
        quantization: "Q4_K_M",
        context_length: 262_144,
        kv: QWEN35_4B_9B_KV,
        performance_tier: PerformanceTier::Balanced,
        capabilities: QWEN_CAPABILITIES,
        license: "Apache-2.0",
        mtp_draft: false,
    },
    // The same weights plus one MTP block (`qwen35.nextn_predict_layers = 1`,
    // 33 blocks), whose full-attention cache is counted here. Unsloth's
    // template for this build differs from the plain 9B's: thinking is ON
    // unless `enable_thinking: false`, and up to two leading system or
    // developer messages are merged instead of raising.
    CuratedChatModel {
        id: "qwen3.5-9b-mtp-q4_k_m",
        name: "Qwen 3.5 9B (MTP)",
        description: "Qwen 3.5 9B packaged with its multi-token-prediction layers, so the server can draft several tokens per step. An experiment in faster answers; the speed-up has not been measured here yet.",
        repo: "unsloth/Qwen3.5-9B-MTP-GGUF",
        file: "Qwen3.5-9B-Q4_K_M.gguf",
        size_bytes: 5_868_826_976,
        sha256: "e8dd94817e95d6c0939102049d068418269978377b13616c4726235e232841fe",
        quantization: "Q4_K_M",
        context_length: 262_144,
        kv: KvFootprint {
            per_token_bytes: 9 * 4 * 512 * 2,
            windowed_bytes: 0,
        },
        performance_tier: PerformanceTier::Balanced,
        capabilities: QWEN_CAPABILITIES,
        license: "Apache-2.0",
        mtp_draft: true,
    },
    // Gemma 4 12B: 48 blocks, 8 global (1 KV head, 512-wide) and 40 sliding
    // over 1024 tokens (8 KV heads, 256-wide). Template: thinking off unless
    // `enable_thinking: true`.
    CuratedChatModel {
        id: "gemma-4-12b-it-q4_k_m",
        name: "Gemma 4 12B",
        description: "Google's mid-size Gemma 4 for 16 to 24 GB machines: a step up from the small models in writing and reasoning, with native tool calls.",
        repo: "unsloth/gemma-4-12b-it-GGUF",
        file: "gemma-4-12b-it-Q4_K_M.gguf",
        size_bytes: 7_121_861_440,
        sha256: "0a270ec9fe6b34f4a0d33992b6135117b484ebc4766ab76b51d4ae8c457e4c42",
        quantization: "Q4_K_M",
        context_length: 262_144,
        kv: KvFootprint {
            per_token_bytes: 8 * 1024 * 2,
            windowed_bytes: 40 * 8 * 512 * 2 * (1024 + 512),
        },
        performance_tier: PerformanceTier::Balanced,
        capabilities: GEMMA_CAPABILITIES,
        license: "Apache-2.0",
        mtp_draft: false,
    },
    // Unsloth publishes no plain Q4_K_M for this model; UD-Q4_K_M is its
    // dynamic Q4_K_M and the file Lattice's remote evals run. 64 blocks + 1
    // MTP block, 17 of them full attention (4 KV heads, 256-wide). Template:
    // thinking ON unless `enable_thinking: false`; `reasoning_effort` accepts
    // only xhigh (default), high, medium and low and raises on anything else;
    // `preserve_thinking` defaults on, so prior reasoning stays in the prefix.
    CuratedChatModel {
        id: "qwen3.8-27b-ud-q4_k_m",
        name: "Qwen 3.8 27B",
        description: "Qwen's newest dense model, for machines with 32 GB of memory or more: much stronger than the 9B at long, multi-step research, and slower.",
        repo: "unsloth/Qwen3.8-27B-GGUF",
        file: "Qwen3.8-27B-UD-Q4_K_M.gguf",
        size_bytes: 16_464_440_224,
        sha256: "322e194ff79741c7baa497c240f677f54b201b0efab44ca8e50f122b39123482",
        quantization: "UD-Q4_K_M",
        context_length: 262_144,
        kv: KvFootprint {
            per_token_bytes: 17 * 4 * 512 * 2,
            windowed_bytes: 0,
        },
        performance_tier: PerformanceTier::Accurate,
        capabilities: QWEN_CAPABILITIES,
        license: "Apache-2.0",
        mtp_draft: false,
    },
    // No plain Q4_K_M here either; UD-Q4_K_M is the nearest. 30 blocks, 5
    // global (2 KV heads, 512-wide) and 25 sliding over 1024 tokens (8 KV
    // heads, 256-wide). Template: thinking off unless `enable_thinking: true`.
    CuratedChatModel {
        id: "gemma-4-26b-a4b-it-ud-q4_k_m",
        name: "Gemma 4 26B A4B",
        description: "Google's mixture-of-experts Gemma 4: 26B parameters with about 4B active per token, so it answers quickly for its size. For machines with 32 GB of memory or more.",
        repo: "unsloth/gemma-4-26B-A4B-it-GGUF",
        file: "gemma-4-26B-A4B-it-UD-Q4_K_M.gguf",
        size_bytes: 16_947_541_728,
        sha256: "f2c28b3dc4776931ac6f879e11f203dec637ea0f14267a86ec8f6165f63f293f",
        quantization: "UD-Q4_K_M",
        context_length: 262_144,
        kv: KvFootprint {
            per_token_bytes: 5 * 2 * 1024 * 2,
            windowed_bytes: 25 * 8 * 512 * 2 * (1024 + 512),
        },
        performance_tier: PerformanceTier::Accurate,
        capabilities: GEMMA_CAPABILITIES,
        license: "Apache-2.0",
        mtp_draft: false,
    },
    // UD-Q4_K_XL because Unsloth's KL-divergence table for this architecture
    // (measured on Qwen 3.5 35B A3B) puts it at 0.0137 against Q4_K_M's 0.0192
    // for about 0.7 GB more; no such table exists for the other entries, which
    // therefore stay on Q4_K_M. 40 blocks, 10 full attention (2 KV heads,
    // 256-wide). Template: thinking ON unless `enable_thinking: false`;
    // `preserve_thinking` (default off) keeps prior reasoning in the prefix.
    CuratedChatModel {
        id: "qwen3.6-35b-a3b-ud-q4_k_xl",
        name: "Qwen 3.6 35B A3B",
        description: "Qwen's mixture-of-experts model: 35B parameters with 3B active per token, quick for its quality. Needs 36 GB of memory or more.",
        repo: "unsloth/Qwen3.6-35B-A3B-GGUF",
        file: "Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf",
        size_bytes: 22_360_456_160,
        sha256: "707a55a8a4397ecde44de0c499d3e68c1ad1d240d1da65826b4949d1043f4450",
        quantization: "UD-Q4_K_XL",
        context_length: 262_144,
        kv: KvFootprint {
            per_token_bytes: 10 * 2 * 512 * 2,
            windowed_bytes: 0,
        },
        performance_tier: PerformanceTier::Accurate,
        capabilities: QWEN_CAPABILITIES,
        license: "Apache-2.0",
        mtp_draft: false,
    },
];

/// Get curated list of LLM models for text generation and chat: every entry
/// in [`CURATED_CHAT_MODELS`], as catalog metadata.
pub fn get_curated_llm_models() -> Vec<ModelMetadata> {
    CURATED_CHAT_MODELS
        .iter()
        .map(CuratedChatModel::metadata)
        .collect()
}

/// First-run chat-model tier, by system RAM. One family, sized to the
/// machine: a model that does not fit swaps, and a first chat that swaps looks
/// broken.
///
/// - `< 8 GB`  → Qwen 3.5 2B Q4 (2.4 GB working set)
/// - `8–16 GB` → Qwen 3.5 4B Q4 (4.1 GB)
/// - `≥ 16 GB` → Qwen 3.5 9B Q4 (7.0 GB)
///
/// Each fits inside 56% of its tier's smallest machine, the share the catalog
/// browser calls "Fits". Larger machines still start on the 9B: it is what
/// the evaluations measure, it downloads in minutes rather than the better
/// part of an hour, and the 27B and the mixture-of-experts models are one
/// choice away in the catalog.
pub fn recommend_chat_model_for_ram(effective_ram_gb: f64) -> &'static str {
    if effective_ram_gb < 8.0 {
        "qwen3.5-2b-q4_k_m"
    } else if effective_ram_gb < 16.0 {
        "qwen3.5-4b-q4_k_m"
    } else {
        "qwen3.5-9b-q4_k_m"
    }
}

/// Get curated list of embedding models for semantic search.
///
/// Returns models optimized for:
/// - **Qwen3 Embedding 0.6B**: multilingual instruction-aware retrieval (1024 dim).
///   The first-run default wherever a GPU backend is available.
/// - **all-MiniLM-L6-v2**: Compact, fast general-purpose embeddings (384 dim).
///   The first-run default on CPU-only machines, where 0.6B parameters would
///   make bulk indexing impractical.
///
/// These models generate vector representations for semantic similarity.
/// All entries use safetensors files compatible with the Candle runtime.
pub fn get_curated_embedding_models() -> Vec<ModelMetadata> {
    vec![
        ModelMetadata {
            id: "qwen3-embedding-0.6b".into(),
            name: "Qwen3 Embedding 0.6B".into(),
            category: ModelCategory::Embedding,
            description: "Instruction-aware multilingual retrieval, and the default wherever a GPU can run it. Local runtime uses up to 2048 tokens per passage and half precision on Metal.".into(),
            size_gb: 1.2,
            minimum_ram_gb: 6.0,
            recommended_ram_gb: 8.0,
            context_length: 2048,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["F16".into(), "F32".into()],
            capabilities: vec!["embedding".into(), "retrieval".into(), "multilingual".into()],
            download_url: Some("https://huggingface.co/Qwen/Qwen3-Embedding-0.6B".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("Qwen/Qwen3-Embedding-0.6B".into()),
            default_filename: None,
            files: ["model.safetensors", "tokenizer.json", "config.json", "tokenizer_config.json", "1_Pooling/config.json"].into_iter().map(|name| {
                super::metadata::ModelFileMetadata::new(name.into(), format!("https://huggingface.co/Qwen/Qwen3-Embedding-0.6B/resolve/main/{name}"), if name == "model.safetensors" { 1_200_000_000 } else { 1_000 })
            }).collect(),
            total_size_bytes: 1_210_000_000,
            embedding_dimensions: Some(1024),
            embedding_compatibility: None,
            format: ModelFormat::Safetensors,
        },
        ModelMetadata {
            id: "all-minilm-l6-v2".into(),
            name: "all-MiniLM-L6-v2".into(),
            category: ModelCategory::Embedding,
            description: "Compact embedding model (384 dimensions). Very fast inference, good quality for most use cases.".into(),
            size_gb: 0.09,
            minimum_ram_gb: 1.0,
            recommended_ram_gb: 2.0,
            context_length: 512,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["F16".into(), "F32".into()],
            capabilities: vec!["embedding".into(), "retrieval".into(), "fast".into()],
            download_url: Some("https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("sentence-transformers/all-MiniLM-L6-v2".into()),
            default_filename: None, // Multi-file model bundle (ONNX + tokenizer metadata)
            files: vec![
                super::metadata::ModelFileMetadata::new(
                    "model.safetensors".to_string(),
                    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors".to_string(),
                    90_900_000,
                ),
                super::metadata::ModelFileMetadata::new(
                    "tokenizer.json".to_string(),
                    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json".to_string(),
                    700_000,
                ),
                super::metadata::ModelFileMetadata::new(
                    "config.json".to_string(),
                    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json".to_string(),
                    1_000,
                ),
                super::metadata::ModelFileMetadata::new("tokenizer_config.json".into(), "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer_config.json".into(), 1000),
                super::metadata::ModelFileMetadata::new("sentence_bert_config.json".into(), "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/sentence_bert_config.json".into(), 1000),
                super::metadata::ModelFileMetadata::new("1_Pooling/config.json".into(), "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/1_Pooling/config.json".into(), 1000),
            ],
            total_size_bytes: 91_700_000,
            embedding_dimensions: Some(384),
            embedding_compatibility: None,
            format: ModelFormat::Safetensors,
        },
    ]
}

/// Curated embedding entries whose training documents Matryoshka Representation
/// Learning, i.e. whose leading coordinates are individually usable as a
/// shorter embedding.
///
/// A per-entry flag rather than a guess: truncating a model that was not
/// trained this way silently destroys its geometry, and the failure looks like
/// "search got worse" rather than like an error. Qwen3-Embedding ships MRL as a
/// documented feature (its card advertises user-defined output dimensions).
/// all-MiniLM-L6-v2 predates the technique entirely.
///
/// Matched against the *catalog* id, and also against the HuggingFace
/// `model_id`, because a downloaded model records whichever the download path
/// gave it.
const MATRYOSHKA_EMBEDDING_MODELS: &[&str] = &["qwen3-embedding-0.6b", "qwen/qwen3-embedding-0.6b"];

/// Whether a curated embedding entry advertises Matryoshka support.
///
/// `false` for anything not in the catalog: an unknown model has made no such
/// promise, and the safe reading of silence is "do not truncate".
pub fn embedding_model_supports_matryoshka(model_id: &str) -> bool {
    let normalized = model_id.trim().to_lowercase();
    MATRYOSHKA_EMBEDDING_MODELS.contains(&normalized.as_str())
}

/// Local name of an OCR entry's language-model weights. Every OCR entry is
/// downloaded under these two fixed names, whatever the remote files are
/// called, so the OCR engine loads one layout:
///
///     ~/.cache/lattice/models/<id>/{model.gguf, mmproj.gguf}
pub const OCR_MODEL_FILE: &str = "model.gguf";

/// Local name of an OCR entry's vision projector, passed as `--mmproj`.
pub const OCR_PROJECTOR_FILE: &str = "mmproj.gguf";

/// A downloadable OCR model: a GGUF language model plus its vision projector,
/// served by a dedicated `llama-server`.
///
/// Sizes and SHA-256 digests are the Hugging Face tree API's `size` and
/// `lfs.oid` for the pinned files, so a truncated or substituted download
/// fails verification instead of failing at load.
#[derive(Debug, Clone, Copy)]
pub struct CuratedOcrModel {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Hugging Face repository holding both files.
    pub repo: &'static str,
    pub model_remote_file: &'static str,
    pub model_bytes: u64,
    pub model_sha256: &'static str,
    pub projector_remote_file: &'static str,
    pub projector_bytes: u64,
    pub projector_sha256: &'static str,
    /// The instruction the model card prescribes for plain text recognition.
    pub prompt: &'static str,
    pub license: &'static str,
    pub minimum_ram_gb: f64,
    pub recommended_ram_gb: f64,
}

/// The OCR catalog, in order of preference when several are downloaded.
///
/// PaddleOCR-VL-1.6 (0.9B, 2026-06) scores 96.33 on OmniDocBench v1.6. The
/// pinned llama.cpp lists its `paddleocr` architecture, and the official
/// PaddlePaddle GGUF loads in the bundled binary in under a second and reads a
/// dense page in about five on Apple Silicon (verified 2026-10-13). The card's
/// prompt for text is `OCR:`; it also documents `Table Recognition:` and
/// `Formula Recognition:` for cropped elements, which a whole page does not need.
pub const CURATED_OCR_MODELS: &[CuratedOcrModel] = &[CuratedOcrModel {
    id: "paddleocr-vl-1.6",
    name: "PaddleOCR-VL 1.6",
    description: "Reads scanned PDF pages so they can be searched and cited. \
                  A 0.9B document model run only while scans are being indexed.",
    repo: "PaddlePaddle/PaddleOCR-VL-1.6-GGUF",
    model_remote_file: "PaddleOCR-VL-1.6-GGUF.gguf",
    model_bytes: 935_769_056,
    model_sha256: "f3ae46ec885050acf4b3d31944431e1fd90d50664fb09126af4a3c050ba14ee8",
    projector_remote_file: "PaddleOCR-VL-1.6-GGUF-mmproj.gguf",
    projector_bytes: 881_770_560,
    projector_sha256: "204d757d7610d9b3faab10d506d69e5b244e32bf765e2bab2d0167e65e0a058a",
    prompt: "OCR:",
    license: "Apache-2.0",
    minimum_ram_gb: 4.0,
    recommended_ram_gb: 8.0,
}];

impl CuratedOcrModel {
    fn metadata(&self) -> ModelMetadata {
        let total_size_bytes = self.model_bytes + self.projector_bytes;
        let file = |local: &str, remote: &str, bytes: u64, sha256: &str| {
            super::metadata::ModelFileMetadata::with_checksum(
                local.into(),
                format!("https://huggingface.co/{}/resolve/main/{remote}", self.repo),
                bytes,
                sha256.into(),
            )
        };
        ModelMetadata {
            id: self.id.into(),
            name: self.name.into(),
            category: ModelCategory::OCR,
            description: self.description.into(),
            size_gb: total_size_bytes as f64 / 1_000_000_000.0,
            minimum_ram_gb: self.minimum_ram_gb,
            recommended_ram_gb: self.recommended_ram_gb,
            context_length: 4096,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["BF16".into()],
            capabilities: vec![
                "ocr".into(),
                "vision".into(),
                "document-understanding".into(),
            ],
            download_url: Some(format!("https://huggingface.co/{}", self.repo)),
            license: self.license.into(),
            requires_auth: false,
            model_id: Some(self.repo.into()),
            default_filename: None, // two-file bundle
            // The weights first: the download saga records the first `.gguf`
            // as the model's location, and the engine reads its directory.
            files: vec![
                file(
                    OCR_MODEL_FILE,
                    self.model_remote_file,
                    self.model_bytes,
                    self.model_sha256,
                ),
                file(
                    OCR_PROJECTOR_FILE,
                    self.projector_remote_file,
                    self.projector_bytes,
                    self.projector_sha256,
                ),
            ],
            total_size_bytes,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        }
    }
}

/// Curated OCR models, as catalog entries.
pub fn get_curated_ocr_models() -> Vec<ModelMetadata> {
    CURATED_OCR_MODELS
        .iter()
        .map(CuratedOcrModel::metadata)
        .collect()
}

/// Curated on-device transcription models.
///
/// Both entries are quantized in candle's own GGUF layout — the only whisper
/// weights `quantized_model::Whisper::load` can read. Every URL here has been
/// verified to resolve and every model has been verified to transcribe; whisper
/// base and small do **not** exist in this format anywhere public, which is why
/// tiny and the distilled medium are the two rungs on offer.
///
/// Each file is downloaded under a **fixed local name** even though the remote
/// name may be size-qualified, so the engine loads one stable layout:
///
///     ~/.cache/lattice/models/<id>/{model.gguf, config.json, tokenizer.json}
///
/// `melfilters.bytes` is deliberately **not** downloaded: the 80-bin mel
/// filterbank is a constant of the architecture, identical for every 80-mel
/// model, and is bundled with the engine
/// (`features/transcription/engine/melfilters.bytes`).
///
/// `model.gguf` is the primary file per `features/download/saga.rs::select_primary_model_file`,
/// so the recorded `ModelLocation` is `LocalFile{…/model.gguf}` and
/// `DownloadedModelRepository::is_downloaded` reduces to `path.is_file()`.
/// The engine takes `location.enclosing_dir()` to find the siblings.
pub fn get_curated_transcription_models() -> Vec<ModelMetadata> {
    vec![
        ModelMetadata {
            id: "whisper-tiny-q8".into(),
            name: "Whisper Tiny".into(),
            category: ModelCategory::Transcription,
            description: "Fast on-device transcription for voice memos and meetings. \
                          Good on clear English speech; misses names and accents."
                .into(),
            size_gb: 0.042,
            minimum_ram_gb: 1.0,
            recommended_ram_gb: 2.0,
            context_length: 448,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["Q8_0".into()],
            capabilities: vec!["transcription".into(), "multilingual".into()],
            download_url: Some("https://huggingface.co/lmz/candle-whisper".into()),
            license: "MIT".into(),
            requires_auth: false,
            model_id: Some("lmz/candle-whisper".into()),
            default_filename: None, // multi-file bundle
            files: vec![
                super::metadata::ModelFileMetadata::new(
                    "model.gguf".into(),
                    "https://huggingface.co/lmz/candle-whisper/resolve/main/model-tiny-q80.gguf"
                        .into(),
                    0,
                ),
                super::metadata::ModelFileMetadata::new(
                    "config.json".into(),
                    "https://huggingface.co/lmz/candle-whisper/resolve/main/config-tiny.json"
                        .into(),
                    0,
                ),
                super::metadata::ModelFileMetadata::new(
                    "tokenizer.json".into(),
                    "https://huggingface.co/lmz/candle-whisper/resolve/main/tokenizer-tiny.json"
                        .into(),
                    0,
                ),
            ],
            total_size_bytes: 41_841_632,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
        ModelMetadata {
            id: "whisper-medium-distil-q8".into(),
            name: "Whisper Medium (distilled)".into(),
            category: ModelCategory::Transcription,
            description: "Higher-accuracy on-device transcription. Much better with names, \
                          accents and crosstalk; needs about 430 MB of disk."
                .into(),
            size_gb: 0.43,
            minimum_ram_gb: 2.0,
            recommended_ram_gb: 4.0,
            context_length: 448,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["Q8_0".into()],
            capabilities: vec!["transcription".into(), "multilingual".into()],
            download_url: Some(
                "https://huggingface.co/Demonthos/candle-quantized-whisper-medium-distil".into(),
            ),
            license: "MIT".into(),
            requires_auth: false,
            model_id: Some("Demonthos/candle-quantized-whisper-medium-distil".into()),
            default_filename: None,
            files: vec![
                super::metadata::ModelFileMetadata::new(
                    "model.gguf".into(),
                    "https://huggingface.co/Demonthos/candle-quantized-whisper-medium-distil/resolve/main/model.gguf"
                        .into(),
                    0,
                ),
                super::metadata::ModelFileMetadata::new(
                    "config.json".into(),
                    "https://huggingface.co/Demonthos/candle-quantized-whisper-medium-distil/resolve/main/config.json"
                        .into(),
                    0,
                ),
                super::metadata::ModelFileMetadata::new(
                    "tokenizer.json".into(),
                    "https://huggingface.co/Demonthos/candle-quantized-whisper-medium-distil/resolve/main/tokenizer.json"
                        .into(),
                    0,
                ),
            ],
            total_size_bytes: 430_005_504,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
    ]
}

/// Get all curated models across all categories.
///
/// Returns combined list of LLMs, embedding models, OCR models and
/// transcription models.
pub fn get_all_curated_models() -> Vec<ModelMetadata> {
    let mut models = Vec::new();
    models.extend(get_curated_llm_models());
    models.extend(get_curated_embedding_models());
    models.extend(get_curated_ocr_models());
    models.extend(get_curated_transcription_models());
    models
}

/// Get curated models by category.
///
/// # Arguments
/// - `category` - Model category to filter by
///
/// # Returns
/// List of models in the specified category.
pub fn get_curated_models_by_category(category: ModelCategory) -> Vec<ModelMetadata> {
    match category {
        ModelCategory::LLM => get_curated_llm_models(),
        ModelCategory::Embedding => get_curated_embedding_models(),
        ModelCategory::OCR => get_curated_ocr_models(),
        ModelCategory::Transcription => get_curated_transcription_models(),
    }
}

/// Look up the category of a model by its ID in the curated model catalog.
///
/// Searches all curated model lists (chat, embedding, OCR) to find the matching
/// model and return its category.
///
/// # Arguments
/// * `model_id` - The model identifier (case-insensitive)
///
/// # Returns
/// * `Some(ModelCategory)` if found in any curated list
/// * `None` if not found in catalog
///
/// # Example
/// ```
/// use crate::domain::models::curated::{get_model_category_by_id, ModelCategory};
///
/// let category = get_model_category_by_id("all-minilm-l6-v2");
/// assert_eq!(category, Some(ModelCategory::Embedding));
/// ```
pub fn get_model_category_by_id(model_id: &str) -> Option<ModelCategory> {
    let normalized_id = model_id.to_lowercase();

    // Search chat models
    for model in get_curated_llm_models() {
        if model.id.to_lowercase() == normalized_id {
            return Some(model.category);
        }
    }

    // Search embedding models
    for model in get_curated_embedding_models() {
        if model.id.to_lowercase() == normalized_id {
            return Some(model.category);
        }
    }

    // Search OCR models
    for model in get_curated_ocr_models() {
        if model.id.to_lowercase() == normalized_id {
            return Some(model.category);
        }
    }

    // Search transcription models
    for model in get_curated_transcription_models() {
        if model.id.to_lowercase() == normalized_id {
            return Some(model.category);
        }
    }

    None
}

#[cfg(test)]
#[path = "curated_tests.rs"]
mod tests;
