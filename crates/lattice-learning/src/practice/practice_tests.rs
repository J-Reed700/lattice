use crate::{
    dto::*,
    practice_generation::{grade, grade_schema, solution, tutor, validate_citations},
    practice_repository::{
        default_rubric, validate_tutor_bounds, LearningPracticeRepository, SubmissionWrite,
    },
    repository::LearningRepository,
};
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
use crate::persistence::db;

async fn setup(pool: &SqlitePool) -> Result<(LearningProgramDto, String)> {
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(pool)
        .await
        .map_err(db)?;
    let repo = LearningRepository::new(pool.clone());
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
        .await
        .map_err(db)?;
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
        .await
        .map_err(db)?;
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
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
    let pool = file_pool(path.clone()).await?;
    let (p, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
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
    pool.close().await;
    let reopened = file_pool(path).await?;
    let session = LearningPracticeRepository::new(reopened.clone())
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
    let (p, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);
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
    assert!(repo.save_artifact(&stale, "stale").await.is_err());
    assert_eq!(
        repo.get_session(&start.session_id).await?.artifact.text,
        "durable answer"
    );
    Ok(())
}

#[tokio::test]
async fn demonstrate_mode_blocks_all_selected_aids_and_session_stays_frozen() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (p, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);
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
    let (p, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);
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

/// A constrained decoder writes a reply in schema order. With the score first,
/// the 27B answered a plainly wrong artifact with `uncertain` and no score, 9
/// times in 9, against a prompt that says zero is an observed wrong attempt.
#[test]
fn the_grader_writes_its_grounds_before_it_scores() {
    let schema = grade_schema(&default_rubric());
    let keys = |pointer: &str| -> Vec<String> {
        schema
            .pointer(pointer)
            .and_then(serde_json::Value::as_object)
            .map(|object| object.keys().cloned().collect())
            .unwrap_or_default()
    };

    assert_eq!(keys("/properties"), ["criteria", "status"]);
    assert_eq!(
        keys("/properties/criteria/items/properties"),
        ["criterion_id", "evidence_quote", "observation", "score"]
    );
    assert_eq!(
        schema.pointer("/properties/criteria/items/required"),
        Some(&serde_json::json!([
            "criterion_id",
            "evidence_quote",
            "observation",
            "score"
        ]))
    );
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
}

impl Reply {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            schemas: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl LLMPort for Reply {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if let Ok(mut schemas) = self.schemas.lock() {
            schemas.extend(request.json_schema.clone());
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
    let (p, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);
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

#[tokio::test]
async fn a_tutor_reply_is_read_at_any_length() -> Result<()> {
    let evidence = "I tried the second approach first. ".repeat(18);
    let session = started_session(&evidence).await?;
    let passage = "The passage states one condition after another. ".repeat(20);
    let sources = [passage_source(&passage)];
    let cited = json!([{"sourceId": "logical-source", "versionId": "version-1", "quote": passage}]);
    let proposal = |text: &str, quote: Option<&str>| json!([{"kind": "misconception", "text": text, "evidenceQuote": quote}]);
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
        ("citation quote", tutor_reply(short, cited, json!([]))),
        (
            "proposal text",
            tutor_reply(short, json!([]), proposal(&wordy, None)),
        ),
        (
            "proposal evidence quote",
            tutor_reply(short, json!([]), proposal(short, Some(evidence.as_str()))),
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
    let cited = json!([{"sourceId": "logical-source", "versionId": "version-1", "quote": passage}]);
    let reply = |text: &str, citations: serde_json::Value| {
        json!({"solution": text, "citations": citations}).to_string()
    };
    let short = "State the condition, then apply it.";
    let long = "Show the step, then the reason it is taken. ".repeat(160);
    let padded = format!("{}{}", reply(short, json!([])), " ".repeat(20_001));
    for (what, text) in [
        ("a short solution", reply(short, json!([]))),
        ("solution", reply(&long, json!([]))),
        ("citation quote", reply(short, cited)),
        ("whitespace around the reply", padded),
    ] {
        let llm = Reply::new(text);
        let read = solution(&llm, &session, &sources).await;
        assert!(read.is_ok(), "{what}: {:?}", read.err());
    }
    Ok(())
}

#[tokio::test]
async fn the_graders_observation_and_quote_are_read_at_any_length() -> Result<()> {
    let artifact = "I tried the second approach first. ".repeat(18);
    let session = started_session(&artifact).await?;
    let reply = |observation: &str, quote: &str| {
        let criteria = session
            .rubric
            .iter()
            .map(|c| {
                json!({
                    "criterion_id": c.id, "score": 2,
                    "observation": observation, "evidence_quote": quote
                })
            })
            .collect::<Vec<_>>();
        json!({"status": "provisional", "criteria": criteria}).to_string()
    };
    let short = "The artifact states the condition.";
    let first = "I tried the second approach first.";
    let observation = "The artifact states the condition but never says why it holds. ".repeat(18);
    for (what, text) in [
        ("short feedback", reply(short, first)),
        ("observation", reply(&observation, first)),
        ("evidence quote", reply(short, artifact.trim())),
    ] {
        let llm = Reply::new(text);
        let graded = grade(&llm, &session).await;
        assert!(graded.is_ok(), "{what}: {:?}", graded.err());
    }
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
    let criteria = session
        .rubric
        .iter()
        .map(|c| {
            json!({
                "criterion_id": c.id, "score": null,
                "observation": "Not enough to judge.", "evidence_quote": null
            })
        })
        .collect::<Vec<_>>();
    let grading = Reply::new(json!({"status": "uncertain", "criteria": criteria}).to_string());
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
    let (p, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);
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


mod profiles_tests;
