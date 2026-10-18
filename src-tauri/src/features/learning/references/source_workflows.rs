//! Web-source ingestion workflow. The command supplies the guarded fetch operation.
use crate::features::learning::dto::{
    AddLearningWebSourceRequestDto, LearningSourceKind, LearningSourceWorkspaceDto,
};
use crate::features::learning::references::trust::capture::web_capture;
use crate::features::learning::source_identity::{source_request_hash, validate_source_ids};
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use lattice_core::application::contracts::web::ReferencePage;
use lattice_core::domain::learning::claim_review::SourceAddedBy;
use lattice_core::shared::error::Result;

/// The address a web source is read from: a valid HTTP or HTTPS URL of at
/// most 2048 characters, trimmed.
pub fn web_source_url(raw: &str) -> Result<String> {
    let url = url::Url::parse(raw).map_err(|_| {
        lattice_core::shared::error::AppError::InvalidInput(
            "Source URL must be a valid HTTP or HTTPS address.".into(),
        )
    })?;
    if raw.chars().count() > 2048 {
        return Err(lattice_core::shared::error::AppError::InvalidInput(
            "Source URL must not exceed 2048 characters.".into(),
        ));
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Err(lattice_core::shared::error::AppError::InvalidInput(
            "Source URL must use HTTP or HTTPS.".into(),
        ));
    }
    Ok(raw.trim().to_string())
}

/// The source captured from a fetched page, with the provenance signals the
/// fetch read (`trust::capture::web_capture`); a page without a title or text
/// is refused.
pub fn captured_page(
    requested_url: &str,
    page: ReferencePage,
) -> Result<crate::features::learning::source_library::CapturedLearningSource> {
    if page
        .output
        .title
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
        || page.output.content.trim().is_empty()
    {
        return Err(lattice_core::shared::error::AppError::InvalidInput(
            "The page did not contain usable article text.".into(),
        ));
    }
    let title = page.output.title.clone().unwrap_or_default();
    Ok(web_capture(page, requested_url, title))
}

/// Saves a page the user added by its address (`added_by = user`).
pub async fn add_web_source<F, Fetch>(
    repository: &LearningSourceLibraryRepository,
    request: AddLearningWebSourceRequestDto,
    fetch: F,
) -> Result<LearningSourceWorkspaceDto>
where
    F: FnOnce(String) -> Fetch,
    Fetch: std::future::Future<Output = Result<ReferencePage>>,
{
    validate_source_ids(
        &request.program_id,
        &request.source_id,
        &request.version_id,
        &request.operation_id,
    )?;
    let requested_url = web_source_url(&request.url)?;
    let requested_url = requested_url.as_str();
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
    let page = fetch(requested_url.to_string()).await?;
    let captured = captured_page(requested_url, page)?;
    repo.add_by(
        crate::features::learning::source_library::NewLearningSource {
            operation_id: &request.operation_id,
            source_id: &request.source_id,
            version_id: &request.version_id,
            program_id: &request.program_id,
            kind: LearningSourceKind::Web,
            operation_kind: "add_web",
            origin: requested_url,
            requested_url: Some(requested_url),
            freshness: request.freshness_policy,
        },
        captured,
        hash,
        SourceAddedBy::User,
    )
    .await?;
    repo.workspace(&request.program_id).await
}
