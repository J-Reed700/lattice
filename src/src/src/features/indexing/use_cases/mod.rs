//! # Indexing Use Cases
//!
//! Use cases for indexing documents into the knowledge base.
//!
//! This module provides document indexing operations:
//! - **Index File**: Index a single file
//! - **Index Directory**: Batch index all files in a directory
//! - **Reindex Document**: Update an existing document's index
//! - **Delete Document**: Delete a document and all its associated data
//! - **Rename Document**: Rename a document's display name

pub mod delete_document;
pub mod index_directory;
pub mod index_file;
pub mod reindex_document;
pub mod rename_document;

pub use delete_document::DeleteDocumentUseCase;
pub use index_directory::IndexDirectoryUseCase;
pub use index_file::IndexFileUseCase;
pub use reindex_document::ReindexDocumentUseCase;
pub use rename_document::RenameDocumentUseCase;

pub(crate) mod embedding_input;
