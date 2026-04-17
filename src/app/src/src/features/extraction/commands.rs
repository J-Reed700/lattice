//! Extraction commands for wikilink parsing.
//!
//! Thin command controllers that delegate to extraction use cases.
//! Commands validate inputs and delegate business logic to the application layer.

use crate::features::extraction::dto::{
    DocumentRefDto, ExtractAndResolveRequestDto, ExtractTitleRequestDto, ParseWikilinksRequestDto,
    ResolveWikilinkRequestDto, WikiLinkDto,
};
use crate::interfaces::di::Container;
use crate::shared::error::AppError;
use serde::{Deserialize, Serialize};
use tauri::State;

// Re-export for plugin layer visibility
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
/// # Arguments
///
/// * `text` - Markdown content to parse
/// * `source_path` - Optional source document path for context
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(ParsedLinksResponse)` - Extracted links with metadata
/// * `Err(AppError)` - If parsing fails
///
/// # Errors
///
/// * `AppError::Other` - Regex compilation or parsing error
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface WikiLink {
///   target: string;        // Link target (e.g., "note-name")
///   displayText?: string;  // Display text if specified (e.g., "custom text")
///   header?: string;       // Section header if specified (e.g., "#section")
///   rawLink: string;       // Original link syntax
/// }
///
/// interface ParsedLinks {
///   links: WikiLink[];
///   count: number;
/// }
///
/// const text = `
/// This links to [[my-note]].
/// This has [[display|custom text]].
/// This links to [[document#section]].
/// `;
///
/// const result = await invoke<ParsedLinks>('parse_wikilinks', {
///   text,
///   sourcePath: '/path/to/current/document.md'
/// });
///
/// console.log(`Found ${result.count} links`);
/// result.links.forEach(link => {
///   console.log(`Target: ${link.target}, Display: ${link.displayText || 'default'}`);
/// });
/// ```
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
/// # Use Cases
///
/// - **Backlink Tracking**: Discover incoming links to documents
/// - **Link Validation**: Find broken or unresolved links
/// - **Graph Visualization**: Build document relationship graphs
/// - **Navigation**: Enable click-to-follow for wikilinks
///
/// # Performance
///
/// - **Parse Time**: ~1-10ms for typical documents (1000-10000 chars)
/// - **Regex-Based**: Efficient single-pass parsing
/// - **Async**: Non-blocking operation
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
/// # Arguments
///
/// * `content` - Markdown document content
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(Some(String))` - Extracted title if found
/// * `Ok(None)` - No suitable title found
/// * `Err(AppError)` - If extraction logic fails
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // H1 header (highest priority)
/// const doc1 = "# My Document Title\n\nContent...";
/// const title1 = await invoke<string | null>('extract_document_title', {
///   content: doc1
/// });
/// console.log(title1); // "My Document Title"
///
/// // YAML frontmatter (second priority)
/// const doc2 = `---
/// title: "Project Plan"
/// date: 2024-01-01
/// ---
/// Content...`;
/// const title2 = await invoke<string | null>('extract_document_title', {
///   content: doc2
/// });
/// console.log(title2); // "Project Plan"
///
/// // First line fallback (lowest priority)
/// const doc3 = "This is the first line\n\nMore content...";
/// const title3 = await invoke<string | null>('extract_document_title', {
///   content: doc3
/// });
/// console.log(title3); // "This is the first line"
///
/// // No title found
/// const doc4 = "\n\n  \n\n";
/// const title4 = await invoke<string | null>('extract_document_title', {
///   content: doc4
/// });
/// console.log(title4); // null
/// ```
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
/// # Use Cases
///
/// - **Document Indexing**: Extract titles for search index
/// - **Search Results**: Display titles in search results
/// - **UI Display**: Show document names in file lists
/// - **Navigation**: Use titles in breadcrumbs and menus
///
/// # Performance
///
/// - **Extraction Time**: ~1-5ms for typical documents
/// - **Regex-Based**: Efficient pattern matching
/// - **Async**: Non-blocking operation
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
/// # Arguments
///
/// * `target` - Wikilink target to resolve (e.g., "my-note" or "notes/document")
/// * `source_path` - Source document path for context
/// * `available_documents` - List of documents to search
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(ResolveLinkResponse)` - Resolved path or `None`
/// * `Err(AppError)` - If resolution logic fails
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface DocumentRef {
///   documentId: string;
///   filePath: string;
///   title: string;
/// }
///
/// const documents: DocumentRef[] = [
///   { documentId: "doc-1", filePath: "notes/test.md", title: "Test Note" },
///   { documentId: "doc-2", filePath: "projects/project.md", title: "My Project" },
///   { documentId: "doc-3", filePath: "archive/old-note.md", title: "Old Note" }
/// ];
///
/// // Exact path match (highest priority)
/// const result1 = await invoke<{ resolvedPath: string | null }>('resolve_wikilink', {
///   target: "notes/test",
///   sourcePath: "index.md",
///   availableDocuments: documents
/// });
/// console.log(result1.resolvedPath); // "notes/test.md"
///
/// // Title match
/// const result2 = await invoke<{ resolvedPath: string | null }>('resolve_wikilink', {
///   target: "Test Note",
///   sourcePath: "index.md",
///   availableDocuments: documents
/// });
/// console.log(result2.resolvedPath); // "notes/test.md"
///
/// // No match found
/// const result3 = await invoke<{ resolvedPath: string | null }>('resolve_wikilink', {
///   target: "nonexistent",
///   sourcePath: "index.md",
///   availableDocuments: documents
/// });
/// console.log(result3.resolvedPath); // null
/// ```
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
/// # Use Cases
///
/// - **Link Navigation**: Click-to-follow wikilinks
/// - **Backlink Tracking**: Find documents that link to current document
/// - **Link Validation**: Identify broken links
/// - **Auto-Complete**: Suggest link targets while typing
///
/// # Performance
///
/// - **Resolution Time**: ~1-10ms for 100 documents, ~10-50ms for 1000 documents
/// - **Linear Search**: O(n) worst case, optimized with early exit
/// - **Async**: Non-blocking operation
///
/// # Architecture
///
/// Thin controller delegating to ResolveWikilinkUseCase (DDD pattern)
pub async fn resolve_wikilink(
    target: String,
    source_path: String,
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
/// # Arguments
///
/// * `document_id` - Document identifier for context
/// * `content` - Markdown content containing wikilinks
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(ExtractAndResolveResponseDto)` - Links with resolution results
/// * `Err(AppError)` - If parsing or resolution fails
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface ResolvedLink {
///   link: WikiLink;
///   resolvedDocumentId: string | null;
///   confidence: number;
/// }
///
/// interface ExtractAndResolveResponse {
///   links: ResolvedLink[];
/// }
///
/// const result = await invoke<ExtractAndResolveResponse>('extract_and_resolve_links', {
///   documentId: 'doc-123',
///   content: 'This links to [[my-note]] and [[other-document|Display Text]]'
/// });
///
/// result.links.forEach(item => {
///   if (item.resolvedDocumentId) {
///     console.log(`Link "${item.link.target}" resolved to document ${item.resolvedDocumentId} with confidence ${item.confidence}`);
///   } else {
///     console.log(`Link "${item.link.target}" could not be resolved`);
///   }
/// });
/// ```
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
// Integration tests will be added in Phase 5 with Container test utilities.
