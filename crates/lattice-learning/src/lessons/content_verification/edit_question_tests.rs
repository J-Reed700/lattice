#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! The user rewrites a question of a published lesson: its prompt, choices,
//! right answer and explanation are written, the statements that stood on
//! what changed end, the rest keep their state, and the review's record
//! stays honest.
use super::claim_ledger_tests::{figure, lesson, section, unreadable, Course};
use super::GOOD;
use crate::assessment_engine::{
    LearningAssessmentCandidate, LearningAssessmentKey, LearningItemFormat,
};
use crate::assessment_repository::{insert_bank_items_in, BankItem};
use crate::content_verification::digest;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::lessons::claim_reviews::service::{
    ClaimReviewService, QuestionEdit, QuestionEditOutcome, QuestionText,
};
use crate::lessons::claim_reviews::teaching_findings::{
    TeachingFindingSiteDto, TeachingFindingsRepository,
};
use crate::review_findings::{FindingKind, FindingSite, ReviewFinding, SetAside};
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, ClaimState, EventProvenance,
    Ledger, UnitKind, UnverifiedReason,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

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

/// A section's worth of units come first: the two sections of the lesson.
const SECTIONS: usize = 2;

const CHANGED_SINCE: &str =
    "This question changed since you opened it. Copy your changes, close the editor and open it again.";

fn q0() -> QuestionText {
    QuestionText {
        kind: LearningItemFormat::SingleChoice,
        prompt: P0.into(),
        choices: C0.map(String::from).to_vec(),
        right: Some(0),
        explanation: E0.into(),
    }
}

fn q1() -> QuestionText {
    QuestionText {
        kind: LearningItemFormat::SingleChoice,
        prompt: P1.into(),
        choices: C1.map(String::from).to_vec(),
        right: Some(0),
        explanation: E1.into(),
    }
}

fn q2() -> QuestionText {
    QuestionText {
        kind: LearningItemFormat::TrueFalse,
        prompt: P2.into(),
        choices: vec!["True".into(), "False".into()],
        right: Some(1),
        explanation: E2.into(),
    }
}

fn bank_item(outcome: &str, text: &QuestionText) -> BankItem {
    let candidate = LearningAssessmentCandidate {
        id: uuid::Uuid::new_v4().to_string(),
        outcome_ids: vec![outcome.into()],
        format: text.kind,
        difficulty: 3,
        prompt: text.prompt.clone(),
        options: text.choices.clone(),
        left: Vec::new(),
        source_version_ids: Vec::new(),
        rubric: Vec::new(),
        key: LearningAssessmentKey::Choice(text.right.unwrap()),
        explanation: text.explanation.clone(),
        distractor_rationales: Vec::new(),
    };
    BankItem {
        item: candidate.into(),
        source_quote: String::new(),
        lesson_id: None,
        origin: "lesson",
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
    ClaimReviewRepository::new(course.pool.clone())
        .load(&course.lesson_id)
        .await
}

/// A statement the review holds on `sentence` of `field`, not yet checked.
async fn claim(
    course: &Course,
    unit: usize,
    kind: UnitKind,
    field: &str,
    sentence: &str,
    field_text: &str,
) -> Result<()> {
    let repository = ClaimReviewRepository::new(course.pool.clone());
    let id = ClaimId(uuid::Uuid::new_v4().to_string());
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

/// A published lesson of two sections and three questions, the first with a
/// statement on its prompt, three of its choices and both sentences of its
/// explanation, the second with one on its explanation, the third true or
/// false. The questions are in the lesson's quiz bank in this order.
async fn published() -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&unreadable(), &lesson(&[GOOD, &figure(7)]))
        .await
        .expect("the lesson publishes, its statements not verified");
    course.publish(prepared).await?;
    let outcome = uuid::Uuid::new_v4().to_string();
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
    let mut items: Vec<BankItem> = [q0(), q1(), q2()]
        .iter()
        .map(|text| bank_item(&outcome, text))
        .collect();
    for item in &mut items {
        item.lesson_id = Some(course.lesson_id.clone());
    }
    let mut connection = course.pool.acquire().await?;
    let stored = insert_bank_items_in(&mut connection, &course.program_id, &items, None).await?;
    drop(connection);
    assert_eq!(stored, 3);
    let unit = SECTIONS;
    claim(&course, unit, UnitKind::Question, "prompt", P0, P0).await?;
    for slot in 1..4 {
        let field = format!("options/{slot}");
        claim(
            &course,
            unit,
            UnitKind::Question,
            &field,
            C0[slot],
            C0[slot],
        )
        .await?;
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

async fn stored_report(course: &Course) -> Result<(String, String)> {
    Ok(sqlx::query_as::<_, (String, String)>(
        "SELECT content_sha256, report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .fetch_one(&course.pool)
    .await?)
}

/// Everything an edit may write.
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

async fn edit(
    course: &Course,
    index: usize,
    before: QuestionText,
    after: QuestionText,
    dry_run: bool,
) -> Result<QuestionEditOutcome> {
    ClaimReviewService::new(course.pool.clone())
        .edit_question(QuestionEdit {
            program_id: course.program_id.clone(),
            lesson_id: course.lesson_id.clone(),
            question_index: index,
            before,
            after,
            dry_run,
        })
        .await
}

fn refusal(error: &AppError) -> &str {
    match error {
        AppError::InvalidInput(message)
        | AppError::InvalidState(message)
        | AppError::NotFound(message) => message,
        other => panic!("not a refusal: {other:?}"),
    }
}

/// The refusal's text, or `accepted` when there was none.
fn refused<T>(result: Result<T>) -> String {
    match result {
        Ok(_) => "accepted".into(),
        Err(error) => refusal(&error).to_owned(),
    }
}

fn outcome(
    kept: u32,
    changed: u32,
    removed: u32,
    written: u32,
    revision: Option<i64>,
) -> QuestionEditOutcome {
    QuestionEditOutcome {
        changed: true,
        statements_kept: kept,
        statements_changed: changed,
        statements_removed: removed,
        sentences_written: written,
        checked_statements_affected: 0,
        revision,
    }
}

fn old_claim<'a>(ledger: &'a Ledger, statement: &str) -> &'a ClaimLedgerEntry {
    entry_for(ledger, statement)
}

/// The claim `old` ended edited by the user, and the claim `new` is its
/// successor.
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
    let service = ClaimReviewService::new(course.pool.clone());

    for (index, expected) in [q0(), q1(), q2()].into_iter().enumerate() {
        let read = service
            .question_text(&course.program_id, &course.lesson_id, index)
            .await?;
        assert_eq!(read, expected, "question {index}");
    }

    let error = service
        .question_text(&course.program_id, &course.lesson_id, 3)
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::NotFound(_)), "{error:?}");
    Ok(())
}

#[tokio::test]
async fn a_question_of_a_lesson_from_another_course_is_not_read() -> Result<()> {
    let course = published().await?;

    let error = ClaimReviewService::new(course.pool.clone())
        .question_text("another-course", &course.lesson_id, 0)
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::NotFound(_)), "{error:?}");
    assert_eq!(refusal(&error), "This lesson is not in this course.");
    Ok(())
}

#[tokio::test]
async fn a_save_writes_the_prompt_and_the_explanation() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;
    let after = QuestionText {
        prompt: NEW_PROMPT.into(),
        explanation: NEW_EXPLANATION.into(),
        ..q0()
    };

    let saved = edit(&course, 0, q0(), after, false).await?;

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
    let after = QuestionText {
        prompt: NEW_PROMPT.into(),
        explanation: NEW_EXPLANATION.into(),
        ..q0()
    };

    let saved = edit(&course, 0, q0(), after, false).await?;

    assert_eq!(saved, outcome(4, 2, 0, 3, Some(seen.revision.0 + 1)));
    let now = ledger(&course).await?;
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
    let mut choices = C0.map(String::from).to_vec();
    choices[0] = "It keeps them exactly.".into();
    choices[1] = "It trims them silently.".into();
    let after = QuestionText { choices, ..q0() };

    let saved = edit(&course, 0, q0(), after.clone(), false).await?;

    assert_eq!(saved, outcome(5, 1, 0, 1, Some(seen.revision.0 + 1)));
    let now = ledger(&course).await?;
    assert_edited_into(&now, C0[1], "It trims them silently.");
    assert_confirmed_at(
        &now,
        "It trims them silently.",
        SECTIONS,
        UnitKind::Question,
        "options/1",
        "It trims them silently.",
    );
    assert!(
        now.claims
            .iter()
            .all(|entry| entry.statement() != "It keeps them exactly."),
        "a new statement is written only where it replaces an edited one"
    );
    assert_eq!(
        rows(&course).await?[0].1,
        serde_json::to_string(&after.choices)?
    );
    Ok(())
}

#[tokio::test]
async fn removing_a_choice_compares_the_slots_that_follow_it_and_ends_the_statement_of_the_last_slot(
) -> Result<()> {
    let course = published().await?;
    let seen = ledger(&course).await?;
    let choices = vec![C0[0].to_owned(), C0[2].to_owned(), C0[3].to_owned()];
    let after = QuestionText { choices, ..q0() };

    let saved = edit(&course, 0, q0(), after.clone(), false).await?;

    assert_eq!(saved, outcome(3, 2, 1, 2, Some(seen.revision.0 + 1)));
    let now = ledger(&course).await?;
    let at = |statement: &str, field: &str| -> Vec<&ClaimLedgerEntry> {
        now.claims
            .iter()
            .filter(|entry| entry.statement() == statement && entry.location().field == field)
            .collect()
    };
    let last_slot = at(C0[3], "options/3");
    assert_eq!(last_slot.len(), 1);
    assert!(
        matches!(
            last_slot[0].state(),
            ClaimState::Removed {
                by: Actor::User,
                ..
            }
        ),
        "{:?}",
        last_slot[0].state()
    );
    let moved_up = at(C0[3], "options/2");
    assert_eq!(moved_up.len(), 1);
    assert!(matches!(moved_up[0].state(), ClaimState::Confirmed { .. }));
    let old_slot_two = at(C0[2], "options/2");
    assert_eq!(old_slot_two.len(), 1);
    match old_slot_two[0].state() {
        ClaimState::Edited {
            successor: Some(id),
            by: Actor::User,
            ..
        } => assert_eq!(id, at(C0[2], "options/1")[0].claim_id()),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&rows(&course).await?[0].1)?,
        after.choices
    );
    Ok(())
}

#[tokio::test]
async fn adding_a_choice_writes_it_and_touches_no_statement() -> Result<()> {
    let course = published().await?;
    let seen = ledger(&course).await?;
    let mut choices = C0.map(String::from).to_vec();
    choices.push("It ignores them.".into());
    let after = QuestionText { choices, ..q0() };

    let saved = edit(&course, 0, q0(), after.clone(), false).await?;

    assert_eq!(saved, outcome(6, 0, 0, 0, Some(seen.revision.0 + 1)));
    let now = ledger(&course).await?;
    assert_eq!(format!("{:?}", now.claims), format!("{:?}", seen.claims));
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&rows(&course).await?[0].1)?,
        after.choices
    );
    Ok(())
}

#[tokio::test]
async fn a_change_of_the_right_answer_alone_touches_no_statement_and_is_still_a_change(
) -> Result<()> {
    let course = published().await?;
    let seen = ledger(&course).await?;
    let after = QuestionText {
        right: Some(2),
        ..q0()
    };

    let saved = edit(&course, 0, q0(), after, false).await?;

    assert_eq!(saved, outcome(6, 0, 0, 0, Some(seen.revision.0 + 1)));
    let now = ledger(&course).await?;
    assert_eq!(format!("{:?}", now.claims), format!("{:?}", seen.claims));
    let stored: Value = serde_json::from_str(&rows(&course).await?[0].2)?;
    assert_eq!(stored, json!({"kind":"choice","value":2}));
    assert_eq!(ledger(&course).await?.revision.0, seen.revision.0 + 1);
    Ok(())
}

#[tokio::test]
async fn a_true_or_false_question_keeps_its_choices_and_changes_only_its_right_answer() -> Result<()>
{
    let course = published().await?;
    let seen = rows(&course).await?;
    let after = QuestionText {
        choices: vec!["Yes".into(), "No".into()],
        right: Some(0),
        ..q2()
    };

    let saved = edit(&course, 2, q2(), after, false).await?;

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
    let after = QuestionText {
        right: None,
        ..q2()
    };

    let result = edit(&course, 2, q2(), after, false).await;

    assert_eq!(refused(result), "Choose the right answer.");
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn the_second_question_is_the_one_written_when_the_second_is_edited() -> Result<()> {
    let course = published().await?;
    let seen = rows(&course).await?;
    let after = QuestionText {
        explanation: "A short row gets None for every field it lacks.".into(),
        ..q1()
    };

    let saved = edit(&course, 1, q1(), after, false).await?;

    assert_eq!(saved.statements_changed, 1);
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
    let nine: Vec<String> = (0..9).map(|n| format!("Choice {n}.")).collect();
    let cases: Vec<(QuestionText, &str)> = vec![
        (
            QuestionText {
                prompt: "  \n ".into(),
                ..q0()
            },
            "A question needs a prompt.",
        ),
        (
            QuestionText {
                choices: vec!["Only.".into()],
                right: Some(0),
                ..q0()
            },
            "A question needs at least two choices.",
        ),
        (
            QuestionText {
                choices: vec![],
                right: None,
                ..q0()
            },
            "A question needs at least two choices.",
        ),
        (
            QuestionText {
                choices: vec!["One.".into(), "  ".into()],
                right: Some(0),
                ..q0()
            },
            "Each choice needs some text.",
        ),
        (
            QuestionText {
                choices: vec!["Same.".into(), "Same.".into()],
                right: Some(0),
                ..q0()
            },
            "Each choice needs different text.",
        ),
        (
            QuestionText {
                right: Some(4),
                ..q0()
            },
            "Choose the right answer.",
        ),
        (
            QuestionText {
                right: None,
                ..q0()
            },
            "Choose the right answer.",
        ),
        (
            QuestionText {
                choices: nine,
                right: Some(0),
                ..q0()
            },
            "A question can have at most 8 choices.",
        ),
    ];

    for (after, text) in cases {
        let result = edit(&course, 0, q0(), after.clone(), false).await;
        assert_eq!(refused(result), text, "{after:?}");
        assert_eq!(snapshot(&course).await?, seen);
    }
    Ok(())
}

#[tokio::test]
async fn a_question_changed_since_it_was_opened_is_refused_and_nothing_is_written() -> Result<()> {
    let course = published().await?;
    sqlx::query(
        "UPDATE learning_assessment_candidates SET prompt=? WHERE lesson_id=? AND prompt=?",
    )
    .bind("What does DictReader do with spaces in a field name?")
    .bind(&course.lesson_id)
    .bind(P0)
    .execute(&course.pool)
    .await?;
    let seen = snapshot(&course).await?;
    let after = QuestionText {
        explanation: NEW_EXPLANATION.into(),
        ..q0()
    };

    let result = edit(&course, 0, q0(), after, false).await;

    assert_eq!(refused(result), CHANGED_SINCE);
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn an_edit_that_changes_nothing_writes_nothing() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;

    let saved = edit(&course, 0, q0(), q0(), false).await?;

    assert!(!saved.changed);
    assert_eq!(saved.revision, None);
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn a_dry_run_writes_nothing_and_gives_the_counts_the_save_then_gives() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;
    let after = QuestionText {
        prompt: NEW_PROMPT.into(),
        explanation: NEW_EXPLANATION.into(),
        ..q0()
    };

    let dry = edit(&course, 0, q0(), after.clone(), true).await?;

    assert_eq!(snapshot(&course).await?, seen);
    assert!(dry.changed);
    assert_eq!(dry.revision, None);
    assert_eq!(dry.statements_changed, 2);
    let saved = edit(&course, 0, q0(), after, false).await?;
    assert_eq!(
        QuestionEditOutcome {
            revision: None,
            ..saved
        },
        dry
    );
    Ok(())
}

#[tokio::test]
async fn the_revision_moves_once() -> Result<()> {
    let course = published().await?;
    let seen = ledger(&course).await?.revision.0;
    let after = QuestionText {
        prompt: NEW_PROMPT.into(),
        ..q0()
    };

    let saved = edit(&course, 0, q0(), after, false).await?;

    assert_eq!(saved.revision, Some(seen + 1));
    assert_eq!(ledger(&course).await?.revision.0, seen + 1);
    Ok(())
}

#[tokio::test]
async fn a_lesson_from_another_course_is_refused() -> Result<()> {
    let course = published().await?;
    let seen = snapshot(&course).await?;
    let after = QuestionText {
        prompt: NEW_PROMPT.into(),
        ..q0()
    };

    let result = ClaimReviewService::new(course.pool.clone())
        .edit_question(QuestionEdit {
            program_id: "another-course".into(),
            lesson_id: course.lesson_id.clone(),
            question_index: 0,
            before: q0(),
            after,
            dry_run: false,
        })
        .await;

    assert!(matches!(result, Err(AppError::NotFound(_))), "{result:?}");
    assert_eq!(refused(result), "This lesson is not in this course.");
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
        "INSERT INTO learning_lesson_reviews (lesson_id, program_id, job_id, state, draft_sha256, checker_model, opened_at, updated_at)          VALUES (?, ?, ?, 'needs_review', 'sha', 'fixture', 1, 1)          ON CONFLICT(lesson_id) DO UPDATE SET state='needs_review'",
    )
    .bind(&course.lesson_id)
    .bind(&course.program_id)
    .bind(&job)
    .execute(&course.pool)
    .await?;
    let seen = snapshot(&course).await?;
    let after = QuestionText {
        prompt: NEW_PROMPT.into(),
        ..q0()
    };

    let result = edit(&course, 0, q0(), after, false).await;

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
    let store = TeachingFindingsRepository::new(course.pool.clone());
    let candidate = json!({"blocks":[
        {"title":"Section 0","body":section(GOOD)},
        {"title":"Section 1","body":section(&figure(7))}
    ],"questions":[
        {"prompt":P0,"options":C0,"explanation":E0},
        {"prompt":P1,"options":C1,"explanation":E1}
    ]});
    let note = |kind, site, text: &str| SetAside {
        finding: ReviewFinding::new(kind, Some(site), text),
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

/// Each note's site, index and whether its part changed after it, in list order.
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
    let after = QuestionText {
        prompt: NEW_PROMPT.into(),
        ..q0()
    };

    edit(&course, 0, q0(), after, false).await?;

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
    let after = QuestionText {
        right: Some(1),
        ..q0()
    };

    edit(&course, 0, q0(), after, false).await?;

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
    let after = QuestionText {
        prompt: NEW_PROMPT.into(),
        choices: choices.clone(),
        explanation: NEW_EXPLANATION.into(),
        ..q0()
    };

    edit(&course, 0, q0(), after, false).await?;

    let now = stored_report(&course).await?;
    let text = format!("{}\n\n{NEW_EXPLANATION}", choices.join("\n\n"));
    assert_eq!(
        now.0,
        digest(&json!([before.0, SECTIONS, NEW_PROMPT, text]).to_string())
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

#[tokio::test]
async fn the_choices_and_the_right_answer_stored_before_a_change_lose_their_rationales_and_a_prompt_change_keeps_them(
) -> Result<()> {
    let course = published().await?;
    sqlx::query(
        "UPDATE learning_assessment_answer_keys SET distractor_rationales_json=? \
         WHERE candidate_id IN (SELECT id FROM learning_assessment_candidates WHERE lesson_id=? AND prompt=?)",
    )
    .bind(r#"["","Trimming is not done.","The column stays.","No error is raised."]"#)
    .bind(&course.lesson_id)
    .bind(P0)
    .execute(&course.pool)
    .await?;
    let kept = rows(&course).await?[0].4.clone();
    let prompt_only = QuestionText {
        prompt: NEW_PROMPT.into(),
        ..q0()
    };
    edit(&course, 0, q0(), prompt_only.clone(), false).await?;
    assert_eq!(rows(&course).await?[0].4, kept);

    let mut choices = C0.map(String::from).to_vec();
    choices[2] = "It drops the whole column.".into();
    let changed = QuestionText {
        choices,
        ..prompt_only.clone()
    };
    edit(&course, 0, prompt_only, changed, false).await?;

    assert_eq!(rows(&course).await?[0].4, "[]");
    Ok(())
}
