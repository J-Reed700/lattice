//! Infrastructure adapters implementing application ports.

pub mod content_extraction_adapter;
pub mod fs;

pub use content_extraction_adapter::ContentExtractionAdapter;
pub use fs::{SystemFileSystemAdapter, TokioChecksumAdapter};
