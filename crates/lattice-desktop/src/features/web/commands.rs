//! Web content ingestion commands.

use crate::features::web::dto::GetUrlPreviewRequestDto;
use crate::interfaces::di::Container;
use lattice_core::application::contracts::web::{CleanArticle, UrlPreview};
use lattice_core::shared::error::AppError;
use serde::{Deserialize, Serialize};
use tauri::State;

mod ingest;

pub use ingest::{ingest_web_url, WebIngestResponse};

/// A web page as the reader shows it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WebPageDto {
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub word_count: u32,
    pub fetched_at: String,
    pub from_cache: bool,
}

/// Read a web page for the reader pane, answering from the page cache if already read.
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
