//! Document indexing pipeline including chunking, extraction, and embedding.

// Engine sub-modules (flattened from former `modules/` subdirectory).
pub mod chunker;
pub mod error;
pub mod extraction;
pub mod metadata_extractor;
pub mod progress;
pub mod state;
pub mod storage;

#[cfg(test)]
mod indexer_tests;

pub use chunker::{ChunkerConfig, SemanticChunker, TextChunk};
pub use error::{IndexingError, Result};
pub use extraction::{ContentExtractor, ExtractedContent};
pub use metadata_extractor::{extract_metadata, DocumentMetadata};
pub use progress::{IndexProgress, IndexStatus, ProgressTracker};
pub use state::IndexingState;
pub use storage::IndexStorage;

#[cfg(test)]
mod chunker_test;
