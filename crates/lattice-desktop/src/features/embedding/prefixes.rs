//! The instruction prefixes embedding families were trained with, in one table.
//!
//! Several families score noticeably worse without a fixed string in front of
//! the text, and each wants a different one: E5 wants `query: ` / `passage: `,
//! Nomic wants `search_query: ` / `search_document: `, BGE's English v1.5
//! models want a query instruction and no document one, and Qwen3-Embedding
//! wants its instruct template on queries only. EmbeddingGemma wants task and
//! title prompts on both sides, mxbai-embed-large and Snowflake arctic-embed
//! want a query instruction or prefix, and jina-embeddings-v5 wants `Query: ` /
//! `Document: ` (documents only for its non-retrieval tasks). BGE-M3 and plain
//! sentence encoders want nothing at all, and guessing one for them would
//! corrupt their vectors just as surely as omitting one corrupts E5's.
//!
//! The table is keyed by name because a name is all a model arriving through a
//! Hugging Face search brings with it. This is the **only** place that reads an
//! embedding model's name: the service resolves its policy once at load and
//! carries it, so no other code path has to guess from a string. It also holds
//! the fallback for a GGUF whose name claims no family, keyed by architecture,
//! so this file is the only place that knows the families.
//!
//! A document prefix goes into every stored vector, so a policy that has one
//! joins the service's `model_identity()` — switching policies then opens a new
//! vector generation instead of mixing two spaces in one index. A policy that
//! only prefixes queries leaves stored vectors untouched and so leaves the
//! identity untouched.
//!
//! The model prefix is the outermost one. The chunker's context prefix
//! ("[Document: … | Page | Section]") sits between it and the passage.

use std::borrow::Cow;

const SEARCH_INSTRUCTION: &str = "Represent this sentence for searching relevant passages: ";

/// What a model family wants in front of a query and in front of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingPrefixes {
    /// Short stable name, used in the vector identity when `document` is set.
    pub id: &'static str,
    pub query: &'static str,
    pub document: &'static str,
}

impl EmbeddingPrefixes {
    /// The string the model actually sees for a query.
    pub fn query_input<'a>(&self, text: &'a str) -> Cow<'a, str> {
        prepend(self.query, text)
    }

    /// The string the model actually sees for a passage.
    pub fn document_input<'a>(&self, text: &'a str) -> Cow<'a, str> {
        prepend(self.document, text)
    }
}

fn prepend<'a>(prefix: &str, text: &'a str) -> Cow<'a, str> {
    if prefix.is_empty() {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(format!("{prefix}{text}"))
    }
}

/// No prefix on either side. BGE-M3, plain sentence encoders, and anything unrecognized:
/// silence is not an instruction, and inventing one changes the vectors.
pub const NONE: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "none",
    query: "",
    document: "",
};

/// Qwen3-Embedding's instruct template. Queries only — the model card passes
/// documents through verbatim.
pub const QWEN3_INSTRUCT: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "qwen3-instruct",
    query: "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:",
    document: "",
};

/// E5 (`intfloat/e5-*`, `multilingual-e5-*`). Both sides, and the trailing
/// space is part of the trained string.
pub const E5: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "e5",
    query: "query: ",
    document: "passage: ",
};

/// Nomic Embed. Both sides. Nomic also documents `clustering: ` and
/// `classification: `; retrieval is the only task this runtime performs.
pub const NOMIC: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "nomic",
    query: "search_query: ",
    document: "search_document: ",
};

/// BGE English v1.5. A query instruction and no document prefix; BAAI's card
/// says the document side is deliberately bare.
pub const BGE_ENGLISH: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "bge-en",
    query: SEARCH_INSTRUCTION,
    document: "",
};

/// EmbeddingGemma 1 and 2 (`google/embeddinggemma-*` and their GGUFs). Both
/// sides, from the model's `config_sentence_transformers.json`.
pub const EMBEDDINGGEMMA: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "embeddinggemma",
    query: "task: search result | query: ",
    document: "title: none | text: ",
};

/// mxbai-embed-large (`mixedbread-ai/mxbai-embed-large-*` and its GGUFs). A
/// query instruction and a bare document, from its `config_sentence_transformers.json`.
pub const MXBAI: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "mxbai",
    query: SEARCH_INSTRUCTION,
    document: "",
};

/// Snowflake arctic-embed v1 and v1.5. A query instruction and a bare document,
/// from each model's `config_sentence_transformers.json`.
pub const ARCTIC_V1: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "arctic",
    query: SEARCH_INSTRUCTION,
    document: "",
};

/// Snowflake arctic-embed v2.0. A `query: ` prefix and a bare document, from
/// the model's `config_sentence_transformers.json`.
pub const ARCTIC_V2: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "arctic-v2",
    query: "query: ",
    document: "",
};

/// jina-embeddings-v5 retrieval models. Both sides, from the model's
/// `config_sentence_transformers.json`.
pub const JINA_V5_RETRIEVAL: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "jina-v5-retrieval",
    query: "Query: ",
    document: "Document: ",
};

/// jina-embeddings-v5's text-matching, classification and clustering models,
/// which prompt documents only, per the model's `config_sentence_transformers.json`.
pub const JINA_V5: EmbeddingPrefixes = EmbeddingPrefixes {
    id: "jina-v5",
    query: "",
    document: "Document: ",
};

/// The prefixes `model_ref` was trained with, or `None` when no rule claims it.
///
/// `model_ref` is whatever names the model locally: a curated catalog id, a
/// Hugging Face repo id, a `config.json::_name_or_path`, or the directory the
/// weights were downloaded into. Only the last path segment is matched, so a
/// repo id and a local path for the same model resolve the same way.
///
/// `None` rather than [`NONE`] so the caller can fall back to
/// [`prefixes_for_architecture`] before settling for no prefix at all.
pub fn prefixes_for(model_ref: &str) -> Option<&'static EmbeddingPrefixes> {
    let lowered = model_ref.trim().to_ascii_lowercase();
    let name = lowered.rsplit('/').next().unwrap_or(lowered.as_str());
    let name = name.rsplit('\\').next().unwrap_or(name);

    if name.contains("jina-embeddings-v5") {
        return Some(if name.contains("retrieval") {
            &JINA_V5_RETRIEVAL
        } else {
            &JINA_V5
        });
    }
    if name.contains("embeddinggemma") {
        return Some(&EMBEDDINGGEMMA);
    }
    if name.contains("mxbai-embed") {
        return Some(&MXBAI);
    }
    if name.contains("arctic-embed") {
        return Some(if name.contains("-v2") {
            &ARCTIC_V2
        } else {
            &ARCTIC_V1
        });
    }

    if name.contains("qwen3-embedding") || name.contains("qwen3_embedding") {
        return Some(&QWEN3_INSTRUCT);
    }
    if name.starts_with("e5-") || name.contains("multilingual-e5") {
        return Some(&E5);
    }
    if name.contains("nomic-embed") {
        return Some(&NOMIC);
    }
    // BGE-M3 is multilingual and instruction-free; only the English v1.5 line
    // carries a query instruction, and the Chinese line's is a different string
    // this table does not claim to know.
    if name.contains("bge-m3") {
        return Some(&NONE);
    }
    if name.starts_with("bge-") && name.contains("-en") {
        return Some(&BGE_ENGLISH);
    }
    None
}

/// What a GGUF's `general.architecture` implies when its name claims no family:
/// Qwen3's instruction for `qwen3`, the embeddinggemma prompts for
/// `gemma-embedding` and `gemma-embedding2`, and no prefix for anything else.
pub fn prefixes_for_architecture(architecture: &str) -> &'static EmbeddingPrefixes {
    match architecture {
        "qwen3" => &QWEN3_INSTRUCT,
        "gemma-embedding" | "gemma-embedding2" => &EMBEDDINGGEMMA,
        _ => &NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_ids_local_paths_and_catalog_ids_resolve_alike() {
        for name in [
            "Qwen/Qwen3-Embedding-0.6B",
            "qwen3-embedding-0.6b",
            "/home/me/.cache/lattice/models/qwen3-embedding-0.6b",
        ] {
            assert_eq!(prefixes_for(name), Some(&QWEN3_INSTRUCT), "{name}");
        }
        assert_eq!(prefixes_for("intfloat/e5-base-v2"), Some(&E5));
        assert_eq!(prefixes_for("intfloat/multilingual-e5-large"), Some(&E5));
        assert_eq!(prefixes_for("nomic-ai/nomic-embed-text-v1.5"), Some(&NOMIC));
        assert_eq!(prefixes_for("BAAI/bge-base-en-v1.5"), Some(&BGE_ENGLISH));
    }

    #[test]
    fn families_trained_without_an_instruction_get_none() {
        assert_eq!(prefixes_for("BAAI/bge-m3"), Some(&NONE));
        // Not in the table at all: the caller decides, and its fallback is NONE.
        assert_eq!(prefixes_for("acme/custom-embedder"), None);
        assert_eq!(prefixes_for("custom-embedder-f16.gguf"), None);
        assert_eq!(prefixes_for(""), None);
    }

    fn trained(name: &str) -> Option<(&'static str, &'static str)> {
        prefixes_for(name).map(|p| (p.query, p.document))
    }

    #[test]
    fn embeddinggemma_gets_its_task_and_title_prompts() {
        for name in [
            "google/embeddinggemma-300m",
            "ggml-org/embeddinggemma-300M-GGUF",
            "unsloth/embeddinggemma-2-GGUF",
            "embeddinggemma-300M-Q8_0.gguf",
        ] {
            assert_eq!(
                trained(name),
                Some(("task: search result | query: ", "title: none | text: ")),
                "{name}"
            );
        }
    }

    #[test]
    fn mxbai_embed_large_gets_a_query_instruction_and_a_bare_document() {
        for name in [
            "mixedbread-ai/mxbai-embed-large-v1",
            "ChristianAzinn/mxbai-embed-large-v1-gguf",
        ] {
            assert_eq!(
                trained(name),
                Some((
                    "Represent this sentence for searching relevant passages: ",
                    ""
                )),
                "{name}"
            );
        }
    }

    #[test]
    fn snowflake_arctic_embed_v1_and_v1_5_get_the_bge_style_query_instruction() {
        for name in [
            "Snowflake/snowflake-arctic-embed-m-v1.5",
            "Snowflake/snowflake-arctic-embed-m",
            "snowflake-arctic-embed-l-q8_0.gguf",
        ] {
            assert_eq!(
                trained(name),
                Some((
                    "Represent this sentence for searching relevant passages: ",
                    ""
                )),
                "{name}"
            );
        }
    }

    #[test]
    fn snowflake_arctic_embed_v2_gets_a_plain_query_prefix_and_a_bare_document() {
        for name in [
            "Snowflake/snowflake-arctic-embed-l-v2.0",
            "snowflake-arctic-embed-m-v2.0-q8_0.gguf",
        ] {
            assert_eq!(trained(name), Some(("query: ", "")), "{name}");
        }
    }

    #[test]
    fn jina_v5_retrieval_models_get_query_and_document_prefixes() {
        for name in [
            "jinaai/jina-embeddings-v5-text-small-retrieval-GGUF",
            "jinaai/jina-embeddings-v5-text-nano-retrieval-GGUF",
            "jinaai/jina-embeddings-v5-omni-small-retrieval-GGUF",
        ] {
            assert_eq!(trained(name), Some(("Query: ", "Document: ")), "{name}");
        }
    }

    #[test]
    fn jina_v5_models_for_other_tasks_prefix_documents_only() {
        for name in [
            "jinaai/jina-embeddings-v5-text-small-text-matching-GGUF",
            "jinaai/jina-embeddings-v5-text-small-classification-GGUF",
            "jinaai/jina-embeddings-v5-text-nano-clustering-GGUF",
        ] {
            assert_eq!(trained(name), Some(("", "Document: ")), "{name}");
        }
    }

    #[test]
    fn families_with_different_prompts_never_share_an_id() {
        let names = [
            "google/embeddinggemma-300m",
            "mixedbread-ai/mxbai-embed-large-v1",
            "Snowflake/snowflake-arctic-embed-m-v1.5",
            "Snowflake/snowflake-arctic-embed-l-v2.0",
            "jinaai/jina-embeddings-v5-text-small-retrieval-GGUF",
            "jinaai/jina-embeddings-v5-text-small-text-matching-GGUF",
            "Qwen/Qwen3-Embedding-0.6B",
            "intfloat/e5-base-v2",
            "nomic-ai/nomic-embed-text-v1.5",
            "BAAI/bge-base-en-v1.5",
            "BAAI/bge-m3",
        ];
        let found: Vec<(&str, &EmbeddingPrefixes)> = names
            .iter()
            .map(|n| {
                (
                    *n,
                    prefixes_for(n).unwrap_or_else(|| panic!("{n} has no entry")),
                )
            })
            .collect();
        for (a_name, a) in &found {
            if !a.document.is_empty() {
                assert_ne!(
                    a.id, "none",
                    "{a_name} prefixes documents but is named none"
                );
            }
            for (b_name, b) in &found {
                if (a.query, a.document) != (b.query, b.document) {
                    assert_ne!(a.id, b.id, "{a_name} and {b_name} share an id");
                }
            }
        }
    }

    #[test]
    fn a_prefix_is_prepended_verbatim_and_an_empty_one_borrows() {
        assert_eq!(
            E5.query_input("capital of france"),
            "query: capital of france"
        );
        assert_eq!(E5.document_input("Paris is"), "passage: Paris is");
        assert!(matches!(NONE.document_input("Paris is"), Cow::Borrowed(_)));
        assert!(matches!(
            QWEN3_INSTRUCT.document_input("Paris is"),
            Cow::Borrowed(_)
        ));
        assert!(QWEN3_INSTRUCT
            .query_input("capital of france")
            .ends_with("\nQuery:capital of france"));
    }
}
