use super::*;

#[tauri::command]
#[specta::specta]
pub async fn start_learning_generation_job(
    request: StartLearningGenerationJobRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let repo = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.database().clone(),
    );
    let job = repo.start_job(&request).await.map_err(ApiError::from)?;
    container.jobs().submitted(&job.id).await;
    Ok(job)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_learning_generation_job(
    request: LearningGenerationJobActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    // Trip the in-memory cancellation token immediately before waiting on SQLite
    // so an active worker stops starting new LLM/DB steps while write turns drain.
    container.jobs().begin_cancel(&request.job_id);
    let repo = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.database().clone(),
    );
    match repo.cancel_job(&request).await {
        Ok(job) => {
            container.jobs().finish_cancel(&job.id).await;
            Ok(job)
        }
        Err(error) => {
            container.jobs().abort_cancel(&request.job_id);
            Err(ApiError::from(error))
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn retry_learning_generation_job(
    request: LearningGenerationJobActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let repo = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.database().clone(),
    );
    let job = repo.retry_job(&request).await.map_err(ApiError::from)?;
    container.jobs().submitted(&job.id).await;
    Ok(job)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_generation_job(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.database().clone(),
    )
    .job(&id)
    .await
    .map_err(ApiError::from)
}

pub(super) fn generation_worker(
    container: &Container,
    runtimes: BuiltinRuntimes,
    course_setups: crate::features::learning::course_setup::CourseSetups,
) -> crate::features::learning::generation_jobs::LessonGenerationWorker {
    let model_container = container.clone();
    let source_deps = source_deps(container);
    crate::features::learning::generation_jobs::LessonGenerationWorker {
        course_setups,
        runtimes,
        research_web: Some(container.web_service()),
        verifier: Some(super::audit::container_verifier(container)),
        database: container.database().clone(),
        courses: container.course_indexes().cloned(),
        load_llm: std::sync::Arc::new(move || {
            let container = model_container.clone();
            Box::pin(async move {
                container
                    .learning_llm(
                        crate::features::learning::model_requirements::lesson_preparation(),
                    )
                    .await
            })
        }),
        refresh_sources: std::sync::Arc::new(move |program_id| {
            let deps = source_deps.clone();
            Box::pin(async move {
                crate::features::learning::sources::ensure_before_use_sources(&deps, &program_id)
                    .await
            })
        }),
    }
}
