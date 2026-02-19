//! Application Layer Factories
//!
//! This module contains factories that create domain value objects.
//!
//! # Architecture
//!
//! Factories belong in the application layer when they create domain objects,
//! as per Clean Architecture principles. These factories isolate infrastructure
//! concerns (file I/O, cryptography) from the pure domain layer.
//!
//! # Factories
//!
//! - **ChecksumFactory**: Creates Checksum value objects from file paths or byte content
//! - **FileMetadataFactory**: Creates FileMetadata value objects from file system paths
//!
//! # Design Principle
//!
//! Factories encapsulate the creation logic for complex value objects that require
//! infrastructure dependencies. The domain layer remains pure (no I/O, no external deps),
//! while the application layer coordinates between domain and infrastructure.
//!
//! # Example
//!
//! ```rust,no_run
//! use vault_desktop::application::factories::{ChecksumFactory, FileMetadataFactory};
//! use std::path::Path;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let path = Path::new("/path/to/file.txt");
//!
//! // Create domain value objects using factories
//! let checksum = ChecksumFactory::from_path(path)?;
//! let metadata = FileMetadataFactory::from_path(path)?;
//!
//! println!("File: {}, Checksum: {}", metadata.file_name(), checksum.as_str());
//! # Ok(())
//! # }
//! ```

pub mod checksum_factory;
pub mod file_metadata_factory;

// Re-export public types
pub use checksum_factory::ChecksumFactory;
pub use file_metadata_factory::FileMetadataFactory;
