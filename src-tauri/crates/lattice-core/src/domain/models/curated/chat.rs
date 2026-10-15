//! Curated chat models: the catalog, what a model file supports, and what its context window costs in memory.

use crate::domain::models::metadata::ModelFileMetadata;
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
            files: vec![ModelFileMetadata::with_checksum(
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
pub(super) const CURATED_CHAT_MODELS: &[CuratedChatModel] = &[
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
    // A separate quantization, not the plain 9B file plus a block: 75 of the
    // 427 shared tensors are quantized differently. It adds one MTP block,
    // `blk.32.nextn` (`qwen35.nextn_predict_layers = 1`, 33 blocks), whose
    // full-attention cache is counted here. Unsloth's template for this build
    // differs from the plain 9B's: thinking is ON unless
    // `enable_thinking: false`, and up to two leading system or developer
    // messages are merged instead of raising.
    CuratedChatModel {
        id: "qwen3.5-9b-mtp-q4_k_m",
        name: "Qwen 3.5 9B (MTP)",
        description: "Qwen 3.5 9B packaged with its multi-token-prediction layers, so the server can draft several tokens per step. On Apple Silicon it decodes about 1.5x faster with a draft depth of 3 (1.8x while thinking), at the cost of a single server slot: requests take turns.",
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
