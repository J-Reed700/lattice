//! The instruction prefixes embedding families were trained with, in one table.
//!
//! Several families score noticeably worse without a fixed string in front of
//! the text, and each wants a different one: E5 wants `query: ` / `passage: `,
//! Nomic wants `search_query: ` / `search_document: `, BGE's English v1.5
//! models want a query instruction and no document one, and Qwen3-Embedding
//! wants its instruct template on queries only. BGE-M3 and all-MiniLM want
//! nothing at all, and guessing one for them would corrupt their vectors just
//! as surely as omitting one corrupts E5's.
//!
//! The table is keyed by name because a name is all a model arriving through a
//! Hugging Face search brings with it. This is the **only** place that reads an
//! embedding model's name: the service resolves its policy once at load and
//! carries it, so no other code path has to guess from a string.
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

/// No prefix on either side. all-MiniLM, BGE-M3, and anything unrecognized:
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
    query: "Represent this sentence for searching relevant passages: ",
    document: "",
};

/// The prefixes `model_ref` was trained with, or `None` when no rule claims it.
///
/// `model_ref` is whatever names the model locally: a curated catalog id, a
/// Hugging Face repo id, a `config.json::_name_or_path`, or the directory the
/// weights were downloaded into. Only the last path segment is matched, so a
/// repo id and a local path for the same model resolve the same way.
///
/// `None` rather than [`NONE`] so the caller can fall back to what it knows
/// about the architecture before settling for no prefix at all.
pub fn prefixes_for(model_ref: &str) -> Option<&'static EmbeddingPrefixes> {
    let lowered = model_ref.trim().to_ascii_lowercase();
    let name = lowered.rsplit('/').next().unwrap_or(lowered.as_str());
    let name = name.rsplit('\\').next().unwrap_or(name);

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
        assert_eq!(prefixes_for("sentence-transformers/all-MiniLM-L6-v2"), None);
        assert_eq!(prefixes_for("nreimers/MiniLM-L6-H384-uncased"), None);
        assert_eq!(prefixes_for(""), None);
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
