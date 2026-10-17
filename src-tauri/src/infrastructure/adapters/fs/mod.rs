//! File system adapters.

pub mod model_paths;
pub mod tokio_checksum;
pub mod tokio_filesystem;
pub mod web_article;

pub use tokio_checksum::TokioChecksumAdapter;
pub use tokio_filesystem::TokioFileSystemAdapter;
