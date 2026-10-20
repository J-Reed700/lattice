//! # Passage index
//!
//! An index of passages on disk, kept apart from `lattice.db`: one directory
//! per index, holding `passages.db` (documents, passages and an FTS5 index
//! over them) and `vectors-<identity>.usearch`, keyed by the identity of the
//! embedding model that made it. Backups copy `lattice.db` whole and would
//! carry every index; a directory of its own cannot leak into library search,
//! and deleting an index is removing its directory.
//!
//! A feature owns what its index means (a folder's files, a course's
//! sources), what its documents are keyed by and which chunker cuts them.
//! Everything else is here, shared: the store, the chunkers, the embedding
//! loop, the vectors file and its identity rule, the directories in use, the
//! registry of directories, the job configuration and the full-text half of
//! the search. Searching is not: a feature builds the one hybrid search over
//! [`vectors::Vectors::port`] and a [`text_search::PassageTextSearch`]
//! through the core `HybridSearchFactory`, so every index is ranked by the
//! library's orchestrator.
//!
//! ## Public surface
//!
//! - `store` — `PassageStore`: documents, passages, full text, vocabulary
//! - `registry` — every directory under a base, inspected without opening
//! - `vectors` — `Vectors`: the vectors file and the identity rule
//! - `chunking` — `NewPassage`; `lines` for files, `spans` for prose
//! - `build` — `embed_pending`, `save`, `Embedding`, `Progress`
//! - `live` — `LiveIndexes`, `LiveIndex`, `Engine`, `EmbedderSource`
//! - `text_search` — `PassageTextSearch`, `match_expression`, `WordMatch`
//! - `job` — `build_job_config`, `ProgressSaver`
//! - `testing` (tests only) — `FakeEmbedder`, the stand-in model every
//!   feature's index tests share

pub mod build;
pub mod chunking;
pub mod job;
pub mod live;
pub mod registry;
pub mod store;
pub mod text_search;
pub mod vectors;

#[cfg(test)]
pub mod testing;
#[cfg(test)]
mod tests;

pub use build::{embed_pending, EmbedText, Embedded, Embedding, Progress};
pub use chunking::NewPassage;
pub use live::{EmbedderSource, Engine, LiveIndex, LiveIndexes};
pub use store::{Counts, Document, Passage, PassageStore};
pub use text_search::{indexed_term, PassageTextSearch, WordMatch};
pub use vectors::Vectors;
