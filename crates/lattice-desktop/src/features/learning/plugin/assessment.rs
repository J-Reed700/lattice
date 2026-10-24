use super::*;

// Quizzes and tests (docs/design/2026-10-23-quizzes-and-tests.md, section 6.3).

pub(super) fn assessment_repo(
    container: &Container,
) -> crate::features::learning::assessment_repository::LearningAssessmentRepository {
    crate::features::learning::assessment_repository::LearningAssessmentRepository::new(
        container.db_pool().clone(),
    )
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_quiz_overview(
    program_id: String,
    container: State<'_, Container>,
) -> Result<LearningQuizOverviewDto, ApiError> {
    assessment_repo(&container)
        .overview(&program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn update_learning_quiz_settings(
    request: UpdateLearningQuizSettingsRequestDto,
    container: State<'_, Container>,
) -> Result<LearningQuizOverviewDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    let repo = assessment_repo(&container);
    repo.update_settings(&request, &hash)
        .await
        .map_err(ApiError::from)?;
    repo.overview(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn prepare_learning_quiz_bank(
    request: PrepareLearningQuizBankRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let id = crate::features::learning::quiz_bank_jobs::queue_bank(
        container.db_pool(),
        &request.operation_id,
        &request.program_id,
        &request.scope,
        request.top_up,
    )
    .await
    .map_err(ApiError::from)?;
    container.jobs().submitted(&id).await;
    crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    )
    .job(&id)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn start_learning_quiz(
    request: StartLearningQuizRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    assessment_repo(&container)
        .start_quiz(&request, &hash)
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

/// Keyed answers are graded at once; written answers and short-answer
/// near-misses go to the grading job, which this starts.
#[tauri::command]
#[specta::specta]
pub async fn submit_learning_assessment_form(
    request: MutateLearningAssessmentFormRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    let (form, job) = assessment_repo(&container)
        .submit(&request, &hash)
        .await
        .map_err(ApiError::from)?;
    if let Some(job) = job {
        container.jobs().submitted(&job).await;
    }
    Ok(form)
}

/// Grade the answers still pending after a stopped or failed grading job.
#[tauri::command]
#[specta::specta]
pub async fn resume_learning_quiz_grading(
    request: ResumeLearningQuizGradingRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let repo = assessment_repo(&container);
    let Some(job) = repo
        .resume_grading(&request)
        .await
        .map_err(ApiError::from)?
    else {
        return repo
            .get_form(&request.form_id)
            .await
            .map_err(ApiError::from);
    };
    container.jobs().submitted(&job).await;
    repo.get_form(&request.form_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn contest_learning_quiz_grade(
    request: ContestLearningQuizGradeRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    let (form, job) = assessment_repo(&container)
        .contest(&request, &hash)
        .await
        .map_err(ApiError::from)?;
    if let Some(job) = job {
        container.jobs().submitted(&job).await;
    }
    Ok(form)
}

#[tauri::command]
#[specta::specta]
pub async fn override_learning_quiz_grade(
    request: OverrideLearningQuizGradeRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    assessment_repo(&container)
        .override_grade(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn report_learning_quiz_item(
    request: ReportLearningQuizItemRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    assessment_repo(&container)
        .report_item(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn add_learning_quiz_misses_to_recall(
    request: AddLearningQuizMissesToRecallRequestDto,
    container: State<'_, Container>,
) -> Result<LearningQuizMissesToRecallDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    assessment_repo(&container)
        .add_misses_to_recall(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn accept_learning_follow_up(
    request: DecideLearningFollowUpRequestDto,
    container: State<'_, Container>,
) -> Result<LearningQuizOverviewDto, ApiError> {
    let hash = source_request_hash(&(&request, true)).map_err(ApiError::from)?;
    let repo = assessment_repo(&container);
    repo.decide_follow_up(&request, true, &hash)
        .await
        .map_err(ApiError::from)?;
    repo.overview(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn dismiss_learning_follow_up(
    request: DecideLearningFollowUpRequestDto,
    container: State<'_, Container>,
) -> Result<LearningQuizOverviewDto, ApiError> {
    let hash = source_request_hash(&(&request, false)).map_err(ApiError::from)?;
    let repo = assessment_repo(&container);
    repo.decide_follow_up(&request, false, &hash)
        .await
        .map_err(ApiError::from)?;
    repo.overview(&request.program_id)
        .await
        .map_err(ApiError::from)
}

/// The job workers for quiz banks and grading, with the Learning model.
pub(super) fn quiz_workers(
    container: &Container,
) -> (
    crate::features::learning::quiz_bank_jobs::QuizBankWorker,
    crate::features::learning::quiz_bank_jobs::QuizGradingWorker,
) {
    let loader = |container: Container,
                  requirements: lattice_core::application::ports::llm_port::ModelRequirements|
     -> crate::features::learning::quiz_bank_jobs::ModelLoader {
        std::sync::Arc::new(move || {
            let container = container.clone();
            let requirements = requirements.clone();
            Box::pin(async move { container.learning_llm(requirements).await })
        })
    };
    (
        crate::features::learning::quiz_bank_jobs::QuizBankWorker {
            pool: container.db_pool().clone(),
            load_llm: loader(container.clone(), model_requirements::assessment()),
        },
        crate::features::learning::quiz_bank_jobs::QuizGradingWorker {
            pool: container.db_pool().clone(),
            load_llm: loader(container.clone(), model_requirements::assessment()),
        },
    )
}
