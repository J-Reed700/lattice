//! Thin commands for ingesting, previewing and extracting web content.

use crate::features::web::dto::GetUrlPreviewRequestDto;
use crate::interfaces::di::Container;
use lattice_core::application::contracts::web::{CleanArticle, UrlPreview};
use lattice_core::shared::error::AppError;
// TODO: Fix web_ingestion module test errors before enabling
// use crate::features::web::ingestion::types::{is_safe_url, normalize_url};
use serde::{Deserialize, Serialize};
use tauri::State;

mod ingest;

pub use ingest::{ingest_web_url, WebIngestResponse};

/// A web page as the reader shows it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WebPageDto {
    /// The URL after redirects.
    pub url: String,
    /// Page title, when the page gave one.
    pub title: Option<String>,
    /// Extracted article text — the same text the model was given.
    pub text: String,
    pub word_count: u32,
    /// RFC 3339. When the text was actually fetched, not when it was served.
    pub fetched_at: String,
    /// True when this came out of the page cache rather than off the network.
    pub from_cache: bool,
}

/// Reads a web page for the reader pane through the same fetch path the
/// model's `fetch_url_content` uses, so a page the turn already read comes
/// from the page cache: the reader shows the text the model read.
///
/// # Errors
///
/// An invalid or blocked URL, or a failed fetch.
#[tracing::instrument(skip(container), fields(url = %url))]
pub async fn read_web_page(
    url: String,
    container: State<'_, Container>,
) -> Result<WebPageDto, AppError> {
    let read = match container.web_service().read_page(&url).await {
        Ok(read) => read,
        Err(e) => {
            return Err(e);
        }
    };

    Ok(WebPageDto {
        url: read.output.url,
        title: read.output.title,
        text: read.output.content,
        word_count: u32::try_from(read.output.word_count).unwrap_or(u32::MAX),
        fetched_at: read.fetched_at.to_rfc3339(),
        from_cache: read.output.from_cache,
    })
}

/// A URL's metadata preview (OpenGraph, Schema.org and HTML meta tags),
/// without indexing its content.
#[tracing::instrument(skip(container), fields(url = %url))]
pub async fn fetch_url_preview(
    url: String,
    container: State<'_, Container>,
) -> Result<UrlPreview, AppError> {
    let preview_use_case = container.get_url_preview_use_case();

    let preview = match preview_use_case
        .execute(GetUrlPreviewRequestDto { url: url.clone() })
        .await
    {
        Ok(preview) => preview,
        Err(e) => {
            return Err(e);
        }
    };

    let preview = UrlPreview {
        url: preview.url,
        title: preview.title,
        description: preview.description,
        site_name: preview.site_name,
        image: preview.image,
        author: None,
        published_date: None,
        word_count: 0,
        reading_time_minutes: 0,
        language: None,
        content_type: None,
        keywords: Vec::new(),
    };

    Ok(preview)
}

/// A web page's clean article content, via Mozilla's Readability algorithm.
#[tracing::instrument(skip(container), fields(url = %url))]
pub async fn extract_article(
    url: String,
    container: State<'_, Container>,
) -> Result<CleanArticle, AppError> {
    let article_extractor = container.article_extractor_service();

    let article = match article_extractor.extract_article_from_url(&url).await {
        Ok(article) => article,
        Err(e) => {
            return Err(e);
        }
    };

    Ok(article)
}

// #[cfg(test)]
// mod tests { ... }
