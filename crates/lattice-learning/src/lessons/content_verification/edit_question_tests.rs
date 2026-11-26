#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! The user rewrites a question of a published lesson through `ContentEdits`:
//! its prompt, choices, right answer and explanation are written, the statements
//! that stood on what changed end, the rest keep their state, and the review's record
//! stays honest.
use super::claim_ledger_tests::{figure, lesson, section, unreadable, Course};
use super::GOOD;
use crate::assessment::assessment_engine::checks::answer_in_prompt_message;
use crate::assessment_engine::{
    LearningAssessmentCandidate, LearningAssessmentKey, LearningItemFormat,
};
use crate::assessment_repository::{insert_bank_items_in, BankItem};
use crate::edits::keys;
use crate::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::lessons::claim_reviews::service::ClaimReviewService;
use crate::lessons::claim_reviews::teaching_findings::{
    TeachingFindingSiteDto, TeachingFindingsRepository,
};
use crate::review_findings::{FindingKind, FindingSite, ReviewFinding, SetAside};
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, ClaimState, EventProvenance,
    Ledger, UnitKind, UnverifiedReason,
};
use lattice_core::shared::edits::{
    ContentEditOutcome, ContentEditPreview, ContentEditRequest, EditOp, EditSource, EditableContent,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::hash::sha256_hex;
use serde_json::{json, Value};
use uuid::Uuid;

const P0: &str = "What does DictReader do with spaces in a header name?";
const C0: [&str; 4] = [
    "It keeps them.",
    "It trims them.",
    "It drops the column.",
    "It raises an error.",
];
const E0_FIRST: &str = "Header names keep their spaces exactly as written.";
const E0_SECOND: &str = "Trimming is up to you.";
const E0: &str = "Header names keep their spaces exactly as written. Trimming is up to you.";

const P1: &str = "How does DictReader treat a row that is too short?";
const C1: [&str; 4] = [
    "It fills the missing fields with None.",
    "It skips the row.",
    "It raises an error.",
    "It pads them with empty strings.",
];
const E1: &str = "A short row gets None for each missing field.";

const P2: &str = "DictReader reads every row as a list.";
const E2: &str = "It reads each row as a dictionary.";

const NEW_PROMPT: &str = "What does DictReader do with a header name that holds spaces?";
const NEW_EXPLANATION: &str = "Header names keep their spaces as the file writes them. Trimming is up to you. Strip them yourself if you need to.";
const SECTIONS: usize = 2;
const CHANGED_SINCE: &str = "This changed since you opened it.";

struct QuestionSpec<'a> {
    kind: LearningItemFormat,
    prompt: &'a str,
    choices: &'a [&'a str],
    right: usize,
    explanation: &'a str,
}

fn spec0() -> QuestionSpec<'static> {
    QuestionSpec {
        kind: LearningItemFormat::SingleChoice,
        prompt: P0,
        choices: &C0,
        right: 0,
        explanation: E0,
    }
}

fn spec1() -> QuestionSpec<'static> {
    QuestionSpec {
        kind: LearningItemFormat::SingleChoice,
        prompt: P1,
        choices: &C1,
        right: 0,
        explanation: E1,
    }
}

fn spec2() -> QuestionSpec<'static> {
    QuestionSpec {
        kind: LearningItemFormat::TrueFalse,
        prompt: P2,
        choices: &["True", "False"],
        right: 1,
        explanation: E2,
    }
}

fn bank_item(outcome: &str, spec: &QuestionSpec<'_>) -> BankItem {
    let candidate = LearningAssessmentCandidate {
        id: Uuid::new_v4().to_string(),
        outcome_ids: vec![outcome.into()],
        format: spec.kind,
        difficulty: 3,
        prompt: spec.prompt.into(),
        options: spec.choices.iter().map(|s| (*s).to_owned()).collect(),
        left: Vec::new(),
        source_version_ids: Vec::new(),
        rubric: Vec::new(),
        key: LearningAssessmentKey::Choice(spec.right),
        explanation: spec.explanation.into(),
        distractor_rationales: Vec::new(),
    };
    BankItem {
        item: candidate.into(),
        source_quote: String::new(),
        lesson_id: None,
        origin: "lesson",
        review_status: "agreed",
    }
}

fn provenance() -> EventProvenance {
    EventProvenance {
        actor: Actor::Machine,
        at: chrono::Utc::now().timestamp_millis(),
        job_id: None,
        app_version: "test".into(),
        checker_model: Some("fixture".into()),
    }
}

async fn ledger(course: &Course) -> Result<Ledger> {
    ClaimReviewRepository::new(course.db.clone())
        .load(&course.lesson_id)
        .await
}

async fn claim(
    course: &Course,
    unit: usize,
    kind: UnitKind,
    field: &str,
    sentence: &str,
    field_text: &str,
) -> Result<()> {
    let repository = ClaimReviewRepository::new(course.db.clone());
    let id = ClaimId(Uuid::new_v4().to_string());
    let revision = ledger(course).await?.revision;
    let revision = repository
        .append(
            &course.lesson_id,
            &id,
            provenance(),
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(unit, kind, field, sentence, field_text),
                statement: sentence.into(),
            },
            revision,
        )
        .await?;
    repository
        .append(
            &course.lesson_id,
            &id,
            provenance(),
            ClaimEvent::PublishedUnverified {
                reason: UnverifiedReason::Unchecked,
            },
            revision,
        )
        .await?;
    Ok(())
}

async fn published() -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&unreadable(), &lesson(&[GOOD, &figure(7)]))
        .await
        .expect("the lesson publishes, its statements not verified");
    course.publish(prepared).await?;
    let outcome = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,NULL,?,?,?,0,0)",
    )
    .bind(&outcome)
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .bind("Read a header row")
    .bind("Read a header row with DictReader")
    .execute(&course.pool)
    .await?;
    let mut items: Vec<BankItem> = [spec0(), spec1(), spec2()]
        .iter()
        .map(|spec| bank_item(&outcome, spec))
        .collect();
    for item in &mut items {
        item.lesson_id = Some(course.lesson_id.clone());
    }
    let mut tx = course.db.begin_write().await?;
    let stored = insert_bank_items_in(&mut tx, &course.program_id, &items, None).await?;
    tx.commit().await?;
    assert_eq!(stored, 3);
    let unit = SECTIONS;
    claim(&course, unit, UnitKind::Question, "prompt", P0, P0).await?;
    for (slot, choice) in C0.iter().copied().enumerate().skip(1) {
        let field = format!("options/{slot}");
        claim(&course, unit, UnitKind::Question, &field, choice, choice).await?;
    }
    for sentence in [E0_FIRST, E0_SECOND] {
        claim(
            &course,
            unit,
            UnitKind::AnswerKey,
            "explanation",
            sentence,
            E0,
        )
        .await?;
    }
    claim(
        &course,
        unit + 1,
        UnitKind::AnswerKey,
        "explanation",
        E1,
        E1,
    )
    .await?;
    Ok(course)
}

fn entry_for<'a>(ledger: &'a Ledger, statement: &str) -> &'a ClaimLedgerEntry {
    let found: Vec<_> = ledger
        .claims
        .iter()
        .filter(|entry| entry.statement() == statement)
        .collect();
    assert_eq!(found.len(), 1, "{statement}: {:?}", ledger.claims);
    found[0]
}

type Row = (String, String, String, String, String);

async fn rows(course: &Course) -> Result<Vec<Row>> {
    Ok(sqlx::query_as::<_, Row>(
        "SELECT c.prompt, c.options_json, k.key_json, k.explanation, k.distractor_rationales_json \
         FROM learning_assessment_candidates c JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id \
         WHERE c.lesson_id=? ORDER BY c.rowid",
    )
    .bind(&course.lesson_id)
    .fetch_all(&course.pool)
    .await?)
}

async fn question_ids(course: &Course) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM learning_assessment_candidates WHERE lesson_id=? ORDER BY rowid",
    )
    .bind(&course.lesson_id)
    .fetch_all(&course.pool)
    .await?)
}

async fn stored_report(course: &Course) -> Result<(String, String)> {
    Ok(sqlx::query_as::<_, (String, String)>(
        "SELECT content_sha256, report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .fetch_one(&course.pool)
    .await?)
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    rows: Vec<Row>,
    ledger: String,
    revision: i64,
    report: (String, String),
}

async fn snapshot(course: &Course) -> Result<Snapshot> {
    let ledger = ledger(course).await?;
    Ok(Snapshot {
        rows: rows(course).await?,
        ledger: format!("{:?}", ledger.claims),
        revision: ledger.revision.0,
        report: stored_report(course).await?,
    })
}

fn replace(path: &str, value: impl Into<Value>) -> EditOp {
    EditOp::Replace {
        path: path.into(),
        value: value.into(),
    }
}

async fn open_question(course: &Course, index: usize) -> Result<EditableContent> {
    let ids = question_ids(course).await?;
    let opened = course.edits().open(&keys::question_ref(&ids[index])).await;
    assert!(opened.is_ok(), "{opened:?}");
    opened
}

async fn try_edit(course: &Course, index: usize, ops: Vec<EditOp>) -> Result<ContentEditOutcome> {
    let opened = open_question(course, index).await?;
    course
        .edits()
        .apply(&ContentEditRequest {
            edit_id: Uuid::new_v4().to_string(),
            content: opened.content,
            expected_revision: opened.revision,
            ops,
            source: EditSource::Learner,
            target: None,
        })
        .await
}

async fn edit(course: &Course, index: usize, ops: Vec<EditOp>) -> Result<ContentEditOutcome> {
    let outcome = try_edit(course, index, ops).await;
    assert!(outcome.is_ok(), "{outcome:?}");
    outcome
}

async fn preview_edit(
    course: &Course,
    index: usize,
    ops: Vec<EditOp>,
) -> Result<ContentEditPreview> {
    let opened = open_question(course, index).await?;
    let preview = course
        .edits()
        .preview(&ContentEditRequest {
            edit_id: Uuid::new_v4().to_string(),
            content: opened.content,
            expected_revision: opened.revision,
            ops,
            source: EditSource::Learner,
            target: None,
        })
        .await;
    assert!(preview.is_ok(), "{preview:?}");
    preview
}

fn refusal(error: &AppError) -> &str {
    match error {
        AppError::InvalidInput(message)
        | AppError::InvalidState(message)
        | AppError::NotFound(message)
        | AppError::Conflict(message) => message,
        other => panic!("not a refusal: {other:?}"),
    }
}

fn refused<T: std::fmt::Debug>(result: Result<T>) -> String {
    refusal(&result.unwrap_err()).to_owned()
}

fn old_claim<'a>(ledger: &'a Ledger, statement: &str) -> &'a ClaimLedgerEntry {
    entry_for(ledger, statement)
}

fn assert_edited_into(ledger: &Ledger, old: &str, new: &str) {
    let successor = entry_for(ledger, new).claim_id().clone();
    match old_claim(ledger, old).state() {
        ClaimState::Edited {
            successor: Some(id),
            by: Actor::User,
            ..
        } => assert_eq!(id, &successor, "{old}"),
        other => panic!("{old}: {other:?}"),
    }
}

fn assert_confirmed_at(
    ledger: &Ledger,
    statement: &str,
    unit: usize,
    kind: UnitKind,
    field: &str,
    text: &str,
) {
    let entry = entry_for(ledger, statement);
    assert!(
        matches!(entry.state(), ClaimState::Confirmed { .. }),
        "{statement}: {:?}",
        entry.state()
    );
    let place = entry.location();
    assert_eq!(
        (place.unit_index, place.unit_kind, place.field.as_str()),
        (unit, kind, field),
        "{statement}"
    );
    let (start, end) = place.span.unwrap();
    assert_eq!(text.get(start..end), Some(statement));
    assert!(entry
        .events()
        .iter()
        .any(|record| record.provenance.actor == Actor::User));
}

#[tokio::test]
async fn a_question_is_read_by_its_place_among_the_lessons_questions() -> Result<()> {
    let course = published().await?;
    let ids = question_ids(&course).await?;
    for (index, prompt) in [P0, P1, P2].into_iter().enumerate() {
        let opened = course.edits().open(&keys::question_ref(&ids[index])).await;
        assert!(opened.is_ok(), "{opened:?}");
        assert_eq!(opened?.document["prompt"], json!(prompt));
    }
    let error = course
        .edits()
        .open(&keys::question_ref("missing-question"))
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::NotFound(_)), "{error:?}");
    Ok(())
}

#[tokio::test]
async fn a_question_of_a_lesson_from_another_course_is_not_read() -> Result<()> {
    let course = published().await?;
    let error = course
        .edits()
        .open(&keys::question_ref("another-course-question"))
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::NotFound(_)), "{error:?}");
    Ok(())
}

#[tokio::test]
async fn a_save_writes_the_prompt_and_the_explanation() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;

    let saved = edit(
        &course,
        0,
        vec![
            replace("/prompt", NEW_PROMPT),
            replace("/explanation", NEW_EXPLANATION),
        ],
    )
    .await?;

    assert!(saved.changed);
    let now = rows(&course).await?;
    assert_eq!(now[0].0, NEW_PROMPT);
    assert_eq!(now[0].3, NEW_EXPLANATION);
    assert_eq!(now[0].1, seen.rows[0].1);
    assert_eq!(now[0].2, seen.rows[0].2);
    assert_eq!(now[1..], seen.rows[1..]);
    Ok(())
}

#[tokio::test]
async fn statements_on_a_changed_prompt_and_explanation_end_the_unchanged_ones_stay_and_new_sentences_are_confirmed(
) -> Result<()> {
    let course = published().await?;
    let seen = ledger(&course).await?;
    let second = format!("{:?}", entry_for(&seen, E0_SECOND).state());
    let other_question = format!("{:?}", entry_for(&seen, E1));
    let kept_choice = format!("{:?}", entry_for(&seen, C0[1]));

    edit(
        &course,
        0,
        vec![
            replace("/prompt", NEW_PROMPT),
            replace("/explanation", NEW_EXPLANATION),
        ],
    )
    .await?;

    let now = ledger(&course).await?;
    assert_eq!(now.revision.0, seen.revision.0 + 1);
    let new_first = "Header names keep their spaces as the file writes them.";
    let new_third = "Strip them yourself if you need to.";
    assert_edited_into(&now, P0, NEW_PROMPT);
    assert_edited_into(&now, E0_FIRST, new_first);
    assert_confirmed_at(
        &now,
        NEW_PROMPT,
        SECTIONS,
        UnitKind::Question,
        "prompt",
        NEW_PROMPT,
    );
    assert_confirmed_at(
        &now,
        new_first,
        SECTIONS,
        UnitKind::AnswerKey,
        "explanation",
        NEW_EXPLANATION,
    );
    assert_confirmed_at(
        &now,
        new_third,
        SECTIONS,
        UnitKind::AnswerKey,
        "explanation",
        NEW_EXPLANATION,
    );
    assert_eq!(format!("{:?}", entry_for(&now, E0_SECOND).state()), second);
    assert_eq!(format!("{:?}", entry_for(&now, E1)), other_question);
    assert_eq!(format!("{:?}", entry_for(&now, C0[1])), kept_choice);
    Ok(())
}

#[tokio::test]
async fn a_changed_choice_follows_the_title_rule() -> Result<()> {
    let course = published().await?;
    let seen = ledger(&course).await?;

    edit(
        &course,
        0,
        vec![
            replace("/choices/0/text", "It keeps them exactly."),
            replace("/choices/1/text", "It trims them silently."),
        ],
    )
    .await?;

    let now = ledger(&course).await?;
    assert_eq!(now.revision.0, seen.revision.0 + 1);
    assert_edited_into(&now, C0[1], "It trims them silently.");
    assert_confirmed_at(
        &now,
        "It trims them silently.",
        SECTIONS,
        UnitKind::Question,
        "options/1",
        "It trims them silently.",
    );
    assert!(now
        .claims
        .iter()
        .all(|entry| entry.statement() != "It keeps them exactly."));
    let stored: Vec<String> = serde_json::from_str(&rows(&course).await?[0].1)?;
    assert_eq!(
        stored,
        [
            "It keeps them exactly.",
            "It trims them silently.",
            C0[2],
            C0[3]
        ]
    );
    Ok(())
}

#[tokio::test]
async fn removing_a_choice_compares_the_slots_that_follow_it_and_ends_the_statement_of_the_last_slot(
) -> Result<()> {
    let course = published().await?;

    edit(
        &course,
        0,
        vec![EditOp::Remove {
            path: "/choices/1".into(),
        }],
    )
    .await?;

    let now = ledger(&course).await?;
    let at = |statement: &str, field: &str| -> Vec<&ClaimLedgerEntry> {
        now.claims
            .iter()
            .filter(|entry| entry.statement() == statement && entry.location().field == field)
            .collect()
    };
    assert!(matches!(
        at(C0[3], "options/3")[0].state(),
        ClaimState::Removed {
            by: Actor::User,
            ..
        }
    ));
    let moved_up = at(C0[3], "options/2")[0];
    assert!(matches!(moved_up.state(), ClaimState::Confirmed { .. }));
    let slot_one_new = at(C0[2], "options/1")[0];
    assert!(matches!(slot_one_new.state(), ClaimState::Confirmed { .. }));
    match at(C0[1], "options/1")[0].state() {
        ClaimState::Edited {
            successor: Some(id),
            by: Actor::User,
            ..
        } => assert_eq!(id, slot_one_new.claim_id()),
        other => panic!("{other:?}"),
    }
    match at(C0[2], "options/2")[0].state() {
        ClaimState::Edited {
            successor: Some(id),
            by: Actor::User,
            ..
        } => assert_eq!(id, moved_up.claim_id()),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&rows(&course).await?[0].1)?,
        vec![C0[0], C0[2], C0[3]]
    );
    Ok(())
}

#[tokio::test]
async fn adding_a_choice_writes_it_and_touches_no_statement() -> Result<()> {
    let course = published().await?;
    let seen = ledger(&course).await?;

    edit(
        &course,
        0,
        vec![EditOp::Add {
            path: "/choices/-".into(),
            value: json!({"text": "It ignores them.", "correct": false, "rationale": ""}),
        }],
    )
    .await?;

    let now = ledger(&course).await?;
    assert_eq!(now.revision.0, seen.revision.0 + 1);
    assert_eq!(format!("{:?}", now.claims), format!("{:?}", seen.claims));
    let stored: Vec<String> = serde_json::from_str(&rows(&course).await?[0].1)?;
    assert_eq!(stored.last().map(String::as_str), Some("It ignores them."));
    Ok(())
}

#[tokio::test]
async fn a_change_of_the_right_answer_alone_touches_no_statement_and_is_still_a_change(
) -> Result<()> {
    let course = published().await?;
    let seen = ledger(&course).await?;

    let saved = edit(
        &course,
        0,
        vec![
            replace("/choices/0/correct", false),
            replace("/choices/2/correct", true),
        ],
    )
    .await?;

    assert!(saved.changed);
    let now = ledger(&course).await?;
    assert_eq!(format!("{:?}", now.claims), format!("{:?}", seen.claims));
    let stored: Value = serde_json::from_str(&rows(&course).await?[0].2)?;
    assert_eq!(stored, json!({"kind":"choice","value":2}));
    assert_eq!(now.revision.0, seen.revision.0 + 1);
    Ok(())
}

#[tokio::test]
async fn a_true_or_false_question_keeps_its_choices_and_changes_only_its_right_answer() -> Result<()>
{
    let course = published().await?;
    let seen = rows(&course).await?;

    let saved = edit(&course, 2, vec![replace("/answer", "True")]).await?;

    assert!(saved.changed);
    let now = rows(&course).await?;
    assert_eq!(now[2].1, seen[2].1);
    let stored: Value = serde_json::from_str(&now[2].2)?;
    assert_eq!(stored, json!({"kind":"choice","value":0}));
    Ok(())
}

#[tokio::test]
async fn a_true_or_false_question_needs_its_right_answer() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;

    let result = try_edit(&course, 2, vec![replace("/answer", "Invalid")]).await;

    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "{result:?}"
    );
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn the_second_question_is_the_one_written_when_the_second_is_edited() -> Result<()> {
    let course = published().await?;
    let seen = rows(&course).await?;

    edit(
        &course,
        1,
        vec![replace(
            "/explanation",
            "A short row gets None for every field it lacks.",
        )],
    )
    .await?;

    let now = rows(&course).await?;
    assert_eq!(now[0], seen[0]);
    assert_eq!(now[1].3, "A short row gets None for every field it lacks.");
    let ledger = ledger(&course).await?;
    let written = entry_for(&ledger, "A short row gets None for every field it lacks.");
    assert_eq!(written.location().unit_index, SECTIONS + 1);
    Ok(())
}

#[tokio::test]
async fn each_content_refusal_gives_its_text_and_writes_nothing() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;
    let add_choice = |n: usize| EditOp::Add {
        path: "/choices/-".into(),
        value: json!({"text": format!("Choice {n}."), "correct": false, "rationale": ""}),
    };
    let cases: Vec<(Vec<EditOp>, &str)> = vec![
        (vec![replace("/prompt", "  \n ")], "Prompt can't be empty."),
        (
            vec![
                EditOp::Remove {
                    path: "/choices/3".into(),
                },
                EditOp::Remove {
                    path: "/choices/2".into(),
                },
                EditOp::Remove {
                    path: "/choices/1".into(),
                },
            ],
            "Choices needs at least 2.",
        ),
        (
            vec![replace("/choices/1/text", "  ")],
            "Choice can't be empty.",
        ),
        (
            vec![replace("/choices/1/text", C0[0])],
            "Two choices say the same thing.",
        ),
        (
            vec![replace("/choices/0/correct", false)],
            "Mark a choice as the right answer.",
        ),
        (
            (4..9).map(add_choice).collect(),
            "Choices can have at most 8.",
        ),
    ];

    for (ops, text) in cases {
        let result = try_edit(&course, 0, ops).await;
        assert_eq!(refused(result), text);
        assert_eq!(snapshot(&course).await?, seen);
    }
    Ok(())
}

#[tokio::test]
async fn a_question_changed_since_it_was_opened_is_refused_and_nothing_is_written() -> Result<()> {
    let course = published().await?;
    let opened = open_question(&course, 0).await?;
    sqlx::query(
        "UPDATE learning_assessment_candidates SET prompt=? WHERE lesson_id=? AND prompt=?",
    )
    .bind("What does DictReader do with spaces in a field name?")
    .bind(&course.lesson_id)
    .bind(P0)
    .execute(&course.pool)
    .await?;
    let seen = snapshot(&course).await?;

    let result = course
        .edits()
        .apply(&ContentEditRequest {
            edit_id: Uuid::new_v4().to_string(),
            content: opened.content,
            expected_revision: opened.revision,
            ops: vec![replace("/explanation", NEW_EXPLANATION)],
            source: EditSource::Learner,
            target: None,
        })
        .await;

    assert_eq!(refused(result), CHANGED_SINCE);
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn an_edit_that_changes_nothing_writes_nothing() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;

    let saved = edit(&course, 0, vec![replace("/prompt", P0)]).await?;

    assert!(!saved.changed);
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn a_dry_run_writes_nothing_and_gives_the_counts_the_save_then_gives() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;
    let ops = vec![
        replace("/prompt", NEW_PROMPT),
        replace("/explanation", NEW_EXPLANATION),
    ];

    let preview = preview_edit(&course, 0, ops.clone()).await?;

    assert_eq!(snapshot(&course).await?, seen);
    assert!(preview.current);
    assert!(preview.changed);
    assert!(preview.problems.is_empty());

    let saved = edit(&course, 0, ops).await?;
    assert!(saved.changed);
    let now = ledger(&course).await?;
    assert!(matches!(
        old_claim(&now, P0).state(),
        ClaimState::Edited { .. }
    ));
    assert!(matches!(
        old_claim(&now, E0_FIRST).state(),
        ClaimState::Edited { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn the_revision_moves_once() -> Result<()> {
    let course = published().await?;
    let seen = ledger(&course).await?.revision.0;

    let saved = edit(&course, 0, vec![replace("/prompt", NEW_PROMPT)]).await?;

    assert!(saved.changed);
    assert_eq!(ledger(&course).await?.revision.0, seen + 1);
    Ok(())
}

#[tokio::test]
async fn a_lesson_from_another_course_is_refused() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;
    let missing = keys::question_ref("missing-question");

    let opened = course.edits().open(&missing).await;
    assert!(opened.is_err(), "{opened:?}");
    assert!(matches!(opened.unwrap_err(), AppError::NotFound(_)));
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn a_lesson_still_being_prepared_is_refused() -> Result<()> {
    let course = published().await?;
    let job: String = sqlx::query_scalar("SELECT id FROM jobs LIMIT 1")
        .fetch_one(&course.pool)
        .await?;
    sqlx::query(
        "INSERT INTO learning_lesson_reviews (lesson_id, program_id, job_id, state, draft_sha256, checker_model, opened_at, updated_at) \
         VALUES (?, ?, ?, 'needs_review', 'sha', 'fixture', 1, 1) \
         ON CONFLICT(lesson_id) DO UPDATE SET state='needs_review'",
    )
    .bind(&course.lesson_id)
    .bind(&course.program_id)
    .bind(&job)
    .execute(&course.pool)
    .await?;
    let seen = snapshot(&course).await?;

    let result = try_edit(&course, 0, vec![replace("/prompt", NEW_PROMPT)]).await;

    assert!(
        matches!(result, Err(AppError::InvalidState(_))),
        "{result:?}"
    );
    assert_eq!(
        refused(result),
        "This lesson is still being prepared. You can edit it once it is ready."
    );
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

async fn notes(course: &Course) -> Result<TeachingFindingsRepository> {
    let store = TeachingFindingsRepository::new(course.db.clone());
    let candidate = json!({"blocks":[
        {"title":"Section 0","body":section(GOOD)},
        {"title":"Section 1","body":section(&figure(7))}
    ],"questions":[
        {"prompt":P0,"options":C0,"explanation":E0},
        {"prompt":P1,"options":C1,"explanation":E1}
    ]});
    let note = |kind, site, text: &str| SetAside {
        finding: ReviewFinding::of_kind(kind, site, text),
        repairs: 0,
    };
    store
        .record(
            &course.lesson_id,
            "job",
            &candidate,
            &[
                note(FindingKind::AnswerKey, FindingSite::Question(0), "First."),
                note(FindingKind::AnswerKey, FindingSite::Question(1), "Second."),
                note(FindingKind::Teaching, FindingSite::Section(0), "Third."),
                note(FindingKind::Structure, FindingSite::Lesson, "Whole."),
            ],
            true,
        )
        .await?;
    Ok(store)
}

async fn changed_after(
    store: &TeachingFindingsRepository,
    lesson: &str,
) -> Result<Vec<(TeachingFindingSiteDto, Option<u32>, bool)>> {
    Ok(store
        .list(lesson)
        .await?
        .into_iter()
        .map(|note| (note.site, note.unit_index, note.part_changed))
        .collect())
}

fn only_question_zero_changed() -> Vec<(TeachingFindingSiteDto, Option<u32>, bool)> {
    vec![
        (TeachingFindingSiteDto::Lesson, None, false),
        (TeachingFindingSiteDto::Section, Some(0), false),
        (TeachingFindingSiteDto::Question, Some(0), true),
        (TeachingFindingSiteDto::Question, Some(1), false),
    ]
}

#[tokio::test]
async fn notes_on_the_edited_question_say_it_changed_after_them_and_other_notes_do_not(
) -> Result<()> {
    let course = published().await?;
    let store = notes(&course).await?;
    assert!(changed_after(&store, &course.lesson_id)
        .await?
        .iter()
        .all(|(_, _, changed)| !changed));

    edit(&course, 0, vec![replace("/prompt", NEW_PROMPT)]).await?;

    assert_eq!(
        changed_after(&store, &course.lesson_id).await?,
        only_question_zero_changed()
    );
    Ok(())
}

#[tokio::test]
async fn a_change_of_the_right_answer_alone_marks_the_questions_notes() -> Result<()> {
    let course = published().await?;
    let store = notes(&course).await?;

    edit(
        &course,
        0,
        vec![
            replace("/choices/0/correct", false),
            replace("/choices/1/correct", true),
        ],
    )
    .await?;

    assert_eq!(
        changed_after(&store, &course.lesson_id).await?,
        only_question_zero_changed()
    );
    Ok(())
}

#[tokio::test]
async fn the_report_takes_the_questions_new_title_and_text_and_one_confirmed_finding_per_written_sentence(
) -> Result<()> {
    let course = published().await?;
    let before = stored_report(&course).await?;
    let mut choices = C0.map(String::from).to_vec();
    choices[1] = "It trims them silently.".into();

    edit(
        &course,
        0,
        vec![
            replace("/prompt", NEW_PROMPT),
            replace("/choices/1/text", "It trims them silently."),
            replace("/explanation", NEW_EXPLANATION),
        ],
    )
    .await?;

    let now = stored_report(&course).await?;
    let text = format!("{}\n\n{NEW_EXPLANATION}", choices.join("\n\n"));
    assert_eq!(
        now.0,
        sha256_hex(json!([before.0, SECTIONS, NEW_PROMPT, text]).to_string())
    );
    let findings = serde_json::from_str::<Value>(&now.1)?["findings"]
        .as_array()
        .unwrap()
        .clone();
    let ledger = ledger(&course).await?;
    let on_question: Vec<&Value> = findings
        .iter()
        .filter(|finding| finding["unit"] == json!(SECTIONS))
        .collect();
    let written = [
        NEW_PROMPT,
        "It trims them silently.",
        "Header names keep their spaces as the file writes them.",
        "Strip them yourself if you need to.",
    ];
    assert_eq!(on_question.len(), written.len(), "{on_question:?}");
    for sentence in written {
        let finding = on_question
            .iter()
            .find(|finding| finding["statement"] == json!(sentence))
            .unwrap_or_else(|| panic!("{sentence}: {on_question:?}"));
        assert_eq!(finding["resolution"]["kind"], json!("confirmed"));
        assert_eq!(
            finding["claim_id"],
            json!(entry_for(&ledger, sentence).claim_id().0)
        );
    }
    Ok(())
}

const R0: [&str; 4] = [
    "",
    "Trimming is not done.",
    "The column stays.",
    "No error is raised.",
];

async fn give_the_first_question_its_reasons(course: &Course) -> Result<()> {
    sqlx::query(
        "UPDATE learning_assessment_answer_keys SET distractor_rationales_json=? \
         WHERE candidate_id IN (SELECT id FROM learning_assessment_candidates WHERE lesson_id=? AND prompt=?)",
    )
    .bind(serde_json::to_string(&R0)?)
    .bind(&course.lesson_id)
    .bind(P0)
    .execute(&course.pool)
    .await?;
    Ok(())
}

async fn stored_reasons(course: &Course) -> Result<Vec<String>> {
    Ok(serde_json::from_str(&rows(course).await?[0].4)?)
}

#[tokio::test]
async fn a_prompt_change_keeps_the_reasons_and_the_right_choice_is_left_with_none() -> Result<()> {
    let course = published().await?;
    give_the_first_question_its_reasons(&course).await?;
    let kept = rows(&course).await?[0].4.clone();

    edit(&course, 0, vec![replace("/prompt", NEW_PROMPT)]).await?;
    assert_eq!(rows(&course).await?[0].4, kept);

    edit(
        &course,
        0,
        vec![
            replace("/choices/0/correct", false),
            replace("/choices/2/correct", true),
            replace("/choices/0/rationale", "It does keep them."),
            replace("/choices/2/rationale", ""),
        ],
    )
    .await?;

    assert_eq!(
        stored_reasons(&course).await?,
        ["It does keep them.", R0[1], "", R0[3]]
    );
    Ok(())
}

const LONGEST: &str = "It keeps them exactly as the file wrote them, spaces and all.";
const LONGEST_REFUSAL: &str = "The correct option stands out as much the longest.";
fn leak_refusal() -> String {
    answer_in_prompt_message(C0[0])
}

#[tokio::test]
async fn an_edit_the_quiz_engine_would_refuse_is_refused_and_writes_nothing() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;
    let cases = [
        (
            vec![replace("/choices/0/text", LONGEST)],
            LONGEST_REFUSAL.to_owned(),
        ),
        (
            vec![replace("/prompt", format!("{P0} It keeps them."))],
            leak_refusal(),
        ),
    ];

    for (ops, text) in cases {
        let result = try_edit(&course, 0, ops).await;
        assert!(
            matches!(result, Err(AppError::InvalidInput(_))),
            "{result:?}"
        );
        assert_eq!(refused(result), text);
        assert_eq!(snapshot(&course).await?, seen);
    }
    Ok(())
}

#[tokio::test]
async fn a_dry_run_gives_the_same_refusals_as_the_save() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;
    let cases = [
        (
            vec![replace("/choices/0/text", LONGEST)],
            LONGEST_REFUSAL.to_owned(),
        ),
        (
            vec![replace("/prompt", format!("{P0} It keeps them."))],
            leak_refusal(),
        ),
    ];

    for (ops, text) in cases {
        let preview = preview_edit(&course, 0, ops).await?;
        assert!(
            preview.problems.iter().any(|p| p.message == text),
            "{preview:?}"
        );
        assert_eq!(snapshot(&course).await?, seen);
    }
    Ok(())
}

#[tokio::test]
async fn the_longest_right_choice_of_two_is_not_refused() -> Result<()> {
    let course = published().await?;
    let saved = edit(
        &course,
        0,
        vec![
            EditOp::Remove {
                path: "/choices/3".into(),
            },
            EditOp::Remove {
                path: "/choices/2".into(),
            },
            replace("/choices/0/text", LONGEST),
        ],
    )
    .await?;
    assert!(saved.changed);
    Ok(())
}

async fn set_status(course: &Course, prompt: &str, status: &str) -> Result<()> {
    sqlx::query(
        "UPDATE learning_assessment_candidates SET review_status=? WHERE lesson_id=? AND prompt=?",
    )
    .bind(status)
    .bind(&course.lesson_id)
    .bind(prompt)
    .execute(&course.pool)
    .await?;
    Ok(())
}

async fn statuses(course: &Course) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT review_status FROM learning_assessment_candidates WHERE lesson_id=? ORDER BY rowid",
    )
    .bind(&course.lesson_id)
    .fetch_all(&course.pool)
    .await?)
}

#[tokio::test]
async fn a_valid_save_returns_a_dropped_question_to_the_bank() -> Result<()> {
    let course = published().await?;
    set_status(&course, P0, "dropped").await?;

    edit(&course, 0, vec![replace("/prompt", NEW_PROMPT)]).await?;

    assert_eq!(statuses(&course).await?, ["agreed", "agreed", "agreed"]);
    Ok(())
}

#[tokio::test]
async fn a_refused_save_leaves_a_dropped_question_dropped() -> Result<()> {
    let course = published().await?;
    set_status(&course, P0, "dropped").await?;

    let result = try_edit(&course, 0, vec![replace("/choices/0/text", LONGEST)]).await;

    assert_eq!(refused(result), LONGEST_REFUSAL);
    assert_eq!(statuses(&course).await?, ["dropped", "agreed", "agreed"]);
    Ok(())
}

#[tokio::test]
async fn a_dry_run_leaves_a_dropped_question_dropped() -> Result<()> {
    let course = published().await?;
    set_status(&course, P0, "dropped").await?;

    preview_edit(&course, 0, vec![replace("/prompt", NEW_PROMPT)]).await?;

    assert_eq!(statuses(&course).await?, ["dropped", "agreed", "agreed"]);
    Ok(())
}

const SAME_QUESTION: &str = "Another question in this course has the same prompt and choices.";

type Held = (String, String, String, String, String, String);

async fn held(course: &Course) -> Result<Vec<Held>> {
    Ok(sqlx::query_as::<_, Held>(
        "SELECT c.prompt, c.options_json, k.key_json, k.explanation, c.content_sha256, c.review_status \
         FROM learning_assessment_candidates c JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id \
         WHERE c.lesson_id=? ORDER BY c.rowid",
    )
    .bind(&course.lesson_id)
    .fetch_all(&course.pool)
    .await?)
}

async fn add_question(course: &Course, spec: &QuestionSpec<'_>) -> Result<()> {
    let outcome: String = sqlx::query_scalar(
        "SELECT id FROM learning_outcome_definitions WHERE program_id=? AND lesson_id=?",
    )
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .fetch_one(&course.pool)
    .await?;
    let mut entry = bank_item(&outcome, spec);
    entry.lesson_id = Some(course.lesson_id.clone());
    let mut tx = course.db.begin_write().await?;
    let stored = insert_bank_items_in(&mut tx, &course.program_id, &[entry], None).await?;
    tx.commit().await?;
    assert_eq!(stored, 1);
    Ok(())
}

fn copy_q0_ops() -> Vec<EditOp> {
    vec![
        replace("/prompt", P0),
        replace("/choices/0/text", C0[0]),
        replace("/choices/1/text", C0[1]),
        replace("/choices/2/text", C0[2]),
        replace("/choices/3/text", C0[3]),
    ]
}

#[tokio::test]
async fn an_edit_that_matches_another_question_is_refused_and_writes_nothing() -> Result<()> {
    let course = published().await?;
    let seen = held(&course).await?;
    let report = snapshot(&course).await?;

    let result = try_edit(&course, 1, copy_q0_ops()).await;

    assert_eq!(refused(result), SAME_QUESTION);
    assert_eq!(held(&course).await?, seen);
    assert_eq!(snapshot(&course).await?, report);
    Ok(())
}

#[tokio::test]
async fn saving_a_dropped_repeat_as_it_stands_is_refused_and_it_stays_dropped() -> Result<()> {
    let course = published().await?;
    add_question(&course, &spec0()).await?;
    assert_eq!(
        statuses(&course).await?,
        ["agreed", "agreed", "agreed", "dropped"]
    );
    let seen = held(&course).await?;

    let result = try_edit(&course, 3, vec![replace("/explanation", NEW_EXPLANATION)]).await;

    assert_eq!(refused(result), SAME_QUESTION);
    assert_eq!(held(&course).await?, seen);
    assert_eq!(
        statuses(&course).await?,
        ["agreed", "agreed", "agreed", "dropped"]
    );
    Ok(())
}

#[tokio::test]
async fn a_saved_question_keeps_the_token_of_its_new_content() -> Result<()> {
    let course = published().await?;
    edit(&course, 0, vec![replace("/prompt", NEW_PROMPT)]).await?;

    add_question(&course, &spec0()).await?;
    add_question(
        &course,
        &QuestionSpec {
            prompt: NEW_PROMPT,
            ..spec0()
        },
    )
    .await?;

    assert_eq!(
        statuses(&course).await?,
        ["agreed", "agreed", "agreed", "agreed", "dropped"]
    );
    Ok(())
}

#[tokio::test]
async fn a_dry_run_refuses_an_edit_that_matches_another_question() -> Result<()> {
    let course = published().await?;
    let seen = held(&course).await?;

    let preview = preview_edit(&course, 1, copy_q0_ops()).await?;

    assert!(
        preview.problems.iter().any(|p| p.message == SAME_QUESTION),
        "{preview:?}"
    );
    assert_eq!(held(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn a_new_explanation_on_a_question_that_repeats_nothing_is_saved() -> Result<()> {
    let course = published().await?;
    let saved = edit(&course, 0, vec![replace("/explanation", NEW_EXPLANATION)]).await?;

    assert!(saved.changed);
    assert_eq!(statuses(&course).await?, ["agreed", "agreed", "agreed"]);
    assert_eq!(held(&course).await?[0].3, NEW_EXPLANATION);
    Ok(())
}

#[tokio::test]
async fn a_save_leaves_a_reported_question_reported() -> Result<()> {
    let course = published().await?;
    set_status(&course, P0, "reported").await?;

    edit(&course, 0, vec![replace("/prompt", NEW_PROMPT)]).await?;

    assert_eq!(statuses(&course).await?, ["reported", "agreed", "agreed"]);
    Ok(())
}

const EDITED_THE_QUESTION: &str =
    "This changed when you edited the question. Edit the question again to change it back.";

#[tokio::test]
async fn the_undo_of_a_statement_a_question_edit_ended_is_refused_with_the_questions_text(
) -> Result<()> {
    let course = published().await?;
    let seen = ledger(&course).await?;
    let ended = [
        entry_for(&seen, P0).claim_id().clone(),
        entry_for(&seen, E0_FIRST).claim_id().clone(),
    ];
    edit(
        &course,
        0,
        vec![
            replace("/prompt", NEW_PROMPT),
            replace("/explanation", NEW_EXPLANATION),
        ],
    )
    .await?;
    let written = snapshot(&course).await?;

    for claim in &ended {
        let revision = ledger(&course).await?.revision.0;
        let result = ClaimReviewService::new(course.db.clone())
            .resolve(
                &course.lesson_id,
                &claim.0,
                ClaimActionRequestDto::Undo,
                revision,
            )
            .await;
        assert!(
            matches!(result, Err(AppError::InvalidState(_))),
            "{result:?}"
        );
        assert_eq!(refused(result), EDITED_THE_QUESTION);
    }
    assert_eq!(snapshot(&course).await?, written);
    Ok(())
}

#[tokio::test]
async fn a_questions_reasons_are_read_with_it_and_an_edit_of_one_is_written_in_its_slot(
) -> Result<()> {
    let course = published().await?;
    give_the_first_question_its_reasons(&course).await?;
    let opened = open_question(&course, 0).await?;
    for (slot, reason) in R0.iter().enumerate() {
        assert_eq!(opened.document["choices"][slot]["rationale"], json!(reason));
    }

    let mut rationales = R0.map(String::from).to_vec();
    rationales[2] = "The column stays, spaces and all.".into();
    let saved = edit(
        &course,
        0,
        vec![replace("/choices/2/rationale", rationales[2].as_str())],
    )
    .await?;

    assert!(saved.changed);
    assert_eq!(stored_reasons(&course).await?, rationales);
    let reopened = open_question(&course, 0).await?;
    assert_eq!(
        reopened.document["choices"][2]["rationale"],
        json!(rationales[2])
    );
    Ok(())
}

#[tokio::test]
async fn a_choice_removed_or_added_keeps_each_reason_with_its_choice() -> Result<()> {
    let course = published().await?;
    give_the_first_question_its_reasons(&course).await?;

    edit(
        &course,
        0,
        vec![EditOp::Remove {
            path: "/choices/2".into(),
        }],
    )
    .await?;
    assert_eq!(
        rows(&course).await?[0].1,
        serde_json::to_string(&[C0[0], C0[1], C0[3]])?
    );
    assert_eq!(stored_reasons(&course).await?, [R0[0], R0[1], R0[3]]);

    edit(
        &course,
        0,
        vec![EditOp::Add {
            path: "/choices/-".into(),
            value: json!({
                "text": "It ignores them.",
                "correct": false,
                "rationale": "Nothing ignores a header name."
            }),
        }],
    )
    .await?;
    assert_eq!(
        stored_reasons(&course).await?,
        [R0[0], R0[1], R0[3], "Nothing ignores a header name."]
    );
    Ok(())
}

#[tokio::test]
async fn reasons_that_do_not_match_the_choices_are_refused_and_write_nothing() -> Result<()> {
    let course = published().await?;
    give_the_first_question_its_reasons(&course).await?;
    let seen = snapshot(&course).await?;

    let result = try_edit(
        &course,
        0,
        vec![EditOp::Add {
            path: "/choices/-".into(),
            value: json!({"text": "Extra.", "correct": false}),
        }],
    )
    .await;

    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "{result:?}"
    );
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn a_statement_on_a_choices_reason_ends_when_the_reason_is_rewritten_and_its_replacement_is_the_users(
) -> Result<()> {
    let course = published().await?;
    give_the_first_question_its_reasons(&course).await?;
    claim(
        &course,
        SECTIONS,
        UnitKind::AnswerKey,
        "rationales/1",
        R0[1],
        R0[1],
    )
    .await?;
    let seen = ledger(&course).await?;
    let replacement = "Nothing trims a header name.";

    let saved = edit(
        &course,
        0,
        vec![replace("/choices/1/rationale", replacement)],
    )
    .await?;

    assert!(saved.changed);
    let now = ledger(&course).await?;
    assert_eq!(now.revision.0, seen.revision.0 + 1);
    assert_edited_into(&now, R0[1], replacement);
    assert_confirmed_at(
        &now,
        replacement,
        SECTIONS,
        UnitKind::AnswerKey,
        "rationales/1",
        replacement,
    );
    Ok(())
}

#[tokio::test]
async fn a_save_whose_example_cannot_be_recorded_writes_nothing() -> Result<()> {
    let course = published().await?;
    course.flag_locally(P0).await?;
    let before = snapshot(&course).await?;
    sqlx::query(
        "CREATE TRIGGER refuse_for_test BEFORE INSERT ON learning_checker_examples BEGIN SELECT RAISE(ABORT, 'refused for the test'); END",
    )
    .execute(&course.pool)
    .await?;

    let result = try_edit(&course, 0, vec![replace("/prompt", NEW_PROMPT)]).await;

    assert!(result.is_err());
    assert_eq!(snapshot(&course).await?, before);
    Ok(())
}
