//! Starting attempts, tutor turns, proposals, revealed solutions, opened
//! sources and frozen source versions in the practice workbench.
use super::*;
use crate::features::learning::{
    references::source_library::{
        CapturedLearningSource, LearningSourceLibraryRepository, NewLearningSource,
    },
    repository::LearningRepository,
    tests,
};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[track_caller]
fn assert_invalid<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "expected InvalidInput, got {result:?}"
    );
}

#[track_caller]
fn assert_not_found<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(AppError::NotFound(_))),
        "expected NotFound, got {result:?}"
    );
}

async fn fk_pool() -> Result<SqlitePool> {
    let pool = tests::pool().await?;
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .map_err(db)?;
    Ok(pool)
}

/// An accepted program whose lessons are all ready, plus its first lesson id.
async fn setup(pool: &SqlitePool) -> Result<(LearningProgramDto, String)> {
    let repo = LearningRepository::new(pool.clone());
    let program = tests::fixture();
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE program_id=?")
        .bind(&program.summary.id)
        .execute(pool)
        .await
        .map_err(db)?;
    let ready = repo.get(&program.summary.id).await?;
    let lesson = ready.modules[0].lessons[0].id.clone();
    Ok((ready, lesson))
}

async fn add_block(
    pool: &SqlitePool,
    lesson_id: &str,
    ordinal: i64,
    kind: &str,
    body: &str,
    source_ids: &[String],
    rubric: &[LearningPracticeCriterionDto],
) -> Result<()> {
    sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json,rubric_json) VALUES(?,?,?,?,?,?,?)")
        .bind(lesson_id)
        .bind(ordinal)
        .bind(kind)
        .bind("Exercise")
        .bind(body)
        .bind(json(source_ids)?)
        .bind(json(rubric)?)
        .execute(pool)
        .await
        .map_err(db)?;
    Ok(())
}

/// Captures a pasted source and returns `(source_id, version_id)`.
async fn add_version(pool: &SqlitePool, program_id: &str) -> Result<(String, String)> {
    let (op, source_id, version_id) = (id(), id(), id());
    LearningSourceLibraryRepository::new(pool.clone())
        .add(
            NewLearningSource {
                operation_id: &op,
                source_id: &source_id,
                version_id: &version_id,
                program_id,
                kind: LearningSourceKind::Pasted,
                operation_kind: "add_text",
                origin: "Bakery handbook",
                requested_url: None,
                freshness: LearningSourcePolicy::Fixed,
            },
            CapturedLearningSource {
                title: "Bakery handbook".into(),
                publisher: None,
                requested_url: None,
                resolved_url: None,
                text: "Gluten forms when flour is hydrated and kneaded.".into(),
                truncated: false,
                extraction_version: "test_v1".into(),
                signals: crate::features::learning::references::trust::capture::not_extracted(
                    "test fixture",
                ),
            },
            id(),
        )
        .await?;
    Ok((source_id, version_id))
}

fn start_request(
    program: &LearningProgramDto,
    lesson_id: &str,
    mode: LearningPracticeMode,
) -> StartLearningPracticeSessionRequestDto {
    StartLearningPracticeSessionRequestDto {
        operation_id: id(),
        session_id: id(),
        program_id: program.summary.id.clone(),
        lesson_id: lesson_id.into(),
        expected_program_revision: program.summary.revision,
        mode,
        task_kind: LearningPracticeTaskKind::Independent,
        revises_session_id: None,
    }
}

async fn submit(
    repo: &LearningPracticeRepository,
    program_id: &str,
    session_id: &str,
    expected_revision: i64,
) -> Result<LearningPracticeWorkspaceDto> {
    let session = repo.get_session(session_id).await?;
    let criteria = session
        .rubric
        .iter()
        .map(|c| LearningPracticeCriterionResultDto {
            criterion_id: c.id.clone(),
            dimension: c.dimension.clone(),
            score: Some(2),
            max_points: c.max_points,
            observation: "Explains why kneading matters.".into(),
            evidence_quote: None,
        })
        .collect();
    let req = SubmitLearningPracticeAttemptRequestDto {
        operation_id: id(),
        program_id: program_id.into(),
        session_id: session_id.into(),
        expected_revision,
    };
    repo.submit(
        &req,
        SubmissionWrite {
            payload_hash: id(),
            operation_id: req.operation_id.clone(),
            grade_status: LearningPracticeGradeStatus::Provisional,
            criteria,
            evidence: vec![],
            grader_model: "fixture-model".into(),
        },
        &session.rubric,
    )
    .await
}

fn turn(
    kind: LearningTutorRequestKind,
    proposals: Vec<LearningPracticeProposalDto>,
) -> TutorTurnWrite {
    TutorTurnWrite {
        id: id(),
        operation_id: id(),
        payload_hash: id(),
        prompt: "Why does bread dough need resting?".into(),
        request_kind: kind,
        response: "Resting lets the gluten network relax.".into(),
        hint_level: Some(LearningPracticeHintLevel::PartialStrategy),
        citations: vec![],
        proposals,
        model_name: "fixture-model".into(),
    }
}

fn proposal(kind: LearningPracticeProposalKind, text: &str) -> LearningPracticeProposalDto {
    LearningPracticeProposalDto {
        id: id(),
        kind,
        text: text.into(),
        evidence_quote: Some("the dough tore".into()),
        tutor_turn_id: None,
        status: LearningPracticeProposalStatus::Pending,
        created_at: 0,
        decided_at: None,
    }
}

#[tokio::test]
async fn starting_needs_an_active_program_a_ready_lesson_and_owned_sources() -> Result<()> {
    let pool = fk_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());

    let draft = tests::fixture();
    LearningRepository::new(pool.clone()).create(&draft).await?;
    let draft_lesson = draft.modules[0].lessons[0].id.clone();
    let mut on_draft = start_request(&draft, &draft_lesson, LearningPracticeMode::Practice);
    on_draft.expected_program_revision = 0;
    assert_invalid(repo.start(&on_draft, "draft").await);

    let mut unknown = start_request(&program, &lesson, LearningPracticeMode::Practice);
    unknown.program_id = id();
    assert_not_found(repo.start(&unknown, "unknown").await);

    let mut stale = start_request(&program, &lesson, LearningPracticeMode::Practice);
    stale.expected_program_revision += 1;
    assert_invalid(repo.start(&stale, "stale").await);

    assert_not_found(
        repo.start(
            &start_request(&program, &draft_lesson, LearningPracticeMode::Practice),
            "foreign",
        )
        .await,
    );
    assert_invalid(
        repo.start(
            &start_request(&program, "not-a-uuid", LearningPracticeMode::Practice),
            "bad",
        )
        .await,
    );

    let outline_lesson = program.modules[1].lessons[1].id.clone();
    sqlx::query("UPDATE learning_lessons SET preparation='outline' WHERE id=?")
        .bind(&outline_lesson)
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_invalid(
        repo.start(
            &start_request(&program, &outline_lesson, LearningPracticeMode::Practice),
            "outline",
        )
        .await,
    );

    let mut guided = start_request(&program, &lesson, LearningPracticeMode::Practice);
    guided.task_kind = LearningPracticeTaskKind::Guided;
    assert_invalid(repo.start(&guided, "guided").await);

    let unowned_lesson = program.modules[0].lessons[1].id.clone();
    add_block(
        &pool,
        &unowned_lesson,
        0,
        "explanation",
        "Why dough rises",
        &[id()],
        &[],
    )
    .await?;
    assert_invalid(
        repo.start(
            &start_request(&program, &unowned_lesson, LearningPracticeMode::Practice),
            "unowned",
        )
        .await,
    );
    let crowded_lesson = program.modules[1].lessons[0].id.clone();
    let many: Vec<String> = (0..33).map(|_| id()).collect();
    add_block(
        &pool,
        &crowded_lesson,
        0,
        "explanation",
        "Too many sources",
        &many,
        &[],
    )
    .await?;
    assert_invalid(
        repo.start(
            &start_request(&program, &crowded_lesson, LearningPracticeMode::Practice),
            "crowded",
        )
        .await,
    );

    assert!(repo
        .workspace(&program.summary.id)
        .await?
        .sessions
        .is_empty());
    assert_not_found(repo.workspace(&id()).await);
    assert_invalid(repo.workspace("not-a-uuid").await);
    Ok(())
}

#[tokio::test]
async fn an_exercise_block_supplies_the_prompt_and_rubric_and_start_replays() -> Result<()> {
    let pool = fk_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let (_, version_id) = add_version(&pool, &program.summary.id).await?;
    let rubric = vec![LearningPracticeCriterionDto {
        id: "shaping".into(),
        dimension: LearningPracticeEvidenceDimension::Application,
        title: "Shaping".into(),
        description: "Shapes a tight loaf.".into(),
        max_points: 5,
    }];
    add_block(
        &pool,
        &lesson,
        0,
        "explanation",
        "Gluten basics",
        &[version_id.clone()],
        &[],
    )
    .await?;
    add_block(
        &pool,
        &lesson,
        1,
        "guided_practice",
        "Shape a boule step by step.",
        &[],
        &rubric,
    )
    .await?;

    let mut guided = start_request(&program, &lesson, LearningPracticeMode::Explore);
    guided.task_kind = LearningPracticeTaskKind::Guided;
    repo.start(&guided, "guided").await?;
    let replay = repo.start(&guided, "guided").await?;
    assert_eq!(replay.sessions.len(), 1);
    let session = repo.get_session(&guided.session_id).await?;
    assert_eq!(session.task_prompt, "Shape a boule step by step.");
    assert_eq!(session.rubric.len(), 1);
    assert_eq!(session.rubric[0].id, "shaping");
    assert_eq!(session.summary.source_version_ids, vec![version_id.clone()]);
    assert_eq!(session.summary.task_kind, LearningPracticeTaskKind::Guided);
    assert_eq!(session.artifact.text, "");
    assert_invalid(repo.start(&guided, "changed-payload").await);

    let independent = start_request(&program, &lesson, LearningPracticeMode::Practice);
    repo.start(&independent, "independent").await?;
    let fallback = repo.get_session(&independent.session_id).await?;
    assert!(fallback.task_prompt.starts_with("Explain the central idea"));
    assert_eq!(fallback.rubric.len(), default_rubric().len());

    let frozen = repo
        .frozen_sources(&program.summary.id, &session.summary.source_version_ids)
        .await?;
    assert_eq!(frozen.len(), 1);
    assert_eq!(frozen[0].version.id, version_id);
    assert_eq!(
        frozen[0].full_text,
        "Gluten forms when flour is hydrated and kneaded."
    );
    assert_not_found(repo.frozen_sources(&program.summary.id, &[id()]).await);
    let other = setup(&pool).await?.0;
    assert_not_found(repo.frozen_sources(&other.summary.id, &[version_id]).await);
    Ok(())
}

#[tokio::test]
async fn a_revision_must_follow_a_submitted_attempt_with_the_same_exercise() -> Result<()> {
    let pool = fk_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    add_block(
        &pool,
        &lesson,
        0,
        "guided_practice",
        "Proof a loaf overnight.",
        &[],
        &[],
    )
    .await?;

    let parent = start_request(&program, &lesson, LearningPracticeMode::Practice);
    repo.start(&parent, "parent").await?;
    let revise =
        |mode: LearningPracticeMode, task_kind: LearningPracticeTaskKind, parent_id: &str| {
            StartLearningPracticeSessionRequestDto {
                task_kind,
                revises_session_id: Some(parent_id.to_owned()),
                ..start_request(&program, &lesson, mode)
            }
        };
    let independent = LearningPracticeTaskKind::Independent;
    assert_invalid(
        repo.start(
            &revise(
                LearningPracticeMode::Practice,
                independent.clone(),
                &parent.session_id,
            ),
            "active-parent",
        )
        .await,
    );
    assert_invalid(
        repo.start(
            &revise(
                LearningPracticeMode::Practice,
                independent.clone(),
                "not-a-uuid",
            ),
            "bad-id",
        )
        .await,
    );

    submit(&repo, &program.summary.id, &parent.session_id, 0).await?;
    assert_invalid(
        repo.start(
            &revise(
                LearningPracticeMode::Demonstrate,
                independent.clone(),
                &parent.session_id,
            ),
            "demo",
        )
        .await,
    );
    assert_invalid(
        repo.start(
            &revise(
                LearningPracticeMode::Practice,
                LearningPracticeTaskKind::Guided,
                &parent.session_id,
            ),
            "guided",
        )
        .await,
    );
    let other_lesson = program.modules[0].lessons[1].id.clone();
    let mut elsewhere = revise(
        LearningPracticeMode::Practice,
        independent.clone(),
        &parent.session_id,
    );
    elsewhere.lesson_id = other_lesson;
    assert_invalid(repo.start(&elsewhere, "elsewhere").await);

    let ok = revise(
        LearningPracticeMode::Explore,
        independent,
        &parent.session_id,
    );
    repo.start(&ok, "revision").await?;
    let revision = repo.get_session(&ok.session_id).await?;
    assert_eq!(revision.summary.mode, LearningPracticeMode::Explore);
    assert_eq!(
        revision.task_prompt,
        repo.get_session(&parent.session_id).await?.task_prompt
    );
    assert!(revision
        .assistance
        .iter()
        .any(|a| a.kind == LearningPracticeAssistanceKind::TutorResponse
            && a.details.get("revisesSessionId").and_then(|v| v.as_str())
                == Some(parent.session_id.as_str())));
    Ok(())
}

#[tokio::test]
async fn tutor_turns_and_their_proposals_are_ledgered_and_decided_once() -> Result<()> {
    let pool = fk_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let start = start_request(&program, &lesson, LearningPracticeMode::Practice);
    repo.start(&start, "start").await?;
    let (pid, sid) = (program.summary.id.clone(), start.session_id.clone());

    let misconception = proposal(
        LearningPracticeProposalKind::Misconception,
        "Believes yeast is a chemical",
    );
    let follow_up = proposal(
        LearningPracticeProposalKind::FollowUp,
        "Compare sourdough starters",
    );
    let hint = turn(
        LearningTutorRequestKind::Hint,
        vec![misconception.clone(), follow_up.clone()],
    );
    repo.commit_tutor_turn(&pid, &sid, 0, hint.clone()).await?;
    repo.commit_tutor_turn(&pid, &sid, 0, hint.clone()).await?;
    let mut conflicting = hint.clone();
    conflicting.payload_hash = id();
    assert_invalid(repo.commit_tutor_turn(&pid, &sid, 1, conflicting).await);
    assert_invalid(
        repo.commit_tutor_turn(
            &pid,
            &sid,
            0,
            turn(LearningTutorRequestKind::Question, vec![]),
        )
        .await,
    );
    let mut bad_turn = turn(LearningTutorRequestKind::Question, vec![]);
    bad_turn.id = "not-a-uuid".into();
    assert_invalid(repo.commit_tutor_turn(&pid, &sid, 1, bad_turn).await);

    let question = turn(LearningTutorRequestKind::Question, vec![]);
    repo.commit_tutor_turn(&pid, &sid, 1, question.clone())
        .await?;
    let session = repo.get_session(&sid).await?;
    assert_eq!(session.summary.revision, 2);
    assert_eq!(session.tutor_turns.len(), 2);
    let stored = session
        .tutor_turns
        .iter()
        .find(|t| t.id == hint.id)
        .expect("hint turn");
    assert_eq!(
        stored.hint_level,
        Some(LearningPracticeHintLevel::PartialStrategy)
    );
    assert_eq!(
        stored.proposal_ids,
        vec![misconception.id.clone(), follow_up.id.clone()]
    );
    assert_eq!(session.proposals.len(), 2);
    assert!(session
        .proposals
        .iter()
        .all(|p| p.tutor_turn_id.as_deref() == Some(hint.id.as_str())
            && matches!(p.status, LearningPracticeProposalStatus::Pending)));
    let kinds: Vec<_> = session.assistance.iter().map(|a| a.kind.clone()).collect();
    assert!(kinds.contains(&LearningPracticeAssistanceKind::Hint));
    assert!(kinds.contains(&LearningPracticeAssistanceKind::TutorResponse));

    let decide =
        |proposal_id: &str, expected_revision: i64| DecideLearningPracticeProposalRequestDto {
            operation_id: id(),
            program_id: pid.clone(),
            session_id: sid.clone(),
            expected_revision,
            proposal_id: proposal_id.into(),
        };
    let accept = decide(&misconception.id, 2);
    repo.decide_proposal(&accept, true, "accept").await?;
    repo.decide_proposal(&accept, true, "accept").await?;
    assert_invalid(
        repo.decide_proposal(&decide(&misconception.id, 3), false, "again")
            .await,
    );
    assert_invalid(
        repo.decide_proposal(&decide(&follow_up.id, 2), false, "stale")
            .await,
    );
    assert_invalid(
        repo.decide_proposal(&decide("not-a-uuid", 3), false, "bad")
            .await,
    );
    repo.decide_proposal(&decide(&follow_up.id, 3), false, "reject")
        .await?;

    let decided = repo.get_session(&sid).await?;
    assert_eq!(decided.summary.revision, 4);
    let status = |proposal_id: &str| {
        decided
            .proposals
            .iter()
            .find(|p| p.id == proposal_id)
            .map(|p| (p.status.clone(), p.decided_at.is_some()))
    };
    assert!(matches!(
        status(&misconception.id),
        Some((LearningPracticeProposalStatus::Accepted, true))
    ));
    assert!(matches!(
        status(&follow_up.id),
        Some((LearningPracticeProposalStatus::Rejected, true))
    ));

    submit(&repo, &pid, &sid, 4).await?;
    assert_invalid(
        repo.commit_tutor_turn(
            &pid,
            &sid,
            5,
            turn(LearningTutorRequestKind::Critique, vec![]),
        )
        .await,
    );
    assert_invalid(
        repo.decide_proposal(&decide(&follow_up.id, 5), true, "after-submit")
            .await,
    );
    Ok(())
}

#[tokio::test]
async fn sources_and_solutions_are_aids_for_live_assisted_attempts_only() -> Result<()> {
    let pool = fk_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let (source_id, version_id) = add_version(&pool, &program.summary.id).await?;
    let (_, unfrozen_version) = add_version(&pool, &program.summary.id).await?;
    add_block(
        &pool,
        &lesson,
        0,
        "explanation",
        "Gluten basics",
        &[version_id.clone()],
        &[],
    )
    .await?;
    let start = start_request(&program, &lesson, LearningPracticeMode::Explore);
    repo.start(&start, "start").await?;
    let (pid, sid) = (program.summary.id.clone(), start.session_id.clone());

    let open = |source_id: &str, version_id: &str, expected_revision: i64| {
        OpenLearningPracticeSourceRequestDto {
            operation_id: id(),
            program_id: pid.clone(),
            session_id: sid.clone(),
            expected_revision,
            source_id: source_id.into(),
            version_id: version_id.into(),
        }
    };
    assert_invalid(
        repo.open_source(&open(&source_id, &unfrozen_version, 0), "unfrozen")
            .await,
    );
    assert_not_found(
        repo.open_source(&open("another-source", &version_id, 0), "mismatch")
            .await,
    );
    assert_invalid(
        repo.open_source(&open(&source_id, &version_id, 3), "stale")
            .await,
    );
    let opened = open(&source_id, &version_id, 0);
    repo.open_source(&opened, "open").await?;
    repo.open_source(&opened, "open").await?;
    assert_eq!(repo.get_session(&sid).await?.summary.revision, 1);

    let citation = LearningPracticeCitationDto {
        source_id: source_id.clone(),
        version_id: version_id.clone(),
        quote: "Gluten forms".into(),
    };
    let reveal = RevealLearningPracticeSolutionRequestDto {
        operation_id: id(),
        program_id: pid.clone(),
        session_id: sid.clone(),
        expected_revision: 1,
    };
    let solution = "Knead until the dough passes the windowpane test.";
    repo.commit_solution(&reveal, "reveal", solution, std::slice::from_ref(&citation))
        .await?;
    repo.commit_solution(&reveal, "reveal", solution, std::slice::from_ref(&citation))
        .await?;
    let mut stale_reveal = reveal.clone();
    stale_reveal.operation_id = id();
    assert_invalid(
        repo.commit_solution(&stale_reveal, "again", solution, &[])
            .await,
    );

    let session = repo.get_session(&sid).await?;
    assert_eq!(session.summary.revision, 2);
    assert_eq!(session.revealed_solution.as_deref(), Some(solution));
    assert_eq!(session.revealed_solution_citations.len(), 1);
    assert_eq!(session.revealed_solution_citations[0].quote, "Gluten forms");
    let revealed = session
        .assistance
        .iter()
        .find(|a| a.kind == LearningPracticeAssistanceKind::SolutionRevealed)
        .expect("reveal ledger entry");
    assert_eq!(
        revealed
            .details
            .get("citationCount")
            .and_then(|v| v.as_i64()),
        Some(1)
    );
    assert_eq!(revealed.mode, LearningPracticeMode::Explore);
    assert!(session
        .assistance
        .iter()
        .any(|a| a.kind == LearningPracticeAssistanceKind::SourceOpened));

    let change = ChangeLearningPracticeModeRequestDto {
        operation_id: id(),
        program_id: pid.clone(),
        session_id: sid.clone(),
        expected_revision: 2,
        mode: LearningPracticeMode::Demonstrate,
    };
    repo.change_mode(&change, "demo").await?;
    repo.change_mode(&change, "demo").await?;
    let demo = repo.get_session(&sid).await?;
    assert_eq!(demo.summary.revision, 3);
    let changed = demo
        .assistance
        .iter()
        .find(|a| a.kind == LearningPracticeAssistanceKind::ModeChanged)
        .expect("mode change ledger entry");
    assert_eq!(
        changed.details.get("from").and_then(|v| v.as_str()),
        Some("explore")
    );
    assert_eq!(
        changed.details.get("to").and_then(|v| v.as_str()),
        Some("demonstrate")
    );
    let mut demo_reveal = reveal.clone();
    demo_reveal.operation_id = id();
    demo_reveal.expected_revision = 3;
    assert_invalid(
        repo.commit_solution(&demo_reveal, "demo-reveal", solution, &[])
            .await,
    );
    assert_invalid(
        repo.commit_tutor_turn(&pid, &sid, 3, turn(LearningTutorRequestKind::Hint, vec![]))
            .await,
    );
    assert_invalid(
        repo.open_source(&open(&source_id, &version_id, 3), "demo-open")
            .await,
    );

    let (summary, prompt, rubric, text, artifact_revision) =
        repo.session_header(&pid, &sid).await?;
    assert_eq!(summary.id, sid);
    assert_eq!(prompt, demo.task_prompt);
    assert_eq!(rubric.len(), demo.rubric.len());
    assert_eq!((text.as_str(), artifact_revision), ("", 0));
    let other = setup(&pool).await?.0;
    assert_not_found(repo.session_header(&other.summary.id, &sid).await);
    assert_invalid(repo.session_header(&pid, "not-a-uuid").await);

    submit(&repo, &pid, &sid, 3).await?;
    let mut after = reveal.clone();
    after.operation_id = id();
    after.expected_revision = 4;
    assert_invalid(repo.commit_solution(&after, "after", solution, &[]).await);
    let mut back = change.clone();
    back.operation_id = id();
    back.expected_revision = 4;
    back.mode = LearningPracticeMode::Practice;
    assert_invalid(repo.change_mode(&back, "after").await);
    let summary = repo.workspace(&pid).await?.sessions.remove(0);
    assert_eq!(summary.status, LearningPracticeSessionStatus::Submitted);
    assert_eq!(
        summary.grade_status,
        Some(LearningPracticeGradeStatus::Provisional)
    );
    Ok(())
}
