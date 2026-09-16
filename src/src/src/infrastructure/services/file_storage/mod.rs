//! File storage service with content-addressed storage and deduplication.
//!
//! This module provides a robust file storage system with the following features:
//!
//! - **Content-addressed storage**: Files are stored using SHA256 hashes
//! - **Automatic deduplication**: Identical files share storage
//! - **Reference counting**: Files are deleted only when no longer referenced
//! - **Atomic operations**: File copies are atomic to prevent corruption
//! - **Integrity verification**: Hash verification ensures data integrity
//!
//! # Architecture
//!
//! The module is organized into focused submodules following the "bricks and studs" philosophy:
//!
//! - `models`: Data structures (FileRecord)
//! - `hash`: SHA256 hashing and validation
//! - `fs_ops`: File system operations (atomic copy)
//! - `queries`: Database operations (CRUD, ref counting)
//! - `service`: High-level FileStorageService orchestration
//!
//! # Example
//!
//! ```ignore
//! use file_storage::FileStorageService;
//!
//! let service = FileStorageService::new(vault_path, pool);
//!
//! // Store a file (automatically deduplicated)
//! let record = service.store_file(path, "text/plain", None).await?;
//!
//! // Get file path
//! let path = service.get_file_path(&record.id).await?;
//!
//! // Delete file (uses reference counting)
//! service.delete_file(&record.id).await?;
//! ```

mod fs_ops;
mod hash;
mod models;
mod queries;
mod service;

#[cfg(test)]
mod tests;

// Public exports
pub use models::{FileRecord, DEFAULT_MAX_FILE_SIZE};
pub use service::FileStorageService;
