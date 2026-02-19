//! # Curated Model Catalog
//!
//! Hardcoded catalog of recommended AI models across categories:
//! - LLMs: GGUF models optimized for local inference
//! - Embedding models: Semantic search optimized
//! - OCR models: Vision-language models for text extraction
//!
//! ## Design
//!
//! This module provides a curated list of models that work well with Recall.
//! Models are selected based on:
//! - Size/performance tradeoff
//! - License compatibility
//! - Community adoption
//! - Quantization availability
//!
//! ## Example
//!
//! ```rust
//! use vault_desktop::domain::curated_models::get_curated_llm_models;
//!
//! let llms = get_curated_llm_models();
//! for model in llms {
//!     println!("{}: {} GB", model.name, model.size_gb);
//! }
//! ```

use super::embedding_constants::{
    DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME, DEFAULT_EMBEDDING_MODEL_NAME,
};
use super::model_management::{ModelCategory, ModelMetadata, PerformanceTier};

// ============================================================================
// LLM Models - GGUF Quantized for Local Inference
// ============================================================================

/// Get curated list of LLM models for text generation and chat.
///
/// Returns models across size ranges:
/// - Small (< 2GB): TinyLlama, Phi-3 Mini
/// - Medium (2-8GB): Mistral 7B, Llama 3.2 7B, Qwen 2.5 7B
/// - Large (8-16GB): Llama 3.1 13B, Mixtral 8x7B
///
/// All models are GGUF format with Q4_K_M quantization for optimal
/// size/quality tradeoff.
pub fn get_curated_llm_models() -> Vec<ModelMetadata> {
    vec![
        // === Small Models (< 2GB) ===
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
            download_url: Some("https://huggingface.co/microsoft/Phi-3.5-mini-instruct-gguf".into()),
            license: "MIT".into(),
            requires_auth: false,
            model_id: Some("microsoft/Phi-3.5-mini-instruct-gguf".into()),
            default_filename: Some("Phi-3.5-mini-instruct-Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
        },

        // === Medium Models (2-8GB) ===
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
        },
        ModelMetadata {
            id: "llama-3.2-7b-instruct-q4_k_m".into(),
            name: "Llama 3.2 7B Instruct".into(),
            category: ModelCategory::LLM,
            description: "Meta's latest 7B model with improved reasoning and coding. Strong multilingual support.".into(),
            size_gb: 4.7,
            minimum_ram_gb: 8.0,
            recommended_ram_gb: 12.0,
            context_length: 8192,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q6_K".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "multilingual".into()],
            download_url: Some("https://huggingface.co/meta-llama/Llama-3.2-7B-Instruct-gguf".into()),
            license: "Llama-3.2".into(),
            requires_auth: true,
            model_id: Some("meta-llama/Llama-3.2-7B-Instruct-gguf".into()),
            default_filename: Some("Llama-3.2-7B-Instruct-Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
        },
        ModelMetadata {
            id: "qwen2.5-7b-instruct-q4_k_m".into(),
            name: "Qwen 2.5 7B Instruct".into(),
            category: ModelCategory::LLM,
            description: "Alibaba's latest model with strong multilingual and coding capabilities. Excellent for Chinese language.".into(),
            size_gb: 4.5,
            minimum_ram_gb: 8.0,
            recommended_ram_gb: 12.0,
            context_length: 131072,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q6_K".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "multilingual".into(), "long-context".into()],
            download_url: Some("https://huggingface.co/Qwen/Qwen2.5-7B-Instruct-GGUF".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("Qwen/Qwen2.5-7B-Instruct-GGUF".into()),
            default_filename: Some("qwen2.5-7b-instruct-q4_k_m.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
        },

        // === Large Models (8-16GB) ===
        ModelMetadata {
            id: "llama-3.1-13b-instruct-q4_k_m".into(),
            name: "Llama 3.1 13B Instruct".into(),
            category: ModelCategory::LLM,
            description: "Meta's 13B model with enhanced reasoning. Best-in-class for size, excellent code generation.".into(),
            size_gb: 7.9,
            minimum_ram_gb: 16.0,
            recommended_ram_gb: 24.0,
            context_length: 131072,
            performance_tier: PerformanceTier::Accurate,
            supported_quantizations: vec!["Q4_K_M".into(), "Q5_K_M".into(), "Q6_K".into()],
            capabilities: vec!["chat".into(), "code".into(), "reasoning".into(), "long-context".into()],
            download_url: Some("https://huggingface.co/meta-llama/Llama-3.1-13B-Instruct-gguf".into()),
            license: "Llama-3.1".into(),
            requires_auth: true,
            model_id: Some("meta-llama/Llama-3.1-13B-Instruct-gguf".into()),
            default_filename: Some("Llama-3.1-13B-Instruct-Q4_K_M.gguf".into()),
            files: vec![],
            total_size_bytes: 0,
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
        },
    ]
}

// ============================================================================
// Embedding Models - Semantic Search Optimized
// ============================================================================

/// Get curated list of embedding models for semantic search.
///
/// Returns models optimized for:
/// - **BGE-M3**: Multilingual, hybrid dense/sparse retrieval
/// - **DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME**: Higher-quality, general-purpose embeddings
/// - **instructor-xl**: Instruction-aware embeddings
///
/// These models generate vector representations for semantic similarity.
pub fn get_curated_embedding_models() -> Vec<ModelMetadata> {
    vec![
        ModelMetadata {
            id: DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME.into(),
            name: DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME.into(),
            category: ModelCategory::Embedding,
            description: format!(
                "General-purpose embedding model ({} dimensions). Strong semantic retrieval quality.",
                DEFAULT_EMBEDDING_DIM
            ),
            size_gb: 0.5,
            minimum_ram_gb: 2.0,
            recommended_ram_gb: 4.0,
            context_length: 512,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["F16".into(), "F32".into()],
            capabilities: vec!["embedding".into(), "retrieval".into()],
            download_url: Some(format!(
                "https://huggingface.co/{}",
                DEFAULT_EMBEDDING_MODEL_NAME
            )),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some(DEFAULT_EMBEDDING_MODEL_NAME.into()),
            default_filename: None, // Multi-file model bundle (ONNX + tokenizer metadata)
            files: vec![
                super::model_metadata::ModelFileMetadata::new(
                    "model.onnx".to_string(),
                    "https://huggingface.co/Xenova/all-mpnet-base-v2/resolve/main/onnx/model.onnx".to_string(),
                    420_000_000, // Approximate size
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "tokenizer.json".to_string(),
                    "https://huggingface.co/Xenova/all-mpnet-base-v2/resolve/main/tokenizer.json".to_string(),
                    8_000_000, // Approximate size
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "config.json".to_string(),
                    "https://huggingface.co/Xenova/all-mpnet-base-v2/resolve/main/config.json".to_string(),
                    1_024, // Approximate size
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "special_tokens_map.json".to_string(),
                    "https://huggingface.co/Xenova/all-mpnet-base-v2/resolve/main/special_tokens_map.json".to_string(),
                    1_024, // Approximate size
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "tokenizer_config.json".to_string(),
                    "https://huggingface.co/Xenova/all-mpnet-base-v2/resolve/main/tokenizer_config.json".to_string(),
                    2_048, // Approximate size
                ),
            ],
            total_size_bytes: 428_004_096,
        },
        ModelMetadata {
            id: "bge-m3".into(),
            name: "BGE-M3".into(),
            category: ModelCategory::Embedding,
            description: "Multi-lingual embedding model with hybrid dense/sparse retrieval. State-of-the-art for search.".into(),
            size_gb: 2.3,
            minimum_ram_gb: 4.0,
            recommended_ram_gb: 8.0,
            context_length: 8192,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["F16".into(), "F32".into()],
            capabilities: vec!["embedding".into(), "retrieval".into(), "multilingual".into(), "hybrid-search".into()],
            download_url: Some("https://huggingface.co/BAAI/bge-m3".into()),
            license: "MIT".into(),
            requires_auth: false,
            model_id: Some("BAAI/bge-m3".into()),
            default_filename: None, // Multi-file model - no single default file
            files: vec![
                super::model_metadata::ModelFileMetadata::new(
                    "model.onnx".to_string(),
                    "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/model.onnx".to_string(),
                    725_000,
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "model.onnx_data".to_string(),
                    "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/model.onnx_data".to_string(),
                    2_270_000_000,
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "Constant_7_attr__value".to_string(),
                    "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/Constant_7_attr__value".to_string(),
                    65_600,
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "config.json".to_string(),
                    "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/config.json".to_string(),
                    698,
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "sentencepiece.bpe.model".to_string(),
                    "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/sentencepiece.bpe.model".to_string(),
                    5_070_000,
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "special_tokens_map.json".to_string(),
                    "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/special_tokens_map.json".to_string(),
                    964,
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "tokenizer.json".to_string(),
                    "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/tokenizer.json".to_string(),
                    17_100_000,
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "tokenizer_config.json".to_string(),
                    "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/tokenizer_config.json".to_string(),
                    1_170,
                ),
            ],
            // Total size: model.onnx (725KB) + model.onnx_data (2.27GB) + other files (22.8MB) ≈ 2.29GB
            total_size_bytes: 725_000_u64 + 2_270_000_000_u64 + 65_600_u64 + 698_u64 + 5_070_000_u64 + 964_u64 + 17_100_000_u64 + 1_170_u64,
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
                super::model_metadata::ModelFileMetadata::new(
                    "model.onnx".to_string(),
                    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx".to_string(),
                    90_000_000, // Approximate size
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "tokenizer.json".to_string(),
                    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json".to_string(),
                    700_000, // Approximate size
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "config.json".to_string(),
                    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/config.json".to_string(),
                    1_024, // Approximate size
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "special_tokens_map.json".to_string(),
                    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/special_tokens_map.json".to_string(),
                    1_024, // Approximate size
                ),
                super::model_metadata::ModelFileMetadata::new(
                    "tokenizer_config.json".to_string(),
                    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer_config.json".to_string(),
                    2_048, // Approximate size
                ),
            ],
            total_size_bytes: 90_704_096,
        },
        ModelMetadata {
            id: "instructor-xl".into(),
            name: "Instructor-XL".into(),
            category: ModelCategory::Embedding,
            description: format!(
                "Instruction-aware embeddings ({} dimensions). Best quality, customizable with instructions.",
                DEFAULT_EMBEDDING_DIM
            ),
            size_gb: 4.9,
            minimum_ram_gb: 8.0,
            recommended_ram_gb: 12.0,
            context_length: 512,
            performance_tier: PerformanceTier::Accurate,
            supported_quantizations: vec!["F16".into(), "F32".into()],
            capabilities: vec!["embedding".into(), "retrieval".into(), "instruction-aware".into()],
            download_url: Some("https://huggingface.co/hkunlp/instructor-xl".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("hkunlp/instructor-xl".into()),
            default_filename: Some("onnx/model.onnx".into()),
            files: vec![],
            total_size_bytes: 0,
        },
    ]
}

// ============================================================================
// OCR Models - Vision-Language Models
// ============================================================================

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
        },
    ]
}

/// Get all curated models across all categories.
///
/// Returns combined list of LLMs, embedding models, and OCR models.
pub fn get_all_curated_models() -> Vec<ModelMetadata> {
    let mut models = Vec::new();
    models.extend(get_curated_llm_models());
    models.extend(get_curated_embedding_models());
    models.extend(get_curated_ocr_models());
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
/// use crate::domain::curated_models::{get_model_category_by_id, ModelCategory};
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

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_models_count() {
        let models = get_curated_llm_models();
        assert_eq!(models.len(), 8);
    }

    #[test]
    fn test_embedding_models_count() {
        let models = get_curated_embedding_models();
        assert_eq!(models.len(), 4);
    }

    #[test]
    fn test_ocr_models_count() {
        let models = get_curated_ocr_models();
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn test_all_models_count() {
        let models = get_all_curated_models();
        assert_eq!(models.len(), 14);
    }

    #[test]
    fn test_get_by_category() {
        let llms = get_curated_models_by_category(ModelCategory::LLM);
        assert_eq!(llms.len(), 8);

        let embeddings = get_curated_models_by_category(ModelCategory::Embedding);
        assert_eq!(embeddings.len(), 4);

        let ocr = get_curated_models_by_category(ModelCategory::OCR);
        assert_eq!(ocr.len(), 2);
    }

    #[test]
    fn test_all_models_have_required_fields() {
        let models = get_all_curated_models();

        for model in models {
            assert!(!model.id.is_empty());
            assert!(!model.name.is_empty());
            assert!(!model.description.is_empty());
            assert!(model.size_gb > 0.0);
            assert!(model.minimum_ram_gb > 0.0);
            assert!(model.recommended_ram_gb >= model.minimum_ram_gb);
            assert!(model.context_length > 0);
            assert!(!model.capabilities.is_empty());
            assert!(!model.license.is_empty());
        }
    }

    #[test]
    fn test_llm_models_have_download_urls() {
        let models = get_curated_llm_models();

        for model in models {
            assert!(model.download_url.is_some());
            let url = model.download_url.unwrap();
            assert!(url.starts_with("https://"));
        }
    }

    #[test]
    fn test_model_categories_match() {
        let llms = get_curated_llm_models();
        for model in llms {
            assert_eq!(model.category, ModelCategory::LLM);
        }

        let embeddings = get_curated_embedding_models();
        for model in embeddings {
            assert_eq!(model.category, ModelCategory::Embedding);
        }

        let ocr = get_curated_ocr_models();
        for model in ocr {
            assert_eq!(model.category, ModelCategory::OCR);
        }
    }

    #[test]
    fn test_small_llm_models() {
        let models = get_curated_llm_models();
        let small_models: Vec<_> = models.iter().filter(|m| m.size_gb < 2.5).collect();

        assert!(!small_models.is_empty());
        for model in small_models {
            assert!(model.minimum_ram_gb <= 4.0);
        }
    }

    #[test]
    fn test_performance_tiers() {
        let models = get_all_curated_models();

        let has_fast = models
            .iter()
            .any(|m| matches!(m.performance_tier, PerformanceTier::Fast));
        let has_balanced = models
            .iter()
            .any(|m| matches!(m.performance_tier, PerformanceTier::Balanced));
        let has_accurate = models
            .iter()
            .any(|m| matches!(m.performance_tier, PerformanceTier::Accurate));

        assert!(has_fast);
        assert!(has_balanced);
        assert!(has_accurate);
    }

    #[test]
    fn test_phi3_mini_metadata() {
        let models = get_curated_llm_models();
        let phi3 = models.iter().find(|m| m.id.contains("phi-3-mini")).unwrap();

        assert_eq!(phi3.category, ModelCategory::LLM);
        assert!(phi3.size_gb < 3.0);
        assert_eq!(phi3.license, "MIT");
        assert!(phi3.capabilities.contains(&"chat".into()));
        assert!(phi3.capabilities.contains(&"code".into()));
    }

    #[test]
    fn test_bge_m3_metadata() {
        let models = get_curated_embedding_models();
        let bge = models.iter().find(|m| m.id == "bge-m3").unwrap();

        assert_eq!(bge.category, ModelCategory::Embedding);
        assert!(bge.capabilities.contains(&"multilingual".into()));
        assert!(bge.capabilities.contains(&"hybrid-search".into()));
    }

    #[test]
    fn test_lookup_embedding_model() {
        let category = get_model_category_by_id("all-minilm-l6-v2");
        assert_eq!(category, Some(ModelCategory::Embedding));
    }

    #[test]
    fn test_lookup_case_insensitive() {
        let category1 = get_model_category_by_id("ALL-MINILM-L6-V2");
        let category2 = get_model_category_by_id("all-minilm-l6-v2");
        assert_eq!(category1, category2);
        assert_eq!(category1, Some(ModelCategory::Embedding));
    }

    #[test]
    fn test_lookup_nonexistent_model() {
        let category = get_model_category_by_id("nonexistent-model-xyz-123");
        assert_eq!(category, None);
    }
}
