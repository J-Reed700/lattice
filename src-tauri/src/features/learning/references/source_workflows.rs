//! Web-source ingestion workflow. The command supplies the guarded fetch operation.
use crate::features::function_calling::dto::FetchUrlContentOutput;
use crate::features::learning::dto::{
    AddLearningWebSourceRequestDto, LearningSourceKind, LearningSourceWorkspaceDto,
};
use crate::features::learning::source_identity::{source_request_hash, validate_source_ids};
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use crate::shared::error::Result;

pub async fn add_web_source<F, Fetch>(
    repository: &LearningSourceLibraryRepository,
    request: AddLearningWebSourceRequestDto,
    fetch: F,
) -> Result<LearningSourceWorkspaceDto>
where
    F: FnOnce(String) -> Fetch,
    Fetch: std::future::Future<Output = Result<FetchUrlContentOutput>>,
{
    validate_source_ids(
        &request.program_id,
        &request.source_id,
        &request.version_id,
        &request.operation_id,
    )?;
    let url = url::Url::parse(&request.url).map_err(|_| {
        crate::shared::error::AppError::InvalidInput(
            "Source URL must be a valid HTTP or HTTPS address.".into(),
        )
    })?;
    if request.url.chars().count() > 2048 {
        return Err(crate::shared::error::AppError::InvalidInput(
            "Source URL must not exceed 2048 characters.".into(),
        ));
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Err(crate::shared::error::AppError::InvalidInput(
            "Source URL must use HTTP or HTTPS.".into(),
        ));
    }
    let requested_url = request.url.trim();
    let repo = repository;
    let hash = source_request_hash(&request)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.source_id,
            "add_web",
            &hash,
        )
        .await?
    {
        return repo.workspace(&request.program_id).await;
    }
    repo.preflight_new_source(&request.program_id, &request.source_id, &request.version_id)
        .await?;
    let article = fetch(request.url.trim().to_string()).await?;
    if article
        .title
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
        || article.content.trim().is_empty()
    {
        return Err(crate::shared::error::AppError::InvalidInput(
            "The page did not contain usable article text.".into(),
        ));
    }
    let captured = crate::features::learning::source_library::CapturedLearningSource {
        title: article.title.unwrap_or_default(),
        publisher: None,
        requested_url: Some(requested_url.into()),
        resolved_url: Some(article.url),
        text: article.content,
        truncated: article.content_truncated,
        extraction_version: crate::features::web::services::REFERENCE_TEXT_EXTRACTION_VERSION
            .into(),
    };
    repo.add(
        &request.operation_id,
        &request.source_id,
        &request.version_id,
        &request.program_id,
        LearningSourceKind::Web,
        "add_web",
        requested_url,
        Some(requested_url),
        request.freshness_policy,
        captured,
        hash,
    )
    .await?;
    repo.workspace(&request.program_id).await
}
