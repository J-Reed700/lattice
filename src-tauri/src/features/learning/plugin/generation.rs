use super::*;

#[tauri::command]
#[specta::specta]
pub async fn start_learning_generation_job(
    request: StartLearningGenerationJobRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let repo = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
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
    let job = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    )
    .cancel_job(&request)
    .await
    .map_err(ApiError::from)?;
    container.jobs().cancelled(&job.id).await;
    Ok(job)
}

#[tauri::command]
#[specta::specta]
pub async fn retry_learning_generation_job(
    request: LearningGenerationJobActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let repo = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
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
        container.db_pool().clone(),
    )
    .job(&id)
    .await
    .map_err(ApiError::from)
}

pub(super) fn generation_worker(
    container: &Container,
) -> crate::features::learning::generation_jobs::LessonGenerationWorker {
    let model_container = container.clone();
    let source_container = container.clone();
    let embedding_container = container.clone();
    crate::features::learning::generation_jobs::LessonGenerationWorker {
        research_web: Some(container.web_service()),
        pool: container.db_pool().clone(),
        load_embedding: std::sync::Arc::new(move || {
            let container = embedding_container.clone();
            Box::pin(async move { container.get_or_load_embedding().await.ok() })
        }),
        load_llm: std::sync::Arc::new(move || {
            let container = model_container.clone();
            Box::pin(async move { container.get_or_load_llm().await })
        }),
        refresh_sources: std::sync::Arc::new(move |program_id| {
            let container = source_container.clone();
            Box::pin(async move {
                crate::features::learning::sources::ensure_before_use_sources(
                    &container,
                    &program_id,
                )
                .await
            })
        }),
    }
}
