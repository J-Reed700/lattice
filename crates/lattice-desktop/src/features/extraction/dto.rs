//! Extraction DTOs
//!
//! Data Transfer Objects for wikilink extraction and resolution operations.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WikiLinkDto {
    pub target: String,
    pub display_text: Option<String>,
    pub header: Option<String>,
    pub line_number: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseWikilinksRequestDto {
    pub text: String,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseWikilinksResponseDto {
    pub links: Vec<WikiLinkDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRefDto {
    pub document_id: String,
    pub file_path: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveWikilinkRequestDto {
    pub link_target: String,
    pub source_document_id: Option<String>,
    pub available_documents: Vec<DocumentRefDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveWikilinkResponseDto {
    pub resolved_document_id: Option<String>,
    pub resolved_path: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractTitleRequestDto {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractTitleResponseDto {
    pub title: Option<String>,
    pub strategy: TitleExtractionStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum TitleExtractionStrategy {
    H1Header,
    Frontmatter,
    FirstLine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractAndResolveRequestDto {
    pub document_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtractAndResolveResponseDto {
    pub links: Vec<ResolvedLinkDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLinkDto {
    pub link: WikiLinkDto,
    pub resolved_document_id: Option<String>,
    pub confidence: f32,
}
