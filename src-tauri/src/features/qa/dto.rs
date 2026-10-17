//! # Q&A DTOs
//!
//! The model health check's response. The source citations answers carry
//! live in `lattice_core::application::contracts::sources`.

use lattice_core::domain::models::model_ref::ModelRef;
use serde::{Deserialize, Serialize};

/// Serialized health response from the QA command.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LLMHealthStatusDto {
    pub available: bool,
    /// The Settings default chat model the check ran on.
    pub model: ModelRef,
    /// The name its server reports for it.
    pub model_name: String,
    pub max_context_tokens: usize,
    pub backend: String,
}
