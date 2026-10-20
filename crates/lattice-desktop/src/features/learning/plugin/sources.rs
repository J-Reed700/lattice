use super::*;
use lattice_core::application::ports::ReferenceSnapshot;
use lattice_core::domain::learning::claim_review::SourceAddedBy;

/// A course's active sources changed: its index is out of date, and a build
/// is queued (§3.4).
async fn sources_changed(container: &Container, program_id: &str) {
    if let Some(courses) = container.course_indexes() {
        courses.refresh(program_id).await;
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_source_workspace(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    source_library(&container)
        .workspace(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_source_version(
    request: GetLearningSourceVersionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceVersionDto, ApiError> {
    let repo = source_library(&container);
    let mut version = repo.get_version(&request).await.map_err(ApiError::from)?;
    // A page's frozen copy finishes after its version is saved; the first
    // read that finds it ready records it (claim review §5.4).
    if version.version.snapshot_hash.is_none() {
        if let Some(url) = version.version.resolved_url.clone() {
            match container.reference_snapshots().status(&url).await {
                Ok(Some(ReferenceSnapshot::Ready { sha256 })) => {
                    repo.link_snapshot(&request.program_id, &request.version_id, &sha256)
                        .await
                        .map_err(ApiError::from)?;
                    version.version.snapshot_hash = Some(sha256);
                }
                Ok(_) => {}
                // The captured text still reads; the frozen copy is optional.
                Err(error) => tracing::debug!(%error, "Could not look up a source's frozen copy"),
            }
        }
    }
    Ok(version)
}

#[tauri::command]
#[specta::specta]
pub async fn search_learning_sources(
    request: SearchLearningSourcesRequestDto,
    container: State<'_, Container>,
) -> Result<Vec<LearningSourceSearchResultDto>, ApiError> {
    source_library(&container)
        .search(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn add_learning_web_source(
    request: AddLearningWebSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    let web = container.web_service();
    let program_id = request.program_id.clone();
    let workspace = crate::features::learning::source_workflows::add_web_source(
        &source_library(&container),
        request,
        move |url| async move { web.fetch_reference_page(&url).await },
    )
    .await
    .map_err(ApiError::from)?;
    sources_changed(&container, &program_id).await;
    Ok(workspace)
}

#[tauri::command]
#[specta::specta]
pub async fn add_learning_document_source(
    request: AddLearningDocumentSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    validate_source_ids(
        &request.program_id,
        &request.source_id,
        &request.version_id,
        &request.operation_id,
    )
    .map_err(ApiError::from)?;
    uuid::Uuid::parse_str(&request.document_id).map_err(|_| {
        ApiError::from(lattice_core::shared::error::AppError::InvalidInput(
            "Invalid document ID".into(),
        ))
    })?;
    let repo = source_library(&container);
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.source_id,
            "add_document",
            &hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    repo.preflight_new_source(&request.program_id, &request.source_id, &request.version_id)
        .await
        .map_err(ApiError::from)?;
    let captured = crate::features::learning::sources::capture_document(
        container.library_passages().as_ref(),
        &request.document_id,
    )
    .await
    .map_err(ApiError::from)?;
    repo.add_by(
        crate::features::learning::source_library::NewLearningSource {
            operation_id: &request.operation_id,
            source_id: &request.source_id,
            version_id: &request.version_id,
            program_id: &request.program_id,
            kind: LearningSourceKind::Document,
            operation_kind: "add_document",
            origin: &request.document_id,
            requested_url: None,
            freshness: LearningSourcePolicy::Fixed,
        },
        captured,
        hash,
        SourceAddedBy::User,
    )
    .await
    .map_err(ApiError::from)?;
    sources_changed(&container, &request.program_id).await;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn add_learning_text_source(
    request: AddLearningTextSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    validate_source_ids(
        &request.program_id,
        &request.source_id,
        &request.version_id,
        &request.operation_id,
    )
    .map_err(ApiError::from)?;
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    let captured = crate::features::learning::source_library::CapturedLearningSource {
        title: request.title.clone(),
        publisher: request.publisher.clone(),
        requested_url: None,
        resolved_url: None,
        text: request.text.clone(),
        truncated: false,
        extraction_version: crate::features::learning::source_library::PASTED_TEXT_EXTRACTION
            .into(),
        signals: crate::features::learning::references::trust::capture::pasted_signals(
            request.publisher.as_deref(),
            &request.text,
        ),
    };
    source_library(&container)
        .add_by(
            crate::features::learning::source_library::NewLearningSource {
                operation_id: &request.operation_id,
                source_id: &request.source_id,
                version_id: &request.version_id,
                program_id: &request.program_id,
                kind: LearningSourceKind::Pasted,
                operation_kind: "add_text",
                origin: &request.title,
                requested_url: None,
                freshness: LearningSourcePolicy::Fixed,
            },
            captured,
            hash,
            SourceAddedBy::User,
        )
        .await
        .map_err(ApiError::from)?;
    sources_changed(&container, &request.program_id).await;
    source_library(&container)
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn refresh_learning_source(
    request: RefreshLearningSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    let repo = source_library(&container);
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.source_id,
            "refresh",
            &hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    let (revision, url, _) = repo
        .source_for_refresh(&request.program_id, &request.source_id)
        .await
        .map_err(ApiError::from)?;
    if revision != request.expected_revision {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
                "Source changed; reload and retry.".into(),
            ),
        ));
    }
    let captured_result = async {
        let page = container
            .web_service()
            .fetch_reference_page(&url)
            .await
            .map_err(|e| lattice_core::shared::error::AppError::Network(e.to_string()))?;
        let title = page.output.title.clone().unwrap_or_else(|| url.clone());
        Ok::<_, lattice_core::shared::error::AppError>(
            crate::features::learning::references::trust::capture::web_capture(page, &url, title),
        )
    }
    .await;
    let (captured, failure) = match captured_result {
        Ok(c) => (Some(c), None),
        Err(e) => (None, Some(e.to_string())),
    };
    repo.refresh(&request, captured, failure)
        .await
        .map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn adopt_learning_source_version(
    request: AdoptLearningSourceVersionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    let repo = source_library(&container);
    repo.adopt(&request).await.map_err(ApiError::from)?;
    sources_changed(&container, &request.program_id).await;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn update_learning_source_policy(
    request: UpdateLearningSourcePolicyRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    let repo = source_library(&container);
    repo.update_policy(&request).await.map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_learning_source(
    request: DeleteLearningSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    source_library(&container)
        .delete_source(&request)
        .await
        .map_err(ApiError::from)?;
    sources_changed(&container, &request.program_id).await;
    source_library(&container)
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn reimport_learning_source(
    request: ReimportLearningSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    source_library(&container)
        .reimport_source(&request)
        .await
        .map_err(ApiError::from)?;
    sources_changed(&container, &request.program_id).await;
    source_library(&container)
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn create_learning_source_selector(
    request: CreateLearningSourceSelectorRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceSelectorDto, ApiError> {
    crate::features::learning::recall_repository::LearningRecallRepository::new(
        container.db_pool().clone(),
    )
    .create_selector(&request)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_source_selector(
    program_id: String,
    selector_id: String,
    container: State<'_, Container>,
) -> Result<LearningSourceSelectorDto, ApiError> {
    crate::features::learning::recall_repository::LearningRecallRepository::new(
        container.db_pool().clone(),
    )
    .get_selector(&program_id, &selector_id)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn match_learning_source_selector(
    request: MatchLearningSourceSelectorRequestDto,
    container: State<'_, Container>,
) -> Result<crate::features::learning::source_selector::LearningQuoteMatch, ApiError> {
    crate::features::learning::recall_repository::LearningRecallRepository::new(
        container.db_pool().clone(),
    )
    .match_selector(&request)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn search_learning_sources_semantically(
    request: SearchLearningSourcesSemanticallyRequestDto,
    container: State<'_, Container>,
) -> Result<Vec<LearningSourceSemanticSearchResultDto>, ApiError> {
    // The course's index searches by meaning once an embedding model is
    // active, and by words otherwise.
    crate::features::learning::recall_repository::LearningRecallRepository::new(
        container.db_pool().clone(),
    )
    .search_sources(&request, container.course_indexes())
    .await
    .map_err(ApiError::from)
}
