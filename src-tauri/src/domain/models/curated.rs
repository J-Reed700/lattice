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

/// Capability tag for a chat model whose GGUF chat template carries a native
/// tool-call format that llama.cpp parses under `--jinja`.
///
/// Set only where that is known: Qwen 3.5 and Llama 3.1 ship such templates.
/// TinyLlama, Phi-3/3.5, Mistral 7B v0.2 and Mixtral v0.1 have none, and
/// Llama 3.2 3B's small-model tool use is too unreliable to turn on by default.
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
    get_curated_llm_models().iter().any(|model| {
        model
            .default_filename
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(file_name))
            && model
                .capabilities
                .iter()
                .any(|capability| capability == TOOL_CALLING_CAPABILITY)
    })
}

/// Get curated list of LLM models for text generation and chat.
///
/// Returns models across size ranges:
/// - Small (< 2GB): TinyLlama, Phi-3 Mini
/// - Medium (2-8GB): Mistral 7B, Llama 3.1 8B, Qwen 3.5 4B and 9B
/// - Large (8-16GB): Mixtral 8x7B
///
/// All models are GGUF format with Q4_K_M quantization for optimal
/// size/quality tradeoff.
pub fn get_curated_llm_models() -> Vec<ModelMetadata> {
    vec![
        ModelMetadata {
            id: "tinyllama-1.1b-chat-v1.0-q4_k_m".into(),
            name: "TinyLlama 1.1B Chat".into(),
            category: ModelCategory::LLM,
            description: "Ultra-compact chat model for low-resource systems. Good for simple queries and testing.".into(),
            size_gb: 0.7,
            minimum_ram_gb: 2.0,
            recommended_ram_gb: 4.0,
            context_length: 2048,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q8_0".into()],
            capabilities: vec!["chat".into(), "simple-qa".into()],
            download_url: Some("https://huggingface.co/TheBloke/TinyLlama-1.1B-Chat-v1.0-GGUF".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("TheBloke/TinyLlama-1.1B-Chat-v1.0-GGUF".into()),
            default_filename: Some("tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
        ModelMetadata {
            id: "phi-3-mini-4k-instruct-q4_k_m".into(),
            name: "Phi-3 Mini (4K context)".into(),
            category: ModelCategory::LLM,
            description: "Microsoft's compact 3.8B parameter model. Excellent performance for its size, strong reasoning.".into(),
            size_gb: 2.3,
            minimum_ram_gb: 4.0,
            recommended_ram_gb: 8.0,
            context_length: 4096,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q6_K".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into()],
            download_url: Some("https://huggingface.co/microsoft/Phi-3-mini-4k-instruct-gguf".into()),
            license: "MIT".into(),
            requires_auth: false,
            model_id: Some("microsoft/Phi-3-mini-4k-instruct-gguf".into()),
            default_filename: Some("Phi-3-mini-4k-instruct-q4.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
        ModelMetadata {
            id: "phi-3.5-mini-instruct-q4_k_m".into(),
            name: "Phi-3.5 Mini Instruct".into(),
            category: ModelCategory::LLM,
            description: "Microsoft's improved 3.8B model with enhanced instruction following and reasoning capabilities.".into(),
            size_gb: 2.4,
            minimum_ram_gb: 4.0,
            recommended_ram_gb: 8.0,
            context_length: 128000,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q6_K".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "long-context".into()],
            download_url: Some("https://huggingface.co/bartowski/Phi-3.5-mini-instruct-GGUF".into()),
            license: "MIT".into(),
            requires_auth: false,
            model_id: Some("bartowski/Phi-3.5-mini-instruct-GGUF".into()),
            default_filename: Some("Phi-3.5-mini-instruct-Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },

        ModelMetadata {
            id: "mistral-7b-instruct-v0.2-q4_k_m".into(),
            name: "Mistral 7B Instruct v0.2".into(),
            category: ModelCategory::LLM,
            description: "Mistral AI's flagship 7B model. Excellent general-purpose performance, strong instruction following.".into(),
            size_gb: 4.4,
            minimum_ram_gb: 8.0,
            recommended_ram_gb: 12.0,
            context_length: 32768,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q6_K".into(), "Q8_0".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "long-context".into()],
            download_url: Some("https://huggingface.co/TheBloke/Mistral-7B-Instruct-v0.2-GGUF".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("TheBloke/Mistral-7B-Instruct-v0.2-GGUF".into()),
            default_filename: Some("mistral-7b-instruct-v0.2.Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
        ModelMetadata {
            id: "llama-3.2-3b-instruct-q4_k_m".into(),
            name: "Llama 3.2 3B Instruct".into(),
            category: ModelCategory::LLM,
            description: "Meta's compact 3B model. Quick on modest hardware, with multilingual support and a long context.".into(),
            size_gb: 1.9,
            minimum_ram_gb: 4.0,
            recommended_ram_gb: 8.0,
            context_length: 131072,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q6_K".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "multilingual".into()],
            download_url: Some("https://huggingface.co/bartowski/Llama-3.2-3B-Instruct-GGUF".into()),
            license: "Llama-3.2".into(),
            requires_auth: false,
            model_id: Some("bartowski/Llama-3.2-3B-Instruct-GGUF".into()),
            default_filename: Some("Llama-3.2-3B-Instruct-Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
        ModelMetadata {
            id: "qwen3.5-2b-q4_k_m".into(),
            name: "Qwen 3.5 2B".into(),
            category: ModelCategory::LLM,
            description: "The smallest Qwen 3.5. Answers from your library on machines with little memory.".into(),
            size_gb: 1.2,
            minimum_ram_gb: 4.0,
            recommended_ram_gb: 8.0,
            context_length: 262144,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q8_0".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "multilingual".into(), "long-context".into(), TOOL_CALLING_CAPABILITY.into()],
            download_url: Some("https://huggingface.co/unsloth/Qwen3.5-2B-GGUF".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("unsloth/Qwen3.5-2B-GGUF".into()),
            default_filename: Some("Qwen3.5-2B-Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
        ModelMetadata {
            id: "qwen3.5-4b-q4_k_m".into(),
            name: "Qwen 3.5 4B".into(),
            category: ModelCategory::LLM,
            description: "A compact Qwen 3.5 that follows instructions and cites sources well for its size.".into(),
            size_gb: 2.6,
            minimum_ram_gb: 8.0,
            recommended_ram_gb: 12.0,
            context_length: 262144,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q8_0".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "multilingual".into(), "long-context".into(), TOOL_CALLING_CAPABILITY.into()],
            download_url: Some("https://huggingface.co/unsloth/Qwen3.5-4B-GGUF".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("unsloth/Qwen3.5-4B-GGUF".into()),
            default_filename: Some("Qwen3.5-4B-Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
        ModelMetadata {
            id: "qwen3.5-9b-q4_k_m".into(),
            name: "Qwen 3.5 9B".into(),
            category: ModelCategory::LLM,
            description: "The recommended chat model where memory allows: strong reasoning, tool use and grounding in retrieved passages.".into(),
            size_gb: 5.3,
            minimum_ram_gb: 16.0,
            recommended_ram_gb: 24.0,
            context_length: 262144,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q8_0".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "multilingual".into(), "long-context".into(), TOOL_CALLING_CAPABILITY.into()],
            download_url: Some("https://huggingface.co/unsloth/Qwen3.5-9B-GGUF".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("unsloth/Qwen3.5-9B-GGUF".into()),
            default_filename: Some("Qwen3.5-9B-Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },

        ModelMetadata {
            id: "llama-3.1-8b-instruct-q4_k_m".into(),
            name: "Llama 3.1 8B Instruct".into(),
            category: ModelCategory::LLM,
            description: "Meta's 8B model with strong reasoning and code generation, and a long context.".into(),
            size_gb: 4.6,
            minimum_ram_gb: 8.0,
            recommended_ram_gb: 16.0,
            context_length: 131072,
            performance_tier: PerformanceTier::Accurate,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q6_K".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "long-context".into(), TOOL_CALLING_CAPABILITY.into()],
            download_url: Some("https://huggingface.co/bartowski/Meta-Llama-3.1-8B-Instruct-GGUF".into()),
            license: "Llama-3.1".into(),
            requires_auth: false,
            model_id: Some("bartowski/Meta-Llama-3.1-8B-Instruct-GGUF".into()),
            default_filename: Some("Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
        ModelMetadata {
            id: "mixtral-8x7b-instruct-v0.1-q4_k_m".into(),
            name: "Mixtral 8x7B Instruct".into(),
            category: ModelCategory::LLM,
            description: "Mistral's Mixture-of-Experts model. 47B total parameters, 13B active. Top-tier performance.".into(),
            size_gb: 26.0,
            minimum_ram_gb: 32.0,
            recommended_ram_gb: 48.0,
            context_length: 32768,
            performance_tier: PerformanceTier::Accurate,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "expert-knowledge".into()],
            download_url: Some("https://huggingface.co/TheBloke/Mixtral-8x7B-Instruct-v0.1-GGUF".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("TheBloke/Mixtral-8x7B-Instruct-v0.1-GGUF".into()),
            default_filename: Some("mixtral-8x7b-instruct-v0.1.Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },

        // The bundled `llama-server` accepts GGUF. Models that were
        // previously listed here in
        // safetensors format (Gemma 2 9B IT, Gemma 4 E4B IT
        // multimodal, Mistral 7B Instruct v0.3) are either available
        // as GGUF from third-party packagers or, in the case of
        // Gemma 4 E4B's multimodal variant, not yet GGUF-quantized.
        // When suitable GGUFs land we re-add the entries pointing at
        // the GGUF repo (TheBloke/, MaziyarPanahi/, bartowski/, etc.).
        // ModelFormat::Safetensors continues to exist for embedding
        // models (Qwen3 Embedding, all-MiniLM-L6-v2) — Candle loads those
        // natively from safetensors weights.
    ]
}

/// First-run chat-model tier. `effective_ram_gb` = system RAM + discrete
/// VRAM (or unified memory on Apple Silicon). One family, sized to the
/// machine: a model that does not fit swaps, and a first chat that swaps looks
/// broken.
///
/// - `< 8 GB`  → Qwen 3.5 2B Q4
/// - `8–16 GB` → Qwen 3.5 4B Q4
/// - `≥ 16 GB` → Qwen 3.5 9B Q4
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

/// Get curated list of OCR models for text extraction from images/PDFs.
///
/// Returns vision-language models:
/// - **Qwen 2.5 VL 7B**: Latest multimodal model, strong OCR
/// - **Florence-2**: Microsoft's vision foundation model
///
/// These models can extract and understand text from images/documents.
pub fn get_curated_ocr_models() -> Vec<ModelMetadata> {
    vec![
        ModelMetadata {
            id: "qwen2.5-vl-7b-instruct-q4_k_m".into(),
            name: "Qwen 2.5 VL 7B Instruct".into(),
            category: ModelCategory::OCR,
            description: "Alibaba's vision-language model. Strong OCR, document understanding, and image analysis.".into(),
            size_gb: 4.8,
            minimum_ram_gb: 8.0,
            recommended_ram_gb: 12.0,
            context_length: 32768,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into()],
            capabilities: vec!["ocr".into(), "vision".into(), "document-understanding".into(), "image-analysis".into()],
            download_url: Some("https://huggingface.co/Qwen/Qwen2.5-VL-7B-Instruct-GGUF".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: None,
            default_filename: None,
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
        ModelMetadata {
            id: "florence-2-large".into(),
            name: "Florence-2 Large".into(),
            category: ModelCategory::OCR,
            description: "Microsoft's vision foundation model. Excellent OCR, object detection, and image captioning.".into(),
            size_gb: 1.5,
            minimum_ram_gb: 4.0,
            recommended_ram_gb: 8.0,
            context_length: 1024,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["F16".into()],
            capabilities: vec!["ocr".into(), "vision".into(), "object-detection".into(), "captioning".into()],
            download_url: Some("https://huggingface.co/microsoft/Florence-2-large".into()),
            license: "MIT".into(),
            requires_auth: false,
            model_id: None,
            default_filename: None,
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
    ]
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
