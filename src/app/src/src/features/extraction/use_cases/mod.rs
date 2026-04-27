//! Extraction feature — use cases (wikilink + title extraction).

pub mod extract_and_resolve_links;
pub mod extract_title;
pub mod parse_wikilinks;
pub mod resolve_wikilink;

pub use extract_and_resolve_links::ExtractAndResolveLinksUseCase;
pub use extract_title::ExtractDocumentTitleUseCase;
pub use parse_wikilinks::ParseWikilinksUseCase;
pub use resolve_wikilink::ResolveWikilinkUseCase;
