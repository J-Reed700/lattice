//! The vector space a model's vectors belong to, computable from its row and
//! header before the model loads.

use lattice_core::domain::value_objects::ArtifactIdentity;

use crate::features::embedding::prefixes::{
    prefixes_for, prefixes_for_architecture, EmbeddingPrefixes,
};

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
/// `architecture` implies. A name always wins over the architecture.
pub(crate) fn resolve_prefixes(model_ref: &str, architecture: &str) -> &'static EmbeddingPrefixes {
    prefixes_for(model_ref).unwrap_or_else(|| prefixes_for_architecture(architecture))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(name: &str, architecture: &str) -> (&'static str, &'static str) {
        let p = resolve_prefixes(name, architecture);
        (p.query, p.document)
    }

    #[test]
    fn an_embeddinggemma_architecture_gets_its_prompts_when_the_name_says_nothing() {
        for architecture in ["gemma-embedding", "gemma-embedding2"] {
            assert_eq!(
                pair("my-embedder.gguf", architecture),
                ("task: search result | query: ", "title: none | text: "),
                "{architecture}"
            );
        }
    }

    #[test]
    fn a_name_that_claims_a_family_beats_a_qwen3_architecture() {
        assert_eq!(
            pair(
                "jinaai/jina-embeddings-v5-text-small-retrieval-GGUF",
                "qwen3"
            ),
            ("Query: ", "Document: ")
        );
    }

    #[test]
    fn an_unnamed_qwen3_architecture_still_gets_the_qwen3_instruction() {
        let (query, document) = pair("my-embedder.gguf", "qwen3");
        assert!(query.ends_with("\nQuery:"), "{query}");
        assert_eq!(document, "");
    }

    #[test]
    fn an_unnamed_unknown_architecture_gets_no_prefix() {
        assert_eq!(pair("my-embedder.gguf", "bert"), ("", ""));
    }
}
