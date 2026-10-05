use super::*;

pub(super) fn assessment_repo(
    container: &Container,
) -> crate::features::learning::assessment_repository::LearningAssessmentRepository {
    crate::features::learning::assessment_repository::LearningAssessmentRepository::new(
        container.db_pool().clone(),
    )
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_assessment_workspace(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningAssessmentWorkspaceDto, ApiError> {
    assessment_repo(&container)
        .workspace(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn create_learning_assessment_blueprint(
    request: CreateLearningAssessmentBlueprintRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentWorkspaceDto, ApiError> {
    let repo = assessment_repo(&container);
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    if let Some(workspace) = repo
        .preflight_blueprint(&request, &hash)
        .await
        .map_err(ApiError::from)?
    {
        return Ok(workspace);
    }
    let assessment_workspace = repo
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let source_workspace = source_library(&container)
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let library = crate::features::learning::source_library::LearningSourceLibraryRepository::new(
        container.db_pool().clone(),
    );
    let mut sources = Vec::new();
    for version_id in &request.source_version_ids {
        let source = source_workspace
            .sources
            .iter()
            .find(|s| s.versions.iter().any(|v| &v.id == version_id))
            .ok_or_else(|| {
                ApiError::from(crate::shared::error::AppError::InvalidInput(
                    "Assessment source is not part of this program.".into(),
                ))
            })?;
        sources.push(
            library
                .get_version(&GetLearningSourceVersionRequestDto {
                    program_id: request.program_id.clone(),
                    source_id: source.id.clone(),
                    version_id: version_id.clone(),
                })
                .await
                .map_err(ApiError::from)?,
        );
    }
    let program = LearningRepository::new(container.db_pool().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let requested_outcomes: std::collections::HashSet<_> = request
        .requirements
        .iter()
        .map(|requirement| requirement.outcome_id.as_str())
        .collect();
    let module_ids: std::collections::HashSet<_> = assessment_workspace
        .outcomes
        .iter()
        .filter(|outcome| requested_outcomes.contains(outcome.id.as_str()))
        .filter_map(|outcome| outcome.module_id.as_deref())
        .collect();
    let lessons: Vec<_> = program
        .modules
        .iter()
        .filter(|module| module_ids.contains(module.id.as_str()))
        .flat_map(|module| module.lessons.iter())
        .cloned()
        .collect();
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    let candidates = crate::features::learning::assessment_generation::generate(
        llm.as_ref(),
        &request,
        &assessment_workspace.outcomes,
        &sources,
        &lessons,
    )
    .await
    .map_err(ApiError::from)?;
    repo.create_blueprint(&request, &candidates, llm.model_name(), &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn start_learning_assessment_form(
    request: StartLearningAssessmentFormRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    assessment_repo(&container)
        .start_form(&request, &hash)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn get_learning_assessment_form(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    assessment_repo(&container)
        .get_form(&id)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn save_learning_assessment_response(
    request: SaveLearningAssessmentResponseRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    assessment_repo(&container)
        .save_response(&request, &hash)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn interrupt_learning_assessment_form(
    request: MutateLearningAssessmentFormRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    assessment_repo(&container)
        .interrupt(&request, &hash)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn submit_learning_assessment_form(
    request: MutateLearningAssessmentFormRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let repo = assessment_repo(&container);
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    if let Some(form) = repo
        .preflight_submit(&request, &hash)
        .await
        .map_err(ApiError::from)?
    {
        return Ok(form);
    }
    let form = repo
        .get_form(&request.form_id)
        .await
        .map_err(ApiError::from)?;
    if form.items.iter().any(|item| {
        item.selected_index.is_none()
            && item
                .text_response
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
            && item.artifact_json.is_none()
            && item.ordered_values.is_empty()
    }) {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Save a response for every assessment item before submitting.".into(),
            ),
        ));
    }
    let (graded, model) = if form.items.iter().any(|i| {
        matches!(
            i.format,
            LearningItemFormat::Explanation | LearningItemFormat::Artifact
        )
    }) {
        let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
        match crate::features::learning::assessment_generation::grade_open(llm.as_ref(), &form)
            .await
        {
            Ok(v) => (v.0, Some(v.1)),
            Err(_) => {
                let mut out = std::collections::HashMap::new();
                for item in form.items.iter().filter(|i| {
                    matches!(
                        i.format,
                        LearningItemFormat::Explanation | LearningItemFormat::Artifact
                    )
                }) {
                    out.insert(item.id.clone(),(LearningAssessmentGradeStatus::Uncertain,item.rubric.iter().map(|c|LearningAssessmentCriterionResultDto{criterion_id:c.id.clone(),score:None,max_points:c.max_points,observation:"Response could not be graded reliably; review with a person.".into(),artifact_quote:None}).collect()));
                }
                (out, Some(llm.model_name().into()))
            }
        }
    } else {
        (std::collections::HashMap::new(), None)
    };
    repo.submit(&request, &hash, graded, model)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn accept_learning_follow_up(
    request: DecideLearningFollowUpRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentWorkspaceDto, ApiError> {
    let hash = source_request_hash(&(&request, true)).map_err(ApiError::from)?;
    assessment_repo(&container)
        .decide_follow_up(&request, true, &hash)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn dismiss_learning_follow_up(
    request: DecideLearningFollowUpRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentWorkspaceDto, ApiError> {
    let hash = source_request_hash(&(&request, false)).map_err(ApiError::from)?;
    assessment_repo(&container)
        .decide_follow_up(&request, false, &hash)
        .await
        .map_err(ApiError::from)
}
