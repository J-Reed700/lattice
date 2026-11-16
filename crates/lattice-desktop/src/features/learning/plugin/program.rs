use super::*;
use crate::features::learning::patient_model::{patiently, PatientModel};
use lattice_core::shared::runtime::jobs::Backoff;

#[tauri::command]
#[specta::specta]
pub async fn list_learning_programs(
    container: State<'_, Container>,
) -> Result<Vec<LearningProgramSummaryDto>, ApiError> {
    LearningRepository::new(container.database().clone())
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
    LearningRepository::new(container.database().clone())
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
    crate::features::learning::lesson_evidence::get(container.database(), &program_id, &lesson_id)
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
    crate::features::learning::outline_evidence_view::get(container.database(), &program_id)
        .await
        .map_err(ApiError::from)
}

/// Drafts a course outline: the body of `generate_learning_program`.
pub(crate) async fn generate_outline(
    container: &Container,
    request: GenerateLearningProgramRequestDto,
    request_id: Option<String>,
    on_progress: impl Fn(crate::features::learning::outline_progress::LearningOutlineProgressDto)
        + Send
        + Sync
        + 'static,
) -> Result<LearningProgramDto, ApiError> {
    service::validate_request(&request).map_err(ApiError::from)?;
    let run = container
        .outline_run_registry()
        .register(request_id, on_progress)
        .map_err(ApiError::from)?;
    let progress = &run.progress;
    progress
        .run(async {
            progress
                .stage(crate::features::learning::outline_progress::OutlineStage::ReadingSources);
            let deps = source_deps(container);
            let sources = crate::features::learning::sources::acquire(&deps, &request).await?;
            progress.stage(crate::features::learning::outline_progress::OutlineStage::LoadingModel);
            // A temporary model outage is waited out, call by call; see
            // `patient_model`. A refusal, such as no Learning model or too
            // small a window, is not an outage and fails once.
            let pin = patiently(progress.as_ref(), Backoff::default(), |_| {
                container.learning_llm(model_requirements::outline())
            })
            .await?;
            progress.model(pin.llm.model_name());
            model_requirements::ensure_window(
                pin.llm.as_ref(),
                &model_requirements::outline(),
                "Drafting this course outline",
            )?;
            let llm = PatientModel::new(pin.llm.as_ref(), progress.as_ref());
            // Supported outline assertions are weighed for their sources'
            // standing, as a lesson's claims are.
            crate::features::learning::content_verification::with_outline_trust(
                container.database().clone(),
                service::generate_with_references_and_progress(
                    &LearningRepository::new(container.database().clone()),
                    &container.draft_run_registry(),
                    &llm,
                    request,
                    sources.sources,
                    &sources.references,
                    progress,
                    Some(container.web().as_ref()),
                ),
            )
            .await
        })
        .await
        .map_err(|error| progress.api_error(error))
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
    generate_outline(&container, request, request_id, move |update| {
        let _ = on_progress.send(update);
    })
    .await
}

/// Repairs a saved outline: the body of `repair_learning_outline`.
pub(crate) async fn repair_outline(
    container: &Container,
    request: crate::features::learning::outline_draft::RepairLearningOutlineRequestDto,
    request_id: Option<String>,
    on_progress: impl Fn(crate::features::learning::outline_progress::LearningOutlineProgressDto)
        + Send
        + Sync
        + 'static,
) -> Result<LearningProgramDto, ApiError> {
    uuid::Uuid::parse_str(&request.program_id).map_err(|_| {
        ApiError::from(lattice_core::shared::error::AppError::InvalidInput(
            "Invalid program ID".into(),
        ))
    })?;
    let run = container
        .outline_run_registry()
        .register(request_id, on_progress)
        .map_err(ApiError::from)?;
    let progress = &run.progress;
    progress
        .run(async {
            progress.stage(crate::features::learning::outline_progress::OutlineStage::LoadingModel);
            let pin = patiently(progress.as_ref(), Backoff::default(), |_| {
                container.learning_llm(model_requirements::outline())
            })
            .await?;
            progress.model(pin.llm.model_name());
            model_requirements::ensure_window(
                pin.llm.as_ref(),
                &model_requirements::outline(),
                "Repairing this course outline",
            )?;
            let llm = PatientModel::new(pin.llm.as_ref(), progress.as_ref());
            crate::features::learning::content_verification::with_outline_trust(
                container.database().clone(),
                crate::features::learning::outline_draft::repair_saved(
                    &LearningRepository::new(container.database().clone()),
                    &container.draft_run_registry(),
                    &llm,
                    &request,
                    progress,
                    Some(container.web().as_ref()),
                ),
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
    repair_outline(&container, request, request_id, move |update| {
        let _ = on_progress.send(update);
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_learning_outline(
    request_id: String,
    container: State<'_, Container>,
) -> Result<bool, ApiError> {
    uuid::Uuid::parse_str(&request_id).map_err(|_| {
        ApiError::from(lattice_core::shared::error::AppError::InvalidInput(
            "Invalid outline request ID".into(),
        ))
    })?;
    Ok(container.outline_run_registry().cancel(&request_id))
}
/// Accepts a drafted course: the body of `accept_learning_program`.
pub(crate) async fn accept_program(
    container: &Container,
    request: AcceptLearningProgramRequestDto,
) -> Result<LearningProgramDto, ApiError> {
    service::accept(
        &LearningRepository::new(container.database().clone()),
        &container.draft_run_registry(),
        request,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn accept_learning_program(
    request: AcceptLearningProgramRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    accept_program(&container, request).await
}
#[tauri::command]
#[specta::specta]
pub async fn prepare_learning_lesson(
    request: PrepareLearningLessonRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    let program = LearningRepository::new(container.database().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    if program.summary.revision != request.expected_revision {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::Conflict(
                "Program changed; reload and retry.".into(),
            ),
        ));
    }
    if program.summary.status != LearningProgramStatus::Active {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
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
            container.database().clone(),
        );
    let job = job_repo
        .start_lesson_job(&job_request)
        .await
        .map_err(ApiError::from)?;
    container.jobs().submitted(&job.id).await;
    // Acknowledge the durable request immediately. Source refresh, model calls,
    // and publication belong to the worker; the UI observes the saved job.
    LearningRepository::new(container.database().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)
}

/// Queues a job for each unprepared lesson of a module: the body of `prepare_learning_module`.
pub(crate) async fn prepare_module(
    container: &Container,
    request: PrepareLearningModuleRequestDto,
) -> Result<LearningProgramDto, ApiError> {
    let operation_id = crate::features::learning::curriculum_repository::stable_job_operation_id(
        &request.program_id,
        &format!("module:{}", request.module_id),
        request.expected_revision,
    );
    let jobs = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.database().clone(),
    )
    .start_module_jobs(&StartLearningModuleJobsRequestDto {
        operation_id,
        program_id: request.program_id.clone(),
        module_id: request.module_id,
        expected_revision: request.expected_revision,
    })
    .await
    .map_err(ApiError::from)?;
    for job in &jobs {
        container.jobs().submitted(&job.id).await;
    }
    LearningRepository::new(container.database().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn prepare_learning_module(
    request: PrepareLearningModuleRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    prepare_module(&container, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn prepare_learning_ahead(
    request: PrepareLearningAheadRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    let jobs = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.database().clone(),
    )
    .start_jobs_ahead(&request.program_id, &request.lesson_id)
    .await
    .map_err(ApiError::from)?;
    for job in &jobs {
        container.jobs().submitted(&job.id).await;
    }
    LearningRepository::new(container.database().clone())
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
        &LearningRepository::new(container.database().clone()),
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
        ApiError::from(lattice_core::shared::error::AppError::InvalidInput(
            "Invalid program ID".into(),
        ))
    })?;
    LearningRepository::new(container.database().clone())
        .delete(&id)
        .await
        .map_err(ApiError::from)?;
    // The course's evidence index goes with it (§3.4).
    if let Some(courses) = container.course_indexes() {
        courses.delete(&id).await;
    }
    Ok(())
}
