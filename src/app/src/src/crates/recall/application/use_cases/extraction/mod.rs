//! Extraction Use Cases
//!
//! Use cases for extracting structured information from document content.
//!
//! # Use Cases
//!
//! - **ParseWikilinksUseCase** - Extract wikilinks from markdown content
//! - **ExtractDocumentTitleUseCase** - Extract title using multiple strategies
//! - **ResolveWikilinkUseCase** - Resolve wikilink targets to document IDs/paths
//! - **ExtractAndResolveLinksUseCase** - Composite use case combining parse + resolve
//!
//! # Example
//!
//! ```rust,no_run
//! use crate::application::use_cases::extraction::*;
//! use crate::application::dtos::*;
//!
//! // Parse wikilinks
//! let parse_use_case = ParseWikilinksUseCase::new();
//! let links = parse_use_case.execute(ParseWikilinksRequestDto {
//!     text: "See [[note]]".into(),
//!     source_path: Some("index.md".into()),
//! }).await?;
//!
//! // Extract title
//! let title_use_case = ExtractDocumentTitleUseCase::new();
//! let title = title_use_case.execute(ExtractTitleRequestDto {
//!     content: "# My Document".into(),
//! }).await?;
//!
//! // Resolve wikilink
//! let resolve_use_case = ResolveWikilinkUseCase::new();
//! let resolved = resolve_use_case.execute(ResolveWikilinkRequestDto {
//!     link_target: "note".into(),
//!     source_document_id: None,
//!     available_documents: vec![],
//! }).await?;
//!
//! // Extract and resolve (composite)
//! let composite_use_case = ExtractAndResolveLinksUseCase::new(
//!     Arc::new(parse_use_case),
//!     Arc::new(resolve_use_case),
//!     document_repository,
//! );
//! let result = composite_use_case.execute(ExtractAndResolveRequestDto {
//!     document_id: "doc-123".into(),
//!     content: "See [[note]]".into(),
//! }).await?;
//! ```

pub mod extract_and_resolve_links;
pub mod extract_document_title;
pub mod parse_wikilinks;
pub mod resolve_wikilink;

pub use extract_and_resolve_links::ExtractAndResolveLinksUseCase;
pub use extract_document_title::ExtractDocumentTitleUseCase;
pub use parse_wikilinks::ParseWikilinksUseCase;
pub use resolve_wikilink::ResolveWikilinkUseCase;
