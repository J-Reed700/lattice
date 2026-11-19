use super::*;
use lattice_core::shared::error::AppError;
use lattice_learning::practice::assistance_policy::{
    written_help, written_level, AssistancePolicy, Help, HelpLevel, WrittenAssistance,
};
use lattice_learning::practice::session::kind::Kinds;

pub(super) fn practice_repo(
    container: &Container,
) -> crate::features::learning::practice_repository::LearningPracticeRepository {
    crate::features::learning::practice_repository::LearningPracticeRepository::new(
        container.database().clone(),
    )
}

pub(super) fn practice_request_hash<T: serde::Serialize>(
    request: &T,
) -> lattice_core::shared::error::Result<String> {
    let mut value = serde_json::to_value(request)
        .map_err(|e| lattice_core::shared::error::AppError::Serialization(e.to_string()))?;
    if let Some(fields) = value.as_object_mut() {
        fields.remove("operationId");
        fields.remove("expectedRevision");
        fields.remove("expectedProgramRevision");
    }
    let bytes = serde_json::to_vec(&value)
        .map_err(|e| lattice_core::shared::error::AppError::Serialization(e.to_string()))?;
    Ok(lattice_core::shared::hash::sha256_hex(bytes))
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_practice_workspace(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    practice_repo(&container)
        .workspace(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_practice_session(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningPracticeSessionDto, ApiError> {
    practice_repo(&container)
        .get_session(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn start_learning_practice_session(
    request: StartLearningPracticeSessionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .start(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_practice_artifact(
    request: SaveLearningPracticeArtifactRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .save_artifact(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn change_learning_practice_mode(
    request: ChangeLearningPracticeModeRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .change_mode(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn open_learning_practice_source(
    request: OpenLearningPracticeSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .open_source(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn request_learning_tutor_response(
    request: RequestLearningTutorResponseRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let repo = practice_repo(&container);
    let payload_hash = practice_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.session_id,
            "tutor",
            &payload_hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    let session = repo
        .validate_live_session(
            &request.program_id,
            &request.session_id,
            request.expected_revision,
            false,
        )
        .await
        .map_err(ApiError::from)?;
    crate::features::learning::practice_repository::validate_tutor_bounds(
        &request.prompt,
        "placeholder response",
    )
    .map_err(ApiError::from)?;
    match (&request.request_kind, &request.hint_level) {
        (LearningTutorRequestKind::Hint, None) => {
            return Err(ApiError::from(
                lattice_core::shared::error::AppError::InvalidInput(
                    "Choose an explicit hint level.".into(),
                ),
            ));
        }
        (LearningTutorRequestKind::Hint, Some(_))
            if written_level(&session.summary.mode) == HelpLevel::HintsOnly
                && !WrittenAssistance.allows(
                    written_help(&request.request_kind, request.hint_level.as_ref()),
                    HelpLevel::HintsOnly,
                ) =>
        {
            return Err(ApiError::from(
                lattice_core::shared::error::AppError::InvalidInput(
                    "Worked explanations are available only in Explore mode.".into(),
                ),
            ));
        }
        (LearningTutorRequestKind::Question | LearningTutorRequestKind::Critique, Some(_)) => {
            return Err(ApiError::from(
                lattice_core::shared::error::AppError::InvalidInput(
                    "Hint levels apply only to hint requests.".into(),
                ),
            ));
        }
        _ => {}
    }
    let sources = repo
        .frozen_sources(&request.program_id, &session.summary.source_version_ids)
        .await
        .map_err(ApiError::from)?;
    let pin = learning_model(
        &container,
        model_requirements::practice(),
        "Practice tutoring",
    )
    .await?;
    let llm = &pin.llm;
    let generated = crate::features::learning::practice_generation::tutor(
        llm.as_ref(),
        &session,
        &request,
        &sources,
    )
    .await
    .map_err(ApiError::from)?;
    let turn = crate::features::learning::practice_repository::TutorTurnWrite {
        id: uuid::Uuid::new_v4().to_string(),
        operation_id: request.operation_id.clone(),
        payload_hash,
        prompt: request.prompt.clone(),
        request_kind: request.request_kind.clone(),
        response: generated.response,
        hint_level: request.hint_level.clone(),
        citations: generated.citations,
        proposals: generated
            .proposals
            .into_iter()
            .map(|p| LearningPracticeProposalDto {
                id: uuid::Uuid::new_v4().to_string(),
                kind: p.kind,
                text: p.text,
                evidence_quote: p.evidence_quote,
                tutor_turn_id: None,
                status: LearningPracticeProposalStatus::Pending,
                created_at: chrono::Utc::now().timestamp_millis(),
                decided_at: None,
            })
            .collect(),
        model_name: llm.model_name().to_string(),
    };
    repo.commit_tutor_turn(
        &request.program_id,
        &request.session_id,
        request.expected_revision,
        turn,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn reveal_learning_practice_solution(
    request: RevealLearningPracticeSolutionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let repo = practice_repo(&container);
    let payload_hash = practice_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.session_id,
            "reveal_solution",
            &payload_hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    let session = repo
        .validate_live_session(
            &request.program_id,
            &request.session_id,
            request.expected_revision,
            false,
        )
        .await
        .map_err(ApiError::from)?;
    if !WrittenAssistance.allows(Help::WorkedSolution, written_level(&session.summary.mode)) {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
                "Worked solutions are available only in Explore mode.".into(),
            ),
        ));
    }
    let sources = repo
        .frozen_sources(&request.program_id, &session.summary.source_version_ids)
        .await
        .map_err(ApiError::from)?;
    let pin = learning_model(
        &container,
        model_requirements::practice(),
        "Writing a worked solution",
    )
    .await?;
    let llm = &pin.llm;
    let generated =
        crate::features::learning::practice_generation::solution(llm.as_ref(), &session, &sources)
            .await
            .map_err(ApiError::from)?;
    repo.commit_solution(
        &request,
        &payload_hash,
        &generated.solution,
        &generated.citations,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn submit_learning_practice_attempt(
    request: SubmitLearningPracticeAttemptRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let repo = practice_repo(&container);
    let payload_hash = practice_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.session_id,
            "submit",
            &payload_hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    let session = repo
        .validate_live_session(
            &request.program_id,
            &request.session_id,
            request.expected_revision,
            true,
        )
        .await
        .map_err(ApiError::from)?;
    let pin = learning_model(
        &container,
        model_requirements::practice(),
        "Grading this practice",
    )
    .await?;
    let llm = &pin.llm;
    let (grade_status, criteria) =
        match crate::features::learning::practice_generation::grade(llm.as_ref(), &session).await {
            Ok(value) => value,
            // Only invalid feedback goes to a person; any other error leaves
            // the attempt unsaved so the learner can send it again.
            Err(error)
                if !matches!(
                    error,
                    AppError::ModelAnswerUnusable(_) | AppError::AnswerCutOff(_)
                ) =>
            {
                return Err(ApiError::from(error))
            }
            Err(_) => {
                let criteria = session
                    .rubric
                    .iter()
                    .map(|c| {
                        LearningPracticeCriterionResultDto {
                    criterion_id: c.id.clone(),
                    dimension: c.dimension.clone(),
                    score: None,
                    max_points: c.max_points,
                    observation:
                        "Evidence could not be graded reliably; review this response with a person."
                            .into(),
                    evidence_quote: None,
                }
                    })
                    .collect();
                (LearningPracticeGradeStatus::Uncertain, criteria)
            }
        };
    let mut assistance_kinds = Vec::new();
    for event in &session.assistance {
        if !assistance_kinds.contains(&event.kind) {
            assistance_kinds.push(event.kind.clone());
        }
    }
    let evidence = session
        .rubric
        .iter()
        .map(|c| {
            let r = criteria.iter().find(|r| r.criterion_id == c.id);
            LearningPracticeEvidenceEventDto {
                dimension: c.dimension.clone(),
                observed: r.and_then(|x| x.score).is_some_and(|score| score > 0),
                observation: r
                    .map(|x| x.observation.clone())
                    .unwrap_or_else(|| "No criterion-specific evidence was returned.".into()),
                evidence_quote: r.and_then(|x| x.evidence_quote.clone()),
                assistance_kinds: assistance_kinds.clone(),
            }
        })
        .collect();
    let write = crate::features::learning::practice_repository::SubmissionWrite {
        payload_hash,
        operation_id: request.operation_id.clone(),
        grade_status,
        criteria,
        evidence,
        grader_model: llm.model_name().to_string(),
    };
    repo.submit(&request, write, &session.rubric)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn accept_learning_practice_proposal(
    request: DecideLearningPracticeProposalRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .decide_proposal(&request, true, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn reject_learning_practice_proposal(
    request: DecideLearningPracticeProposalRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .decide_proposal(&request, false, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_practice_profile(
    request: GetLearningPracticeProfileRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeProfileDto, ApiError> {
    practice_repo(&container)
        .get_profile(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_practice_profile(
    request: SaveLearningPracticeProfileRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeProfileDto, ApiError> {
    practice_repo(&container)
        .save_profile(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_practice_fit(
    request: PracticeFitRequestDto,
    container: State<'_, Container>,
    kinds: State<'_, Kinds>,
) -> Result<PracticeFitDto, ApiError> {
    crate::features::learning::practice_fit::practice_fit(
        &practice_repo(&container),
        &kinds,
        &request.program_id,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn check_practice_fit(
    request: PracticeFitRequestDto,
    container: State<'_, Container>,
    kinds: State<'_, Kinds>,
) -> Result<PracticeFitDto, ApiError> {
    let repo = practice_repo(&container);
    let known =
        crate::features::learning::practice_fit::practice_fit(&repo, &kinds, &request.program_id)
            .await
            .map_err(ApiError::from)?;
    if known
        .kinds
        .iter()
        .all(|entry| entry.fit != KindFit::Checking)
    {
        return Ok(known);
    }
    let pin = learning_model(
        &container,
        model_requirements::practice(),
        "Checking which kinds of practice the course has",
    )
    .await?;
    crate::features::learning::practice_fit::check_practice_fit(
        &repo,
        &kinds,
        pin.llm.as_ref(),
        &request.program_id,
    )
    .await
    .map_err(ApiError::from)
}
