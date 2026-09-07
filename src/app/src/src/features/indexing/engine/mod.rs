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
//! ### IndexingService (Actor)
//! - **Concurrent Processing**: Tokio-based async actor
//! - **Queue Management**: Priority queue for indexing tasks
//! - **Progress Tracking**: Real-time progress events
//! - **Error Recovery**: Automatic retries with exponential backoff
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
//! ### Index a Single File
//!
//! ```rust,no_run
//! use lattice::indexing::{IndexingService, IndexTask};
//! use std::path::PathBuf;
//!
//! let service = IndexingService::new(
//!     pool,
//!     app_dir,
//!     embedder,
//!     tokenizer,
//!     1000, // queue capacity
//! );
//!
//! let task = IndexTask {
//!     file_path: PathBuf::from("/path/to/document.pdf"),
//!     priority: 1,
//! };
//!
//! service.queue_task(task).await?;
//! ```
//!
//! ### Index a Directory
//!
//! ```rust,no_run
//! use lattice::indexing::IndexingService;
//!
//! service.index_directory("/path/to/docs").await?;
//! // Recursively indexes all supported files
//! ```
//!
//! ### Monitor Progress
//!
//! ```rust,no_run
//! use lattice::indexing::{IndexProgress, IndexingEvent};
//!
//! let mut events = service.subscribe_events();
//!
//! while let Some(event) = events.recv().await {
//!     match event {
//!         IndexingEvent::Started { file_path } => {
//!             println!("Indexing: {:?}", file_path);
//!         }
//!         IndexingEvent::Progress { current, total } => {
//!             println!("Progress: {}/{}", current, total);
//!         }
//!         IndexingEvent::Completed { file_id } => {
//!             println!("Done: {}", file_id);
//!         }
//!         IndexingEvent::Failed { file_path, error } => {
//!             eprintln!("Error: {:?} - {}", file_path, error);
//!         }
//!     }
//! }
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
//! - **Storage Fails**: Rolls back transaction, re-queues task
//! - **File Too Large**: Skips with warning
//!
//! ## Memory Management
//!
//! - **Streaming Processing**: Files processed in chunks, not loaded entirely
//! - **Bounded Queue**: Prevents memory exhaustion (default: 1000 tasks)
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
//! 3. **Monitor Queue**: Use progress tracking to avoid overwhelming system
//! 4. **Tune Chunk Size**: Balance between context and precision

// Engine sub-modules (flattened from former `modules/` subdirectory).
pub mod actor;
pub mod builder;
pub mod chunker;
pub mod error;
pub mod error_ext;
pub mod events;
pub mod extraction;
pub mod metadata_extractor;
pub mod progress;
pub mod queue;
pub mod state;
pub mod storage;
// Kept at root due SQLx offline query metadata path sensitivity.
pub mod transaction;

#[cfg(test)]
mod indexer_tests;

pub use actor::{IndexingActor, IndexingService};
pub use builder::{
    IndexingServiceBuilder, Ready as IndexingReady, Uninitialized as IndexingUninitialized,
};
pub use chunker::{ChunkerConfig, ContextualizedChunk, SemanticChunker, TextChunk};
pub use error::{IndexingError, Result};
pub use events::IndexingEvent;
pub use extraction::{ContentExtractor, ExtractedContent};
pub use metadata_extractor::{extract_metadata, DocumentMetadata};
pub use progress::{IndexProgress, IndexStatus, ProgressTracker};
pub use queue::{IndexTask, IndexingQueue};
pub use state::IndexingState;
pub use storage::IndexStorage;
pub use transaction::FileIndexTransaction;

#[cfg(test)]
mod chunker_test;
