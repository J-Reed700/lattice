//! Acquire immutable, bounded source snapshots for Learning Studio.

use super::dto::{GenerateLearningProgramRequestDto, LearningSourceDto};
use super::repository::LearningRepository;
use crate::features::web::traits::WebServiceTrait;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use std::collections::HashSet;

const MAX_URLS: usize = 8;
const MAX_DOCUMENT_IDS: usize = 8;
const MAX_SOURCES: usize = 12;
// Keep this in sync with the repository's 2,400-character document excerpt
// and generation's source snapshot limit.
const MAX_EXCERPT_CHARS: usize = 2400;
const MAX_TITLE_CHARS: usize = 180;

fn bounded(mut source: LearningSourceDto) -> LearningSourceDto {
    source.title = source.title.chars().take(MAX_TITLE_CHARS).collect();
    source.excerpt = source.excerpt.chars().take(MAX_EXCERPT_CHARS).collect();
    source
}

/// Keep at least one acquired passage per selected document where possible,
/// then spend remaining slots round-robin so a long document cannot crowd out
/// the rest of the selected material.
fn select_document_sources(
    sources: Vec<LearningSourceDto>,
    limit: usize,
) -> Vec<LearningSourceDto> {
    let mut groups: Vec<(String, Vec<LearningSourceDto>)> = Vec::new();
    for source in sources {
        let document_id = source
            .id
            .split_once(':')
            .map(|(id, _)| id)
            .unwrap_or(&source.id)
            .to_owned();
        if let Some((_, group)) = groups.iter_mut().find(|(id, _)| id == &document_id) {
            group.push(source);
        } else {
            groups.push((document_id, vec![source]));
        }
    }
    let mut selected = Vec::new();
    for passage_index in 0..groups
        .iter()
        .map(|(_, group)| group.len())
        .max()
        .unwrap_or_default()
    {
        for (_, group) in &groups {
            if let Some(source) = group.get(passage_index) {
                if selected.len() == limit {
                    return selected;
                }
                selected.push(bounded(source.clone()));
            }
        }
    }
    selected
}

/// Acquire sources selected by document ID and supplied URL. URLs are checked
/// through the host's DNS-aware safe web service, including its pinned public
/// resolver and redirect checks. Any acquisition failure is returned to the
/// caller; a partial result is never presented as a complete source set.
pub async fn acquire(
    container: &Container,
    request: &GenerateLearningProgramRequestDto,
) -> Result<Vec<LearningSourceDto>> {
    if request.document_ids.len() > MAX_DOCUMENT_IDS {
        return Err(AppError::InvalidInput(format!(
            "Select no more than {MAX_DOCUMENT_IDS} documents for one learning program."
        )));
    }
    if request.source_urls.len() > MAX_URLS {
        return Err(AppError::InvalidInput(format!(
            "Add no more than {MAX_URLS} online sources for one learning program."
        )));
    }
    let mut unique_documents = HashSet::new();
    if request
        .document_ids
        .iter()
        .any(|id| id.trim().is_empty() || id.trim() != id || !unique_documents.insert(id.as_str()))
    {
        return Err(AppError::InvalidInput(
            "The selected document list contains an empty or duplicate identifier.".into(),
        ));
    }
    let mut unique_urls = HashSet::new();
    if request
        .source_urls
        .iter()
        .any(|url| url.trim().is_empty() || !unique_urls.insert(url.trim()))
    {
        return Err(AppError::InvalidInput(
            "The source URL list contains an empty or duplicate address.".into(),
        ));
    }

    let document_sources = LearningRepository::new(container.db_pool().clone())
        .acquire_document_sources(&request.document_ids, &request.goal)
        .await?;
    let url_slots = request.source_urls.len().min(MAX_SOURCES);
    let document_limit = MAX_SOURCES.saturating_sub(url_slots);
    let mut sources = select_document_sources(document_sources, document_limit);
    let safe_web = container.web_service();
    for supplied_url in &request.source_urls {
        let url = supplied_url.trim();
        container
            .security_context()
            .rate_limiters()
            .web_ingest
            .check_rate_limit("learning_source_fetch")
            .await
            .map_err(|error| AppError::RateLimitExceeded(error.to_string()))?;
        // This safe host service validates the URL with its public-only
        // resolver, revalidates every redirect, and pins checked DNS answers
        // into the connection before performing the actual read.
        let article = safe_web.fetch_url_content(url).await.map_err(|error| {
            AppError::Network(format!("Could not acquire source {url}: {error}"))
        })?;
        let excerpt: String = article.content.chars().take(MAX_EXCERPT_CHARS).collect();
        let title = article.title.as_deref().unwrap_or_default().trim();
        if title.is_empty() || excerpt.trim().is_empty() {
            return Err(AppError::InvalidInput(format!(
                "Source {url} did not contain usable article text."
            )));
        }
        sources.push(LearningSourceDto {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.chars().take(MAX_TITLE_CHARS).collect(),
            url: Some(article.url),
            excerpt,
            acquired_at: chrono::Utc::now().timestamp_millis(),
        });
    }
    if sources.is_empty() {
        return Err(AppError::InvalidInput(
            "Select at least one document or add a public source URL before generating a program."
                .into(),
        ));
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquired_source_snapshot_is_bounded_without_losing_provenance() {
        let source = LearningSourceDto {
            id: "stable-id".into(),
            title: "t".repeat(240),
            url: Some("https://example.org/reading".into()),
            excerpt: "x".repeat(5000),
            acquired_at: 456,
        };
        let bounded = bounded(source);
        assert_eq!(bounded.excerpt.chars().count(), MAX_EXCERPT_CHARS);
        assert_eq!(bounded.title.chars().count(), MAX_TITLE_CHARS);
        assert_eq!(bounded.id, "stable-id");
        assert_eq!(bounded.url.as_deref(), Some("https://example.org/reading"));
        assert_eq!(bounded.acquired_at, 456);
    }

    #[test]
    fn document_passages_are_selected_round_robin() {
        let source = |id: &str| LearningSourceDto {
            id: id.into(),
            title: id.into(),
            url: None,
            excerpt: "passage".into(),
            acquired_at: 0,
        };
        let selected = select_document_sources(
            vec![
                source("doc-a:chunk-1"),
                source("doc-a:chunk-2"),
                source("doc-b:chunk-1"),
            ],
            2,
        );
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].id, "doc-a:chunk-1");
        assert_eq!(selected[1].id, "doc-b:chunk-1");
    }
}
