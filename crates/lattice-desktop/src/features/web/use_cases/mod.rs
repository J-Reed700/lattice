//! Web ingestion use cases: ingest a web URL (fetch and index) and get a URL
//! preview (metadata only).

pub mod get_url_preview;
pub mod ingest_web_url;

pub use get_url_preview::GetUrlPreviewUseCase;
pub use ingest_web_url::IngestWebUrlUseCase;
