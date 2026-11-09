//! The session's commands on an in-memory database, with a fake model, a fake runner and a fake clock.

use super::counterpart::CounterpartReply;
use super::debrief::{Debrief, Debriefer, SessionEvidence};
use super::dto::*;
use super::examples::ExampleCheck;
use super::hints::CodeHint;
use super::observation::WorkspaceObservation;
use super::policy::{CodingCue, CodingPolicy};
use super::problem::*;
use super::records::CodingOp;
use super::session::{default_budget_seconds, Clock, CodingAids, CodingRuns, CodingSessions};
use crate::assessment_engine::LearningRubricCriterion;
use crate::dto::LearningPracticeMode;
use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::*;
use crate::practical_generation::{
    GeneratedPracticalActivity, GeneratedPracticalFile, PracticalFileRole,
};
use crate::practical_repository::LearningPracticalRepository;
use crate::practical_runs::CodingRunStart;
use crate::practice::test_support::fixture;
use crate::repository::LearningRepository;
use async_trait::async_trait;
use lattice_core::domain::learning::practice::{
    ScoreStatus, Scorecard, ScorecardEvidence, ScorecardRow, PROBLEM_PATTERNS,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Mutex;

const START: i64 = 1_000_000;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn case(
    id: &str,
    category: &str,
    arg: &str,
    expected: &str,
    visibility: CaseVisibility,
) -> ProblemCase {
    ProblemCase {
        id: id.into(),
        category: category.into(),
        args: vec![arg.into()],
        expected: Some(expected.into()),
        compare: CaseCompare::Exact,
        visibility,
        scale: false,
        digest: None,
    }
}

fn problem() -> Problem {
    Problem {
        spec: ProblemSpec {
            title: "Double it".into(),
            statement: "Return twice the number.".into(),
            signature: ProblemSignature {
                function: "double".into(),
                params: vec![SignatureParam {
                    name: "n".into(),
                    annotation: "int".into(),
                }],
                returns: "int".into(),
            },
            constraints: vec!["n fits in an int".into()],
            clarifications: vec![],
            language: "python".into(),
            difficulty: ProblemDifficulty::Medium,
            target_complexity: "O(1) time".into(),
            patterns: PROBLEM_PATTERNS
                .iter()
                .take(1)
                .map(|p| p.id.to_owned())
                .collect(),
            starter: "def double(n):\n    pass\n".into(),
        },
        reference: "def double(n):\n    return n * 2\n".into(),
        misreadings: vec![
            Misreading {
                id: "m1".into(),
                description: "Adds two instead of doubling".into(),
                code: "def double(n):\n    return n + 2\n".into(),
                killed_by: vec!["h1".into()],
            },
            Misreading {
                id: "m2".into(),
                description: "Squares instead of doubling".into(),
                code: "def double(n):\n    return n * n\n".into(),
                killed_by: vec!["h2".into()],
            },
        ],
        cases: vec![
            case("e1", "a small number", "2", "4", CaseVisibility::Example),
            case("h1", "zero", "0", "0", CaseVisibility::Hidden),
            case(
                "h2",
                "a negative number",
                "-3",
                "-6",
                CaseVisibility::Hidden,
            ),
        ],
        reference_ms: BTreeMap::new(),
        bug: None,
    }
}

struct FakeClock(AtomicI64);

impl FakeClock {
    fn set(&self, at: i64) {
        self.0.store(at, Ordering::SeqCst);
    }
    fn advance(&self, by: i64) {
        self.0.fetch_add(by, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct FakeAids {
    replies: Mutex<VecDeque<Result<Option<CounterpartReply>>>>,
    reply_calls: AtomicUsize,
}

impl FakeAids {
    fn queue(&self, reply: Result<Option<CounterpartReply>>) {
        self.replies.lock().unwrap().push_back(reply);
    }
}

#[async_trait]
impl CodingAids for FakeAids {
    fn model_name(&self) -> &str {
        "fake-model"
    }

    async fn check_example(
        &self,
        _problem: &Problem,
        _args: &[String],
        expected: &str,
    ) -> Result<ExampleCheck> {
        Ok(match expected {
            "4" => ExampleCheck {
                verdict: ExampleVerdict::Right,
                reference_actual: Some("4".into()),
                why_invalid: None,
                catches: vec!["m1".into()],
            },
            "nonsense" => ExampleCheck {
                verdict: ExampleVerdict::Invalid,
                reference_actual: None,
                why_invalid: Some("That is not a literal.".into()),
                catches: vec![],
            },
            _ => ExampleCheck {
                verdict: ExampleVerdict::Wrong,
                reference_actual: Some("4".into()),
                why_invalid: None,
                catches: vec![],
            },
        })
    }

    async fn reference_answer(&self, _problem: &Problem, _args: &[String]) -> Result<CaseResult> {
        Ok(CaseResult {
            case_id: "input".into(),
            status: CaseStatus::Ran,
            actual: Some("4".into()),
            printed: String::new(),
            error: None,
            elapsed_ms: Some(1),
        })
    }

    async fn hint(
        &self,
        _problem: &Problem,
        _observation: &WorkspaceObservation,
        rung: CodeHintRung,
        _in_role: bool,
    ) -> Result<CodeHint> {
        Ok(CodeHint {
            rung,
            text: "Look at what the function returns.".into(),
            reason: "The body is still empty.".into(),
            lines: vec![],
            path: None,
        })
    }

    async fn reply(
        &self,
        _problem: &Problem,
        _policy: &CodingPolicy,
        _cue: &CodingCue,
        _observation: &WorkspaceObservation,
    ) -> Result<Option<CounterpartReply>> {
        self.reply_calls.fetch_add(1, Ordering::SeqCst);
        self.replies.lock().unwrap().pop_front().unwrap_or(Ok(None))
    }
}

/// Writes the run a real one would: a row, and one check per case with its full detail.
struct FakeRuns {
    pool: SqlitePool,
    repo: LearningPracticalRepository,
    failing: Mutex<Vec<String>>,
}

#[async_trait]
impl CodingRuns for FakeRuns {
    async fn start(
        &self,
        start: &CodingRunStart,
        payload_hash: &str,
    ) -> Result<LearningPracticalRunDto> {
        let base = &start.base;
        let failing = self.failing.lock().unwrap().clone();
        let details = start
            .cases
            .iter()
            .map(|case| CaseCheckDetailDto {
                case_id: case.id.clone(),
                category: case.category.clone(),
                visibility: case.visibility,
                scale: case.scale,
                mine: start.mine_ids.contains(&case.id),
                status: if failing.contains(&case.id) {
                    CaseStatus::Failed
                } else if case.expected.is_some() {
                    CaseStatus::Passed
                } else {
                    CaseStatus::Ran
                },
                args: Some(case.args.clone()),
                expected: case.expected.clone(),
                actual: Some("99".into()),
                printed: Some("secret print".into()),
                error: None,
                elapsed_ms: Some(1),
            })
            .collect::<Vec<_>>();
        let status = if details
            .iter()
            .any(|detail| detail.status == CaseStatus::Failed)
        {
            "failed"
        } else {
            "passed"
        };
        sqlx::query("INSERT INTO learning_practical_runs(id,program_id,activity_id,activity_revision,activity_snapshot_json,operation_id,payload_hash,status,learner_files_json,created_at,completed_at,coding_session_id,scope) VALUES(?,?,?,?,'{}',?,?,?,?,?,?,?,?)")
            .bind(&base.run_id).bind(&base.program_id).bind(&base.activity_id)
            .bind(base.expected_activity_revision).bind(&base.operation_id).bind(payload_hash)
            .bind(status).bind(serde_json::to_string(&base.learner_files).unwrap())
            .bind(START).bind(START).bind(&start.coding_session_id).bind(start.scope.as_str())
            .execute(&self.pool).await.unwrap();
        for (ordinal, detail) in details.iter().enumerate() {
            sqlx::query("INSERT INTO learning_practical_run_checks(run_id,ordinal,name,status,message,detail_json) VALUES(?,?,?,?,'',?)")
                .bind(&base.run_id).bind(ordinal as i64).bind(&detail.case_id)
                .bind(if detail.status == CaseStatus::Failed { "failed" } else { "passed" })
                .bind(serde_json::to_string(detail).unwrap())
                .execute(&self.pool).await.unwrap();
        }
        self.repo.run(&base.run_id).await
    }
}

#[derive(Default)]
struct FakeDebriefer {
    calls: AtomicUsize,
}

#[async_trait]
impl Debriefer for FakeDebriefer {
    async fn debrief(&self, _evidence: &SessionEvidence) -> Result<Debrief> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Debrief {
            scorecard: Scorecard {
                rows: vec![
                    ScorecardRow {
                        criterion_id: "code".into(),
                        title: "Code".into(),
                        status: ScoreStatus::Observed,
                        points: Some(3),
                        max_points: 4,
                        evidence: vec![ScorecardEvidence::Example { ordinal: 1 }],
                        feedback: "Clear.".into(),
                        next_step: "Name the edge cases.".into(),
                    },
                    ScorecardRow {
                        criterion_id: "communication".into(),
                        title: "Communication".into(),
                        status: ScoreStatus::InsufficientEvidence,
                        points: None,
                        max_points: 4,
                        evidence: vec![],
                        feedback: "Nothing to judge.".into(),
                        next_step: "Talk through the plan.".into(),
                    },
                ],
            },
            approach: "Multiply by two.".into(),
            complexity: "O(1) time".into(),
        })
    }
}

struct Ctx {
    pool: SqlitePool,
    repo: LearningPracticalRepository,
    program_id: String,
    activity_id: String,
    clock: FakeClock,
    aids: FakeAids,
    runs: FakeRuns,
    debriefer: FakeDebriefer,
}

impl Ctx {
    async fn new() -> Self {
        let (pool, program_id, lesson_id, _outcome) = fixture().await.unwrap();
        let repo = LearningPracticalRepository::new(pool.clone());
        let activity_id = id();
        let request = GenerateLearningPracticalActivityRequestDto {
            operation_id: id(),
            activity_id: activity_id.clone(),
            program_id: program_id.clone(),
            lesson_id,
            expected_program_revision: LearningRepository::new(pool.clone())
                .get(&program_id)
                .await
                .unwrap()
                .summary
                .revision,
            kind: LearningPracticalActivityKind::CodingProblem,
            learner_brief: "Practice doubling.".into(),
            practice_mode: LearningPracticeMode::Practice,
            allowed_aids: vec![],
            runtime_profile_id: None,
            builtin_runtime: None,
            interview_target: None,
        };
        let generated = GeneratedPracticalActivity {
            title: "Double it".into(),
            brief: "Return twice the number.".into(),
            source_version_ids: vec![],
            rubric: vec![LearningRubricCriterion {
                id: id(),
                title: "Correctness".into(),
                description: "Every case passes.".into(),
                max_points: 2,
            }],
            files: vec![
                GeneratedPracticalFile {
                    path: SOLUTION_PATH.into(),
                    role: PracticalFileRole::Starter,
                    content: problem().spec.starter,
                    editable: true,
                },
                GeneratedPracticalFile {
                    path: "problem/problem.json".into(),
                    role: PracticalFileRole::Check,
                    content: serde_json::to_string(&problem()).unwrap(),
                    editable: false,
                },
            ],
        };
        repo.save_generated_activity(&request, &generated, "fake-model", "payload")
            .await
            .unwrap();
        Self {
            runs: FakeRuns {
                pool: pool.clone(),
                repo: repo.clone(),
                failing: Mutex::new(vec![]),
            },
            pool,
            repo,
            program_id,
            activity_id,
            clock: FakeClock(AtomicI64::new(START)),
            aids: FakeAids::default(),
            debriefer: FakeDebriefer::default(),
        }
    }

    fn sessions(&self) -> CodingSessions<'_> {
        CodingSessions {
            repo: &self.repo,
            aids: &self.aids,
            runs: &self.runs,
            debriefer: &self.debriefer,
            clock: &self.clock,
        }
    }

    async fn start(&self, setting: CodingSetting) -> CodingSessionDto {
        self.sessions()
            .start(&StartCodingSessionRequestDto {
                operation_id: id(),
                program_id: self.program_id.clone(),
                activity_id: self.activity_id.clone(),
                setting,
                budget_seconds: None,
            })
            .await
            .unwrap()
    }

    async fn get(&self, session: &CodingSessionDto) -> CodingSessionDto {
        self.sessions()
            .get(&CodingSessionRequestDto {
                program_id: self.program_id.clone(),
                session_id: session.session_id.clone(),
            })
            .await
            .unwrap()
    }

    /// Drops the cue a new session holds, so a test starts from none.
    async fn clear_cue(&self, session: &CodingSessionDto) {
        self.repo
            .take_coding_cue(&self.program_id, &session.session_id)
            .await
            .unwrap();
    }

    /// Inserts a bare run at the fixed clock, so its id is the only thing that differs.
    async fn insert_run(&self, session: &CodingSessionDto, run_id: &str) {
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM learning_practical_activities WHERE program_id=? AND id=?",
        )
        .bind(&self.program_id)
        .bind(&self.activity_id)
        .fetch_one(&self.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO learning_practical_runs(id,program_id,activity_id,activity_revision,activity_snapshot_json,operation_id,payload_hash,status,learner_files_json,created_at,completed_at,coding_session_id,scope) VALUES(?,?,?,?,'{}',?,'hash','passed','[]',?,?,?,'all')")
            .bind(run_id).bind(&self.program_id).bind(&self.activity_id).bind(revision)
            .bind(id()).bind(START).bind(START).bind(&session.session_id)
            .execute(&self.pool).await.unwrap();
    }

    async fn run(
        &self,
        session: &CodingSessionDto,
        scope: CodingRunScope,
        input: Option<Vec<String>>,
    ) -> Result<CodingSessionDto> {
        self.sessions()
            .run(&RunCodingSessionRequestDto {
                operation_id: id(),
                run_id: id(),
                program_id: self.program_id.clone(),
                session_id: session.session_id.clone(),
                scope,
                files: starter_files("def double(n):\n    return n * 2\n"),
                input,
            })
            .await
    }

    async fn move_to(
        &self,
        session: &CodingSessionDto,
        stage: CodingStage,
        expected_revision: i64,
    ) -> Result<CodingSessionDto> {
        self.sessions()
            .move_stage(&MoveCodingStageRequestDto {
                operation_id: id(),
                program_id: self.program_id.clone(),
                session_id: session.session_id.clone(),
                stage,
                expected_revision,
            })
            .await
    }

    async fn example(
        &self,
        session: &CodingSessionDto,
        ordinal: Option<u32>,
        expected: &str,
    ) -> Result<CodingSessionDto> {
        self.sessions()
            .check_example(&CheckCodingExampleRequestDto {
                operation_id: id(),
                program_id: self.program_id.clone(),
                session_id: session.session_id.clone(),
                ordinal,
                args: vec!["2".into()],
                expected: expected.into(),
            })
            .await
    }

    async fn finish(
        &self,
        session: &CodingSessionDto,
        operation_id: &str,
    ) -> Result<CodingSessionDto> {
        self.sessions()
            .finish(&FinishCodingSessionRequestDto {
                operation_id: operation_id.into(),
                program_id: self.program_id.clone(),
                session_id: session.session_id.clone(),
                files: starter_files("def double(n):\n    return n * 2\n"),
            })
            .await
    }
}

fn starter_files(content: &str) -> Vec<LearningLabFile> {
    vec![LearningLabFile {
        path: SOLUTION_PATH.into(),
        content: content.into(),
    }]
}

fn is_conflict<T>(result: &Result<T>) -> bool {
    matches!(result, Err(AppError::Conflict(_)))
}

#[tokio::test]
async fn a_session_starts_at_the_first_stage_with_a_budget_only_outside_coached() {
    let ctx = Ctx::new().await;
    let coached = ctx.start(CodingSetting::Coached).await;
    assert_eq!(coached.stage, CodingStage::Understand);
    assert_eq!(coached.stages, stages_of(CodingTask::Problem));
    assert_eq!(coached.clock.budget_seconds, None);
    assert_eq!(coached.draft, starter_files(&problem().spec.starter));
    assert!(coached.cue_pending, "the counterpart opens");
    assert_eq!(coached.problem.starter, problem().spec.starter);

    let mock = ctx.start(CodingSetting::Mock).await;
    assert_eq!(
        mock.clock.budget_seconds,
        Some(default_budget_seconds(ProblemDifficulty::Medium))
    );
    assert_eq!(default_budget_seconds(ProblemDifficulty::Easy), 20 * 60);
    assert_eq!(default_budget_seconds(ProblemDifficulty::Medium), 35 * 60);
    assert_eq!(default_budget_seconds(ProblemDifficulty::Hard), 45 * 60);
}

#[tokio::test]
async fn starting_with_the_same_operation_id_answers_with_the_session_it_made() {
    let ctx = Ctx::new().await;
    let request = StartCodingSessionRequestDto {
        operation_id: id(),
        program_id: ctx.program_id.clone(),
        activity_id: ctx.activity_id.clone(),
        setting: CodingSetting::Mock,
        budget_seconds: Some(600),
    };
    let first = ctx.sessions().start(&request).await.unwrap();
    let second = ctx.sessions().start(&request).await.unwrap();
    assert_eq!(first.session_id, second.session_id);
    assert_eq!(first.clock.budget_seconds, Some(600));
    let listed = ctx
        .sessions()
        .list(&ListCodingSessionsRequestDto {
            program_id: ctx.program_id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn the_list_is_newest_first_and_reports_the_last_submit() {
    let ctx = Ctx::new().await;
    let older = ctx.start(CodingSetting::Coached).await;
    ctx.clock.advance(1_000);
    let newer = ctx.start(CodingSetting::Mock).await;
    ctx.run(&older, CodingRunScope::All, None).await.unwrap();
    let listed = ctx
        .sessions()
        .list(&ListCodingSessionsRequestDto {
            program_id: ctx.program_id.clone(),
        })
        .await
        .unwrap();
    let ids = listed
        .iter()
        .map(|each| each.session_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, [newer.session_id.as_str(), older.session_id.as_str()]);
    assert_eq!(listed[1].passed, Some(3));
    assert_eq!(listed[1].total, Some(3));
    assert_eq!(listed[0].passed, None);
    assert_eq!(listed[0].title, "Double it");
}

#[tokio::test]
async fn moving_forward_records_the_stages_passed_over_and_a_stale_revision_conflicts() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    let moved = ctx
        .move_to(&session, CodingStage::Code, session.revision)
        .await
        .unwrap();
    assert_eq!(moved.stage, CodingStage::Code);
    assert_eq!(moved.revision, session.revision + 1);
    let bundle = ctx
        .repo
        .coding_session(&ctx.program_id, &session.session_id)
        .await
        .unwrap();
    let event = bundle
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "stage")
        .unwrap();
    assert_eq!(event.payload["from"], "understand");
    assert_eq!(event.payload["to"], "code");
    assert_eq!(
        event.payload["skipped"],
        serde_json::json!(["examples", "plan"])
    );

    let stale = ctx
        .move_to(&moved, CodingStage::Test, session.revision)
        .await;
    assert!(is_conflict(&stale), "{stale:?}");
    let back = ctx
        .move_to(&moved, CodingStage::Understand, moved.revision)
        .await
        .unwrap();
    let event = ctx
        .repo
        .coding_session(&ctx.program_id, &session.session_id)
        .await
        .unwrap()
        .events
        .into_iter()
        .rev()
        .find(|event| event.kind == "stage")
        .unwrap();
    assert_eq!(back.stage, CodingStage::Understand);
    assert_eq!(event.payload["skipped"], serde_json::json!([]));
}

#[tokio::test]
async fn a_stage_the_task_lacks_is_refused() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    let refused = ctx
        .move_to(&session, CodingStage::Reproduce, session.revision)
        .await;
    assert!(
        matches!(refused, Err(AppError::InvalidInput(_))),
        "{refused:?}"
    );
}

#[tokio::test]
async fn an_example_is_checked_edited_and_removed() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Coached).await;
    let after = ctx.example(&session, None, "4").await.unwrap();
    assert_eq!(after.examples.len(), 1);
    assert_eq!(after.examples[0].verdict, ExampleVerdict::Right);
    assert_eq!(after.coverage.caught, 1);
    assert_eq!(after.coverage.total, 2);

    let edited = ctx.example(&after, Some(1), "5").await.unwrap();
    assert_eq!(edited.examples.len(), 1);
    assert_eq!(edited.examples[0].verdict, ExampleVerdict::Wrong);
    assert_eq!(edited.coverage.caught, 0);

    let removed = ctx
        .sessions()
        .remove_example(&RemoveCodingExampleRequestDto {
            operation_id: id(),
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            ordinal: 1,
        })
        .await
        .unwrap();
    assert!(removed.examples.is_empty());

    let wrong_arity = ctx
        .sessions()
        .check_example(&CheckCodingExampleRequestDto {
            operation_id: id(),
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            ordinal: None,
            args: vec![],
            expected: "4".into(),
        })
        .await;
    assert!(
        matches!(wrong_arity, Err(AppError::InvalidInput(_))),
        "{wrong_arity:?}"
    );
}

#[tokio::test]
async fn what_a_wrong_example_expects_and_the_uncaught_misreading_show_only_in_coached() {
    let ctx = Ctx::new().await;
    for (setting, shown) in [
        (CodingSetting::Coached, true),
        (CodingSetting::Mock, false),
        (CodingSetting::Watched, false),
    ] {
        let session = ctx.start(setting).await;
        let after = ctx.example(&session, None, "5").await.unwrap();
        assert_eq!(after.examples[0].expects.is_some(), shown, "{setting:?}");
        assert_eq!(after.coverage.uncaught.is_some(), shown, "{setting:?}");
        assert_eq!(after.coverage.total, 2);
    }
}

#[tokio::test]
async fn mock_never_shows_a_failing_hidden_case_and_coached_does() {
    let ctx = Ctx::new().await;
    ctx.runs.failing.lock().unwrap().push("h1".into());
    let mock = ctx.start(CodingSetting::Mock).await;
    let after = ctx.run(&mock, CodingRunScope::All, None).await.unwrap();
    let hidden = after.runs[0]
        .cases
        .iter()
        .find(|each| each.case_id == "h1")
        .unwrap();
    assert_eq!(hidden.status, CaseStatus::Failed);
    assert_eq!(hidden.category, "zero");
    assert_eq!(hidden.args, None);
    assert_eq!(hidden.expected, None);
    assert_eq!(hidden.actual, None);
    assert_eq!(hidden.printed, None);
    let passing_hidden = after.runs[0]
        .cases
        .iter()
        .find(|each| each.case_id == "h2")
        .unwrap();
    assert_eq!(passing_hidden.args, None);
    let example = after.runs[0]
        .cases
        .iter()
        .find(|each| each.case_id == "e1")
        .unwrap();
    assert_eq!(example.args, Some(vec!["2".into()]));
    assert_eq!(example.expected.as_deref(), Some("4"));

    let coached = ctx.start(CodingSetting::Coached).await;
    let after = ctx.run(&coached, CodingRunScope::All, None).await.unwrap();
    let hidden = after.runs[0]
        .cases
        .iter()
        .find(|each| each.case_id == "h1")
        .unwrap();
    assert_eq!(hidden.args, Some(vec!["0".into()]));
    assert_eq!(hidden.expected.as_deref(), Some("0"));
    assert_eq!(hidden.actual.as_deref(), Some("99"));
}

#[tokio::test]
async fn the_first_run_of_every_case_is_the_submit() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    let examples = ctx
        .run(&session, CodingRunScope::Examples, None)
        .await
        .unwrap();
    assert!(!examples.submitted);
    assert_eq!(examples.runs[0].cases.len(), 1);
    let all = ctx.run(&examples, CodingRunScope::All, None).await.unwrap();
    assert!(all.submitted);
    assert_eq!(all.runs.len(), 2);
    assert_eq!(all.runs[1].cases.len(), 3);
}

#[tokio::test]
async fn runs_made_in_one_millisecond_keep_the_order_they_were_made_in() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    ctx.insert_run(&session, "run-b").await;
    ctx.insert_run(&session, "run-a").await;
    let read = ctx.get(&session).await;
    let order: Vec<&str> = read.runs.iter().map(|run| run.run_id.as_str()).collect();
    assert_eq!(order, ["run-b", "run-a"]);
}

#[tokio::test]
async fn a_hint_points_at_the_last_run_made_even_when_two_share_a_millisecond() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Coached).await;
    ctx.insert_run(&session, "run-b").await;
    ctx.insert_run(&session, "run-a").await;
    let session = ctx.get(&session).await;
    let after = ctx
        .sessions()
        .ask_hint(&AskCodingHintRequestDto {
            operation_id: id(),
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            files: starter_files("def double(n):\n    pass\n"),
            expected_revision: session.revision,
        })
        .await
        .unwrap();
    assert_eq!(after.hints[0].run_id.as_deref(), Some("run-a"));
}

#[tokio::test]
async fn an_examples_run_adds_the_learners_right_examples_as_their_own_cases() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Coached).await;
    let with_example = ctx.example(&session, None, "4").await.unwrap();
    ctx.example(&with_example, None, "5").await.unwrap();
    let after = ctx
        .run(&session, CodingRunScope::Examples, None)
        .await
        .unwrap();
    let cases = &after.runs[0].cases;
    assert_eq!(
        cases.len(),
        2,
        "the statement's example and the one right example"
    );
    let mine = cases.iter().find(|each| each.mine).unwrap();
    assert_eq!(mine.case_id, "mine-1");
    assert_eq!(mine.category, "Your example 1");
}

#[tokio::test]
async fn an_input_run_shows_what_the_reference_returns_only_in_coached() {
    let ctx = Ctx::new().await;
    let coached = ctx.start(CodingSetting::Coached).await;
    let after = ctx
        .run(&coached, CodingRunScope::Input, Some(vec!["2".into()]))
        .await
        .unwrap();
    assert_eq!(after.runs[0].reference_actual.as_deref(), Some("4"));
    assert_eq!(after.runs[0].cases[0].case_id, "input");

    let mock = ctx.start(CodingSetting::Mock).await;
    let after = ctx
        .run(&mock, CodingRunScope::Input, Some(vec!["2".into()]))
        .await
        .unwrap();
    assert_eq!(after.runs[0].reference_actual, None);

    let missing = ctx.run(&mock, CodingRunScope::Input, None).await;
    assert!(
        matches!(missing, Err(AppError::InvalidInput(_))),
        "{missing:?}"
    );
}

#[tokio::test]
async fn the_worked_solution_is_never_available_in_mock_and_only_after_a_submit_in_coached() {
    let ctx = Ctx::new().await;
    let ctx = &ctx;
    let reveal = |session: &CodingSessionDto| {
        let request = RevealCodingSolutionRequestDto {
            operation_id: id(),
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
        };
        async move { ctx.sessions().reveal_solution(&request).await }
    };

    let mock = ctx.start(CodingSetting::Mock).await;
    let mock = ctx.run(&mock, CodingRunScope::All, None).await.unwrap();
    assert!(mock.submitted);
    assert!(!mock.solution_available);
    assert!(reveal(&mock).await.is_err());
    assert_eq!(ctx.get(&mock).await.worked_solution, None);

    let coached = ctx.start(CodingSetting::Coached).await;
    assert!(!coached.solution_available);
    assert!(reveal(&coached).await.is_err(), "before the first submit");
    let submitted = ctx.run(&coached, CodingRunScope::All, None).await.unwrap();
    assert!(submitted.solution_available);
    assert_eq!(submitted.worked_solution, None, "available is not shown");
    let revealed = reveal(&submitted).await.unwrap();
    assert_eq!(
        revealed.worked_solution.as_deref(),
        Some(problem().reference.as_str())
    );
}

#[tokio::test]
async fn the_clock_counts_open_time_only_and_a_stale_span_ends_at_the_last_event() {
    let ctx = Ctx::new().await;
    ctx.clock.set(1_000);
    let session = ctx.start(CodingSetting::Mock).await;
    let ctx = &ctx;
    let clock = |open: bool| {
        let request = SetCodingClockRequestDto {
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            open,
        };
        async move { ctx.sessions().set_clock(&request).await.unwrap() }
    };

    ctx.clock.set(10_000);
    let opened = clock(true).await;
    assert_eq!(opened.open_since, Some(10_000));
    ctx.clock.set(15_000);
    let closed = clock(false).await;
    assert_eq!((closed.active_ms, closed.open_since), (5_000, None));
    ctx.clock.set(90_000);
    assert_eq!(
        clock(false).await.active_ms,
        5_000,
        "closing a closed clock changes nothing"
    );

    ctx.clock.set(100_000);
    clock(true).await;
    ctx.clock.set(102_000);
    let current = ctx.get(&session).await;
    ctx.move_to(&current, CodingStage::Plan, current.revision)
        .await
        .unwrap();
    ctx.clock.set(9_000_000);
    let reopened = clock(true).await;
    assert_eq!(
        reopened.active_ms,
        5_000 + 2_000,
        "the stale span ended at the stage move"
    );
    assert_eq!(reopened.open_since, Some(9_000_000));
}

#[tokio::test]
async fn a_cue_is_replaced_only_by_one_that_outranks_it() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    let (program, id) = (&ctx.program_id, &session.session_id);
    ctx.clear_cue(&session).await;

    ctx.repo
        .offer_coding_cue(program, id, &CodingCue::Checkpoint)
        .await
        .unwrap();
    ctx.repo
        .offer_coding_cue(program, id, &CodingCue::Idle)
        .await
        .unwrap();
    let kept = ctx.repo.take_coding_cue(program, id).await.unwrap();
    assert!(matches!(kept, Some(CodingCue::Checkpoint)), "{kept:?}");

    ctx.repo
        .offer_coding_cue(program, id, &CodingCue::Idle)
        .await
        .unwrap();
    ctx.repo
        .offer_coding_cue(program, id, &CodingCue::Reply)
        .await
        .unwrap();
    let replaced = ctx.repo.take_coding_cue(program, id).await.unwrap();
    assert!(matches!(replaced, Some(CodingCue::Reply)), "{replaced:?}");
    assert!(
        ctx.repo
            .take_coding_cue(program, id)
            .await
            .unwrap()
            .is_none(),
        "a cue is taken once"
    );
}

#[tokio::test]
async fn respond_with_no_pending_cue_makes_no_model_call() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    ctx.clear_cue(&session).await;
    let after = ctx
        .sessions()
        .respond(&CodingSessionRequestDto {
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(ctx.aids.reply_calls.load(Ordering::SeqCst), 0);
    assert!(!after.cue_pending);
    assert!(after.turns.is_empty());
}

#[tokio::test]
async fn a_reply_becomes_the_counterparts_turn_and_the_cue_is_spent() {
    let ctx = Ctx::new().await;
    for (setting, speaker) in [
        (CodingSetting::Coached, LearningSimulationSpeaker::Coach),
        (CodingSetting::Mock, LearningSimulationSpeaker::Counterpart),
    ] {
        let session = ctx.start(setting).await;
        ctx.aids.queue(Ok(Some(CounterpartReply {
            content: "Walk me through the input.".into(),
            lines: vec![],
            path: None,
        })));
        let after = ctx
            .sessions()
            .respond(&CodingSessionRequestDto {
                program_id: ctx.program_id.clone(),
                session_id: session.session_id.clone(),
            })
            .await
            .unwrap();
        assert_eq!(after.turns.len(), 1, "{setting:?}");
        assert_eq!(after.turns[0].speaker, speaker);
        assert_eq!(after.turns[0].content, "Walk me through the input.");
        assert_eq!(after.turns[0].model_name.as_deref(), Some("fake-model"));
        assert!(!after.cue_pending);
        let bundle = ctx
            .repo
            .coding_session(&ctx.program_id, &session.session_id)
            .await
            .unwrap();
        assert!(bundle.events.iter().any(|event| event.kind == "draft_seen"));
    }
}

#[tokio::test]
async fn silence_stores_no_turn_but_spends_the_cue() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Watched).await;
    ctx.aids.queue(Ok(None));
    let after = ctx
        .sessions()
        .respond(&CodingSessionRequestDto {
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
        })
        .await
        .unwrap();
    assert!(after.turns.is_empty());
    assert!(!after.cue_pending);
}

#[tokio::test]
async fn a_failed_respond_puts_the_cue_back_and_returns_the_error() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    ctx.aids.queue(Err(AppError::ServiceNotAvailable(
        "the model is busy".into(),
    )));
    let request = CodingSessionRequestDto {
        program_id: ctx.program_id.clone(),
        session_id: session.session_id.clone(),
    };
    let failed = ctx.sessions().respond(&request).await;
    assert!(
        matches!(failed, Err(AppError::ServiceNotAvailable(_))),
        "{failed:?}"
    );
    assert!(
        ctx.get(&session).await.cue_pending,
        "the screen can try again"
    );

    ctx.aids.queue(Ok(Some(CounterpartReply {
        content: "Go on.".into(),
        lines: vec![],
        path: None,
    })));
    let retried = ctx.sessions().respond(&request).await.unwrap();
    assert_eq!(retried.turns.len(), 1);
    assert_eq!(ctx.aids.reply_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_cue_that_arrived_during_a_failed_reply_is_not_overwritten() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    let (program, id) = (&ctx.program_id, &session.session_id);
    let taken = ctx
        .repo
        .take_coding_cue(program, id)
        .await
        .unwrap()
        .unwrap();
    ctx.repo
        .offer_coding_cue(program, id, &CodingCue::Submitted)
        .await
        .unwrap();
    ctx.repo
        .restore_coding_cue(program, id, &taken)
        .await
        .unwrap();
    let pending = ctx.repo.take_coding_cue(program, id).await.unwrap();
    assert!(matches!(pending, Some(CodingCue::Submitted)), "{pending:?}");
}

#[tokio::test]
async fn a_hint_is_stored_by_rung_and_a_stale_revision_conflicts() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Coached).await;
    let ctx = &ctx;
    let ask = |revision: i64| {
        let request = AskCodingHintRequestDto {
            operation_id: id(),
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            files: starter_files("def double(n):\n    pass\n"),
            expected_revision: revision,
        };
        async move { ctx.sessions().ask_hint(&request).await }
    };
    assert_eq!(session.next_rung, CodeHintRung::Nudge);
    let after = ask(session.revision).await.unwrap();
    assert_eq!(after.hints.len(), 1);
    assert_eq!(after.hints[0].rung, CodeHintRung::Nudge);
    assert_eq!(after.hints[0].stage, CodingStage::Understand);
    assert_eq!(after.next_rung, CodeHintRung::Idea);
    assert!(
        after.turns.is_empty(),
        "outside Watched a hint is not in the thread"
    );

    let stale = ask(session.revision).await;
    assert!(is_conflict(&stale), "{stale:?}");
    assert_eq!(
        ask(after.revision).await.unwrap().hints[1].rung,
        CodeHintRung::Idea
    );
}

#[tokio::test]
async fn in_watched_a_hint_is_also_the_interviewers_turn() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Watched).await;
    let after = ctx
        .sessions()
        .ask_hint(&AskCodingHintRequestDto {
            operation_id: id(),
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            files: starter_files("def double(n):\n    pass\n"),
            expected_revision: session.revision,
        })
        .await
        .unwrap();
    assert!(after.hints[0].in_role);
    assert_eq!(after.turns.len(), 1);
    assert_eq!(
        after.turns[0].speaker,
        LearningSimulationSpeaker::Counterpart
    );
}

#[tokio::test]
async fn a_learner_turn_is_stored_with_the_draft_and_cues_the_counterpart() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    ctx.clear_cue(&session).await;
    let ctx = &ctx;
    let send = |revision: i64| {
        let request = SendCodingTurnRequestDto {
            operation_id: id(),
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            content: "  I will double the input.  ".into(),
            spoken: false,
            files: starter_files("def double(n):\n    return 0\n"),
            expected_revision: revision,
        };
        async move { ctx.sessions().send_turn(&request).await }
    };
    let after = send(session.revision).await.unwrap();
    assert_eq!(after.turns.len(), 1);
    assert_eq!(after.turns[0].speaker, LearningSimulationSpeaker::Learner);
    assert_eq!(after.turns[0].content, "I will double the input.");
    assert_eq!(after.draft, starter_files("def double(n):\n    return 0\n"));
    assert!(after.cue_pending);
    assert!(is_conflict(&send(session.revision).await));

    let blank = ctx
        .sessions()
        .send_turn(&SendCodingTurnRequestDto {
            operation_id: id(),
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            content: "   ".into(),
            spoken: false,
            files: vec![],
            expected_revision: after.revision,
        })
        .await;
    assert!(blank.is_err());
}

#[tokio::test]
async fn an_observed_draft_is_saved_and_idle_waits_for_a_quiet_spell() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Coached).await;
    ctx.clear_cue(&session).await;
    let ctx = &ctx;
    let observe = |reason: DraftObservation, content: &str| {
        let request = ObserveCodingDraftRequestDto {
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            files: starter_files(content),
            reason,
        };
        async move { ctx.sessions().observe_draft(&request).await.unwrap() }
    };

    let saved = observe(DraftObservation::Pause, "def double(n):\n    return n\n").await;
    assert_eq!(saved.draft, starter_files("def double(n):\n    return n\n"));

    ctx.clock.advance(super::session::IDLE_AFTER_MS - 1);
    assert!(
        !observe(DraftObservation::Idle, "def double(n):\n    return n\n")
            .await
            .cue_pending
    );
    ctx.clock.advance(2);
    assert!(
        observe(DraftObservation::Idle, "def double(n):\n    return n\n")
            .await
            .cue_pending,
        "a stage move 180 s ago no longer counts"
    );
}

#[tokio::test]
async fn a_pause_over_an_unchanged_draft_cues_nothing_and_a_changed_one_cues_a_checkpoint() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Watched).await;
    ctx.clear_cue(&session).await;
    let ctx = &ctx;
    let pause = |content: &str| {
        let request = ObserveCodingDraftRequestDto {
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            files: starter_files(content),
            reason: DraftObservation::Pause,
        };
        async move { ctx.sessions().observe_draft(&request).await.unwrap() }
    };
    assert!(!pause(&problem().spec.starter).await.cue_pending);
    assert!(
        pause("def double(n):\n    return n * 2\n")
            .await
            .cue_pending
    );
}

#[tokio::test]
async fn changing_the_setting_logs_it_and_gives_the_budget_a_session_lacked() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Coached).await;
    let ctx = &ctx;
    let change = |setting: CodingSetting, revision: i64| {
        let request = SetCodingSettingRequestDto {
            operation_id: id(),
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            setting,
            expected_revision: revision,
        };
        async move { ctx.sessions().set_setting(&request).await }
    };
    let mock = change(CodingSetting::Mock, session.revision).await.unwrap();
    assert_eq!(mock.setting, CodingSetting::Mock);
    assert_eq!(
        mock.clock.budget_seconds,
        Some(default_budget_seconds(ProblemDifficulty::Medium))
    );
    assert_eq!(
        mock.settings_used,
        [CodingSetting::Coached, CodingSetting::Mock]
    );
    assert!(is_conflict(
        &change(CodingSetting::Watched, session.revision).await
    ));
    let back = change(CodingSetting::Coached, mock.revision).await.unwrap();
    assert_eq!(
        back.settings_used,
        [CodingSetting::Coached, CodingSetting::Mock]
    );
}

#[tokio::test]
async fn finishing_twice_writes_once_and_shows_everything_in_the_debrief() {
    let ctx = Ctx::new().await;
    ctx.runs.failing.lock().unwrap().push("h1".into());
    let session = ctx.start(CodingSetting::Mock).await;
    let submitted = ctx.run(&session, CodingRunScope::All, None).await.unwrap();
    assert!(submitted.debrief.is_none());

    let first = ctx.finish(&submitted, &id()).await.unwrap();
    assert!(first.finished);
    assert_eq!(ctx.debriefer.calls.load(Ordering::SeqCst), 1);
    let debrief = first.debrief.as_ref().unwrap();
    assert_eq!(debrief.scorecard.rows.len(), 2);
    assert_eq!(debrief.model, "fake-model");
    assert_eq!(debrief.strong_answer.approach, "Multiply by two.");
    assert_eq!(debrief.strong_answer.reference, problem().reference);
    let hidden = debrief
        .cases
        .iter()
        .find(|each| each.case_id == "h1")
        .unwrap();
    assert_eq!(
        hidden.args,
        Some(vec!["0".into()]),
        "the debrief keeps nothing back"
    );
    assert_eq!(debrief.misreadings.len(), 2);
    assert_eq!(
        first.worked_solution.as_deref(),
        Some(problem().reference.as_str())
    );
    assert!(first.clock.open_since.is_none());

    let evidence = ctx
        .repo
        .coding_evidence_count(&submitted.session_id)
        .await
        .unwrap();
    assert_eq!(evidence, 1, "one event for the activity's one outcome");
    let schedules: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_drill_pattern_schedules WHERE material='problem:python'",
    )
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(schedules, problem().spec.patterns.len() as i64);

    let second = ctx.finish(&submitted, &id()).await.unwrap();
    assert!(second.finished);
    assert_eq!(
        ctx.debriefer.calls.load(Ordering::SeqCst),
        1,
        "no second debrief"
    );
    assert_eq!(
        ctx.repo
            .coding_evidence_count(&submitted.session_id)
            .await
            .unwrap(),
        1
    );
    let scorecards: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_scorecards WHERE session_id=?")
            .bind(&submitted.session_id)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(scorecards, 2);
    let schedules_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_drill_pattern_schedules WHERE material='problem:python'",
    )
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(schedules_after, schedules);
}

#[tokio::test]
async fn the_same_finish_operation_replays_and_a_finished_session_takes_no_changes() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Coached).await;
    let operation = id();
    let finished = ctx.finish(&session, &operation).await.unwrap();
    let again = ctx.finish(&finished, &operation).await.unwrap();
    assert_eq!(again.revision, finished.revision);

    let moved = ctx
        .move_to(&finished, CodingStage::Plan, finished.revision)
        .await;
    assert!(moved.is_err(), "{moved:?}");
    let ran = ctx.run(&finished, CodingRunScope::All, None).await;
    assert!(ran.is_err(), "{ran:?}");
}

#[tokio::test]
async fn the_ledger_refuses_an_operation_id_reused_for_another_request() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    let operation = id();
    let ctx = &ctx;
    let move_with = |stage: CodingStage| {
        let request = MoveCodingStageRequestDto {
            operation_id: operation.clone(),
            program_id: ctx.program_id.clone(),
            session_id: session.session_id.clone(),
            stage,
            expected_revision: session.revision,
        };
        async move { ctx.sessions().move_stage(&request).await }
    };
    move_with(CodingStage::Plan).await.unwrap();
    assert!(move_with(CodingStage::Code).await.is_err());
    let op = CodingOp {
        operation_id: &operation,
        kind: "stage",
        payload_hash: "other",
    };
    assert!(ctx
        .repo
        .coding_operation_seen(&ctx.program_id, &session.session_id, &op)
        .await
        .is_err());
}

#[tokio::test]
async fn what_is_due_lists_no_pattern_names() {
    let ctx = Ctx::new().await;
    let session = ctx.start(CodingSetting::Mock).await;
    ctx.finish(&session, &id()).await.unwrap();
    let due = ctx
        .sessions()
        .due(
            &PracticeDueRequestDto {
                program_id: ctx.program_id.clone(),
            },
            true,
        )
        .await
        .unwrap();
    for item in due.items {
        for pattern in PROBLEM_PATTERNS {
            assert!(!item.label.contains(pattern.name), "{}", item.label);
        }
    }
}

/// A due problem pattern in the course, then what the Practice page lists under the course's fit.
async fn due_with_fit(ctx: &Ctx) -> PracticeDueDto {
    sqlx::query(
        "INSERT INTO learning_drill_pattern_schedules(material,pattern_id,stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses) VALUES('problem:python',?,1.0,5.0,0,1,1,1,0)",
    )
    .bind(PROBLEM_PATTERNS[0].id)
    .execute(&ctx.pool)
    .await
    .unwrap();
    let practice = crate::practice_repository::LearningPracticeRepository::new(ctx.pool.clone());
    let code_fits = crate::practice::practice_fit::code_fits(&practice, &ctx.program_id)
        .await
        .unwrap();
    ctx.sessions()
        .due(
            &PracticeDueRequestDto {
                program_id: ctx.program_id.clone(),
            },
            code_fits,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn due_lists_no_problem_patterns_for_a_course_with_a_no_code_verdict() {
    let ctx = Ctx::new().await;
    let practice = crate::practice_repository::LearningPracticeRepository::new(ctx.pool.clone());
    let state = practice.fit_state(&ctx.program_id).await.unwrap();
    let basis = crate::practice::practice_fit::basis_of(&state.outline).unwrap();
    practice
        .record_fit(&ctx.program_id, &basis, false, "About speaking.", "test", 1)
        .await
        .unwrap();
    let due = due_with_fit(&ctx).await;
    assert!(due
        .items
        .iter()
        .all(|item| item.kind != DueKind::CodingPatterns));
}

#[tokio::test]
async fn due_lists_no_problem_patterns_while_the_fit_is_being_checked() {
    let ctx = Ctx::new().await;
    let due = due_with_fit(&ctx).await;
    assert!(due
        .items
        .iter()
        .all(|item| item.kind != DueKind::CodingPatterns));
}

#[tokio::test]
async fn due_lists_problem_patterns_for_a_course_with_a_profile_language() {
    let ctx = Ctx::new().await;
    crate::practice_repository::LearningPracticeRepository::new(ctx.pool.clone())
        .save_profile(&crate::dto::SaveLearningPracticeProfileRequestDto {
            program_id: ctx.program_id.clone(),
            details: "Backend interview".into(),
            language: Some("python".into()),
            interview_at: None,
            company: None,
            role: None,
            expected_revision: 0,
        })
        .await
        .unwrap();
    let due = due_with_fit(&ctx).await;
    assert!(due
        .items
        .iter()
        .any(|item| item.kind == DueKind::CodingPatterns));
}
