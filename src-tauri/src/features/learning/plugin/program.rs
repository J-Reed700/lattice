use super::*;

#[tauri::command]
#[specta::specta]
pub async fn list_learning_programs(
    container: State<'_, Container>,
) -> Result<Vec<LearningProgramSummaryDto>, ApiError> {
    LearningRepository::new(container.db_pool().clone())
        .list()
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn get_learning_program(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    LearningRepository::new(container.db_pool().clone())
        .get(&id)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn get_learning_lesson_evidence(
    program_id: String,
    lesson_id: String,
    container: State<'_, Container>,
) -> Result<Option<crate::features::learning::lesson_evidence::LearningLessonEvidenceDto>, ApiError>
{
    crate::features::learning::lesson_evidence::get(container.db_pool(), &program_id, &lesson_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_outline_evidence(
    program_id: String,
    container: State<'_, Container>,
) -> Result<
    Option<crate::features::learning::outline_evidence_view::LearningOutlineEvidenceDto>,
    ApiError,
> {
    crate::features::learning::outline_evidence_view::get(container.db_pool(), &program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn generate_learning_program(
    request: GenerateLearningProgramRequestDto,
    request_id: Option<String>,
    on_progress: tauri::ipc::Channel<
        crate::features::learning::outline_progress::LearningOutlineProgressDto,
    >,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    service::validate_request(&request).map_err(ApiError::from)?;
    let run = crate::features::learning::outline_progress::OutlineRun::register(
        request_id,
        move |update| {
            let _ = on_progress.send(update);
        },
    )
    .map_err(ApiError::from)?;
    let progress = &run.0;
    progress
        .run(async {
            progress
                .stage(crate::features::learning::outline_progress::OutlineStage::ReadingSources);
            let sources = crate::features::learning::sources::acquire(&container, &request).await?;
            progress.stage(crate::features::learning::outline_progress::OutlineStage::LoadingModel);
            let llm = container.get_or_load_llm().await?;
            progress.model(llm.model_name());
            service::generate_with_references_and_progress(
                &LearningRepository::new(container.db_pool().clone()),
                llm.as_ref(),
                request,
                sources.sources,
                &sources.references,
                progress,
                Some(container.web_service().as_ref()),
            )
            .await
        })
        .await
        .map_err(|error| progress.api_error(error))
}

#[tauri::command]
#[specta::specta]
pub async fn repair_learning_outline(
    request: crate::features::learning::outline_draft::RepairLearningOutlineRequestDto,
    request_id: Option<String>,
    on_progress: tauri::ipc::Channel<
        crate::features::learning::outline_progress::LearningOutlineProgressDto,
    >,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    uuid::Uuid::parse_str(&request.program_id).map_err(|_| {
        ApiError::from(crate::shared::error::AppError::InvalidInput(
            "Invalid program ID".into(),
        ))
    })?;
    let run = crate::features::learning::outline_progress::OutlineRun::register(
        request_id,
        move |update| {
            let _ = on_progress.send(update);
        },
    )
    .map_err(ApiError::from)?;
    let progress = &run.0;
    progress
        .run(async {
            progress.stage(crate::features::learning::outline_progress::OutlineStage::LoadingModel);
            let llm = container.get_or_load_llm().await?;
            progress.model(llm.model_name());
            crate::features::learning::outline_draft::repair_saved(
                &LearningRepository::new(container.db_pool().clone()),
                llm.as_ref(),
                &request,
                progress,
                Some(container.web_service().as_ref()),
            )
            .await
        })
        .await
        .map_err(|error| progress.api_error(error))
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_learning_outline(request_id: String) -> Result<bool, ApiError> {
    uuid::Uuid::parse_str(&request_id).map_err(|_| {
        ApiError::from(crate::shared::error::AppError::InvalidInput(
            "Invalid outline request ID".into(),
        ))
    })?;
    Ok(crate::features::learning::outline_progress::cancel(
        &request_id,
    ))
}
#[tauri::command]
#[specta::specta]
pub async fn accept_learning_program(
    request: AcceptLearningProgramRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    service::accept(
        &LearningRepository::new(container.db_pool().clone()),
        request,
    )
    .await
    .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn prepare_learning_lesson(
    request: PrepareLearningLessonRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    let program = LearningRepository::new(container.db_pool().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    if program.summary.revision != request.expected_revision {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Program changed; reload and retry.".into(),
            ),
        ));
    }
    if program.summary.status != LearningProgramStatus::Active {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Accept this program before preparing lessons.".into(),
            ),
        ));
    }
    let operation_id = crate::features::learning::curriculum_repository::stable_job_operation_id(
        &request.program_id,
        &request.lesson_id,
        request.expected_revision,
    );
    let job_request = StartLearningGenerationJobRequestDto {
        operation_id,
        program_id: request.program_id.clone(),
        expected_revision: request.expected_revision,
        kind: crate::features::learning::curriculum::LearningGenerationJobKind::LessonPreparation,
        request_json: serde_json::json!({"lessonIds":[request.lesson_id]}).to_string(),
        progress_total: 1,
    };
    let job_repo =
        crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
            container.db_pool().clone(),
        );
    let job = job_repo
        .start_lesson_job(&job_request)
        .await
        .map_err(ApiError::from)?;
    generation_worker(&container).spawn(job.id.clone());
    // Acknowledge the durable request immediately. Source refresh, model calls,
    // and publication belong to the worker; the UI observes the saved job.
    LearningRepository::new(container.db_pool().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn complete_learning_lesson(
    request: CompleteLearningLessonRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    service::complete(
        &LearningRepository::new(container.db_pool().clone()),
        request,
    )
    .await
    .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn submit_learning_attempt(
    request: SubmitLearningAttemptRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    service::submit(
        &LearningRepository::new(container.db_pool().clone()),
        request,
    )
    .await
    .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn delete_learning_program(
    id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    uuid::Uuid::parse_str(&id).map_err(|_| {
        ApiError::from(crate::shared::error::AppError::InvalidInput(
            "Invalid program ID".into(),
        ))
    })?;
    LearningRepository::new(container.db_pool().clone())
        .delete(&id)
        .await
        .map_err(ApiError::from)
}
