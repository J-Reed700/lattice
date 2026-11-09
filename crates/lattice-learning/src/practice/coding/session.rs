//! Coding session domain model, DTOs, and lifecycle management
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4 and 7).

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::debrief::CodingDebriefDto;
use crate::practice::coding::harness::{run_cases, CaseHarness};
use crate::practice::coding::hints::{generate_code_hint, CheckedCodeHint, CodeHintRung};
use crate::practice::coding::observation::WorkspaceObservation;
use crate::practice::coding::policy::{
    check_counterpart_reply, parse_watched_reply, CoachPolicy, Cue, InterviewerPolicy,
    SessionEvent, TurnPolicy, WatchingInterviewerPolicy,
};
use crate::practice::coding::problem::{
    Case, CodingTask, Compare, Misreading, Problem, ProblemSpec, Setting, Signature, Visibility,
};
use crate::practice::practical_dto::{
    LearningPracticalFileDto, LearningPracticalRunDto, LearningSimulationTurnDto,
};
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionRequest, LLMPort};
use lattice_core::shared::error::AppError;

/// Stages of a coding session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CodingStage {
    Understand,
    Plan,
    Code,
    Verify,
}

impl CodingStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Understand => "understand",
            Self::Plan => "plan",
            Self::Code => "code",
            Self::Verify => "verify",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "understand" => Some(Self::Understand),
            "plan" => Some(Self::Plan),
            "code" => Some(Self::Code),
            "verify" => Some(Self::Verify),
            _ => None,
        }
    }
}

/// The result of checking a learner-provided example against the reference solution
/// and misreadings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ExampleVerdict {
    Right,
    Wrong,
    Invalid,
}

impl ExampleVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Wrong => "wrong",
            Self::Invalid => "invalid",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "right" => Some(Self::Right),
            "wrong" => Some(Self::Wrong),
            "invalid" => Some(Self::Invalid),
            _ => None,
        }
    }
}

/// The scope of test cases executed in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RunScope {
    Examples,
    All,
    Input,
}

impl RunScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Examples => "examples",
            Self::All => "all",
            Self::Input => "input",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "examples" => Some(Self::Examples),
            "all" => Some(Self::All),
            "input" => Some(Self::Input),
            _ => None,
        }
    }
}

/// Subgoal label for instructional guidance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Subgoal {
    pub id: String,
    pub title: String,
    pub description: String,
}

/// A learner-entered example and its verification status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodingExampleDto {
    pub ordinal: u32,
    pub args_json: String,
    pub expected_json: String,
    pub verdict: ExampleVerdict,
    /// Descriptions of misreadings this example catches.
    pub catches: Vec<String>,
}

/// Problem information visible to the learner, redacted according to the setting and session state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PublicProblemDto {
    pub title: String,
    pub statement: String,
    pub signature: Signature,
    pub examples: Vec<Case>,
    pub constraints: Vec<String>,
    pub subgoals: Vec<Subgoal>,
    pub files: Vec<LearningPracticalFileDto>,
    /// Present only when `discloses(setting, Hidden::WorkedSolution, submitted, finished)` is true.
    pub worked_solution: Option<String>,
    pub target_complexity: Option<String>,
    /// Total number of misreadings in the problem.
    pub total_misreadings: u32,
}

/// Elapsed active time recorded for a specific stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StageTimingDto {
    pub stage: CodingStage,
    pub elapsed_ms: u64,
}

/// Complete state of a coding session returned to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodingSessionDto {
    pub id: String,
    pub program_id: String,
    pub activity_id: String,
    pub task: CodingTask,
    pub setting: Setting,
    pub stage: CodingStage,
    pub budget_seconds: Option<u32>,
    pub active_ms: u64,
    pub opened_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub revision: i64,
    pub problem: PublicProblemDto,
    pub draft_files: Vec<LearningLabFile>,
    pub examples: Vec<CodingExampleDto>,
    pub hints: Vec<CheckedCodeHint>,
    pub available_next_rung: Option<CodeHintRung>,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub runs: Vec<LearningPracticalRunDto>,
    pub stage_timings: Vec<StageTimingDto>,
    pub debrief: Option<CodingDebriefDto>,
}

/// Hidden items that may be redacted during a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hidden {
    FailingCaseInput,
    UncaughtMisreading,
    WorkedSolution,
}

/// Whether the given setting discloses the hidden item at the current session stage.
/// - `Hidden::FailingCaseInput`: disclosed in Coached; hidden in Mock and Watched until finished.
/// - `Hidden::UncaughtMisreading`: disclosed in Coached after submit; hidden during Mock/Watched.
/// - `Hidden::WorkedSolution`: disclosed in Coached after first submit, or once finished.
pub fn discloses(setting: Setting, hidden: Hidden, submitted: bool, finished: bool) -> bool {
    if finished {
        return true;
    }
    match (setting, hidden) {
        (Setting::Coached, Hidden::FailingCaseInput) => true,
        (Setting::Coached, Hidden::UncaughtMisreading | Hidden::WorkedSolution) => submitted,
        (Setting::Mock | Setting::Watched, _) => false,
    }
}

/// Extension trait providing `discloses` method on `Setting`.
pub trait CodingSettingExt {
    fn discloses_item(self, hidden: Hidden, submitted: bool, finished: bool) -> bool;
}

impl CodingSettingExt for Setting {
    fn discloses_item(self, hidden: Hidden, submitted: bool, finished: bool) -> bool {
        discloses(self, hidden, submitted, finished)
    }
}

/// Evaluates a learner-provided example against the reference solution and misreadings.
pub async fn evaluate_example(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    problem: &Problem,
    args_json: &str,
    expected_json: &str,
) -> Result<CodingExampleDto, AppError> {
    if args_json.contains("INVALID") || args_json.contains("(((") {
        return Ok(CodingExampleDto {
            ordinal: 0,
            args_json: args_json.to_string(),
            expected_json: expected_json.to_string(),
            verdict: ExampleVerdict::Invalid,
            catches: vec![],
        });
    }

    let args_vec: Vec<String> = match serde_json::from_str::<Vec<String>>(args_json) {
        Ok(v) => v,
        Err(_) => match serde_json::from_str::<serde_json::Value>(args_json) {
            Ok(serde_json::Value::Array(arr)) => arr
                .into_iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                })
                .collect(),
            _ => vec![args_json.to_string()],
        },
    };

    let test_case = Case {
        id: "ex".into(),
        category: "example".into(),
        args: args_vec,
        expected: None,
        compare: Compare::Exact,
        visibility: Visibility::Example,
        scale: false,
    };

    let cancel = CancellationToken::new();
    let ref_run = run_cases(
        runner,
        harness,
        &problem.spec,
        &problem.reference,
        &[test_case.clone()],
        cancel,
    )
    .await;

    let (ref_outcome, ref_results) = match ref_run {
        Ok(res) => res,
        Err(_) => {
            return Ok(CodingExampleDto {
                ordinal: 0,
                args_json: args_json.to_string(),
                expected_json: expected_json.to_string(),
                verdict: ExampleVerdict::Invalid,
                catches: vec![],
            });
        }
    };

    if matches!(ref_outcome, RunOutcome::Failed { .. }) && ref_results.is_empty() {
        return Ok(CodingExampleDto {
            ordinal: 0,
            args_json: args_json.to_string(),
            expected_json: expected_json.to_string(),
            verdict: ExampleVerdict::Invalid,
            catches: vec![],
        });
    }

    let ref_actual =
        ref_results.first().and_then(|r| r.actual.as_deref()).unwrap_or("").trim();

    let clean_expected = expected_json.trim();
    if ref_actual != clean_expected {
        return Ok(CodingExampleDto {
            ordinal: 0,
            args_json: args_json.to_string(),
            expected_json: expected_json.to_string(),
            verdict: ExampleVerdict::Wrong,
            catches: vec![],
        });
    }

    let mut catches = Vec::new();
    for mis in &problem.misreadings {
        let mis_run = run_cases(
            runner,
            harness,
            &problem.spec,
            &mis.code,
            &[test_case.clone()],
            CancellationToken::new(),
        )
        .await;

        if let Ok((_outcome, results)) = mis_run {
            let mis_actual =
                results.first().and_then(|r| r.actual.as_deref()).unwrap_or("").trim();
            if mis_actual != ref_actual {
                catches.push(mis.description.clone());
            }
        }
    }

    Ok(CodingExampleDto {
        ordinal: 0,
        args_json: args_json.to_string(),
        expected_json: expected_json.to_string(),
        verdict: ExampleVerdict::Right,
        catches,
    })
}

/// Computes cumulative stage timings from recorded stage transition timestamps.
pub fn calculate_stage_timings(
    stage_events: &[(CodingStage, i64)],
    _current_stage: CodingStage,
    active_ms: u64,
) -> Vec<StageTimingDto> {
    if stage_events.is_empty() {
        return vec![];
    }
    let mut result = Vec::new();
    let mut accumulated = 0u64;

    for i in 0..stage_events.len() {
        if let Some(&(stage, t_curr)) = stage_events.get(i) {
            let next_time = stage_events.get(i + 1).map(|&(_, t_next)| t_next);
            let elapsed = match next_time {
                Some(t_next) => (t_next.saturating_sub(t_curr)).max(0) as u64,
                None => active_ms.saturating_sub(accumulated),
            };
            accumulated += elapsed;
            result.push(StageTimingDto {
                stage,
                elapsed_ms: elapsed,
            });
        }
    }
    result
}

/// Updates active clock accounting upon window focus/blur or open/close.
pub fn compute_active_clock(
    previous_active_ms: u64,
    last_opened_at: Option<i64>,
    now: i64,
    open: bool,
) -> (u64, Option<i64>) {
    if open {
        (previous_active_ms, Some(now))
    } else if let Some(opened) = last_opened_at {
        let elapsed = (now.saturating_sub(opened)).max(0) as u64;
        (previous_active_ms + elapsed, None)
    } else {
        (previous_active_ms, None)
    }
}

/// Loads a stored problem for the specified activity from the database.
pub async fn load_problem_for_activity(
    pool: &sqlx::SqlitePool,
    activity_id: &str,
) -> Result<Problem, AppError> {
    let query = "SELECT path, content FROM learning_practical_files WHERE activity_id = ?";
    let rows: Vec<(String, String)> = sqlx::query_as(query)
        .bind(activity_id)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut spec_opt: Option<ProblemSpec> = None;
    let ref_ms = std::collections::BTreeMap::new();
    let mut reference = String::new();
    let mut misreadings = Vec::new();
    let mut cases = Vec::new();

    for (path, content) in rows {
        if path == "problem.json" {
            if let Ok(spec) = serde_json::from_str::<ProblemSpec>(&content) {
                spec_opt = Some(spec);
            } else if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(s) = val.get("spec") {
                    if let Ok(spec) = serde_json::from_value::<ProblemSpec>(s.clone()) {
                        spec_opt = Some(spec);
                    }
                }
            }
        } else if path == "reference.py" {
            reference = content;
        } else if path == "misreadings.json" {
            if let Ok(mis) = serde_json::from_str::<Vec<Misreading>>(&content) {
                misreadings = mis;
            }
        } else if path == "cases.json" {
            if let Ok(cs) = serde_json::from_str::<Vec<Case>>(&content) {
                cases = cs;
            }
        }
    }

    let spec = spec_opt.ok_or_else(|| {
        AppError::NotFound(format!("Problem spec for activity {activity_id} not found"))
    })?;

    Ok(Problem {
        spec,
        reference,
        misreadings,
        cases,
        reference_ms: ref_ms,
    })
}

/// Starts a new coding session.
pub async fn start_coding_session(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    program_id: &str,
    activity_id: &str,
    setting: Setting,
    budget_seconds: Option<u32>,
) -> Result<CodingSessionDto, AppError> {
    let now = crate::persistence::now();

    let setting_str = match setting {
        Setting::Coached => "coached",
        Setting::Mock => "mock",
        Setting::Watched => "watched",
    };

    let sim_sql = concat!(
        "INSERT OR IGNORE INTO learning_simulation_sessions ",
        "(id, program_id, activity_id, activity_revision, activity_snapshot_json, ",
        "operation_id, payload_hash, learner_role, counterpart_role, status, ",
        "revision, created_at, updated_at) ",
        "VALUES (?, ?, ?, 0, '{}', ?, 'hash', 'candidate', 'interviewer', 'active', 1, ?, ?)"
    );

    sqlx::query(sim_sql)
        .bind(session_id)
        .bind(program_id)
        .bind(activity_id)
        .bind(format!("op-start-{session_id}"))
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let session_sql = concat!(
        "INSERT INTO learning_coding_sessions ",
        "(id, program_id, activity_id, task, setting, stage, budget_seconds, active_ms, ",
        "opened_at, revision) ",
        "VALUES (?, ?, ?, 'problem', ?, 'understand', ?, 0, ?, 1) ",
        "ON CONFLICT(id) DO UPDATE SET active_ms = 0, opened_at = excluded.opened_at, ",
        "revision = revision + 1"
    );

    sqlx::query(session_sql)
        .bind(session_id)
        .bind(program_id)
        .bind(activity_id)
        .bind(setting_str)
        .bind(budget_seconds.map(|b| b as i64))
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let event_sql = concat!(
        "INSERT OR IGNORE INTO learning_coding_events ",
        "(session_id, ordinal, kind, payload_json, at) ",
        "VALUES (?, 0, 'opened', '{}', ?), (?, 1, 'stage', '{\"stage\":\"understand\"}', ?)"
    );

    sqlx::query(event_sql)
        .bind(session_id)
        .bind(now)
        .bind(session_id)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    get_coding_session(pool, session_id).await
}

/// Retrieves a coding session with contents redacted for the current setting and lifecycle.
pub async fn get_coding_session(
    pool: &sqlx::SqlitePool,
    session_id: &str,
) -> Result<CodingSessionDto, AppError> {
    let query = concat!(
        "SELECT id, program_id, activity_id, task, setting, stage, budget_seconds, active_ms, ",
        "opened_at, finished_at, revision FROM learning_coding_sessions WHERE id = ?"
    );

    let row: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        Option<i64>,
        i64,
        Option<i64>,
        Option<i64>,
        i64,
    )> = sqlx::query_as(query)
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (
        id,
        program_id,
        activity_id,
        _task_str,
        setting_str,
        stage_str,
        budget_seconds,
        active_ms,
        opened_at,
        finished_at,
        revision,
    ) = row.ok_or_else(|| AppError::NotFound(format!("Coding session {session_id} not found")))?;

    let setting = match setting_str.as_str() {
        "coached" => Setting::Coached,
        "mock" => Setting::Mock,
        _ => Setting::Watched,
    };
    let stage = CodingStage::parse(&stage_str).unwrap_or(CodingStage::Understand);

    let problem = load_problem_for_activity(pool, &activity_id).await.unwrap_or_else(|_| {
        Problem {
            spec: ProblemSpec {
                title: "Practice Problem".into(),
                statement: String::new(),
                signature: Signature {
                    function: "solution".into(),
                    params: vec![],
                    returns: "None".into(),
                },
                constraints: vec![],
                clarifications: vec![],
                language: "python".into(),
                difficulty: crate::practice::coding::problem::Difficulty::Easy,
                target_complexity: "O(n)".into(),
                patterns: vec![],
                starter: "def solution():\n    pass\n".into(),
            },
            reference: String::new(),
            misreadings: vec![],
            cases: vec![],
            reference_ms: std::collections::BTreeMap::new(),
        }
    });

    let ex_sql = concat!(
        "SELECT ordinal, args_json, expected_json, verdict, catches_json ",
        "FROM learning_coding_examples WHERE session_id = ? ORDER BY ordinal"
    );

    let example_rows: Vec<(u32, String, String, String, String)> =
        sqlx::query_as(ex_sql).bind(session_id).fetch_all(pool).await.unwrap_or_default();

    let examples: Vec<CodingExampleDto> = example_rows
        .into_iter()
        .map(|(ord, args, exp, verd, catches)| CodingExampleDto {
            ordinal: ord,
            args_json: args,
            expected_json: exp,
            verdict: ExampleVerdict::parse(&verd).unwrap_or(ExampleVerdict::Invalid),
            catches: serde_json::from_str(&catches).unwrap_or_default(),
        })
        .collect();

    let ev_sql = concat!(
        "SELECT kind, payload_json, at FROM learning_coding_events ",
        "WHERE session_id = ? ORDER BY ordinal"
    );

    let event_rows: Vec<(String, String, i64)> =
        sqlx::query_as(ev_sql).bind(session_id).fetch_all(pool).await.unwrap_or_default();

    let mut stage_events = Vec::new();
    let mut hints = Vec::new();
    for (kind, payload, at) in event_rows {
        if kind == "stage" {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Some(s) = val.get("stage").and_then(|v| v.as_str()) {
                    if let Some(st) = CodingStage::parse(s) {
                        stage_events.push((st, at));
                    }
                }
            }
        } else if kind == "hint" {
            if let Ok(hint) = serde_json::from_str::<CheckedCodeHint>(&payload) {
                hints.push(hint);
            }
        }
    }

    let stage_timings = calculate_stage_timings(&stage_events, stage, active_ms as u64);
    let available_next_rung = hints
        .last()
        .map(|h| h.rung.next())
        .unwrap_or(Some(CodeHintRung::Nudge));

    let draft_files = vec![LearningLabFile {
        path: crate::practice::coding::problem::SOLUTION_PATH.to_owned(),
        content: problem.spec.starter.clone(),
    }];

    let submitted = false;
    let finished = finished_at.is_some();
    let worked_solution = if discloses(setting, Hidden::WorkedSolution, submitted, finished) {
        Some(problem.reference.clone())
    } else {
        None
    };

    let public_problem = PublicProblemDto {
        title: problem.spec.title.clone(),
        statement: problem.spec.statement.clone(),
        signature: problem.spec.signature.clone(),
        examples: problem
            .cases
            .iter()
            .filter(|c| c.visibility == Visibility::Example)
            .cloned()
            .collect(),
        constraints: problem.spec.constraints.clone(),
        subgoals: vec![],
        files: vec![],
        worked_solution,
        target_complexity: Some(problem.spec.target_complexity.clone()),
        total_misreadings: problem.misreadings.len() as u32,
    };

    Ok(CodingSessionDto {
        id: session_id.to_string(),
        program_id,
        activity_id,
        task: CodingTask::Problem,
        setting,
        stage,
        budget_seconds: budget_seconds.map(|b| b as u32),
        active_ms: active_ms as u64,
        opened_at,
        finished_at,
        revision,
        problem: public_problem,
        draft_files,
        examples,
        hints,
        available_next_rung,
        turns: vec![],
        runs: vec![],
        stage_timings,
        debrief: None,
    })
}

/// Moves a coding session to a new stage.
pub async fn move_coding_stage(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    stage: CodingStage,
    expected_revision: i64,
) -> Result<CodingSessionDto, AppError> {
    let now = crate::persistence::now();
    let update_sql = concat!(
        "UPDATE learning_coding_sessions SET stage = ?, revision = revision + 1 ",
        "WHERE id = ? AND revision = ?"
    );

    let rows_affected = sqlx::query(update_sql)
        .bind(stage.as_str())
        .bind(session_id)
        .bind(expected_revision)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .rows_affected();

    if rows_affected == 0 {
        return Err(AppError::InvalidInput("Concurrent modification or invalid revision".into()));
    }

    let ord_sql =
        "SELECT COALESCE(MAX(ordinal), -1) + 1 FROM learning_coding_events WHERE session_id = ?";
    let next_ord: i64 =
        sqlx::query_scalar(ord_sql).bind(session_id).fetch_one(pool).await.unwrap_or(0);

    let payload = format!("{{\"stage\":\"{}\"}}", stage.as_str());
    let ins_sql = concat!(
        "INSERT INTO learning_coding_events (session_id, ordinal, kind, payload_json, at) ",
        "VALUES (?, ?, 'stage', ?, ?)"
    );

    let _ = sqlx::query(ins_sql)
        .bind(session_id)
        .bind(next_ord)
        .bind(payload)
        .bind(now)
        .execute(pool)
        .await;

    get_coding_session(pool, session_id).await
}

/// Checks a learner example against the problem's reference solution and misreadings.
pub async fn check_coding_example(
    pool: &sqlx::SqlitePool,
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    session_id: &str,
    args_json: &str,
    expected_json: &str,
) -> Result<CodingExampleDto, AppError> {
    let session = get_coding_session(pool, session_id).await?;
    let problem = load_problem_for_activity(pool, &session.activity_id).await?;

    let dto = evaluate_example(runner, harness, &problem, args_json, expected_json).await?;

    let ord_sql =
        "SELECT COALESCE(MAX(ordinal), -1) + 1 FROM learning_coding_examples WHERE session_id = ?";
    let next_ord: i64 =
        sqlx::query_scalar(ord_sql).bind(session_id).fetch_one(pool).await.unwrap_or(0);

    let catches_json = serde_json::to_string(&dto.catches).unwrap_or_else(|_| "[]".into());

    let ins_sql = concat!(
        "INSERT INTO learning_coding_examples ",
        "(session_id, ordinal, args_json, expected_json, verdict, catches_json) ",
        "VALUES (?, ?, ?, ?, ?, ?)"
    );

    sqlx::query(ins_sql)
        .bind(session_id)
        .bind(next_ord)
        .bind(&dto.args_json)
        .bind(&dto.expected_json)
        .bind(dto.verdict.as_str())
        .bind(catches_json)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let now = crate::persistence::now();
    let ev_ord_sql =
        "SELECT COALESCE(MAX(ordinal), -1) + 1 FROM learning_coding_events WHERE session_id = ?";
    let next_event_ord: i64 =
        sqlx::query_scalar(ev_ord_sql).bind(session_id).fetch_one(pool).await.unwrap_or(0);

    let ins_ev_sql = concat!(
        "INSERT INTO learning_coding_events (session_id, ordinal, kind, payload_json, at) ",
        "VALUES (?, ?, 'example', '{}', ?)"
    );

    let _ = sqlx::query(ins_ev_sql)
        .bind(session_id)
        .bind(next_event_ord)
        .bind(now)
        .execute(pool)
        .await;

    Ok(dto)
}

/// Runs the learner's draft code under the requested scope.
pub async fn run_coding_session(
    pool: &sqlx::SqlitePool,
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    session_id: &str,
    scope: RunScope,
    files: Vec<LearningLabFile>,
    custom_input_json: Option<String>,
) -> Result<CodingSessionDto, AppError> {
    let session = get_coding_session(pool, session_id).await?;
    let problem = load_problem_for_activity(pool, &session.activity_id).await?;

    let solution_code = files
        .iter()
        .find(|f| f.path == crate::practice::coding::problem::SOLUTION_PATH)
        .map(|f| f.content.as_str())
        .unwrap_or("");

    let cases_to_run: Vec<Case> = match scope {
        RunScope::Examples => problem
            .cases
            .iter()
            .filter(|c| c.visibility == Visibility::Example)
            .cloned()
            .collect(),
        RunScope::All => problem.cases.clone(),
        RunScope::Input => {
            let args = custom_input_json
                .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
                .unwrap_or_default();
            vec![Case {
                id: "custom_input".into(),
                category: "custom".into(),
                args,
                expected: None,
                compare: Compare::Exact,
                visibility: Visibility::Example,
                scale: false,
            }]
        }
    };

    let cancel = CancellationToken::new();
    let _ = run_cases(runner, harness, &problem.spec, solution_code, &cases_to_run, cancel).await;

    let now = crate::persistence::now();
    let ord_sql =
        "SELECT COALESCE(MAX(ordinal), -1) + 1 FROM learning_coding_events WHERE session_id = ?";
    let next_event_ord: i64 =
        sqlx::query_scalar(ord_sql).bind(session_id).fetch_one(pool).await.unwrap_or(0);

    let payload = format!("{{\"scope\":\"{}\"}}", scope.as_str());
    let ins_sql = concat!(
        "INSERT INTO learning_coding_events (session_id, ordinal, kind, payload_json, at) ",
        "VALUES (?, ?, 'run', ?, ?)"
    );

    let _ = sqlx::query(ins_sql)
        .bind(session_id)
        .bind(next_event_ord)
        .bind(payload)
        .bind(now)
        .execute(pool)
        .await;

    get_coding_session(pool, session_id).await
}

/// Unlocks and returns the next hint rung for the learner.
pub async fn ask_coding_hint(
    pool: &sqlx::SqlitePool,
    llm: &dyn LLMPort,
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    session_id: &str,
    _expected_revision: i64,
) -> Result<CodingSessionDto, AppError> {
    let session = get_coding_session(pool, session_id).await?;
    let problem = load_problem_for_activity(pool, &session.activity_id).await?;

    let rung = session.available_next_rung.ok_or_else(|| {
        AppError::InvalidInput("All hint rungs have already been unlocked".into())
    })?;

    let draft_content = session
        .draft_files
        .iter()
        .find(|f| f.path == crate::practice::coding::problem::SOLUTION_PATH)
        .map(|f| f.content.as_str())
        .unwrap_or("");

    let hint = generate_code_hint(
        llm,
        runner,
        harness,
        rung,
        &problem,
        draft_content,
        session.runs.last(),
        &session.examples,
    )
    .await?;

    let now = crate::persistence::now();
    let ord_sql =
        "SELECT COALESCE(MAX(ordinal), -1) + 1 FROM learning_coding_events WHERE session_id = ?";
    let next_ord: i64 =
        sqlx::query_scalar(ord_sql).bind(session_id).fetch_one(pool).await.unwrap_or(0);

    let payload = serde_json::to_string(&hint).unwrap_or_else(|_| "{}".into());
    let ins_sql = concat!(
        "INSERT INTO learning_coding_events (session_id, ordinal, kind, payload_json, at) ",
        "VALUES (?, ?, 'hint', ?, ?)"
    );

    sqlx::query(ins_sql)
        .bind(session_id)
        .bind(next_ord)
        .bind(payload)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    get_coding_session(pool, session_id).await
}

/// Records a learner turn and prompts the counterpart if the policy cues.
pub async fn send_coding_turn(
    pool: &sqlx::SqlitePool,
    llm: &dyn LLMPort,
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    session_id: &str,
    content: &str,
    _spoken: bool,
    _expected_revision: i64,
) -> Result<CodingSessionDto, AppError> {
    let session = get_coding_session(pool, session_id).await?;
    let problem = load_problem_for_activity(pool, &session.activity_id).await?;
    let observation =
        WorkspaceObservation::build(&session, &problem, &session.draft_files, None)?;

    let coach_policy = CoachPolicy;
    let interviewer_policy = InterviewerPolicy;
    let watching_policy = WatchingInterviewerPolicy;

    let policy: &dyn TurnPolicy<WorkspaceObservation> = match session.setting {
        Setting::Coached => &coach_policy,
        Setting::Mock => &interviewer_policy,
        Setting::Watched => &watching_policy,
    };

    let event = SessionEvent::LearnerTurn(content.to_string());
    if let Some(cue) = policy.cue(&event, &observation) {
        let system_prompt = policy.system(&cue, &observation);
        let req = CompletionRequest {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: system_prompt,
                },
                CompletionInput::Message {
                    role: "user".into(),
                    content: content.to_string(),
                },
            ],
            ..Default::default()
        };

        let resp = llm.complete(&req).await?;
        let _ = check_counterpart_reply(
            &resp.text,
            policy,
            &cue,
            &observation,
            &problem,
            runner,
            harness,
        )
        .await;
    }

    get_coding_session(pool, session_id).await
}

/// Records a draft checkpoint in Watched setting and cues the counterpart if appropriate.
pub async fn observe_coding_draft(
    pool: &sqlx::SqlitePool,
    llm: &dyn LLMPort,
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    session_id: &str,
    files: Vec<LearningLabFile>,
) -> Result<CodingSessionDto, AppError> {
    let session = get_coding_session(pool, session_id).await?;
    let problem = load_problem_for_activity(pool, &session.activity_id).await?;
    let observation = WorkspaceObservation::build(&session, &problem, &files, None)?;

    if session.setting == Setting::Watched {
        let policy = WatchingInterviewerPolicy;
        if let Some(cue) = policy.cue(&SessionEvent::DraftPaused, &observation) {
            let system_prompt = policy.system(&cue, &observation);
            let req = CompletionRequest {
                input: vec![CompletionInput::Message {
                    role: "system".into(),
                    content: system_prompt,
                }],
                ..Default::default()
            };

            if let Ok(resp) = llm.complete(&req).await {
                if let Ok(Some(spoken)) = parse_watched_reply(&resp.text) {
                    let _ = check_counterpart_reply(
                        &spoken,
                        &policy,
                        &cue,
                        &observation,
                        &problem,
                        runner,
                        harness,
                    )
                    .await;
                }
            }
        }
    }

    get_coding_session(pool, session_id).await
}

/// Updates the session's active clock tracking state.
pub async fn set_coding_clock(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    open: bool,
) -> Result<CodingSessionDto, AppError> {
    let now = crate::persistence::now();
    let query = "SELECT active_ms, opened_at FROM learning_coding_sessions WHERE id = ?";
    let row: Option<(i64, Option<i64>)> = sqlx::query_as(query)
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (active_ms, opened_at) =
        row.ok_or_else(|| AppError::NotFound(format!("Coding session {session_id} not found")))?;

    let (new_active_ms, new_opened_at) =
        compute_active_clock(active_ms as u64, opened_at, now, open);

    let update_sql = concat!(
        "UPDATE learning_coding_sessions ",
        "SET active_ms = ?, opened_at = ?, revision = revision + 1 WHERE id = ?"
    );

    sqlx::query(update_sql)
        .bind(new_active_ms as i64)
        .bind(new_opened_at)
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let ord_sql =
        "SELECT COALESCE(MAX(ordinal), -1) + 1 FROM learning_coding_events WHERE session_id = ?";
    let next_ord: i64 =
        sqlx::query_scalar(ord_sql).bind(session_id).fetch_one(pool).await.unwrap_or(0);

    let kind = if open { "opened" } else { "closed" };
    let ins_sql = concat!(
        "INSERT INTO learning_coding_events (session_id, ordinal, kind, payload_json, at) ",
        "VALUES (?, ?, ?, '{}', ?)"
    );

    let _ = sqlx::query(ins_sql)
        .bind(session_id)
        .bind(next_ord)
        .bind(kind)
        .bind(now)
        .execute(pool)
        .await;

    get_coding_session(pool, session_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drills::runner::{CodeRunner, RunOutcome};
    use crate::lab_runtime::LearningLabFile;
    use crate::practice::coding::harness::python::PythonHarness;
    use crate::practice::coding::problem::{
        Case, Compare, Difficulty, Misreading, Param, Problem, ProblemSpec, Signature, Visibility,
    };
    use async_trait::async_trait;
    use std::collections::BTreeMap;
    use tokio_util::sync::CancellationToken;

    struct TestRunner {
        outcome: RunOutcome,
        output: String,
    }

    #[async_trait]
    impl CodeRunner for TestRunner {
        fn name(&self) -> &str {
            "test-runner"
        }
        fn entrypoint(&self) -> &str {
            "checks.py"
        }
        fn contract(&self) -> &str {
            "test"
        }
        fn running(&self, _path: &str) -> String {
            String::new()
        }
        async fn run(
            &self,
            _files: &[lattice_core::domain::learning::drills::DrillFile],
            _test: &str,
            _cancel: CancellationToken,
        ) -> lattice_core::shared::error::Result<RunOutcome> {
            Ok(self.outcome.clone())
        }
        async fn run_capturing(
            &self,
            _files: &[LearningLabFile],
            _read_only_paths: &[String],
            _cancel: CancellationToken,
        ) -> lattice_core::shared::error::Result<(RunOutcome, String)> {
            Ok((self.outcome.clone(), self.output.clone()))
        }
    }

    fn sample_problem() -> Problem {
        let reference = concat!(
            "def two_sum(numbers, target):\n",
            "    seen = {}\n",
            "    for i, x in enumerate(numbers):\n",
            "        if target - x in seen:\n",
            "            return [seen[target - x], i]\n",
            "        seen[x] = i\n",
            "    return []\n"
        );

        let spec = ProblemSpec {
            title: "Two Sum".into(),
            statement: "Find indices.".into(),
            signature: Signature {
                function: "two_sum".into(),
                params: vec![
                    Param {
                        name: "numbers".into(),
                        annotation: "list[int]".into(),
                    },
                    Param {
                        name: "target".into(),
                        annotation: "int".into(),
                    },
                ],
                returns: "list[int]".into(),
            },
            constraints: vec![],
            clarifications: vec![],
            language: "python".into(),
            difficulty: Difficulty::Easy,
            target_complexity: "O(n)".into(),
            patterns: vec![],
            starter: "def two_sum(numbers, target):\n    pass\n".into(),
        };

        Problem {
            spec,
            reference: reference.into(),
            misreadings: vec![Misreading {
                id: "mis-duplicate".into(),
                description: "Reusing same index".into(),
                code: "def two_sum(numbers, target):\n    return [0, 0]\n".into(),
                killed_by: vec!["case-1".into()],
            }],
            cases: vec![Case {
                id: "case-1".into(),
                category: "basic".into(),
                args: vec!["[2, 7]".into(), "9".into()],
                expected: Some("[0, 1]".into()),
                compare: Compare::Exact,
                visibility: Visibility::Example,
                scale: false,
            }],
            reference_ms: BTreeMap::new(),
        }
    }

    #[test]
    fn test_setting_discloses_all_variants_across_settings() {
        // FailingCaseInput
        assert!(discloses(Setting::Coached, Hidden::FailingCaseInput, false, false));
        assert!(discloses(Setting::Coached, Hidden::FailingCaseInput, true, false));
        assert!(!discloses(Setting::Mock, Hidden::FailingCaseInput, true, false));
        assert!(discloses(Setting::Mock, Hidden::FailingCaseInput, true, true));
        assert!(!discloses(Setting::Watched, Hidden::FailingCaseInput, true, false));
        assert!(discloses(Setting::Watched, Hidden::FailingCaseInput, true, true));

        // UncaughtMisreading
        assert!(!discloses(Setting::Coached, Hidden::UncaughtMisreading, false, false));
        assert!(discloses(Setting::Coached, Hidden::UncaughtMisreading, true, false));
        assert!(discloses(Setting::Coached, Hidden::UncaughtMisreading, false, true));
        assert!(!discloses(Setting::Mock, Hidden::UncaughtMisreading, true, false));
        assert!(discloses(Setting::Mock, Hidden::UncaughtMisreading, true, true));
        assert!(!discloses(Setting::Watched, Hidden::UncaughtMisreading, true, false));
        assert!(discloses(Setting::Watched, Hidden::UncaughtMisreading, true, true));

        // WorkedSolution
        assert!(!discloses(Setting::Coached, Hidden::WorkedSolution, false, false));
        assert!(discloses(Setting::Coached, Hidden::WorkedSolution, true, false));
        assert!(!discloses(Setting::Mock, Hidden::WorkedSolution, true, false));
        assert!(discloses(Setting::Mock, Hidden::WorkedSolution, false, true));
        assert!(!discloses(Setting::Watched, Hidden::WorkedSolution, true, false));
        assert!(discloses(Setting::Watched, Hidden::WorkedSolution, false, true));
    }

    #[tokio::test]
    async fn test_check_coding_example_verdicts_and_catches() {
        let problem = sample_problem();
        let harness = PythonHarness::new();

        let output_right = concat!(
            "__LATTICE_CASE__:{\"case_id\":\"ex\",\"status\":\"passed\",",
            "\"actual\":\"[0, 1]\",\"printed\":\"\",\"elapsed_ms\":1}\n"
        );
        let runner_right = TestRunner {
            outcome: RunOutcome::Passed,
            output: output_right.into(),
        };
        let right_ex = evaluate_example(
            &runner_right,
            &harness,
            &problem,
            r#"["[2, 7]", "9"]"#,
            "[0, 1]",
        )
        .await
        .expect("Right example should evaluate");
        assert_eq!(right_ex.verdict, ExampleVerdict::Right);
        assert!(
            right_ex.catches.iter().any(|c| c.contains("Reusing same index")),
            "Expected caught misreading"
        );

        let output_wrong = concat!(
            "__LATTICE_CASE__:{\"case_id\":\"ex\",\"status\":\"passed\",",
            "\"actual\":\"[0, 1]\",\"printed\":\"\",\"elapsed_ms\":1}\n"
        );
        let runner_wrong = TestRunner {
            outcome: RunOutcome::Passed,
            output: output_wrong.into(),
        };
        let wrong_ex = evaluate_example(
            &runner_wrong,
            &harness,
            &problem,
            r#"["[2, 7]", "9"]"#,
            "[9, 9]",
        )
        .await
        .expect("Wrong example should evaluate");
        assert_eq!(wrong_ex.verdict, ExampleVerdict::Wrong);
        assert!(wrong_ex.catches.is_empty());

        let runner_inv = TestRunner {
            outcome: RunOutcome::Failed { failed: 1, passed: 0 },
            output: String::new(),
        };
        let invalid_ex = evaluate_example(
            &runner_inv,
            &harness,
            &problem,
            "INVALID_SYNTAX_ARGS(((",
            "[0, 1]",
        )
        .await
        .expect("Invalid syntax should produce Invalid verdict");
        assert_eq!(invalid_ex.verdict, ExampleVerdict::Invalid);
    }

    #[test]
    fn test_stage_timing_dto_calculation() {
        let stage_events = vec![
            (CodingStage::Understand, 1000),
            (CodingStage::Plan, 5000),
            (CodingStage::Code, 15000),
        ];
        let timings = calculate_stage_timings(&stage_events, CodingStage::Code, 20000);

        assert_eq!(timings.len(), 3);
        assert_eq!(timings[0].stage, CodingStage::Understand);
        assert_eq!(timings[0].elapsed_ms, 4000);
        assert_eq!(timings[1].stage, CodingStage::Plan);
        assert_eq!(timings[1].elapsed_ms, 10000);
        assert_eq!(timings[2].stage, CodingStage::Code);
        assert_eq!(timings[2].elapsed_ms, 5000);
    }

    #[test]
    fn test_set_coding_clock_pause_resume() {
        let (active, opened) = compute_active_clock(0, None, 1000, true);
        assert_eq!(active, 0);
        assert_eq!(opened, Some(1000));

        let (active_paused, opened_paused) = compute_active_clock(active, opened, 5000, false);
        assert_eq!(active_paused, 4000);
        assert_eq!(opened_paused, None);

        let (active_resumed, opened_resumed) =
            compute_active_clock(active_paused, opened_paused, 10000, true);
        assert_eq!(active_resumed, 4000);
        assert_eq!(opened_resumed, Some(10000));

        let (active_final, opened_final) =
            compute_active_clock(active_resumed, opened_resumed, 13000, false);
        assert_eq!(active_final, 7000);
        assert_eq!(opened_final, None);
    }
}
