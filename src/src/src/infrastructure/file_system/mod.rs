//! File System Infrastructure
//!
//! This module contains file system operations:
//! - **File System Adapter**: OS-level file operations (open, reveal in folder)
//! - **File Storage**: File reading, writing, and management
//!
//! # Architecture Note
//!
//! Factories (`FileMetadataFactory`, `ChecksumFactory`) have been **MOVED** to the
//! application layer (`application::factories`) as they create domain value objects.
//! Per DDD principles, factories that create domain objects belong in the application
//! layer, not infrastructure.
//!
//! # Security
//! - All file paths must be validated through FileAccessConfig
//! - Prevents directory traversal attacks (CWE-22)
//! - Prevents command injection attacks (CWE-78)

pub mod file_storage;
pub mod file_system_adapter;

pub use file_storage::SecureFileStorage;
pub use file_system_adapter::FileSystemAdapter;
