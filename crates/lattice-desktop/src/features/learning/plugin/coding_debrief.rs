//! Tauri commands for coding session finish/debrief and practice due items
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.8 and 4.9).

use crate::interfaces::di::Container;
use lattice_core::domain::learning::practice::problem_patterns::PROBLEM_PATTERNS;
use lattice_core::shared::ipc::ApiError;
use lattice_learning::practice::coding::debrief::{
    emit_evidence_events, save_scorecard, CodingDebriefDto, CodingDebriefer, Debriefer,
    PatternDebriefDto, SessionEvidence,
};
use lattice_learning::practice::coding::patterns::{
    load_practice_due, rate_problem_pattern, record_problem_pattern_review, PatternOutcomeInput,
    PracticeDueDto,
};
use lattice_learning::practice::coding::problem::{CodingTask, Setting, Signature};
use lattice_learning::practice::coding::session::{
    get_coding_session, load_problem_for_activity, CodingSessionDto, CodingStage, PublicProblemDto,
};
use lattice_learning::practice::practical_dto::{
    LearningPracticalCheckStatus, LearningSimulationSpeaker, LearningSimulationTurnDto,
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
    let now = chrono::Utc::now().timestamp_millis();

    if let Ok(mut coding_session) = get_coding_session(pool, &request.session_id).await {
        if coding_session.revision != request.expected_revision {
            return Err(ApiError::invalid_argument(
                "Coding session changed; reload and retry.",
            ));
        }

        let new_revision = coding_session.revision + 1;
        sqlx::query(
            "UPDATE learning_coding_sessions \
             SET finished_at = ?, revision = ? \
             WHERE id = ? AND revision = ?",
        )
        .bind(now)
        .bind(new_revision)
        .bind(&request.session_id)
        .bind(coding_session.revision)
        .execute(pool)
        .await
        .map_err(ApiError::from)?;

        let _ = sqlx::query(
            "UPDATE learning_simulation_sessions \
             SET status = 'submitted', revision = ?, updated_at = ? \
             WHERE id = ?",
        )
        .bind(new_revision)
        .bind(now)
        .bind(&request.session_id)
        .execute(pool)
        .await;

        let draft_code = coding_session
            .draft_files
            .first()
            .map(|f| f.content.clone())
            .unwrap_or_default();
        let draft_line_count = draft_code.lines().count() as u32;

        let evidence = SessionEvidence {
            session_id: request.session_id.clone(),
            draft_code,
            draft_line_count,
            turns: coding_session.turns.clone(),
            runs: coding_session.runs.clone(),
            examples: coding_session.examples.clone(),
            hints: coding_session.hints.clone(),
        };

        let debriefer = CodingDebriefer::new("coding-debrief-grader");
        let scorecard = debriefer.debrief(&evidence).await.map_err(ApiError::from)?;
        save_scorecard(pool, &scorecard)
            .await
            .map_err(ApiError::from)?;
        emit_evidence_events(pool, &coding_session.program_id, &scorecard, now)
            .await
            .map_err(ApiError::from)?;

        let all_cases_passed = coding_session.runs.last().is_some_and(|run| {
            !run.checks.is_empty()
                && run
                    .checks
                    .iter()
                    .all(|c| c.status == LearningPracticalCheckStatus::Passed)
        });
        let highest_hint_rung = coding_session.hints.iter().map(|h| h.rung).max();
        let rating = rate_problem_pattern(&PatternOutcomeInput {
            all_cases_passed,
            highest_hint_rung,
            solution_seen_before_pass: false,
        });

        let mut patterns = Vec::new();
        let mut target_complexity = coding_session
            .problem
            .target_complexity
            .clone()
            .unwrap_or_default();
        let mut reference_solution = coding_session
            .problem
            .worked_solution
            .clone()
            .unwrap_or_default();
        let mut uncaught_misreadings = Vec::new();

        if let Ok(problem) = load_problem_for_activity(pool, &coding_session.activity_id).await {
            target_complexity = problem.spec.target_complexity.clone();
            reference_solution = problem.reference.clone();
            let caught: std::collections::HashSet<&str> = coding_session
                .examples
                .iter()
                .flat_map(|ex| ex.catches.iter().map(String::as_str))
                .collect();
            uncaught_misreadings = problem
                .misreadings
                .iter()
                .filter(|m| !caught.contains(m.description.as_str()))
                .map(|m| m.description.clone())
                .collect();

            for pat_id in &problem.spec.patterns {
                if let Some(pat) = PROBLEM_PATTERNS.iter().find(|p| p.id == pat_id) {
                    let due_at = record_problem_pattern_review(
                        pool,
                        &coding_session.program_id,
                        &problem.spec.language,
                        pat.id,
                        rating,
                        now,
                    )
                    .await
                    .unwrap_or(now);
                    patterns.push(PatternDebriefDto {
                        pattern_id: pat.id.to_string(),
                        name: pat.name.to_string(),
                        family: format!("{:?}", pat.family),
                        recognition_cue: pat.recognition_cue.to_string(),
                        rating,
                        due_at,
                    });
                }
            }
        }

        let debrief = CodingDebriefDto {
            session_id: request.session_id,
            scorecard,
            patterns,
            approach_summary: String::new(),
            target_complexity,
            reference_solution: reference_solution.clone(),
            uncaught_misreadings,
            stage_timings: coding_session.stage_timings.clone(),
            run_count: coding_session.runs.len() as u32,
            hints_used: coding_session.hints.iter().map(|h| h.rung).collect(),
            setting: coding_session.setting,
        };

        coding_session.finished_at = Some(now);
        coding_session.revision = new_revision;
        coding_session.problem.worked_solution = Some(reference_solution);
        coding_session.debrief = Some(debrief);
        return Ok(coding_session);
    }

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
    save_scorecard(pool, &scorecard)
        .await
        .map_err(ApiError::from)?;
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
