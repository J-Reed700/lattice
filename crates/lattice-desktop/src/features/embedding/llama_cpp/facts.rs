//! What a GGUF header says about an embedding model, and what it must say
//! for llama-server to embed with it.

use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::error::{AppError, Result};

use crate::features::embedding::input_policy::MAX_PASSAGE_TOKENS;

/// The header keys an embedding server depends on, as the composition root
/// reads them from the GGUF. Each is `None` when the header does not say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmbeddingHeader {
    /// `general.architecture`, e.g. `qwen3`, `bert`.
    pub architecture: Option<String>,
    /// `{architecture}.context_length`: the window it was trained for.
    pub trained_tokens: Option<u32>,
    /// `{architecture}.embedding_length`: the width of its vectors.
    pub dimension: Option<u32>,
    /// `{architecture}.pooling_type`, llama.cpp's `llama_pooling_type`.
    pub pooling_type: Option<u32>,
    /// `general.file_type`, llama.cpp's `llama_ftype` (1 is F16).
    pub file_type: Option<u32>,
}

/// How the server pools a sequence's token states into one vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pooling {
    Mean,
    Cls,
    Last,
}

impl Pooling {
    /// The pooling `llama_pooling_type` names, or why a model with it cannot
    /// serve `/v1/embeddings`.
    fn from_header(value: Option<u32>) -> std::result::Result<Self, &'static str> {
        match value {
            Some(1) => Ok(Self::Mean),
            Some(2) => Ok(Self::Cls),
            Some(3) => Ok(Self::Last),
            None => Err("its header names no pooling type"),
            Some(0) => Err("its header says it does not pool (pooling type none)"),
            Some(4) => Err("it pools for ranking, which scores pairs rather than embedding text"),
            Some(_) => Err("its header names a pooling type this llama-server does not know"),
        }
    }
}

/// An embedding model as its GGUF header and recorded identity describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingModelFacts {
    /// The SHA-256 of the GGUF file.
    pub identity: ArtifactIdentity,
    /// The width of its vectors (`n_embd`).
    pub dimension: usize,
    pub pooling: Pooling,
    /// The window it was trained for (`n_ctx_train`).
    pub trained_tokens: u32,
    pub architecture: String,
    /// The name the user knows it by.
    pub display_name: String,
}

impl EmbeddingModelFacts {
    /// The facts of a model whose header is `header`, or why llama-server
    /// cannot embed with it. `/v1/embeddings` needs pooled output, so a
    /// header without a pooling type, or with `none`, is refused.
    pub fn from_header(
        header: EmbeddingHeader,
        identity: ArtifactIdentity,
        display_name: impl Into<String>,
    ) -> Result<Self> {
        let display_name = display_name.into();
        let refuse = |reason: &str| {
            AppError::ModelLoadFailed(format!("{display_name} cannot serve embeddings: {reason}."))
        };
        let pooling = Pooling::from_header(header.pooling_type).map_err(refuse)?;
        let dimension = header
            .dimension
            .filter(|&dimension| dimension > 0)
            .ok_or_else(|| refuse("its header does not say how wide its vectors are"))?;
        let trained_tokens = header
            .trained_tokens
            .filter(|&tokens| tokens > 0)
            .ok_or_else(|| refuse("its header does not say what window it was trained for"))?;
        let architecture = header
            .architecture
            .ok_or_else(|| refuse("its header names no architecture"))?;
        Ok(Self {
            identity,
            dimension: dimension as usize,
            pooling,
            trained_tokens,
            architecture,
            display_name,
        })
    }

    /// The window each server slot gets: the trained window, or the longest
    /// passage the splitter produces when that is shorter.
    pub fn window_tokens(&self) -> u32 {
        self.trained_tokens
            .min(u32::try_from(MAX_PASSAGE_TOKENS).unwrap_or(u32::MAX))
    }
}
