//! # Q&A DTOs
//!
//! The model health check's response. The source citations answers carry
//! live in `lattice_core::application::contracts::sources`.

use serde::{Deserialize, Serialize};

/// Serialized health response from the QA command.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LLMHealthStatusDto {
    pub available: bool,
    pub model: String,
    pub max_context_tokens: usize,
    pub backend: String,
}
