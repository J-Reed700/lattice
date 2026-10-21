//! The vector space a model's vectors belong to, computable from its row and
//! header before the model loads.

use lattice_core::domain::value_objects::ArtifactIdentity;

use crate::features::embedding::prefixes::{prefixes_for, EmbeddingPrefixes, NONE, QWEN3_INSTRUCT};

/// The vector-space key of a model whose GGUF hashes to `file_identity`:
/// the file hash, plus `+prefix-<id>` when its family prefixes documents,
/// since that prefix is part of every stored vector's input. Search keys its
/// index on the same string before the model loads.
pub(crate) fn space_identity(
    file_identity: &ArtifactIdentity,
    prefixes: &EmbeddingPrefixes,
) -> String {
    if prefixes.document.is_empty() {
        file_identity.as_str().to_owned()
    } else {
        format!("{}+prefix-{}", file_identity.as_str(), prefixes.id)
    }
}

/// The prefixes a model is embedded with: its family's, when `model_ref`
/// (the catalog id, repo id or file name) names one, else those its
/// `architecture` implies: Qwen3's instruction for a `qwen3` GGUF, none for
/// anything else.
pub(crate) fn resolve_prefixes(model_ref: &str, architecture: &str) -> &'static EmbeddingPrefixes {
    prefixes_for(model_ref).unwrap_or(if architecture == "qwen3" {
        &QWEN3_INSTRUCT
    } else {
        &NONE
    })
}
