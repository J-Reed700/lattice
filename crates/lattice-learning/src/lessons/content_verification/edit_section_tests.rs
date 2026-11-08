#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! The user rewrites a section of a published lesson: the section's title and
//! text are written, the statements that stood on what changed end, the rest
//! keep their state, and the review's record stays honest.
use super::claim_ledger_tests::{figure, lesson, section, unreadable, Course, Figures};
use super::GOOD;
use crate::content_verification::{digest, LessonVerificationReport, Rewritten};
use crate::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::lessons::claim_reviews::service::{ClaimReviewService, SectionEdit, SectionEditOutcome};
use crate::lessons::claim_reviews::teaching_findings::TeachingFindingsRepository;
use crate::review_findings::{FindingKind, FindingSite, ReviewFinding, SetAside};
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimId, ClaimLedgerEntry, ClaimState, Ledger,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::HashSet;

/// The sentence the user writes in place of a statement.
const NEW: &str = "DictReader keeps field-name whitespace exactly as the header row writes it.";
const CHANGED_SINCE: &str =
    "This section changed since you opened it. Copy your changes, close the editor and open it again.";

/// Two published sections whose statements the judge never read.
async fn unchecked() -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&unreadable(), &lesson(&[GOOD, &figure(7)]))
        .await
        .expect("the lesson publishes, its statements not verified");
    course.publish(prepared).await?;
    Ok(course)
}

/// A lesson whose one statement the sources state, published verified.
async fn verified() -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&Figures::new(), &lesson(&[GOOD]))
        .await
        .expect("the lesson publishes");
    course.publish(prepared).await?;
    Ok(course)
}

async fn ledger(course: &Course) -> Result<Ledger> {
    ClaimReviewRepository::new(course.pool.clone())
        .load(&course.lesson_id)
        .await
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

async fn text_of(course: &Course, ordinal: i64) -> Result<(String, String)> {
    Ok(sqlx::query_as::<_, (String, String)>(
        "SELECT title, body FROM learning_blocks WHERE lesson_id=? AND ordinal=?",
    )
    .bind(&course.lesson_id)
    .bind(ordinal)
    .fetch_one(&course.pool)
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

/// Everything a rewrite may write.
#[derive(Debug, PartialEq)]
struct Snapshot {
    blocks: Vec<(String, String)>,
    ledger: String,
    revision: i64,
    report: Option<(String, String)>,
    content_revision: i64,
}

async fn snapshot(course: &Course) -> Result<Snapshot> {
    let blocks = sqlx::query_as::<_, (String, String)>(
        "SELECT title, body FROM learning_blocks WHERE lesson_id=? ORDER BY ordinal",
    )
    .bind(&course.lesson_id)
    .fetch_all(&course.pool)
    .await?;
    let content_revision =
        sqlx::query_scalar::<_, i64>("SELECT content_revision FROM learning_programs WHERE id=?")
            .bind(&course.program_id)
            .fetch_one(&course.pool)
            .await?;
    let ledger = ledger(course).await?;
    Ok(Snapshot {
        blocks,
        ledger: format!("{:?}", ledger.claims),
        revision: ledger.revision.0,
        report: stored_report(course).await.ok(),
        content_revision,
    })
}

fn request(
    course: &Course,
    section: usize,
    before: (String, String),
    title: &str,
    body: &str,
    dry_run: bool,
) -> SectionEdit {
    SectionEdit {
        program_id: course.program_id.clone(),
        lesson_id: course.lesson_id.clone(),
        section_index: section,
        before_title: before.0,
        before_body: before.1,
        title: title.into(),
        body: body.into(),
        dry_run,
    }
}

async fn rewrite(
    course: &Course,
    section: usize,
    title: &str,
    body: &str,
    dry_run: bool,
) -> Result<SectionEditOutcome> {
    let before = text_of(course, i64::try_from(section).unwrap()).await?;
    ClaimReviewService::new(course.pool.clone())
        .edit_section(request(course, section, before, title, body, dry_run))
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

/// Section 0's text with the statement written over by `NEW`.
fn written_over() -> String {
    section(NEW)
}

fn without_statement() -> String {
    section(GOOD).replacen(&format!("{GOOD} "), "", 1)
}

#[tokio::test]
async fn a_save_writes_the_sections_title_and_text() -> Result<()> {
    let course = unchecked().await?;
    let seen = snapshot(&course).await?;

    let outcome = rewrite(&course, 0, "A better title", &written_over(), false).await?;

    assert!(outcome.changed);
    let after = snapshot(&course).await?;
    assert_eq!(
        after.blocks[0],
        ("A better title".to_owned(), written_over())
    );
    assert_eq!(after.blocks[1], seen.blocks[1]);
    Ok(())
}

#[tokio::test]
async fn claims_on_changed_sentences_end_edited_by_the_user_with_the_written_sentence_as_successor(
) -> Result<()> {
    let course = unchecked().await?;
    let old = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();

    let outcome = rewrite(&course, 0, "Section 0", &written_over(), false).await?;

    assert_eq!(outcome.statements_changed, 1);
    assert_eq!(outcome.sentences_written, 1);
    let ledger = ledger(&course).await?;
    let successor = entry_for(&ledger, NEW).claim_id().clone();
    let ended = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == &old)
        .unwrap();
    match ended.state() {
        ClaimState::Edited {
            successor: Some(id),
            by: Actor::User,
            ..
        } => assert_eq!(id, &successor),
        other => panic!("{other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn claims_on_deleted_sentences_end_removed_by_the_user() -> Result<()> {
    let course = unchecked().await?;
    let old = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();

    let outcome = rewrite(&course, 0, "Section 0", &without_statement(), false).await?;

    assert_eq!(outcome.statements_removed, 1);
    assert_eq!(outcome.sentences_written, 0);
    let ledger = ledger(&course).await?;
    let ended = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == &old)
        .unwrap();
    assert!(
        matches!(
            ended.state(),
            ClaimState::Removed {
                by: Actor::User,
                ..
            }
        ),
        "{:?}",
        ended.state()
    );
    Ok(())
}

#[tokio::test]
async fn the_rest_keep_their_state() -> Result<()> {
    let course = unchecked().await?;
    let seen = ledger(&course).await?;
    let other = format!("{:?}", entry_for(&seen, &figure(7)));
    let kept = format!("{:?}", entry_for(&seen, GOOD));
    let text = section(GOOD);
    let last = text.rfind("Try it yourself").unwrap();
    let changed_practice = format!("{}Try it yourself with a different file.", &text[..last]);

    let outcome = rewrite(&course, 0, "Section 0", &changed_practice, false).await?;

    assert_eq!(outcome.statements_kept, 1);
    assert_eq!(outcome.statements_changed, 0);
    assert_eq!(outcome.statements_removed, 0);
    let after = ledger(&course).await?;
    assert_eq!(format!("{:?}", entry_for(&after, &figure(7))), other);
    assert_eq!(format!("{:?}", entry_for(&after, GOOD)), kept);
    Ok(())
}

#[tokio::test]
async fn each_written_sentence_is_a_claim_confirmed_by_the_user() -> Result<()> {
    let course = unchecked().await?;
    let extra = "Headers are never trimmed for you.";
    let body = format!("{} {NEW} {extra}", section(GOOD));

    let outcome = rewrite(&course, 0, "Section 0", &body, false).await?;

    assert_eq!(outcome.sentences_written, 2);
    let ledger = ledger(&course).await?;
    for sentence in [NEW, extra] {
        let entry = entry_for(&ledger, sentence);
        assert!(
            matches!(entry.state(), ClaimState::Confirmed { .. }),
            "{sentence}: {:?}",
            entry.state()
        );
        assert_eq!(entry.location().field, "body");
        assert_eq!(entry.location().unit_index, 0);
        let (start, end) = entry.location().span.unwrap();
        assert_eq!(body.get(start..end), Some(sentence));
        assert!(entry
            .events()
            .iter()
            .any(|record| record.provenance.actor == Actor::User));
    }
    Ok(())
}

fn findings(report: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(report).unwrap()["findings"]
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn the_report_drops_the_ended_claims_findings_keeps_the_rest_and_adds_one_confirmed_finding_per_written_sentence(
) -> Result<()> {
    let course = unchecked().await?;
    let before = stored_report(&course).await?;
    let seen = findings(&before.1);
    let old = entry_for(&ledger(&course).await?, GOOD)
        .claim_id()
        .0
        .clone();

    rewrite(&course, 0, "Section 0", &written_over(), false).await?;

    let after = stored_report(&course).await?;
    let now = findings(&after.1);
    assert_ne!(after.0, before.0);
    assert!(now.iter().all(|f| f["claim_id"] != json!(old)));
    assert!(now.iter().all(|f| f["quote"] != json!(GOOD)));
    let successor = entry_for(&ledger(&course).await?, NEW).claim_id().0.clone();
    let written: Vec<&Value> = now
        .iter()
        .filter(|f| f["statement"] == json!(NEW))
        .collect();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["claim_id"], json!(successor));
    assert_eq!(written[0]["resolution"]["kind"], json!("confirmed"));
    assert_eq!(written[0]["unit"], json!(0));
    assert_eq!(now.len(), seen.len());
    let position = |list: &[Value], statement: &str| {
        list.iter()
            .position(|f| f["statement"] == json!(statement))
            .unwrap()
    };
    assert_eq!(position(&now, NEW), position(&seen, GOOD));
    assert_eq!(
        now.iter()
            .filter(|f| f["unit"] != json!(0))
            .collect::<Vec<_>>(),
        seen.iter()
            .filter(|f| f["unit"] != json!(0))
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[tokio::test]
async fn the_rewritten_report_drops_findings_with_no_claim_whose_text_is_gone() -> Result<()> {
    let course = unchecked().await?;
    let mut report: LessonVerificationReport =
        serde_json::from_str(&stored_report(&course).await?.1)?;
    let template = report
        .findings
        .iter()
        .find(|finding| finding.unit == 0)
        .unwrap()
        .clone();
    let at = report
        .findings
        .iter()
        .position(|finding| finding.unit == 0)
        .unwrap();
    let mut gone = template.clone();
    gone.claim_id = None;
    gone.quote = "A sentence that is gone.".into();
    let mut stays = template.clone();
    stays.claim_id = None;
    stays.quote = "A sentence that stays.".into();
    report.findings.push(gone);
    report.findings.push(stays);
    let ended = HashSet::from([template.claim_id.clone().unwrap()]);
    let mine = ClaimId("mine".into());
    let before = report.content_sha256.clone();

    report.rewritten(&Rewritten {
        unit: 0,
        new_title: "A title",
        new_body: "A sentence that stays. Mine is new.",
        ended: &ended,
        written: &[("Mine is new.", &mine)],
        at: 7,
    });

    let quotes: Vec<&str> = report
        .findings
        .iter()
        .filter(|finding| finding.unit == 0)
        .map(|finding| finding.statement.as_str())
        .collect();
    assert!(!quotes.contains(&template.statement.as_str()), "{quotes:?}");
    assert!(report
        .findings
        .iter()
        .all(|finding| finding.quote != "A sentence that is gone."));
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.quote == "A sentence that stays."));
    assert_eq!(report.findings[at].statement, "Mine is new.");
    assert_eq!(report.findings[at].claim_id.as_deref(), Some("mine"));
    assert_eq!(
        report.content_sha256,
        digest(&json!([before, 0, "A title", "A sentence that stays. Mine is new."]).to_string())
    );
    Ok(())
}

#[tokio::test]
async fn the_revision_moves_once() -> Result<()> {
    let course = unchecked().await?;
    let seen = ledger(&course).await?.revision.0;

    let outcome = rewrite(&course, 0, "Section 0", &written_over(), false).await?;

    assert_eq!(outcome.revision, Some(seen + 1));
    assert_eq!(ledger(&course).await?.revision.0, seen + 1);
    Ok(())
}

#[tokio::test]
async fn a_section_changed_since_it_was_opened_is_refused_and_nothing_is_written() -> Result<()> {
    let course = unchecked().await?;
    let opened = text_of(&course, 0).await?;
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(format!("{} One more sentence.", opened.1))
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;
    let seen = snapshot(&course).await?;

    let error = ClaimReviewService::new(course.pool.clone())
        .edit_section(request(
            &course,
            0,
            opened,
            "Section 0",
            &written_over(),
            false,
        ))
        .await
        .unwrap_err();

    assert_eq!(refusal(&error), CHANGED_SINCE);
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn a_blank_title_or_blank_text_is_refused_and_nothing_is_written() -> Result<()> {
    let course = unchecked().await?;
    let seen = snapshot(&course).await?;

    let title = rewrite(&course, 0, "   ", &written_over(), false)
        .await
        .unwrap_err();
    let text = rewrite(&course, 0, "Section 0", " \n ", false)
        .await
        .unwrap_err();

    assert_eq!(refusal(&title), "A section needs a title.");
    assert_eq!(refusal(&text), "A section needs some text.");
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn an_edit_that_changes_nothing_writes_nothing() -> Result<()> {
    let course = unchecked().await?;
    let seen = snapshot(&course).await?;
    let (title, body) = text_of(&course, 0).await?;

    let outcome = rewrite(&course, 0, &title, &body, false).await?;

    assert!(!outcome.changed);
    assert_eq!(outcome.revision, None);
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn a_dry_run_writes_nothing_and_gives_the_counts_the_save_then_gives() -> Result<()> {
    let course = unchecked().await?;
    let seen = snapshot(&course).await?;
    let body = format!("{} Headers are never trimmed for you.", written_over());

    let dry = rewrite(&course, 0, "Section 0", &body, true).await?;

    assert_eq!(snapshot(&course).await?, seen);
    assert!(dry.changed);
    assert_eq!(dry.revision, None);
    let saved = rewrite(&course, 0, "Section 0", &body, false).await?;
    assert_eq!(
        SectionEditOutcome {
            revision: None,
            ..saved
        },
        dry
    );
    Ok(())
}

#[tokio::test]
async fn the_checked_count_counts_only_statements_verified_against_a_source() -> Result<()> {
    let course = verified().await?;
    let seen = ledger(&course).await?;
    assert!(
        matches!(
            entry_for(&seen, GOOD).state(),
            ClaimState::ResolvedByEvidence { .. }
        ),
        "{:?}",
        entry_for(&seen, GOOD).state()
    );

    let changed = rewrite(&course, 0, "Section 0", &written_over(), true).await?;
    let practice_only = {
        let text = section(GOOD);
        let last = text.rfind("Try it yourself").unwrap();
        format!("{}Try it yourself with a different file.", &text[..last])
    };
    let untouched = rewrite(&course, 0, "Section 0", &practice_only, true).await?;
    let removed = rewrite(&course, 0, "Section 0", &without_statement(), true).await?;

    assert_eq!(changed.checked_statements_affected, 1);
    assert_eq!(removed.checked_statements_affected, 1);
    assert_eq!(untouched.checked_statements_affected, 0);

    let not_checked = unchecked().await?;
    let outcome = rewrite(&not_checked, 0, "Section 0", &written_over(), true).await?;
    assert_eq!(outcome.statements_changed, 1);
    assert_eq!(outcome.checked_statements_affected, 0);
    Ok(())
}

#[tokio::test]
async fn a_lesson_with_no_review_gets_its_text_written() -> Result<()> {
    let course = unchecked().await?;
    for statement in [
        "DELETE FROM learning_lesson_claim_events WHERE claim_id IN (SELECT claim_id FROM learning_lesson_claims WHERE lesson_id=?)",
        "DELETE FROM learning_lesson_claims WHERE lesson_id=?",
        "DELETE FROM learning_lesson_reviews WHERE lesson_id=?",
    ] {
        sqlx::query(statement)
            .bind(&course.lesson_id)
            .execute(&course.pool)
            .await?;
    }

    let outcome = rewrite(&course, 0, "A new title", &written_over(), false).await?;

    assert!(outcome.changed);
    assert_eq!(outcome.revision, None);
    assert_eq!(
        text_of(&course, 0).await?,
        ("A new title".to_owned(), written_over())
    );
    Ok(())
}

#[tokio::test]
async fn a_lesson_from_another_course_is_refused() -> Result<()> {
    let course = unchecked().await?;
    let seen = snapshot(&course).await?;
    let before = text_of(&course, 0).await?;
    let mut edit = request(&course, 0, before, "Section 0", &written_over(), false);
    edit.program_id = "another-course".into();

    let error = ClaimReviewService::new(course.pool.clone())
        .edit_section(edit)
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::NotFound(_)), "{error:?}");
    assert_eq!(refusal(&error), "This lesson is not in this course.");
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn a_lesson_still_being_prepared_is_refused() -> Result<()> {
    let course = unchecked().await?;
    sqlx::query("UPDATE learning_lesson_reviews SET state='needs_review' WHERE lesson_id=?")
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;
    let seen = snapshot(&course).await?;

    let error = rewrite(&course, 0, "Section 0", &written_over(), false)
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::InvalidState(_)), "{error:?}");
    assert_eq!(
        refusal(&error),
        "This lesson is still being prepared. You can edit it once it is ready."
    );
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

async fn three_notes(course: &Course) -> Result<TeachingFindingsRepository> {
    let store = TeachingFindingsRepository::new(course.pool.clone());
    let candidate = json!({"blocks":[
        {"title":"Section 0","body":section(GOOD)},
        {"title":"Section 1","body":section(&figure(7))}
    ],"questions":[]});
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
                note(FindingKind::Teaching, FindingSite::Section(0), "First."),
                note(FindingKind::Teaching, FindingSite::Section(1), "Second."),
                note(FindingKind::Structure, FindingSite::Lesson, "Whole."),
            ],
            true,
        )
        .await?;
    Ok(store)
}

async fn changed_after(store: &TeachingFindingsRepository, lesson: &str) -> Result<Vec<bool>> {
    Ok(store
        .list(lesson)
        .await?
        .into_iter()
        .map(|finding| finding.part_changed)
        .collect())
}

#[tokio::test]
async fn notes_on_the_edited_section_say_it_changed_after_them_and_other_notes_do_not() -> Result<()>
{
    let course = unchecked().await?;
    let store = three_notes(&course).await?;
    assert_eq!(changed_after(&store, &course.lesson_id).await?, [false; 3]);

    rewrite(&course, 0, "Section 0", &written_over(), false).await?;

    // The list reads the whole lesson first, then each section in order.
    let notes = store.list(&course.lesson_id).await?;
    let answers: Vec<(Option<u32>, bool)> = notes
        .iter()
        .map(|note| (note.unit_index, note.part_changed))
        .collect();
    assert_eq!(answers, [(None, false), (Some(0), true), (Some(1), false)]);
    Ok(())
}

#[tokio::test]
async fn a_note_written_after_the_edit_does_not_say_the_section_changed() -> Result<()> {
    let course = unchecked().await?;
    three_notes(&course).await?;
    rewrite(&course, 0, "Section 0", &written_over(), false).await?;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let store = three_notes(&course).await?;

    assert_eq!(changed_after(&store, &course.lesson_id).await?, [false; 3]);
    Ok(())
}

async fn report_of(course: &Course) -> Result<LessonVerificationReport> {
    Ok(serde_json::from_str(&stored_report(course).await?.1)?)
}

fn rewrite_of<'a>(
    unit: usize,
    title: &'a str,
    body: &'a str,
    ended: &'a HashSet<String>,
    written: &'a [(&'a str, &'a ClaimId)],
) -> Rewritten<'a> {
    Rewritten {
        unit,
        new_title: title,
        new_body: body,
        ended,
        written,
        at: 7,
    }
}

fn push_without_claim(report: &mut LessonVerificationReport, quote: &str) {
    let mut finding = report
        .findings
        .iter()
        .find(|finding| finding.unit == 0)
        .unwrap()
        .clone();
    finding.claim_id = None;
    finding.quote = quote.into();
    finding.statement = quote.into();
    report.findings.push(finding);
}

#[tokio::test]
async fn the_rewritten_report_keeps_a_finding_with_no_claim_whose_quote_the_body_holds_twice(
) -> Result<()> {
    let course = unchecked().await?;
    let mut report = report_of(&course).await?;
    push_without_claim(&mut report, "Twice here.");
    let mine = ClaimId("mine".into());
    let ended = HashSet::new();

    report.rewritten(&rewrite_of(
        0,
        "A title",
        "Twice here. Twice here. Mine is new.",
        &ended,
        &[("Mine is new.", &mine)],
    ));

    assert!(report
        .findings
        .iter()
        .any(|finding| finding.quote == "Twice here."));
    Ok(())
}

#[tokio::test]
async fn the_rewritten_report_keeps_a_finding_with_no_claim_whose_quote_is_the_new_title(
) -> Result<()> {
    let course = unchecked().await?;
    let mut report = report_of(&course).await?;
    push_without_claim(&mut report, "A new title");
    let ended = HashSet::new();

    report.rewritten(&rewrite_of(
        0,
        "A new title",
        "Some other text.",
        &ended,
        &[],
    ));

    assert!(report
        .findings
        .iter()
        .any(|finding| finding.quote == "A new title"));
    Ok(())
}

#[tokio::test]
async fn with_nothing_dropped_the_written_findings_follow_the_units_last_finding() -> Result<()> {
    let course = unchecked().await?;
    let mut report = report_of(&course).await?;
    let last = report
        .findings
        .iter()
        .rposition(|finding| finding.unit == 0)
        .unwrap();
    assert!(report.findings.iter().any(|finding| finding.unit == 1));
    let before = report.findings.len();
    let mine = ClaimId("mine".into());
    let ended = HashSet::new();

    report.rewritten(&rewrite_of(
        0,
        "Section 0",
        &section(GOOD),
        &ended,
        &[("Mine is new.", &mine)],
    ));

    assert_eq!(report.findings.len(), before + 1);
    assert_eq!(report.findings[last + 1].claim_id.as_deref(), Some("mine"));
    assert_eq!(report.findings[last + 2].unit, 1);
    Ok(())
}

#[tokio::test]
async fn with_nothing_dropped_and_no_finding_of_the_unit_the_written_ones_go_last() -> Result<()> {
    let course = unchecked().await?;
    let mut report = report_of(&course).await?;
    let before = report.findings.len();
    let mine = ClaimId("mine".into());
    let ended = HashSet::new();

    report.rewritten(&rewrite_of(
        9,
        "Section 9",
        "Mine is new.",
        &ended,
        &[("Mine is new.", &mine)],
    ));

    assert_eq!(report.findings.len(), before + 1);
    let last = report.findings.last().unwrap();
    assert_eq!(last.claim_id.as_deref(), Some("mine"));
    assert_eq!(last.unit, 9);
    Ok(())
}

#[tokio::test]
async fn a_new_title_with_no_statement_on_the_old_one_writes_no_statement() -> Result<()> {
    let course = unchecked().await?;
    let (_, body) = text_of(&course, 0).await?;

    let outcome = rewrite(&course, 0, "A better title", &body, false).await?;

    assert!(outcome.changed);
    assert_eq!(outcome.sentences_written, 0);
    let ledger = ledger(&course).await?;
    assert!(ledger
        .claims
        .iter()
        .all(|entry| entry.statement() != "A better title"));
    let report = stored_report(&course).await?.1;
    assert!(findings(&report)
        .iter()
        .all(|finding| finding["statement"] != json!("A better title")));
    Ok(())
}

#[tokio::test]
async fn a_title_whose_statement_is_edited_is_written_as_a_statement_confirmed_by_the_user(
) -> Result<()> {
    use lattice_core::domain::learning::claim_review::{
        ClaimEvent, ClaimLocation, EventProvenance, UnitKind, UnverifiedReason,
    };
    let course = unchecked().await?;
    let store = ClaimReviewRepository::new(course.pool.clone());
    let id = ClaimId("on-the-title".into());
    let machine = EventProvenance {
        actor: Actor::Machine,
        at: 1,
        job_id: None,
        app_version: "test".into(),
        checker_model: None,
    };
    let mut revision = ledger(&course).await?.revision;
    for event in [
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(0, UnitKind::Block, "title", "Section 0", "Section 0"),
            statement: "Section 0".into(),
        },
        ClaimEvent::PublishedUnverified {
            reason: UnverifiedReason::Unchecked,
        },
    ] {
        revision = store
            .append(&course.lesson_id, &id, machine.clone(), event, revision)
            .await?;
    }
    let (_, body) = text_of(&course, 0).await?;

    let outcome = rewrite(&course, 0, "A better title", &body, false).await?;

    assert_eq!(outcome.statements_changed, 1);
    assert_eq!(outcome.sentences_written, 1);
    let ledger = ledger(&course).await?;
    let written = entry_for(&ledger, "A better title");
    assert!(matches!(written.state(), ClaimState::Confirmed { .. }));
    assert_eq!(written.location().field, "title");
    let old = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == &id)
        .unwrap();
    assert!(matches!(
        old.state(),
        ClaimState::Edited {
            by: Actor::User,
            ..
        }
    ));
    Ok(())
}

const EDITED_THE_SECTION: &str =
    "This changed when you edited the section. Edit the section again to change it back.";

async fn offered_for(course: &Course, claim: &ClaimId) -> Result<Vec<String>> {
    let review = ClaimReviewService::new(course.pool.clone())
        .review(&course.program_id, &course.lesson_id)
        .await?
        .expect("the published lesson has its review");
    let review = serde_json::to_value(&review)?;
    let item = review["claims"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["claimId"] == json!(claim.0))
        .unwrap_or_else(|| panic!("{}: {review}", claim.0));
    Ok(item["availableActions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action.as_str().unwrap().to_owned())
        .collect())
}

async fn undo(course: &Course, claim: &ClaimId) -> Result<i64> {
    let revision = ledger(course).await?.revision.0;
    ClaimReviewService::new(course.pool.clone())
        .resolve(
            &course.lesson_id,
            &claim.0,
            ClaimActionRequestDto::Undo,
            revision,
        )
        .await
}

/// A claim a section edit ended offers no Undo, and an Undo on it writes nothing.
async fn assert_no_undo_after(body: &str, statement: &str) -> Result<()> {
    let course = unchecked().await?;
    let old = entry_for(&ledger(&course).await?, statement)
        .claim_id()
        .clone();
    rewrite(&course, 0, "Section 0", body, false).await?;
    let seen = snapshot(&course).await?;

    let offered = offered_for(&course, &old).await?;
    assert!(
        !offered.iter().any(|action| action == "undo"),
        "{offered:?}"
    );
    match undo(&course, &old).await {
        Err(AppError::InvalidState(message)) => assert_eq!(message, EDITED_THE_SECTION),
        other => panic!("{other:?}"),
    }
    assert_eq!(snapshot(&course).await?, seen);
    Ok(())
}

#[tokio::test]
async fn a_statement_a_section_edit_changed_offers_no_undo_and_an_undo_is_refused() -> Result<()> {
    assert_no_undo_after(&written_over(), GOOD).await
}

#[tokio::test]
async fn a_statement_a_section_edit_deleted_offers_no_undo_and_an_undo_is_refused() -> Result<()> {
    assert_no_undo_after(&without_statement(), GOOD).await
}
