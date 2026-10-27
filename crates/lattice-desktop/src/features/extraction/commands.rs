//! Extraction commands for wikilink parsing.
//!
//! Thin command controllers that delegate to extraction use cases.
//! Commands validate inputs and delegate business logic to the application layer.

use crate::features::extraction::dto::{
    DocumentRefDto, ExtractAndResolveRequestDto, ExtractTitleRequestDto, ParseWikilinksRequestDto,
    ResolveWikilinkRequestDto, WikiLinkDto,
};
use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use serde::{Deserialize, Serialize};
use tauri::State;

pub use crate::features::extraction::dto::ExtractAndResolveResponseDto;

/// Response for wikilink parsing (for frontend compatibility).
#[derive(Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ParsedLinksResponse {
    pub links: Vec<WikiLinkDto>,
    pub count: usize,
}

/// Response for link resolution (for frontend compatibility).
#[derive(Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolveLinkResponse {
    pub resolved_path: Option<String>,
}

/// Parses wikilinks from markdown text using regex-based extraction
///
/// Extracts all `[[wikilinks]]` from markdown content, supporting multiple wikilink
/// formats including simple links, display text, and section headers. Returns structured
/// data about each link for frontend processing and cross-reference tracking.
///
/// # Errors
///
/// * `AppError::Other` - Regex compilation or parsing error
///
/// # Wikilink Formats
///
/// **Supported Syntax**:
/// - **Simple**: `[[note-name]]` - Links to "note-name"
/// - **Display Text**: `[[target|display]]` - Shows "display" but links to "target"
/// - **Section**: `[[note#section]]` - Links to specific section
/// - **Combined**: `[[note#section|display]]` - Display text with section
///
/// **Not Supported** (treated as plain text):
/// - **Markdown Links**: `[text](url)` - Standard markdown links
/// - **URL Links**: `http://example.com` - Plain URLs
/// - **Escaped Links**: `\[[not a link]]` - Escaped wikilinks
///
/// # Architecture
///
/// Thin controller delegating to ParseWikilinksUseCase (DDD pattern)
pub async fn parse_wikilinks(
    text: String,
    source_path: Option<String>,
    container: State<'_, Container>,
) -> Result<ParsedLinksResponse, AppError> {
    let request = ParseWikilinksRequestDto { text, source_path };

    let use_case = container.parse_wikilinks_use_case();
    let response = use_case.execute(request).await?;
    let count = response.links.len();

    Ok(ParsedLinksResponse {
        links: response.links,
        count,
    })
}

/// Extracts document title from markdown content using multiple strategies
///
/// Intelligently extracts titles from markdown documents using a fallback cascade:
/// first H1 header, YAML frontmatter, or first non-empty line. Returns `None` if
/// no suitable title found. Used for document indexing, search results, and UI display.
///
/// # Extraction Strategies
///
/// **Priority Order**:
/// 1. **H1 Header** (`# Title`): First H1 in document (most common)
/// 2. **YAML Frontmatter**: `title` field in frontmatter (structured metadata)
/// 3. **First Line**: First non-empty line (last resort)
///
/// **Edge Cases**:
/// - Multiple H1s: Uses first H1 only
/// - Markdown formatting: Strips `**bold**`, `*italic*`, `[links](url)`
/// - Whitespace: Trims leading/trailing whitespace
/// - Empty content: Returns `None`
///
/// # Architecture
///
/// Thin controller delegating to ExtractDocumentTitleUseCase (DDD pattern)
pub async fn extract_document_title(
    content: String,
    container: State<'_, Container>,
) -> Result<Option<String>, AppError> {
    let request = ExtractTitleRequestDto { content };

    let use_case = container.extract_document_title_use_case();
    let response = use_case.execute(request).await?;

    Ok(response.title)
}

/// Resolves wikilink to actual document path using intelligent matching
///
/// Attempts to resolve a wikilink target to a real document path using multiple
/// strategies with priority fallback: exact path match, path without extension,
/// title match, and fuzzy match. Returns `None` if no suitable match found.
/// Essential for wikilink navigation and cross-reference resolution.
///
/// # Resolution Strategies
///
/// **Priority Order**:
/// 1. **Exact Path**: Direct file path match (e.g., "notes/test.md")
/// 2. **Path Without Extension**: Path without `.md` (e.g., "notes/test")
/// 3. **Title Match**: Matches document title exactly
/// 4. **Fuzzy Match**: Levenshtein distance-based matching (for typos)
///
/// **Case Sensitivity**: Case-insensitive matching for better UX
///
/// # Architecture
///
/// Thin controller delegating to ResolveWikilinkUseCase (DDD pattern)
pub async fn resolve_wikilink(
    target: String,
    _source_path: String,
    available_documents: Vec<DocumentRefDto>,
    container: State<'_, Container>,
) -> Result<ResolveLinkResponse, AppError> {
    let request = ResolveWikilinkRequestDto {
        link_target: target,
        source_document_id: None,
        available_documents,
    };

    let use_case = container.resolve_wikilink_use_case();
    let response = use_case.execute(request).await?;

    Ok(ResolveLinkResponse {
        resolved_path: response.resolved_path,
    })
}

/// Extracts and resolves all wikilinks in one operation
///
/// Combines parsing and resolution for convenience, extracting all `[[wikilinks]]`
/// from a document and immediately resolving them to actual paths. Returns both
/// parsed link information and resolution results in a single response.
///
/// # Architecture
///
/// Composite use case that orchestrates ParseWikilinksUseCase and ResolveWikilinkUseCase
/// to provide a convenient single-operation interface for extracting and resolving all
/// wikilinks in a document.
pub async fn extract_and_resolve_links(
    document_id: String,
    content: String,
    container: State<'_, Container>,
) -> Result<ExtractAndResolveResponseDto, AppError> {
    let request = ExtractAndResolveRequestDto {
        document_id,
        content,
    };

    let use_case = container.extract_and_resolve_links_use_case();
    use_case.execute(request).await
}

// NOTE: Command tests removed - use case tests provide coverage.
// Command integration tests require container test utilities.
