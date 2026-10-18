//! Web-source ingestion workflow. The command supplies the guarded fetch operation.
use crate::features::learning::dto::{
    AddLearningWebSourceRequestDto, LearningSourceKind, LearningSourceWorkspaceDto,
};
use crate::features::learning::references::trust::capture::web_capture;
use crate::features::learning::source_identity::{source_request_hash, validate_source_ids};
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use lattice_core::application::contracts::web::ReferencePage;
use lattice_core::shared::error::Result;

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
    let url = url::Url::parse(&request.url).map_err(|_| {
        lattice_core::shared::error::AppError::InvalidInput(
            "Source URL must be a valid HTTP or HTTPS address.".into(),
        )
    })?;
    if request.url.chars().count() > 2048 {
        return Err(lattice_core::shared::error::AppError::InvalidInput(
            "Source URL must not exceed 2048 characters.".into(),
        ));
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Err(lattice_core::shared::error::AppError::InvalidInput(
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
    let page = fetch(request.url.trim().to_string()).await?;
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
    let captured = web_capture(page, requested_url, title);
    repo.add(
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
    )
    .await?;
    repo.workspace(&request.program_id).await
}
