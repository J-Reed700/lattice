//! Infrastructure adapters implementing application ports.

pub mod content_extraction_adapter;
pub mod fs;
pub mod system_info;

pub use content_extraction_adapter::ContentExtractionAdapter;
pub use fs::TokioChecksumAdapter;
