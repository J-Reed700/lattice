//! Acquire immutable, bounded source snapshots for Learning Studio.

use crate::application::ports::LibraryPassagesPort;
use crate::features::learning::dto::{GenerateLearningProgramRequestDto, LearningSourceDto};
use crate::features::learning::dto::{
    LearningSourceKind, LearningSourcePolicy, RefreshLearningSourceRequestDto,
};
use crate::features::web::traits::WebServiceTrait;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use std::collections::HashSet;

// Keep this in sync with the repository's 2,400-character document excerpt
// and generation's source snapshot limit.
const MAX_EXCERPT_CHARS: usize = 2400;
const MAX_TITLE_CHARS: usize = 180;

pub struct InitialReference {
    pub source_id: String,
    pub origin: String,
    pub captured: crate::features::learning::source_library::CapturedLearningSource,
}
pub struct AcquiredSources {
    pub sources: Vec<LearningSourceDto>,
    pub references: Vec<InitialReference>,
}
pub(in crate::features::learning) fn validate_capture(
    captured: &crate::features::learning::source_library::CapturedLearningSource,
) -> Result<()> {
    if captured.text.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "The reference contains no readable text.".into(),
        ));
    }
    if captured.truncated {
        return Err(AppError::InvalidInput("The reference could not be captured in full. Use a complete document or a readable chapter URL.".into()));
    }
    Ok(())
}
pub(in crate::features::learning) fn preview(
    id: &str,
    captured: &crate::features::learning::source_library::CapturedLearningSource,
) -> LearningSourceDto {
    let headings = captured
        .text
        .lines()
        .filter(|line| line.trim_start().starts_with('#'))
        .take(35)
        .collect::<Vec<_>>()
        .join("\n");
    let text = if headings.is_empty() {
        captured.text.clone()
    } else {
        format!(
            "{}\n\n{}",
            headings.chars().take(1200).collect::<String>(),
            captured.text
        )
    };
    LearningSourceDto {
        id: id.into(),
        title: captured.title.chars().take(MAX_TITLE_CHARS).collect(),
        url: captured
            .resolved_url
            .clone()
            .or_else(|| captured.requested_url.clone()),
        excerpt: text.chars().take(MAX_EXCERPT_CHARS).collect(),
        acquired_at: chrono::Utc::now().timestamp_millis(),
    }
}
/// Capture every indexed chunk of a library document, in order. The joined
/// text is what retrieval later maps the library's chunk hits back onto.
pub(in crate::features::learning) async fn capture_document(
    library: &dyn LibraryPassagesPort,
    document_id: &str,
) -> Result<crate::features::learning::source_library::CapturedLearningSource> {
    let document = library.document_text(document_id).await?.ok_or_else(|| {
        AppError::InvalidInput(
            "The document has no indexed text. Let its import finish first.".into(),
        )
    })?;
    let captured = crate::features::learning::source_library::CapturedLearningSource {
        title: document.title,
        publisher: None,
        requested_url: None,
        resolved_url: None,
        truncated: false,
        text: document
            .chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
        extraction_version: "document_chunks_v2".into(),
    };
    validate_capture(&captured)?;
    Ok(captured)
}

/// Acquire sources selected by document ID and supplied URL. URLs are checked
/// through the host's DNS-aware safe web service, including its pinned public
/// resolver and redirect checks. Any acquisition failure is returned to the
/// caller; a partial result is never presented as a complete source set.
pub async fn acquire(
    container: &Container,
    request: &GenerateLearningProgramRequestDto,
) -> Result<AcquiredSources> {
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

    let library = container.library_passages();
    let mut sources = Vec::new();
    let mut references = Vec::new();
    for document_id in &request.document_ids {
        let captured = capture_document(library.as_ref(), document_id).await?;
        let id = uuid::Uuid::new_v4().to_string();
        sources.push(preview(&id, &captured));
        references.push(InitialReference {
            source_id: id,
            origin: document_id.clone(),
            captured,
        });
    }
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
        let article = safe_web
            .fetch_reference_content(url)
            .await
            .map_err(|error| {
                AppError::Network(format!("Could not acquire source {url}: {error}"))
            })?;
        let captured = crate::features::learning::source_library::CapturedLearningSource {
            title: article.title.unwrap_or_else(|| url.into()),
            publisher: None,
            requested_url: Some(url.into()),
            resolved_url: Some(article.url),
            text: article.content,
            truncated: article.content_truncated,
            extraction_version: crate::features::web::services::REFERENCE_TEXT_EXTRACTION_VERSION
                .into(),
        };
        validate_capture(&captured)?;
        let id = uuid::Uuid::new_v4().to_string();
        sources.push(preview(&id, &captured));
        references.push(InitialReference {
            source_id: id,
            origin: url.into(),
            captured,
        });
    }
    if sources.is_empty() && (!request.document_ids.is_empty() || !request.source_urls.is_empty()) {
        return Err(AppError::InvalidInput(
            "The selected materials did not provide usable text. Choose different materials or remove them to create a topic-based course."
                .into(),
        ));
    }
    Ok(AcquiredSources {
        sources,
        references,
    })
}

pub(in crate::features::learning) async fn ensure_before_use_sources(
    container: &Container,
    program_id: &str,
) -> crate::shared::error::Result<()> {
    let repo = crate::features::learning::source_library::LearningSourceLibraryRepository::new(
        container.db_pool().clone(),
    );
    let workspace = repo.workspace(program_id).await?;
    for source in workspace.sources.iter().filter(|s| {
        s.kind == LearningSourceKind::Web && s.freshness_policy == LearningSourcePolicy::BeforeUse
    }) {
        if source.pending_version_id.is_some() {
            return Err(crate::shared::error::AppError::InvalidInput(format!("A newer version of '{}' is available. Adopt it or change the freshness policy before preparing a lesson.",source.active_version.as_ref().map(|v|v.title.as_str()).unwrap_or("this source"))));
        }
        let operation_id = uuid::Uuid::new_v4().to_string();
        let request = RefreshLearningSourceRequestDto {
            operation_id: operation_id.clone(),
            program_id: program_id.into(),
            source_id: source.id.clone(),
            expected_revision: source.revision,
        };
        let refreshed = async {
            container
                .security_context()
                .rate_limiters()
                .web_ingest
                .check_rate_limit("learning_source_before_use")
                .await
                .map_err(|e| crate::shared::error::AppError::RateLimitExceeded(e.to_string()))?;
            let url = source.requested_url.as_deref().ok_or_else(|| {
                crate::shared::error::AppError::Database("Web source has no requested URL".into())
            })?;
            let article = container
                .web_service()
                .fetch_reference_content(url)
                .await
                .map_err(|e| crate::shared::error::AppError::Network(e.to_string()))?;
            Ok::<_, crate::shared::error::AppError>(
                crate::features::learning::source_library::CapturedLearningSource {
                    title: article.title.unwrap_or_else(|| url.into()),
                    publisher: None,
                    requested_url: Some(url.into()),
                    resolved_url: Some(article.url),
                    text: article.content,
                    truncated: article.content_truncated,
                    extraction_version:
                        crate::features::web::services::REFERENCE_TEXT_EXTRACTION_VERSION.into(),
                },
            )
        }
        .await;
        let (captured, failure) = match refreshed {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.to_string())),
        };
        repo.refresh(&request, captured, failure).await?;
        let refreshed = repo.workspace(program_id).await?;
        let latest = refreshed
            .sources
            .iter()
            .find(|s| s.id == source.id)
            .and_then(|s| s.latest_check.as_ref())
            .filter(|check| check.operation_id == operation_id);
        match latest.map(|check| &check.status) {
            Some(status) => {
                crate::features::learning::source_library::LearningSourceLibraryRepository::before_use_allows(status)?
            }
            None => {
                return Err(crate::shared::error::AppError::ServiceNotAvailable(format!(
                    "Could not verify the before-use source '{}'. Retry after the source is reachable.",
                    source.active_version.as_ref().map(|v|v.title.as_str()).unwrap_or("web source")
                )))
            }
        }
    }
    Ok(())
}
