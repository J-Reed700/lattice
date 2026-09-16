//! Extraction feature dependency injection.

use std::sync::Arc;

use crate::application::ports::RepositoryPort;
use crate::domain::entities::Document;
use crate::features::extraction::use_cases::extract_and_resolve_links::{
    ParseWikilinksPort, ResolveWikilinkPort,
};
use crate::features::extraction::use_cases::{
    ExtractAndResolveLinksUseCase, ExtractDocumentTitleUseCase, ParseWikilinksUseCase,
    ResolveWikilinkUseCase,
};

#[derive(Clone)]
pub struct ExtractionDi {
    pub parse_wikilinks_use_case: Arc<ParseWikilinksUseCase>,
    pub extract_document_title_use_case: Arc<ExtractDocumentTitleUseCase>,
    pub resolve_wikilink_use_case: Arc<ResolveWikilinkUseCase>,
    pub extract_and_resolve_links_use_case: Arc<ExtractAndResolveLinksUseCase>,
}

pub fn build(document_repo: Arc<dyn RepositoryPort<Document>>) -> ExtractionDi {
    let parse_wikilinks_use_case = Arc::new(ParseWikilinksUseCase::new());
    let resolve_wikilink_use_case = Arc::new(ResolveWikilinkUseCase::new());
    let extract_and_resolve_links_use_case = Arc::new(ExtractAndResolveLinksUseCase::new(
        Arc::clone(&parse_wikilinks_use_case) as Arc<dyn ParseWikilinksPort>,
        Arc::clone(&resolve_wikilink_use_case) as Arc<dyn ResolveWikilinkPort>,
        document_repo,
    ));

    ExtractionDi {
        parse_wikilinks_use_case,
        extract_document_title_use_case: Arc::new(ExtractDocumentTitleUseCase::new()),
        resolve_wikilink_use_case,
        extract_and_resolve_links_use_case,
    }
}
