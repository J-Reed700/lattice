use crate::{
    dto::*,
    practice_generation::{grade, solution, tutor, validate_citations, NOT_JUDGED},
    practice_repository::{
        default_rubric, validate_tutor_bounds, LearningPracticeRepository, SubmissionWrite,
    },
    repository::LearningRepository,
};
use lattice_core::shared::persistence::Database;
use lattice_core::{
    application::ports::{
        llm_port::{CompletionRequest, CompletionResponse},
        LLMPort,
    },
    shared::error::{AppError, Result},
};
use serde_json::json;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn setup(pool: &SqlitePool, database: &Database) -> Result<(LearningProgramDto, String)> {
    sqlx::query("PRAGMA foreign_keys=ON").execute(pool).await?;
    let repo = LearningRepository::new(database.clone());
    let p = crate::tests::fixture();
    repo.create(&p).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: p.summary.id.clone(),
        expected_revision: 0,
        title: p.summary.title.clone(),
    })
    .await?;
    sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE program_id=?")
        .bind(&p.summary.id)
        .execute(pool)
        .await?;
    let ready = repo.get(&p.summary.id).await?;
    let lesson = ready
        .modules
        .first()
        .and_then(|m| m.lessons.first())
        .map(|l| l.id.clone())
        .ok_or_else(|| AppError::Database("fixture lesson missing".into()))?;
    Ok((ready, lesson))
}

async fn active_memory_pool() -> Result<SqlitePool> {
    crate::tests::pool().await
}
async fn file_pool(path: std::path::PathBuf) -> Result<SqlitePool> {
    let options = crate::test_support::file_options(path);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    Ok(pool)
}

fn start_request(p: &LearningProgramDto, lesson: &str) -> StartLearningPracticeSessionRequestDto {
    StartLearningPracticeSessionRequestDto {
        operation_id: id(),
        session_id: id(),
        program_id: p.summary.id.clone(),
        lesson_id: lesson.into(),
        expected_program_revision: p.summary.revision,
        mode: LearningPracticeMode::Practice,
        task_kind: LearningPracticeTaskKind::Independent,
        revises_session_id: None,
    }
}
fn save_request(
    program_id: &str,
    session_id: &str,
    operation_id: &str,
    expected_revision: i64,
    text: &str,
) -> SaveLearningPracticeArtifactRequestDto {
    SaveLearningPracticeArtifactRequestDto {
        operation_id: operation_id.into(),
        program_id: program_id.into(),
        session_id: session_id.into(),
        expected_revision,
        text: text.into(),
    }
}

#[tokio::test]
async fn attempt_session_and_artifact_survive_file_backed_reopen() -> Result<()> {
    let dir = tempfile::tempdir().map_err(|e| AppError::Database(e.to_string()))?;
    let path = dir.path().join("practice.sqlite");
    lattice_core::shared::testing::sqlite::write_migrated(&path).await?;
    let pool = file_pool(path.clone()).await?;
    let db = Database::new(pool.clone());
    let (p, lesson) = setup(&pool, &db).await?;
    let repo = LearningPracticeRepository::new(db.clone());
    let start = start_request(&p, &lesson);
    repo.start(&start, "start-hash").await?;
    let save = save_request(
        &p.summary.id,
        &start.session_id,
        &id(),
        0,
        "I can explain the idea with a new example.",
    );
    repo.save_artifact(&save, "save-hash").await?;
    db.close().await;
    let reopened = file_pool(path).await?;
    let session = LearningPracticeRepository::new(Database::new(reopened))
        .get_session(&start.session_id)
        .await?;
    assert_eq!(
        session.artifact.text,
        "I can explain the idea with a new example."
    );
    assert_eq!(session.artifact.revision, 1);
    assert_eq!(session.summary.source_version_ids, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn artifact_replay_is_idempotent_payload_checked_and_cas_protected() -> Result<()> {
    let pool = active_memory_pool().await?;
    let db = Database::new(pool.clone());
    let (p, lesson) = setup(&pool, &db).await?;
    let repo = LearningPracticeRepository::new(db);
    let start = start_request(&p, &lesson);
    repo.start(&start, "start").await?;
    let op = id();
    let original = save_request(&p.summary.id, &start.session_id, &op, 0, "durable answer");
    repo.save_artifact(&original, "payload-a").await?;
    let mut retry = original.clone();
    retry.expected_revision = 99;
    let replay = repo.save_artifact(&retry, "payload-a").await?;
    assert_eq!(replay.sessions.first().map(|s| s.revision), Some(1));
    assert!(repo
        .save_artifact(&original, "payload-different")
        .await
        .is_err());
    let stale = save_request(&p.summary.id, &start.session_id, &id(), 0, "stale answer");
    crate::tests::assert_conflict(repo.save_artifact(&stale, "stale").await);
    assert_eq!(
        repo.get_session(&start.session_id).await?.artifact.text,
        "durable answer"
    );
    Ok(())
}

#[tokio::test]
async fn stale_session_writes_are_reported_as_conflicts() -> Result<()> {
    let pool = active_memory_pool().await?;
    let db = Database::new(pool.clone());
    let (p, lesson) = setup(&pool, &db).await?;
    let repo = LearningPracticeRepository::new(db);
    let start = start_request(&p, &lesson);
    repo.start(&start, "start").await?;
    let first = save_request(&p.summary.id, &start.session_id, &id(), 0, "first");
    repo.save_artifact(&first, "first").await?;
    crate::tests::assert_conflict(
        repo.validate_live_session(&p.summary.id, &start.session_id, 0, false)
            .await,
    );
    crate::tests::assert_conflict(
        repo.change_mode(
            &ChangeLearningPracticeModeRequestDto {
                operation_id: id(),
                program_id: p.summary.id.clone(),
                session_id: start.session_id.clone(),
                expected_revision: 0,
                mode: LearningPracticeMode::Explore,
            },
            "stale-mode",
        )
        .await,
    );
    let submit = SubmitLearningPracticeAttemptRequestDto {
        operation_id: id(),
        program_id: p.summary.id.clone(),
        session_id: start.session_id.clone(),
        expected_revision: 0,
    };
    crate::tests::assert_conflict(
        repo.submit(
            &submit,
            SubmissionWrite {
                payload_hash: "stale-submit".into(),
                operation_id: submit.operation_id.clone(),
                grade_status: LearningPracticeGradeStatus::Uncertain,
                criteria: vec![],
                evidence: vec![],
                grader_model: "fixture-model".into(),
            },
            &[],
        )
        .await,
    );
    Ok(())
}

#[tokio::test]
async fn demonstrate_mode_blocks_all_selected_aids_and_session_stays_frozen() -> Result<()> {
    let pool = active_memory_pool().await?;
    let db = Database::new(pool.clone());
    let (p, lesson) = setup(&pool, &db).await?;
    let repo = LearningPracticeRepository::new(db);
    let mut start = start_request(&p, &lesson);
    start.mode = LearningPracticeMode::Demonstrate;
    repo.start(&start, "start").await?;
    for _aid in ["source_open", "tutor", "solution"] {
        assert!(repo
            .validate_live_session(&p.summary.id, &start.session_id, 0, false)
            .await
            .is_err());
    }
    let open = OpenLearningPracticeSourceRequestDto {
        operation_id: id(),
        program_id: p.summary.id.clone(),
        session_id: start.session_id.clone(),
        expected_revision: 0,
        source_id: id(),
        version_id: id(),
    };
    assert!(repo.open_source(&open, "open").await.is_err());
    let before = repo.get_session(&start.session_id).await?;
    assert_eq!(before.summary.mode, LearningPracticeMode::Demonstrate);
    assert_eq!(before.summary.revision, 0);
    Ok(())
}

#[tokio::test]
async fn submitted_result_is_an_immutable_artifact_snapshot() -> Result<()> {
    let pool = active_memory_pool().await?;
    let db = Database::new(pool.clone());
    let (p, lesson) = setup(&pool, &db).await?;
    let repo = LearningPracticeRepository::new(db);
    let start = start_request(&p, &lesson);
    repo.start(&start, "start").await?;
    let save = save_request(
        &p.summary.id,
        &start.session_id,
        &id(),
        0,
        "submitted wording",
    );
    repo.save_artifact(&save, "save").await?;
    let session = repo.get_session(&start.session_id).await?;
    let criteria = session
        .rubric
        .iter()
        .map(|c| LearningPracticeCriterionResultDto {
            criterion_id: c.id.clone(),
            dimension: c.dimension.clone(),
            score: None,
            max_points: c.max_points,
            observation: "Insufficient evidence for a reliable score.".into(),
            evidence_quote: None,
        })
        .collect::<Vec<_>>();
    let evidence = session
        .rubric
        .iter()
        .map(|c| LearningPracticeEvidenceEventDto {
            dimension: c.dimension.clone(),
            observed: false,
            observation: "Insufficient evidence.".into(),
            evidence_quote: None,
            assistance_kinds: vec![],
        })
        .collect();
    let req = SubmitLearningPracticeAttemptRequestDto {
        operation_id: id(),
        program_id: p.summary.id.clone(),
        session_id: start.session_id.clone(),
        expected_revision: 1,
    };
    repo.submit(
        &req,
        SubmissionWrite {
            payload_hash: "submit".into(),
            operation_id: req.operation_id.clone(),
            grade_status: LearningPracticeGradeStatus::Uncertain,
            criteria,
            evidence,
            grader_model: "fixture-model".into(),
        },
        &session.rubric,
    )
    .await?;
    assert!(repo
        .save_artifact(
            &save_request(&p.summary.id, &start.session_id, &id(), 2, "rewrite"),
            "rewrite"
        )
        .await
        .is_err());
    let after = repo.get_session(&start.session_id).await?;
    let result = after
        .result
        .ok_or_else(|| AppError::Database("missing submitted result".into()))?;
    assert_eq!(result.artifact_text, "submitted wording");
    assert_eq!(result.grade_status, LearningPracticeGradeStatus::Uncertain);
    assert_eq!(result.evidence.len(), 4);
    let revision = StartLearningPracticeSessionRequestDto {
        operation_id: id(),
        session_id: id(),
        program_id: p.summary.id.clone(),
        lesson_id: lesson.clone(),
        expected_program_revision: p.summary.revision,
        mode: LearningPracticeMode::Practice,
        task_kind: LearningPracticeTaskKind::Independent,
        revises_session_id: Some(start.session_id.clone()),
    };
    repo.start(&revision, "revision-start").await?;
    let revised = repo.get_session(&revision.session_id).await?;
    assert_eq!(revised.artifact.text, "submitted wording");
    assert_eq!(
        revised.summary.revises_session_id.as_deref(),
        Some(start.session_id.as_str())
    );
    assert_eq!(revised.assistance.len(), 1);
    assert!(revised.assistance[0]
        .details
        .get("previousFeedback")
        .is_some());
    assert_eq!(
        repo.get_session(&start.session_id).await?.summary.status,
        LearningPracticeSessionStatus::Submitted
    );
    assert!(repo
        .change_mode(
            &ChangeLearningPracticeModeRequestDto {
                operation_id: id(),
                program_id: p.summary.id,
                session_id: revision.session_id,
                expected_revision: 0,
                mode: LearningPracticeMode::Demonstrate
            },
            "revision-mode"
        )
        .await
        .is_err());
    Ok(())
}

#[test]
fn citations_must_be_exact_quotes_from_the_frozen_owned_version() {
    let source = LearningSourceVersionDto {
        source_id: "logical-source".into(),
        version: LearningSourceVersionSummaryDto {
            id: "version-1".into(),
            version_number: 1,
            title: "Stored source".into(),
            publisher: None,
            resolved_url: None,
            excerpt: "café passage".into(),
            content_sha256: "0".repeat(64),
            word_count: 2,
            truncated: false,
            extraction_version: "test".into(),
            acquired_at: 1,
            snapshot_hash: None,
        },
        full_text: "The café passage supports this.".into(),
        usage: vec![],
    };
    let valid = LearningPracticeCitationDto {
        source_id: "logical-source".into(),
        version_id: "version-1".into(),
        quote: "café passage".into(),
        number: 1,
        support: Default::default(),
    };
    assert!(
        validate_citations(std::slice::from_ref(&valid), std::slice::from_ref(&source)).is_ok()
    );
    let wrong = LearningPracticeCitationDto {
        quote: "cafe passage".into(),
        ..valid.clone()
    };
    assert!(validate_citations(&[wrong], std::slice::from_ref(&source)).is_err());
    let cross = LearningPracticeCitationDto {
        version_id: "new-version".into(),
        ..valid
    };
    assert!(validate_citations(&[cross], std::slice::from_ref(&source)).is_err());
    assert_eq!(default_rubric().len(), 4);
}

/// Every `maxLength`, and every `minLength` above one, in a model-facing
/// schema, each as its JSON pointer and value.
pub(crate) fn text_length_rules(schema: &serde_json::Value) -> Vec<String> {
    fn walk(value: &serde_json::Value, at: &str, found: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, child) in fields {
                    let here = format!("{at}/{key}");
                    match (key.as_str(), child.as_u64()) {
                        ("maxLength", Some(limit)) => found.push(format!("{here} = {limit}")),
                        ("minLength", Some(limit)) if limit > 1 => {
                            found.push(format!("{here} = {limit}"));
                        }
                        _ => walk(child, &here, found),
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    walk(child, &format!("{at}/{index}"), found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(schema, "", &mut found);
    found
}

/// A model that answers every request with one text and keeps each schema it
/// was asked to follow.
pub(crate) struct Reply {
    text: String,
    pub(crate) schemas: std::sync::Mutex<Vec<serde_json::Value>>,
    pub(crate) requests: std::sync::Mutex<Vec<CompletionRequest>>,
}

impl Reply {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            schemas: std::sync::Mutex::new(Vec::new()),
            requests: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl LLMPort for Reply {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request.clone());
        }
        if let Ok(mut schemas) = self.schemas.lock() {
            schemas.extend(request.schema().map(|schema| schema.as_json().clone()));
        }
        Ok(CompletionResponse::from_text(self.text.clone()))
    }
    fn model_name(&self) -> &str {
        "reply"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }
}

/// A frozen source version whose whole text is `text`.
pub(crate) fn passage_source(text: &str) -> LearningSourceVersionDto {
    LearningSourceVersionDto {
        source_id: "logical-source".into(),
        version: LearningSourceVersionSummaryDto {
            id: "version-1".into(),
            version_number: 1,
            title: "Stored source".into(),
            publisher: None,
            resolved_url: None,
            excerpt: text.chars().take(200).collect(),
            content_sha256: "0".repeat(64),
            word_count: text.split_whitespace().count(),
            truncated: false,
            extraction_version: "test".into(),
            acquired_at: 1,
            snapshot_hash: None,
        },
        full_text: text.into(),
        usage: vec![],
    }
}

async fn started_session(artifact: &str) -> Result<LearningPracticeSessionDto> {
    let pool = active_memory_pool().await?;
    let db = Database::new(pool.clone());
    let (p, lesson) = setup(&pool, &db).await?;
    let repo = LearningPracticeRepository::new(db);
    let start = start_request(&p, &lesson);
    repo.start(&start, "start").await?;
    let mut session = repo.get_session(&start.session_id).await?;
    session.artifact.text = artifact.into();
    Ok(session)
}

fn tutor_request() -> RequestLearningTutorResponseRequestDto {
    RequestLearningTutorResponseRequestDto {
        operation_id: id(),
        program_id: id(),
        session_id: id(),
        expected_revision: 0,
        request_kind: LearningTutorRequestKind::Question,
        prompt: "Why does this step hold?".into(),
        hint_level: None,
    }
}

fn tutor_reply(
    response: &str,
    citations: serde_json::Value,
    proposals: serde_json::Value,
) -> String {
    json!({"response": response, "citations": citations, "proposals": proposals}).to_string()
}

/// A grade answering the first `count` criteria of the default rubric, `c1` onward, in the layer's form.
fn grade_entries(
    count: usize,
    evidence: serde_json::Value,
    observation: &str,
    score: &str,
) -> String {
    let entries = (1..=count)
        .map(|at| {
            json!({"of": format!("c{at}"), "evidence": evidence, "observation": observation, "score": score})
        })
        .collect::<Vec<_>>();
    json!({ "entries": entries }).to_string()
}

#[tokio::test]
async fn a_tutor_reply_is_read_at_any_length() -> Result<()> {
    let evidence = "I tried the second approach first. ".repeat(18);
    let session = started_session(&evidence).await?;
    let passage = "The passage states one condition after another. ".repeat(20);
    let sources = [passage_source(&passage)];
    let cited = json!(["q1", "q2", "q3"]);
    let every_sentence: Vec<String> = (1..=18).map(|at| format!("u{at}")).collect();
    let proposal = |text: &str, evidence: &[String]| json!([{"kind": "misconception", "text": text, "evidence": evidence}]);
    let short = "Which step carries the weight?";
    let small = tutor_reply(short, json!([]), json!([]));
    let padded = format!("{small}{}", " ".repeat(20_001));
    let long_response = "Think about why the second step holds. ".repeat(180);
    let wordy = "Confuses the step with its converse. ".repeat(22);
    for (what, reply) in [
        ("a short reply", small),
        (
            "response",
            tutor_reply(&long_response, json!([]), json!([])),
        ),
        ("citations", tutor_reply(short, cited, json!([]))),
        (
            "proposal text",
            tutor_reply(short, json!([]), proposal(&wordy, &[])),
        ),
        (
            "proposal evidence",
            tutor_reply(short, json!([]), proposal(short, &every_sentence)),
        ),
        ("whitespace around the reply", padded),
    ] {
        let llm = Reply::new(reply);
        let read = tutor(&llm, &session, &tutor_request(), &sources).await;
        assert!(read.is_ok(), "{what}: {:?}", read.err());
    }
    Ok(())
}

#[tokio::test]
async fn a_reference_solution_is_read_at_any_length() -> Result<()> {
    let session = started_session("I can explain the idea.").await?;
    let passage = "The passage states one condition after another. ".repeat(20);
    let sources = [passage_source(&passage)];
    let reply = |text: &str, citations: serde_json::Value| {
        json!({"solution": text, "citations": citations}).to_string()
    };
    let short = "State the condition, then apply it.";
    let long = "Show the step, then the reason it is taken. ".repeat(160);
    let padded = format!("{}{}", reply(short, json!([])), " ".repeat(20_001));
    for (what, text) in [
        ("a short solution", reply(short, json!([]))),
        ("solution", reply(&long, json!([]))),
        ("citations", reply(short, json!(["q1", "q2", "q3"]))),
        ("whitespace around the reply", padded),
    ] {
        let llm = Reply::new(text);
        let read = solution(&llm, &session, &sources).await;
        assert!(read.is_ok(), "{what}: {:?}", read.err());
    }
    Ok(())
}

#[tokio::test]
async fn the_graders_observation_and_evidence_are_read_at_any_length() -> Result<()> {
    let artifact = "I tried the second approach first. ".repeat(18);
    let session = started_session(&artifact).await?;
    let short = "The artifact states the condition.";
    let observation = "The artifact states the condition but never says why it holds. ".repeat(18);
    let first = json!(["u1"]);
    let every_sentence: Vec<String> = (1..=18).map(|at| format!("u{at}")).collect();
    for (what, text, quote) in [
        (
            "short feedback",
            grade_entries(4, first.clone(), short, "l3"),
            "I tried the second approach first.",
        ),
        (
            "observation",
            grade_entries(4, first, &observation, "l3"),
            "I tried the second approach first.",
        ),
        (
            "evidence",
            grade_entries(4, json!(every_sentence), short, "l3"),
            artifact.trim(),
        ),
    ] {
        let llm = Reply::new(text);
        let (_, graded) = grade(&llm, &session).await?;
        assert!(
            graded
                .iter()
                .all(|c| c.evidence_quote.as_deref() == Some(quote)),
            "{what}"
        );
    }
    Ok(())
}

fn sent_by(llm: &Reply) -> Vec<CompletionRequest> {
    llm.requests.lock().expect("requests").clone()
}

#[tokio::test]
async fn prompt_snapshot_practice_tutor_reply() -> Result<()> {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let mut session = started_session("I tried the second approach first.").await?;
    session.tutor_turns.push(LearningPracticeTutorTurnDto {
        id: "turn-1".into(),
        prompt: "Where do I start?".into(),
        response: "What do you remember about the condition?".into(),
        request_kind: LearningTutorRequestKind::Question,
        hint_level: None,
        citations: vec![],
        proposal_ids: vec![],
        model_name: "reply".into(),
        created_at: 0,
    });
    let sources = [passage_source("The passage states one condition.")];
    let llm = Reply::new(tutor_reply(
        "Which step carries the weight?",
        json!([]),
        json!([]),
    ));
    tutor(&llm, &session, &tutor_request(), &sources).await?;
    snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "practice-tutor-reply",
        &sent_by(&llm),
    );
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_practice_reference_solution() -> Result<()> {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let session = started_session("I can explain the idea.").await?;
    let sources = [passage_source("The passage states one condition.")];
    let llm = Reply::new(json!({"solution": "Apply the condition.", "citations": []}).to_string());
    solution(&llm, &session, &sources).await?;
    snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "practice-reference-solution",
        &sent_by(&llm),
    );
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_practice_grade() -> Result<()> {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let session = started_session("I tried the second approach first.").await?;
    let llm = Reply::new(grade_entries(
        4,
        json!(["u1"]),
        "The artifact states the condition.",
        "l3",
    ));
    grade(&llm, &session).await?;
    snapshot::assert_requests(env!("CARGO_MANIFEST_DIR"), "practice-grade", &sent_by(&llm));
    Ok(())
}

#[tokio::test]
async fn the_practice_requests_ask_for_no_text_length() -> Result<()> {
    let session = started_session("I can explain the idea.").await?;
    let sources = [passage_source("The passage states one condition.")];
    let tutoring = Reply::new(tutor_reply(
        "Which step carries the weight?",
        json!([]),
        json!([]),
    ));
    tutor(&tutoring, &session, &tutor_request(), &sources).await?;
    let solving =
        Reply::new(json!({"solution": "Apply the condition.", "citations": []}).to_string());
    solution(&solving, &session, &sources).await?;
    let grading = Reply::new(grade_entries(4, json!([]), "Not enough to judge.", "none"));
    grade(&grading, &session).await?;
    for llm in [&tutoring, &solving, &grading] {
        let schemas = llm.schemas.lock().expect("schemas").clone();
        assert!(!schemas.is_empty());
        for schema in &schemas {
            let rules = text_length_rules(schema);
            assert!(rules.is_empty(), "{rules:?}");
        }
    }
    Ok(())
}

mod contracts {
    use super::*;
    use crate::practice_generation::{
        Criterion, GradeInput, Passage, PracticeGrade, ReferenceSolution, SolutionInput, Turn,
        TutorInput, TutorReply, Verdict,
    };
    use futures::future::BoxFuture;
    use lattice_core::application::services::model_contract::packed::Packed;
    use lattice_core::application::services::model_contract::shape::{Each, Held, Shape};
    use lattice_core::application::services::model_contract::shown::{Scope, Told};
    use lattice_core::application::services::model_contract::test_support::snapshot::rendered;
    use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
    use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};

    const STEP: &str = "The first step is to hold the damper shut.";
    const TIDE: &str = "The second step holds when the tide turns.";

    fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
        Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
    }

    /// Everything a request says.
    fn prompt_of(sent: &Sent) -> String {
        rendered(&sent.request)
    }

    /// The handle a request shows in front of `text`.
    fn handle_before(prompt: &str, text: &str) -> String {
        let at = prompt.find(text).expect("the text is shown");
        let head = &prompt[..at];
        let open = head.rfind('[').expect("a handle stands before the text");
        head[open + 1..].trim_end().trim_end_matches(']').to_owned()
    }

    /// The task a request carries: the last user message, a JSON object.
    fn task_of(sent: &Sent) -> serde_json::Value {
        serde_json::from_str(sent.request.user_text()).expect("a task")
    }

    fn tutoring(answer: impl Fn(&str, usize) -> String + Send + Sync + 'static) -> ScriptedModel {
        ScriptedModel::new()
            .on_request::<TutorReply>(move |sent, nth| reply(answer(&prompt_of(sent), nth)))
    }

    fn grading(answer: impl Fn(&str, usize) -> String + Send + Sync + 'static) -> ScriptedModel {
        ScriptedModel::new()
            .on_request::<PracticeGrade>(move |sent, nth| reply(answer(&prompt_of(sent), nth)))
    }

    fn criterion(id: &str, max_points: i64) -> LearningPracticeCriterionDto {
        LearningPracticeCriterionDto {
            id: id.into(),
            dimension: LearningPracticeEvidenceDimension::Recall,
            title: format!("Criterion {id}"),
            description: format!("What {id} asks of the learner."),
            max_points,
        }
    }

    fn entry(of: &str, evidence: serde_json::Value, score: &str) -> serde_json::Value {
        json!({"of": of, "evidence": evidence, "observation": "The artifact shows the step.", "score": score})
    }

    #[test]
    fn the_practice_requests_tell_what_they_should_and_hold_no_identity() {
        let passages = |marks: &mut Marks| {
            Packed::new(
                vec![Passage {
                    at: 0,
                    source_id: marks.internal("source-id-sentinel-001".to_owned()),
                    version_id: marks.internal("version-id-sentinel-002".to_owned()),
                    title: marks.told("A harbour almanac".to_owned()),
                    text: marks.told("The kiln is fired at dawn before the tide.".to_owned()),
                }],
                |_| 1,
            )
        };
        let mut marks = Marks::new();
        let turns = Packed::new(
            vec![Turn {
                entry: marks.told(json!({"request": "Where do I begin with the tide?", "response": "What do you remember about the tide?"})),
                tokens: 3,
            }],
            |turn| turn.tokens,
        );
        let tutoring = TutorInput {
            task_prompt: marks.told("Chart the harbour's tide tables.".to_owned()),
            objective: marks.told("Read a tide table correctly.".to_owned()),
            artifact: marks.told("I shut the damper before the tide turned.".to_owned()),
            rubric: marks.told(json!([{"title": "Recall of the tide table"}])),
            mode: marks.told(json!("practice")),
            revision_context: marks.told(vec![
                json!({"previousFeedback": "The earlier table was incomplete."}),
            ]),
            request_kind: marks.told(json!("question")),
            hint_level: Told(json!(null)),
            learner_request: marks.told("Why does this step hold?".to_owned()),
            turns,
            turns_held: 1,
            passages: passages(&mut marks),
        };
        holds::<TutorReply>(&tutoring, &marks);
        let mut marks = Marks::new();
        let solving = SolutionInput {
            task_prompt: marks.told("Chart the harbour's tide tables.".to_owned()),
            objective: marks.told("Read a tide table correctly.".to_owned()),
            passages: passages(&mut marks),
        };
        holds::<ReferenceSolution>(&solving, &marks);
        let mut marks = Marks::new();
        let grading = GradeInput {
            task_prompt: marks.told("Chart the harbour's tide tables.".to_owned()),
            artifact: marks.told("I shut the damper before the tide turned.".to_owned()),
            criteria: vec![Criterion {
                criterion: LearningPracticeCriterionDto {
                    id: marks.reference("criterion-id-sentinel-003".to_owned()),
                    dimension: LearningPracticeEvidenceDimension::Recall,
                    title: marks.told("Recall of the tide table".to_owned()).0,
                    description: marks.told("States the tide table correctly.".to_owned()).0,
                    max_points: 2,
                },
            }],
        };
        holds::<PracticeGrade>(&grading, &marks);
    }

    #[tokio::test]
    async fn a_tutor_citation_is_the_sentence_the_app_holds() -> Result<()> {
        let session = started_session("I tried the second approach first.").await?;
        let full_text = format!("{STEP} {TIDE}");
        let sources = [passage_source(&full_text)];
        let model = tutoring(|prompt, _| {
            tutor_reply(
                "Which step carries the weight?",
                json!([handle_before(prompt, TIDE)]),
                json!([]),
            )
        });
        let said = tutor(&model, &session, &tutor_request(), &sources).await?;
        assert_eq!(
            said.citations,
            vec![LearningPracticeCitationDto {
                source_id: "logical-source".into(),
                version_id: "version-1".into(),
                quote: TIDE.into(),
                number: 1,
                support: lattice_core::application::contracts::replies::SupportDto::verify_quote(
                    Some(&full_text),
                    TIDE,
                ),
            }]
        );
        let prompt = prompt_of(&model.sent()[0]);
        assert!(!prompt.contains("logical-source") && !prompt.contains("version-1"));
        Ok(())
    }

    #[tokio::test]
    async fn a_citation_naming_nothing_shown_costs_only_that_cite() -> Result<()> {
        let session = started_session("I tried the second approach first.").await?;
        let sources = [passage_source(&format!("{STEP} {TIDE}"))];
        let model = tutoring(|prompt, _| {
            tutor_reply(
                "Which step carries the weight?",
                json!([handle_before(prompt, TIDE), "q99", "u1"]),
                json!([]),
            )
        });
        let said = tutor(&model, &session, &tutor_request(), &sources).await?;
        assert_eq!(said.citations.len(), 1);
        assert_eq!(said.citations[0].quote, TIDE);
        assert_eq!(model.count::<TutorReply>(), 1);
        Ok(())
    }

    fn handles_under(schema: &serde_json::Value, property: &str, found: &mut Vec<String>) {
        match schema {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "properties" {
                        if let Some(inner) = value.get(property) {
                            strings_of(inner, found);
                        }
                    }
                    handles_under(value, property, found);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    handles_under(item, property, found);
                }
            }
            _ => {}
        }
    }

    fn strings_of(value: &serde_json::Value, found: &mut Vec<String>) {
        match value {
            serde_json::Value::String(text) => found.push(text.clone()),
            serde_json::Value::Object(map) => map.values().for_each(|v| strings_of(v, found)),
            serde_json::Value::Array(items) => items.iter().for_each(|v| strings_of(v, found)),
            _ => {}
        }
    }

    #[tokio::test]
    async fn citations_name_only_passage_sentences_and_evidence_only_the_artifact() -> Result<()> {
        let session = started_session("I tried the second approach first.").await?;
        let sources = [passage_source(&format!("{STEP} {TIDE}"))];
        let model =
            tutoring(|_, _| tutor_reply("Which step carries the weight?", json!([]), json!([])))
                .enforcing();
        tutor(&model, &session, &tutor_request(), &sources).await?;
        let sent = model.sent();
        let schema = sent[0]
            .request
            .schema()
            .expect("a schema")
            .as_json()
            .clone();
        let (mut cited, mut resting) = (Vec::new(), Vec::new());
        handles_under(&schema, "citations", &mut cited);
        handles_under(&schema, "evidence", &mut resting);
        let handles = |found: &[String], prefix: &str| {
            found
                .iter()
                .filter(|text| text.len() > 1 && text[1..].chars().all(|c| c.is_ascii_digit()))
                .all(|text| text.starts_with(prefix))
        };
        assert!(cited.iter().any(|text| text.starts_with('q')), "{cited:?}");
        assert!(handles(&cited, "q"), "{cited:?}");
        assert!(
            resting.iter().any(|text| text.starts_with('u')),
            "{resting:?}"
        );
        assert!(handles(&resting, "u"), "{resting:?}");
        Ok(())
    }

    #[tokio::test]
    async fn a_proposal_rests_on_the_learners_own_sentences() -> Result<()> {
        const NEVER: &str = "I never checked the tide table.";
        let session = started_session(&format!("I shut the damper first. {NEVER}")).await?;
        let model = tutoring(|prompt, _| {
            tutor_reply(
                "Which step carries the weight?",
                json!([]),
                json!([
                    {"kind": "misconception", "text": "The table was skipped.", "evidence": [handle_before(prompt, NEVER)]},
                    {"kind": "follow_up", "text": "Try the table next.", "evidence": []}
                ]),
            )
        });
        let said = tutor(&model, &session, &tutor_request(), &[]).await?;
        assert_eq!(said.proposals[0].evidence_quote.as_deref(), Some(NEVER));
        assert_eq!(said.proposals[1].evidence_quote, None);
        Ok(())
    }

    #[tokio::test]
    async fn a_blank_tutor_reply_is_corrected_with_the_reason_and_asked_for_again() -> Result<()> {
        let session = started_session("I tried the second approach first.").await?;
        let model = tutoring(|_, nth| {
            tutor_reply(
                if nth == 0 {
                    "  "
                } else {
                    "Which step carries the weight?"
                },
                json!([]),
                json!([]),
            )
        });
        let said = tutor(&model, &session, &tutor_request(), &[]).await?;
        assert_eq!(said.response, "Which step carries the weight?");
        let sent = model.sent();
        assert_eq!(sent.len(), 2);
        assert!(prompt_of(&sent[1]).contains("Write the tutor's reply in response."));
        Ok(())
    }

    #[tokio::test]
    async fn an_unreadable_tutor_answer_fails_the_request_after_one_correction() -> Result<()> {
        let session = started_session("I tried the second approach first.").await?;
        let model = tutoring(|_, _| "The step holds.".to_owned());
        let failed = tutor(&model, &session, &tutor_request(), &[]).await;
        assert!(
            matches!(failed, Err(AppError::ModelAnswerUnusable(_))),
            "{failed:?}"
        );
        assert_eq!(model.count::<TutorReply>(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn the_tutors_history_fills_half_the_room_and_the_older_turns_are_counted() -> Result<()>
    {
        let turn = |index: usize| LearningPracticeTutorTurnDto {
            id: format!("turn-{index}"),
            prompt: format!("Question {index}"),
            response: "word ".repeat(200),
            request_kind: LearningTutorRequestKind::Question,
            hint_level: None,
            citations: vec![],
            proposal_ids: vec![],
            model_name: "scripted".into(),
            created_at: index as i64,
        };
        let model = || {
            ScriptedModel::new()
                .window(131_072)
                .on_request::<TutorReply>(|_, _| {
                    reply(tutor_reply(
                        "Which step carries the weight?",
                        json!([]),
                        json!([]),
                    ))
                })
        };
        let mut session = started_session("I tried the second approach first.").await?;
        session.tutor_turns = (0..6).map(turn).collect();
        let few = model();
        tutor(&few, &session, &tutor_request(), &[]).await?;
        let shown = task_of(&few.sent()[0]);
        assert_eq!(shown["recentTurns"].as_array().map(Vec::len), Some(6));
        assert_eq!(shown["earlierTurns"], "");
        session.tutor_turns = (0..1_000).map(turn).collect();
        let many = model();
        tutor(&many, &session, &tutor_request(), &[]).await?;
        let shown = task_of(&many.sent()[0]);
        let kept = shown["recentTurns"].as_array().expect("turns");
        assert!(kept.len() > 6 && kept.len() < 1_000);
        assert_eq!(kept[0]["request"], "Question 999");
        assert_eq!(
            shown["earlierTurns"],
            format!("{} earlier turns not shown", 1_000 - kept.len())
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_solution_cites_sentences_and_a_blank_one_is_asked_for_again() -> Result<()> {
        let session = started_session("I can explain the idea.").await?;
        let sources = [passage_source(&format!("{STEP} {TIDE}"))];
        let model = ScriptedModel::new().on_request::<ReferenceSolution>(|sent, nth| {
            let solution = if nth == 0 {
                " "
            } else {
                "Hold the damper, then wait."
            };
            reply(
                json!({"solution": solution, "citations": [handle_before(&prompt_of(sent), STEP)]})
                    .to_string(),
            )
        });
        let said = solution(&model, &session, &sources).await?;
        assert_eq!(said.solution, "Hold the damper, then wait.");
        assert_eq!(said.citations.len(), 1);
        assert_eq!(said.citations[0].quote, STEP);
        assert_eq!(said.citations[0].version_id, "version-1");
        assert_eq!(model.count::<ReferenceSolution>(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn a_score_is_one_of_its_own_criterions_levels() -> Result<()> {
        let mut session = started_session("I shut the damper first.").await?;
        session.rubric = vec![criterion("recall", 2), criterion("reasoning", 4)];
        let model = grading(|prompt, nth| {
            let mut entries = vec![];
            if nth == 0 {
                entries.push(entry("c1", json!(["u1"]), "l3"));
                entries.push(entry("c2", json!(["u1"]), "l9"));
            } else if !prompt.contains("\"id\":\"c1\"") {
                entries.push(entry("c2", json!(["u1"]), "l5"));
            } else {
                entries.push(entry("c1", json!(["u1"]), "l3"));
            }
            json!({ "entries": entries }).to_string()
        });
        let (status, graded) = grade(&model, &session).await?;
        assert_eq!(status, LearningPracticeGradeStatus::Provisional);
        assert_eq!(graded[0].score, Some(2));
        assert_eq!(graded[0].max_points, 2);
        assert_eq!(graded[1].score, Some(4));
        let sent = model.sent();
        assert_eq!(sent.len(), 2);
        let rubric = task_of(&sent[0])["rubric"].clone();
        assert_eq!(rubric[0]["scores"].as_array().map(Vec::len), Some(3));
        assert_eq!(rubric[1]["scores"].as_array().map(Vec::len), Some(5));
        assert!(!prompt_of(&sent[1]).contains("\"id\":\"c1\""));
        Ok(())
    }

    #[tokio::test]
    async fn no_score_is_cannot_tell_and_a_grade_with_none_is_uncertain() -> Result<()> {
        let session = started_session("I am not sure how to start.").await?;
        let model = grading(|_, _| grade_entries(4, json!([]), "Not enough to judge.", "none"));
        let (status, graded) = grade(&model, &session).await?;
        assert_eq!(status, LearningPracticeGradeStatus::Uncertain);
        assert!(graded
            .iter()
            .all(|c| c.score.is_none() && c.evidence_quote.is_none()));
        assert_eq!(model.count::<PracticeGrade>(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn a_score_without_evidence_is_corrected_for_that_criterion_alone() -> Result<()> {
        let mut session = started_session("I shut the damper first.").await?;
        session.rubric = vec![criterion("recall", 2), criterion("reasoning", 2)];
        let model = grading(|prompt, nth| {
            let mut entries = vec![];
            if nth == 0 {
                entries.push(entry("c1", json!([]), "l2"));
                entries.push(entry("c2", json!(["u1"]), "l1"));
            } else if prompt.contains("\"id\":\"c1\"") {
                entries.push(entry("c1", json!(["u1"]), "l2"));
            }
            json!({ "entries": entries }).to_string()
        });
        let (_, graded) = grade(&model, &session).await?;
        assert_eq!(graded[0].score, Some(1));
        assert_eq!(
            graded[0].evidence_quote.as_deref(),
            Some("I shut the damper first.")
        );
        assert_eq!(graded[1].score, Some(0));
        let sent = model.sent();
        assert_eq!(sent.len(), 2);
        assert!(prompt_of(&sent[1]).contains("cites at least one sentence"));
        Ok(())
    }

    #[tokio::test]
    async fn a_criterion_the_grader_never_judges_is_not_judged_and_the_others_stand() -> Result<()>
    {
        let session = started_session("I shut the damper first.").await?;
        let model =
            grading(|_, _| json!({"entries": [entry("c1", json!(["u1"]), "l3")]}).to_string());
        let (status, graded) = grade(&model, &session).await?;
        assert_eq!(status, LearningPracticeGradeStatus::Provisional);
        assert_eq!(graded[0].score, Some(2));
        for result in &graded[1..] {
            assert_eq!(result.score, None);
            assert_eq!(result.observation, NOT_JUDGED);
            assert_eq!(result.evidence_quote, None);
        }
        Ok(())
    }

    /// A constrained decoder writes a reply in schema order. With the score first,
    /// the 27B answered a plainly wrong artifact with `uncertain` and no score, 9
    /// times in 9, against a prompt that says zero is an observed wrong attempt.
    #[test]
    fn the_grader_writes_its_grounds_before_it_scores() {
        let criteria = vec![Criterion {
            criterion: criterion("recall", 2),
        }];
        let mut scope = Scope::for_question();
        let mut showing = scope.request();
        showing.show(&criteria).expect("the criteria are shown");
        let schema =
            <Each<Criterion, Verdict> as Shape>::schema(&showing, Held::Plain).expect("a schema");
        assert_eq!(
            schema
                .as_json()
                .pointer("/properties/entries/items/required"),
            Some(&json!(["of", "evidence", "observation", "score"]))
        );
    }
}

#[test]
fn a_tutor_response_is_stored_however_long() {
    let long = "A step and the reason it is taken. ".repeat(250);
    assert!(validate_tutor_bounds("Why does this step hold?", &long).is_ok());
    assert!(validate_tutor_bounds("Why does this step hold?", "  ").is_err());
}

#[test]
fn a_learners_pasted_tutor_prompt_is_held_to_the_chat_limit() {
    use lattice_core::shared::constants::MAX_CHAT_MESSAGE_CHARS;
    let pasted_code = "def step(x):\n    return x + 1\n".repeat(1_000);
    assert!(pasted_code.chars().count() > 2_000);
    for prompt in [pasted_code, "x".repeat(MAX_CHAT_MESSAGE_CHARS)] {
        let checked = validate_tutor_bounds(&prompt, "A reply.");
        assert!(
            checked.is_ok(),
            "{} characters: {:?}",
            prompt.chars().count(),
            checked.err()
        );
    }
    let over = validate_tutor_bounds(&"x".repeat(MAX_CHAT_MESSAGE_CHARS + 1), "A reply.");
    let message = over.expect_err("past the chat limit").to_string();
    assert!(
        message.contains(&MAX_CHAT_MESSAGE_CHARS.to_string()),
        "{message}"
    );
    assert!(validate_tutor_bounds("  ", "A reply.").is_err());
}

#[tokio::test]
async fn a_learners_long_artifact_is_held_to_the_chat_limit() -> Result<()> {
    use lattice_core::shared::constants::MAX_CHAT_MESSAGE_CHARS;
    let pool = active_memory_pool().await?;
    let db = Database::new(pool.clone());
    let (p, lesson) = setup(&pool, &db).await?;
    let repo = LearningPracticeRepository::new(db);
    let start = start_request(&p, &lesson);
    repo.start(&start, "start").await?;
    let essay = "I tried the second approach first and it held. ".repeat(1_000);
    assert!(essay.chars().count() > 24_000);
    let long = repo
        .save_artifact(
            &save_request(&p.summary.id, &start.session_id, &id(), 0, &essay),
            "essay",
        )
        .await;
    assert!(long.is_ok(), "an essay: {:?}", long.err());
    let at_the_limit = repo
        .save_artifact(
            &save_request(
                &p.summary.id,
                &start.session_id,
                &id(),
                1,
                &"x".repeat(MAX_CHAT_MESSAGE_CHARS),
            ),
            "at-the-limit",
        )
        .await;
    assert!(at_the_limit.is_ok(), "{:?}", at_the_limit.err());
    let over = repo
        .save_artifact(
            &save_request(
                &p.summary.id,
                &start.session_id,
                &id(),
                2,
                &"x".repeat(MAX_CHAT_MESSAGE_CHARS + 1),
            ),
            "over",
        )
        .await;
    assert!(over.is_err());
    Ok(())
}

#[path = "fit_tests.rs"]
mod fit_tests;
#[path = "profiles_tests.rs"]
mod profiles_tests;
