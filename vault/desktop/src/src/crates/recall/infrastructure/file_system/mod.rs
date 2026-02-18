//! File System Infrastructure
//!
//! This module contains file system operations:
//! - **File System Adapter**: OS-level file operations (open, reveal in folder)
//! - **File Storage**: File reading, writing, and management
//! - **File Watcher**: File system monitoring and change detection
//! - **Atomic FS**: Atomic file operations for consistency
//!
//! # Architecture Note
//!
//! Factories (`FileMetadataFactory`, `ChecksumFactory`) have been **MOVED** to the
//! application layer (`application::factories`) as they create domain value objects.
//! Per DDD principles, factories that create domain objects belong in the application
//! layer, not infrastructure.
//!
//! # Migration Status
//! - [x] file_system_adapter.rs - NEW: FileSystemPort implementation (Phase 3)
//! - [x] file_metadata_factory.rs - MOVED to application/factories (DDD compliance)
//! - [x] checksum_factory.rs - MOVED to application/factories (DDD compliance)
//! - [ ] file_storage.rs - Will move from `services/file_storage/file_storage.rs`
//! - [ ] file_watcher.rs - Will move from `services/file_watcher.rs`
//! - [ ] atomic_fs.rs - Will move from `utils/atomic_fs.rs`
//! - [x] path_validator.rs - REMOVED (replaced by FileAccessConfig in Phase 2)
//!
//! # Security
//! - All file paths must be validated through FileAccessConfig
//! - Prevents directory traversal attacks (CWE-22)
//! - Prevents command injection attacks (CWE-78)

pub mod atomic_fs;
pub mod file_storage;
pub mod file_system_adapter;
pub mod file_watcher;

// Re-export public types
pub use file_storage::SecureFileStorage;
pub use file_system_adapter::FileSystemAdapter;
