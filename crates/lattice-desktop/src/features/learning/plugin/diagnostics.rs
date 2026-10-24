use super::*;

#[tauri::command]
#[specta::specta]
pub async fn start_learning_diagnostic(
    request: StartLearningDiagnosticRequestDto,
    container: State<'_, Container>,
) -> Result<LearningDiagnosticAttemptDto, ApiError> {
    let repo = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    );
    if let Some(replay) = repo
        .diagnostic_replay(
            &request.operation_id,
            &request.program_id,
            "start",
            &request,
        )
        .await
        .map_err(ApiError::from)?
    {
        return Ok(replay);
    }
    let program = LearningRepository::new(container.db_pool().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    if program.summary.revision != request.expected_revision
        || program.summary.status != LearningProgramStatus::Active
    {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidState(
                "The course changed. Reload before checking your starting point.".into(),
            ),
        ));
    }
    repo.plan(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let outcomes = assessment_repo(&container)
        .module_outcomes(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let pin = learning_model(
        &container,
        model_requirements::diagnostics(),
        "Writing the starting-point check",
    )
    .await?;
    let llm = &pin.llm;
    // Whole captured texts, not the program's stored previews.
    let sources = LearningRepository::new(container.db_pool().clone())
        .verification_sources(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let tasks = crate::features::learning::diagnostic_generation::author(
        llm.as_ref(),
        &program,
        &outcomes,
        &sources,
    )
    .await
    .map_err(ApiError::from)?;
    repo.start_diagnostic_authored(&request, Some(&tasks))
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn submit_learning_diagnostic(
    request: SubmitLearningDiagnosticRequestDto,
    container: State<'_, Container>,
) -> Result<LearningDiagnosticAttemptDto, ApiError> {
    let repo = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    );
    if let Some(replay) = repo
        .diagnostic_replay(
            &request.operation_id,
            &request.program_id,
            "submit",
            &request,
        )
        .await
        .map_err(ApiError::from)?
    {
        return Ok(replay);
    }
    let attempt = repo
        .diagnostic_for_program(&request.program_id, &request.diagnostic_id)
        .await
        .map_err(ApiError::from)?;
    let tasks = repo
        .diagnostic_tasks(&request.program_id, &request.diagnostic_id)
        .await
        .map_err(ApiError::from)?;
    if tasks.is_empty() || request.save_only {
        return repo
            .submit_diagnostic(&request)
            .await
            .map_err(ApiError::from);
    }
    let program = LearningRepository::new(container.db_pool().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    if attempt.status != LearningDiagnosticStatus::Active
        || request.expected_diagnostic_revision != Some(attempt.revision)
        || request.expected_revision != program.summary.revision
        || program.summary.status != LearningProgramStatus::Active
    {
        return Err(ApiError::from(lattice_core::shared::error::AppError::InvalidState("Your course or answers changed. Reload before submitting the starting-point check.".into())));
    }
    let pin = learning_model(
        &container,
        model_requirements::diagnostics(),
        "Scoring the starting-point check",
    )
    .await?;
    let llm = &pin.llm;
    let findings = crate::features::learning::diagnostic_generation::evaluate(
        llm.as_ref(),
        &attempt,
        &tasks,
        &request.responses,
    )
    .await
    .map_err(ApiError::from)?;
    repo.submit_diagnostic_evaluated(&request, Some(&findings))
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn skip_learning_diagnostic(
    request: SkipLearningDiagnosticRequestDto,
    container: State<'_, Container>,
) -> Result<LearningDiagnosticAttemptDto, ApiError> {
    crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    )
    .skip_diagnostic(&request)
    .await
    .map_err(ApiError::from)
}
