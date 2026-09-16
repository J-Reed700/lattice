//! Learned sparse retrieval — the third fusion branch.
//!
//! # What this branch is for
//!
//! Dense vectors match meaning and miss rare literal tokens; BM25 matches
//! literal tokens and misses paraphrase. A learned sparse head sits between
//! them: its axes are tokenizer term ids, like BM25, but the weights come from
//! the model, so "MPEP 706.07" can activate the terms a rejection-practice
//! passage activates even when the passage never repeats the phrase, and a
//! term repeated for formatting reasons can be weighted down instead of up.
//!
//! BM25 is *kept* alongside it. BM25's exactness on identifiers, code symbols
//! and quoted strings is a different property from what the sparse head
//! learned, and the sparse head can only activate terms that are in the
//! model's vocabulary at all.
//!
//! # Shape
//!
//! - [`store::SqliteSparseTermStore`] writes postings during indexing.
//! - [`service::SparseSearchService`] embeds the query with the same sparse
//!   head and scores stored postings by dot product, in SQL.
//!
//! Both sides key on `EmbeddingPort::model_identity()`. Term id 4211 means
//! different words in two tokenizers, so a model switch must never let the old
//! postings score: the identity filter makes a switched model simply find
//! nothing rather than find nonsense.

pub mod service;
pub mod store;

pub use service::SparseSearchService;
pub use store::SqliteSparseTermStore;
