//! Extraction commands for wikilink parsing.

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

pub async fn extract_document_title(
    content: String,
    container: State<'_, Container>,
) -> Result<Option<String>, AppError> {
    let request = ExtractTitleRequestDto { content };

    let use_case = container.extract_document_title_use_case();
    let response = use_case.execute(request).await?;

    Ok(response.title)
}

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
