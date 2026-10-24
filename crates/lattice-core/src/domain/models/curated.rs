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
//! use lattice_core::domain::models::curated::get_curated_llm_models;
//!
//! let llms = get_curated_llm_models();
//! for model in llms {
//!     println!("{}: {} GB", model.name, model.size_gb);
//! }
//! ```

use crate::domain::models::selection::{ModelCategory, ModelMetadata};

mod chat;
mod embedding;
mod ocr;
mod transcription;

pub use chat::{
    chat_model_file_supports_tool_calling, chat_model_uses_mtp_draft, chat_model_vision,
    curated_chat_model_for_file, curated_chat_models_matching, get_curated_llm_models,
    recommend_chat_model_for_ram, ChatModelVision, CuratedChatModel, CuratedVision,
    CHAT_PROJECTOR_FILE, TOOL_CALLING_CAPABILITY,
};
pub use embedding::{embedding_model_supports_matryoshka, get_curated_embedding_models};
pub use ocr::{
    get_curated_ocr_models, CuratedOcrModel, CURATED_OCR_MODELS, OCR_MODEL_FILE, OCR_PROJECTOR_FILE,
};
pub use transcription::get_curated_transcription_models;

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
/// let category = get_model_category_by_id("qwen3-embedding-0.6b");
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
