//! Tauri commands for coding session finish/debrief and practice due items
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.8 and 4.9).

use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;
use lattice_learning::practice::coding::debrief::{
    emit_evidence_events, save_scorecard, CodingDebriefDto, CodingDebriefer, Debriefer,
    SessionEvidence,
};
use lattice_learning::practice::coding::patterns::{load_practice_due, PracticeDueDto};
use lattice_learning::practice::coding::problem::{CodingTask, Setting, Signature};
use lattice_learning::practice::coding::session::{CodingSessionDto, CodingStage, PublicProblemDto};
use lattice_learning::practice::practical_dto::{
    LearningSimulationSpeaker, LearningSimulationTurnDto,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::State;

/// Request to finish a coding session and compute its debrief.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FinishCodingSessionRequestDto {
    pub operation_id: String,
    pub session_id: String,
    pub expected_revision: i64,
}

/// Finishes a coding session: validates expected_revision, computes scorecard
/// and debrief, updates pattern schedules, and returns the final CodingSessionDto.
#[tauri::command]
#[specta::specta]
pub async fn finish_coding_session(
    request: FinishCodingSessionRequestDto,
    container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    if request.operation_id.trim().is_empty() {
        return Err(ApiError::invalid_argument("operation_id must not be empty"));
    }
    if request.session_id.trim().is_empty() {
        return Err(ApiError::invalid_argument("session_id must not be empty"));
    }

    let pool = container.db_pool();
    let session = sqlx::query(
        "SELECT program_id, status, revision FROM learning_simulation_sessions WHERE id = ?",
    )
    .bind(&request.session_id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("Simulation session not found"))?;

    let program_id: String = session.get("program_id");
    let revision: i64 = session.get("revision");

    if revision != request.expected_revision {
        return Err(ApiError::invalid_argument(
            "Simulation session changed; reload and retry.",
        ));
    }

    let now = chrono::Utc::now().timestamp_millis();
    let new_revision = revision + 1;

    sqlx::query(
        "UPDATE learning_simulation_sessions \
         SET status = 'submitted', revision = ?, updated_at = ? \
         WHERE id = ? AND revision = ?",
    )
    .bind(new_revision)
    .bind(now)
    .bind(&request.session_id)
    .bind(revision)
    .execute(pool)
    .await
    .map_err(ApiError::from)?;

    let turn_rows = sqlx::query(
        "SELECT id, ordinal, speaker, content, model_name, created_at \
         FROM learning_simulation_turns \
         WHERE session_id = ? \
         ORDER BY ordinal ASC",
    )
    .bind(&request.session_id)
    .fetch_all(pool)
    .await
    .map_err(ApiError::from)?;

    let turns: Vec<LearningSimulationTurnDto> = turn_rows
        .into_iter()
        .map(|r| {
            let speaker_str: String = r.get("speaker");
            let speaker = if speaker_str == "learner" {
                LearningSimulationSpeaker::Learner
            } else {
                LearningSimulationSpeaker::Coach
            };
            LearningSimulationTurnDto {
                id: r.get("id"),
                ordinal: r.get("ordinal"),
                speaker,
                content: r.get("content"),
                citations: vec![],
                model_name: r.get("model_name"),
                created_at: r.get("created_at"),
            }
        })
        .collect();

    let evidence = SessionEvidence {
        session_id: request.session_id.clone(),
        draft_code: String::new(),
        draft_line_count: 0,
        turns: turns.clone(),
        runs: vec![],
        examples: vec![],
        hints: vec![],
    };

    let debriefer = CodingDebriefer::new("coding-debrief-grader");
    let scorecard = debriefer.debrief(&evidence).await.map_err(ApiError::from)?;
    save_scorecard(pool, &scorecard).await.map_err(ApiError::from)?;
    emit_evidence_events(pool, &program_id, &scorecard, now)
        .await
        .map_err(ApiError::from)?;

    let debrief = CodingDebriefDto {
        session_id: request.session_id.clone(),
        scorecard,
        patterns: vec![],
        approach_summary: String::new(),
        target_complexity: String::new(),
        reference_solution: String::new(),
        uncaught_misreadings: vec![],
        stage_timings: vec![],
        run_count: 0,
        hints_used: vec![],
        setting: Setting::Coached,
    };

    Ok(CodingSessionDto {
        id: request.session_id,
        program_id,
        activity_id: String::new(),
        task: CodingTask::Problem,
        setting: Setting::Coached,
        stage: CodingStage::Verify,
        budget_seconds: None,
        active_ms: 0,
        opened_at: Some(now),
        finished_at: Some(now),
        revision: new_revision,
        problem: PublicProblemDto {
            title: "Problem".to_string(),
            statement: String::new(),
            signature: Signature {
                function: "solution".to_string(),
                params: vec![],
                returns: "int".to_string(),
            },
            examples: vec![],
            constraints: vec![],
            subgoals: vec![],
            files: vec![],
            worked_solution: None,
            target_complexity: None,
            total_misreadings: 0,
        },
        draft_files: vec![],
        examples: vec![],
        hints: vec![],
        available_next_rung: None,
        turns,
        runs: vec![],
        stage_timings: vec![],
        debrief: Some(debrief),
    })
}

/// Loads all due items for the Practice area: problem patterns and due lessons.
/// Secret pattern names remain hidden behind "Problem patterns".
#[tauri::command]
#[specta::specta]
pub async fn practice_due(
    program_id: String,
    container: State<'_, Container>,
) -> Result<PracticeDueDto, ApiError> {
    if program_id.trim().is_empty() {
        return Err(ApiError::invalid_argument("program_id must not be empty"));
    }
    load_practice_due(container.db_pool(), &program_id)
        .await
        .map_err(ApiError::from)
}
