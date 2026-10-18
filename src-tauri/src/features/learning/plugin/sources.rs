use super::*;

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
    source_library(&container)
        .get_version(&request)
        .await
        .map_err(ApiError::from)
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
    crate::features::learning::source_workflows::add_web_source(
        &source_library(&container),
        request,
        move |url| async move { web.fetch_reference_content(&url).await },
    )
    .await
    .map_err(ApiError::from)
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
    repo.add(
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
    )
    .await
    .map_err(ApiError::from)?;
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
        extraction_version: "pasted_text_v1".into(),
    };
    source_library(&container)
        .add(
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
        )
        .await
        .map_err(ApiError::from)?;
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
        let article = container
            .web_service()
            .fetch_reference_content(&url)
            .await
            .map_err(|e| lattice_core::shared::error::AppError::Network(e.to_string()))?;
        Ok::<_, lattice_core::shared::error::AppError>(
            crate::features::learning::source_library::CapturedLearningSource {
                title: article.title.unwrap_or_else(|| url.clone()),
                publisher: None,
                requested_url: Some(url.clone()),
                resolved_url: Some(article.url),
                text: article.content,
                truncated: article.content_truncated,
                extraction_version:
                    crate::features::web::services::REFERENCE_TEXT_EXTRACTION_VERSION.into(),
            },
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
    // The library queries with the loaded model; without one, sources are
    // ranked by keyword.
    let library = match container.get_or_load_embedding().await {
        Ok(_) => Some(container.library_passages()),
        Err(_) => None,
    };
    crate::features::learning::recall_repository::LearningRecallRepository::new(
        container.db_pool().clone(),
    )
    .search_sources(&request, library.as_deref())
    .await
    .map_err(ApiError::from)
}
