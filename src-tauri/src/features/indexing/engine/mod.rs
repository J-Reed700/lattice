//! # Indexing Module
//!
//! Document indexing pipeline with intelligent chunking, content extraction,
//! and embedding generation for semantic search.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐
//! │   File      │
//! └──────┬──────┘
//!        │
//!        ├─> Metadata Extraction
//!        │   (file type, size, dates)
//!        │
//!        ├─> Content Extraction
//!        │   ├─> PDF (lopdf)
//!        │   ├─> DOCX (docx-rs)
//!        │   ├─> TXT (raw text)
//!        │   └─> Code (syntax-aware)
//!        │
//!        ├─> Semantic Chunking
//!        │   ├─> Sentence boundaries
//!        │   ├─> Token limits (512)
//!        │   └─> Context preservation
//!        │
//!        ├─> Embedding Generation
//!        │   (ONNX model inference)
//!        │
//!        └─> Storage
//!            ├─> SQLite (metadata + chunks)
//!            └─> HNSW (vector index)
//! ```
//!
//! ## Components
//!
//! ### Chunker
//! - **Semantic Chunking**: Preserves meaning across chunk boundaries
//! - **Contextual Retrieval**: Adds document context to each chunk
//! - **Adaptive Sizing**: Splits based on token count, not char count
//!
//! ### Content Extraction
//! - **Multi-Format Support**: PDF, DOCX, TXT, MD, code files
//! - **Encoding Detection**: Automatic charset detection
//! - **Error Handling**: Graceful degradation for corrupted files
//!
//! ## Usage Examples
//!
//! ### Extract and Chunk a File
//!
//! ```rust,no_run
//! # use lattice::features::indexing::engine::{ContentExtractor, SemanticChunker};
//! # async fn example(
//! #     extractor: &ContentExtractor,
//! #     chunker: &SemanticChunker,
//! # ) -> lattice::features::indexing::engine::Result<()> {
//! let extracted = extractor
//!     .extract_from_file(std::path::Path::new("/path/to/document.pdf"))
//!     .await?;
//! let chunks = chunker.chunk_text(&extracted.text)?;
//! println!("{} chunks", chunks.len());
//! # Ok(())
//! # }
//! ```
//!
//! ### Monitor Progress
//!
//! ```rust,no_run
//! # use lattice::features::indexing::engine::IndexingState;
//! # async fn example(state: &IndexingState) {
//! let mut updates = state.subscribe();
//! while let Ok(progress) = updates.recv().await {
//!     println!("{}/{}", progress.processed, progress.total_files);
//! }
//! # }
//! ```
//!
//! ## Chunking Strategy
//!
//! The semantic chunker uses a context-aware approach:
//!
//! 1. **Sentence Segmentation**: Splits on sentence boundaries
//! 2. **Token Counting**: Ensures chunks fit model's max length (512 tokens)
//! 3. **Context Preservation**: Adds document title and metadata to each chunk
//! 4. **Overlap**: 10% overlap between chunks for continuity
//!
//! Example:
//!
//! ```text
//! Document: "Understanding Rust Memory Management"
//!
//! Chunk 1: [CONTEXT] Understanding Rust Memory Management
//!          Rust uses a unique ownership system...
//!
//! Chunk 2: [CONTEXT] Understanding Rust Memory Management
//!          The borrow checker ensures memory safety...
//! ```
//!
//! ## Performance Characteristics
//!
//! | File Type | Extraction Speed | Embedding Speed | Total   |
//! |-----------|-----------------|-----------------|---------|
//! | TXT       | ~1 MB/s         | ~500 chunks/s   | ~2 s/MB |
//! | PDF       | ~500 KB/s       | ~500 chunks/s   | ~5 s/MB |
//! | DOCX      | ~700 KB/s       | ~500 chunks/s   | ~4 s/MB |
//!
//! Bottleneck: Embedding generation (ONNX inference)
//!
//! ## Configuration
//!
//! ```rust
//! use lattice::indexing::ChunkerConfig;
//!
//! let config = ChunkerConfig {
//!     max_tokens: 512,           // Model max length
//!     overlap_tokens: 50,        // Chunk overlap
//!     min_chunk_tokens: 20,      // Discard tiny chunks
//!     preserve_sentences: true,  // Don't split mid-sentence
//! };
//! ```
//!
//! ## Error Handling
//!
//! The indexing pipeline uses graceful degradation:
//!
//! - **Content Extraction Fails**: Falls back to raw text
//! - **Embedding Fails**: Retries up to 3 times with backoff
//! - **Storage Fails**: Rolls back the transaction
//! - **File Too Large**: Skips with warning
//!
//! ## Memory Management
//!
//! - **Streaming Processing**: Files processed in chunks, not loaded entirely
//! - **ONNX Model**: Loaded once, shared across all tasks
//!
//! ## Supported File Types
//!
//! | Extension | Extractor | Notes |
//! |-----------|-----------|-------|
//! | .txt, .md | Raw Text  | UTF-8 detection |
//! | .pdf      | lopdf     | Text + metadata |
//! | .docx     | docx-rs   | Formatted text |
//! | .rs, .py  | Code      | Syntax-aware |
//! | .json     | JSON      | Pretty-printed |
//!
//! ## Performance Tips
//!
//! 1. **Batch Processing**: Index directories, not individual files
//! 2. **Filter Unsupported**: Skip non-text files before queuing
//! 3. **Monitor Progress**: Watch `IndexingState` to avoid overwhelming the system
//! 4. **Tune Chunk Size**: Balance between context and precision

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

pub use chunker::{ChunkerConfig, ContextualizedChunk, SemanticChunker, TextChunk};
pub use error::{IndexingError, Result};
pub use extraction::{ContentExtractor, ExtractedContent};
pub use metadata_extractor::{extract_metadata, DocumentMetadata};
pub use progress::{IndexProgress, IndexStatus, ProgressTracker};
pub use state::IndexingState;
pub use storage::IndexStorage;

#[cfg(test)]
mod chunker_test;
