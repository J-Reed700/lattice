#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! In a published lesson the user puts back a sentence the machine took out
//! (where it stood, in their own words, or at the end of a section they
//! choose), restores the original of a rewrite, and undoes each; the text,
//! the ledger and the stored report change together.
use super::claim_ledger_tests::{figure, lesson, unreadable, Course};
use super::GOOD;
use crate::lessons::claim_reviews::dto::{
    ClaimActionKindDto, ClaimActionRequestDto, PutBackPlaceDto,
};
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::lessons::claim_reviews::service::ClaimReviewService;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, Check, CheckResult, Checker, ClaimActionKind, ClaimEvent, ClaimId, ClaimLedgerEntry,
    ClaimLocation, ClaimState, CutPlace, EventProvenance, EvidenceFingerprint, Ledger,
    RepairOutcome, UnitKind, VerdictKind,
};
use lattice_core::shared::error::{AppError, Result};

use crate::lessons::content_verification::{ClaimVerdict, Finding, LessonVerificationReport};

const CUT: &str = "Rows are read lazily from the file.";
const WRITTEN: &str = "Rows come from the file one at a time.";
const REWRITE: &str = "Rows are read from the file.";
const CHOOSE: &str = "Choose the section to put it back in.";
const CHANGED_AFTER: &str = "This section changed after that, so it can't be undone on its own. Edit the section to change it back.";

async fn published() -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&unreadable(), &lesson(&[GOOD, &figure(7)]))
        .await
        .expect("the lesson publishes, its statements not verified");
    course.publish(prepared).await?;
    Ok(course)
}

fn provenance(actor: Actor) -> EventProvenance {
    EventProvenance {
        actor,
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

async fn append(course: &Course, id: &ClaimId, event: ClaimEvent) -> Result<()> {
    let revision = ledger(course).await?.revision;
    ClaimReviewRepository::new(course.pool.clone())
        .append(
            &course.lesson_id,
            id,
            provenance(Actor::Machine),
            event,
            revision,
        )
        .await?;
    Ok(())
}

async fn bodies(course: &Course) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT body FROM learning_blocks WHERE lesson_id=? ORDER BY ordinal",
    )
    .bind(&course.lesson_id)
    .fetch_all(&course.pool)
    .await?)
}

async fn report(course: &Course) -> Result<String> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .fetch_one(&course.pool)
    .await?)
}

fn failed() -> ClaimEvent {
    ClaimEvent::Checked(Check {
        checker: Checker::Local {
            model: "fixture".into(),
        },
        result: CheckResult::Failed {
            kind: VerdictKind::Unsupported,
        },
        reason: "No saved source states it.".into(),
        fingerprint: EvidenceFingerprint::of("strict", "fixture", &[]),
        evidence: Vec::new(),
        window_tokens: 128_000,
    })
}

fn repair(before: &str, after: Option<&str>, outcome: RepairOutcome) -> ClaimEvent {
    ClaimEvent::RepairAttempted {
        attempt: 1,
        escalated: false,
        before: before.into(),
        after: after.map(str::to_owned),
        outcome,
        goal: None,
    }
}

/// A claim the machine took out of a section: its sentence is not in the
/// text, and the cut recorded `cut` when it is given.
async fn taken_out(
    course: &Course,
    id: &str,
    unit: usize,
    cut: Option<(Option<&str>, Option<&str>)>,
) -> Result<ClaimId> {
    taken_out_as(course, id, CUT, CUT, unit, cut).await
}

/// A claim for `statement` that stood on `sentence`, which the machine took
/// out of a section.
async fn taken_out_as(
    course: &Course,
    id: &str,
    statement: &str,
    sentence: &str,
    unit: usize,
    cut: Option<(Option<&str>, Option<&str>)>,
) -> Result<ClaimId> {
    let id = ClaimId(id.into());
    let body = bodies(course).await?.get(unit).cloned().unwrap_or_default();
    let with = format!("{body} {sentence}");
    append(
        course,
        &id,
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(unit, UnitKind::Block, "body", sentence, &with),
            statement: statement.into(),
        },
    )
    .await?;
    append(course, &id, failed()).await?;
    append(course, &id, repair(statement, None, RepairOutcome::Removed)).await?;
    if let Some((before, after)) = cut {
        let cut = CutPlace {
            sentence: sentence.into(),
            before: before.map(str::to_owned),
            after: after.map(str::to_owned),
        };
        append(course, &id, ClaimEvent::SentenceCut { cut }).await?;
    }
    Ok(id)
}

async fn act(course: &Course, claim: &ClaimId, action: ClaimActionRequestDto) -> Result<i64> {
    let revision = ledger(course).await?.revision.0;
    ClaimReviewService::new(course.pool.clone())
        .resolve(&course.lesson_id, &claim.0, action, revision)
        .await
}

fn put_back(text: Option<&str>, section: Option<u32>) -> ClaimActionRequestDto {
    ClaimActionRequestDto::PutBack {
        text: text.map(str::to_owned),
        section,
    }
}

async fn claim(course: &Course, id: &ClaimId) -> Result<ClaimLedgerEntry> {
    Ok(ledger(course)
        .await?
        .claims
        .into_iter()
        .find(|entry| entry.claim_id() == id)
        .unwrap())
}

/// What a refusal left written: the text, the ledger's revision, the report.
async fn written(course: &Course) -> Result<(Vec<String>, i64, String)> {
    Ok((
        bodies(course).await?,
        ledger(course).await?.revision.0,
        report(course).await?,
    ))
}

#[tokio::test]
async fn a_sentence_the_machine_cut_from_a_published_section_goes_back_after_the_sentence_it_followed(
) -> Result<()> {
    let course = published().await?;
    let id = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let before = written(&course).await?;

    act(&course, &id, put_back(None, None)).await?;

    let after = written(&course).await?;
    assert!(
        after.0[0].starts_with(&format!("{GOOD} {CUT}")),
        "{:?}",
        after.0
    );
    assert_eq!(after.0[1], before.0[1]);
    assert_eq!(after.1, before.1 + 1);
    assert_ne!(after.2, before.2, "the report follows the text");
    assert!(matches!(
        claim(&course, &id).await?.state(),
        ClaimState::Confirmed { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn a_sentence_put_back_in_the_users_words_goes_in_as_they_wrote_it() -> Result<()> {
    let course = published().await?;
    let id = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;

    act(&course, &id, put_back(Some(&format!("  {WRITTEN} ")), None)).await?;

    let body = bodies(&course).await?[0].clone();
    assert!(body.contains(&format!("{GOOD} {WRITTEN}")), "{body}");
    assert!(!body.contains(CUT));
    Ok(())
}

#[tokio::test]
async fn a_put_back_with_blank_text_is_refused_and_writes_nothing() -> Result<()> {
    let course = published().await?;
    let id = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let before = written(&course).await?;

    let error = act(&course, &id, put_back(Some("   "), None))
        .await
        .unwrap_err();

    assert!(
        matches!(&error, AppError::InvalidInput(message) if message == "Write the sentence you want to put back."),
        "{error:?}"
    );
    assert_eq!(written(&course).await?, before);
    Ok(())
}

#[tokio::test]
async fn a_sentence_whose_place_is_unknown_is_refused_until_the_user_chooses_a_section(
) -> Result<()> {
    let course = published().await?;
    let id = taken_out(&course, "old", 9, None).await?;
    let before = written(&course).await?;

    let error = act(&course, &id, put_back(None, None)).await.unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(message) if message == CHOOSE),
        "{error:?}"
    );
    assert_eq!(written(&course).await?, before);

    act(&course, &id, put_back(None, Some(1))).await?;

    let after = bodies(&course).await?;
    assert!(after[1].ends_with(CUT), "{after:?}");
    assert_eq!(after[0], before.0[0]);
    Ok(())
}

#[tokio::test]
async fn the_user_may_choose_a_section_when_the_place_is_known() -> Result<()> {
    let course = published().await?;
    let id = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;

    act(&course, &id, put_back(None, Some(1))).await?;

    let after = bodies(&course).await?;
    assert!(after[1].ends_with(CUT));
    assert!(!after[0].contains(CUT));
    Ok(())
}

#[tokio::test]
async fn a_section_that_is_not_in_the_lesson_is_refused() -> Result<()> {
    let course = published().await?;
    let id = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let before = written(&course).await?;

    let error = act(&course, &id, put_back(None, Some(7)))
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::InvalidInput(_)), "{error:?}");
    assert_eq!(written(&course).await?, before);
    Ok(())
}

#[tokio::test]
async fn only_a_sentence_the_machine_took_out_can_be_put_back() -> Result<()> {
    let course = published().await?;
    let open = ledger(&course).await?.claims[0].claim_id().clone();
    let before = written(&course).await?;

    let error = act(&course, &open, put_back(None, Some(0)))
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::InvalidState(_)), "{error:?}");
    assert_eq!(written(&course).await?, before);
    Ok(())
}

#[tokio::test]
async fn undo_takes_a_put_back_out_of_the_published_section_and_leaves_the_sentence_taken_out(
) -> Result<()> {
    let course = published().await?;
    let id = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let before = bodies(&course).await?;
    act(&course, &id, put_back(None, None)).await?;

    act(&course, &id, ClaimActionRequestDto::Undo).await?;

    assert_eq!(bodies(&course).await?, before);
    let entry = claim(&course, &id).await?;
    assert!(entry.taken_out());
    assert!(entry.allows(ClaimActionKind::PutBack));
    Ok(())
}

#[tokio::test]
async fn the_undo_of_a_put_back_is_refused_with_what_to_do_when_the_section_changed_after_it(
) -> Result<()> {
    let course = published().await?;
    let id = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    act(&course, &id, put_back(None, None)).await?;
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind("The section was rewritten by hand.")
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;
    let before = written(&course).await?;

    let error = act(&course, &id, ClaimActionRequestDto::Undo)
        .await
        .unwrap_err();

    assert!(error.to_string().contains(CHANGED_AFTER), "{error}");
    assert_eq!(written(&course).await?, before);
    Ok(())
}

/// A claim the machine rewrote in the first section: the lesson now says
/// `REWRITE` where it said `CUT`.
async fn rewritten(course: &Course) -> Result<ClaimId> {
    let id = ClaimId("rewritten".into());
    let body = bodies(course).await?[0].clone();
    let now = format!("{body} {REWRITE}");
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(&now)
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;
    append(
        course,
        &id,
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(
                0,
                UnitKind::Block,
                "body",
                CUT,
                &format!("{body} {CUT}"),
            ),
            statement: CUT.into(),
        },
    )
    .await?;
    append(course, &id, failed()).await?;
    append(
        course,
        &id,
        repair(CUT, Some(REWRITE), RepairOutcome::Rewritten),
    )
    .await?;
    append(
        course,
        &id,
        ClaimEvent::Superseded {
            successor: Some(ClaimId("rewrite".into())),
            by: Actor::Machine,
        },
    )
    .await?;
    Ok(id)
}

#[tokio::test]
async fn restoring_the_original_of_a_rewrite_puts_it_in_place_of_the_rewrite_and_undo_brings_the_rewrite_back(
) -> Result<()> {
    let course = published().await?;
    let id = rewritten(&course).await?;
    let before = bodies(&course).await?;
    assert!(claim(&course, &id)
        .await?
        .allows(ClaimActionKind::RestoreOriginal));

    act(&course, &id, ClaimActionRequestDto::RestoreOriginal).await?;

    let after = bodies(&course).await?;
    assert_eq!(after[0], before[0].replace(REWRITE, CUT));
    assert!(matches!(
        claim(&course, &id).await?.state(),
        ClaimState::Confirmed { .. }
    ));

    act(&course, &id, ClaimActionRequestDto::Undo).await?;

    assert_eq!(bodies(&course).await?, before);
    Ok(())
}

#[tokio::test]
async fn the_review_says_where_a_taken_out_sentence_goes_and_lists_the_sections() -> Result<()> {
    let course = published().await?;
    let known = taken_out(&course, "known", 0, Some((Some(GOOD), None))).await?;
    let unknown = taken_out(&course, "unknown", 9, None).await?;
    let review = ClaimReviewService::new(course.pool.clone())
        .review(&course.program_id, &course.lesson_id)
        .await?
        .unwrap();

    let item = |id: &ClaimId| {
        review
            .claims
            .iter()
            .find(|item| item.claim_id == id.0)
            .unwrap()
    };
    assert!(matches!(item(&known).place, Some(PutBackPlaceDto::Known)));
    assert!(matches!(
        item(&unknown).place,
        Some(PutBackPlaceDto::Choose)
    ));
    assert!(item(&known)
        .available_actions
        .iter()
        .any(|action| matches!(action, ClaimActionKindDto::PutBack)));
    let titles: Vec<_> = review.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Section 0", "Section 1"]);
    Ok(())
}

#[tokio::test]
async fn the_review_names_the_unit_a_taken_out_sentence_goes_back_into_as_the_lesson_stands(
) -> Result<()> {
    let course = published().await?;
    let stale = taken_out(&course, "stale", 1, Some((Some(GOOD), None))).await?;
    let unknown = taken_out(&course, "unknown", 9, None).await?;
    let review = ClaimReviewService::new(course.pool.clone())
        .review(&course.program_id, &course.lesson_id)
        .await?
        .unwrap();
    let item = |id: &ClaimId| {
        review
            .claims
            .iter()
            .find(|item| item.claim_id == id.0)
            .unwrap()
    };

    assert_eq!(item(&stale).location.unit_index, 1, "the old place");
    assert_eq!(
        item(&stale).put_back_unit,
        Some(0),
        "the section holding its neighbour"
    );
    assert_eq!(item(&unknown).put_back_unit, None);
    Ok(())
}

#[tokio::test]
async fn a_source_that_supports_a_taken_out_sentence_puts_it_back_where_it_stood() -> Result<()> {
    let course = published().await?;
    let known = taken_out(&course, "known", 0, Some((Some(GOOD), None))).await?;
    let unknown = taken_out(&course, "unknown", 9, None).await?;
    let service = ClaimReviewService::new(course.pool.clone());

    let none = service
        .put_back_for_source(&course.lesson_id, &unknown)
        .await?;
    assert!(none.is_none(), "no place is known: the user chooses it");
    assert!(!bodies(&course).await?[0].contains(CUT));

    let back = service
        .put_back_for_source(&course.lesson_id, &known)
        .await?;
    assert!(back.is_some());
    assert!(bodies(&course).await?[0].starts_with(&format!("{GOOD} {CUT}")));
    Ok(())
}

const OTHER_CUT: &str = "Each row becomes a dictionary.";

async fn act_many(
    course: &Course,
    ids: &[&ClaimId],
    action: ClaimActionRequestDto,
    revision: i64,
) -> Result<i64> {
    let ids: Vec<String> = ids.iter().map(|id| id.0.clone()).collect();
    ClaimReviewService::new(course.pool.clone())
        .resolve_many(&course.lesson_id, &ids, action, revision)
        .await
}

#[tokio::test]
async fn putting_a_sentence_back_brings_back_every_claim_the_machine_took_out_with_it() -> Result<()>
{
    let course = published().await?;
    let first = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let second = taken_out_as(
        &course,
        "also",
        "Rows are not read all at once.",
        CUT,
        0,
        Some((Some(GOOD), None)),
    )
    .await?;
    let before = written(&course).await?;

    act(&course, &first, put_back(None, None)).await?;

    let after = written(&course).await?;
    assert_eq!(after.0[0].matches(CUT).count(), 1, "{:?}", after.0);
    assert_eq!(after.1, before.1 + 1, "one step");
    for id in [&first, &second] {
        let entry = claim(&course, id).await?;
        assert!(!entry.taken_out(), "{id:?} is still taken out");
        assert!(matches!(entry.state(), ClaimState::Confirmed { .. }));
    }
    Ok(())
}

#[tokio::test]
async fn a_claim_taken_out_with_another_sentence_stays_out_when_one_is_put_back() -> Result<()> {
    let course = published().await?;
    let first = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let other = taken_out_as(&course, "other", OTHER_CUT, OTHER_CUT, 0, None).await?;

    act(&course, &first, put_back(None, None)).await?;

    assert!(claim(&course, &other).await?.taken_out());
    Ok(())
}

#[tokio::test]
async fn undoing_a_put_back_takes_back_every_claim_it_brought_back_whichever_it_is_asked_on(
) -> Result<()> {
    for undo_on_second in [false, true] {
        let course = published().await?;
        let first = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
        let second = taken_out_as(
            &course,
            "also",
            "Rows are not read all at once.",
            CUT,
            0,
            Some((Some(GOOD), None)),
        )
        .await?;
        let before = bodies(&course).await?;
        act(&course, &first, put_back(None, None)).await?;
        let revision = ledger(&course).await?.revision.0;

        let asked = if undo_on_second { &second } else { &first };
        act(&course, asked, ClaimActionRequestDto::Undo).await?;

        assert_eq!(bodies(&course).await?, before);
        assert_eq!(ledger(&course).await?.revision.0, revision + 1);
        for id in [&first, &second] {
            assert!(claim(&course, id).await?.taken_out(), "{id:?}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn putting_many_sentences_back_is_one_all_or_nothing_step_and_one_undo_takes_them_all_back(
) -> Result<()> {
    let course = published().await?;
    let first = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let second = taken_out_as(&course, "other", OTHER_CUT, OTHER_CUT, 1, None).await?;
    let before = written(&course).await?;

    let revision = act_many(
        &course,
        &[&first, &second],
        put_back(None, Some(1)),
        before.1,
    )
    .await?;

    assert_eq!(revision, before.1 + 1);
    let after = bodies(&course).await?;
    assert!(
        after[1].contains(CUT) && after[1].contains(OTHER_CUT),
        "{after:?}"
    );
    for id in [&first, &second] {
        assert!(!claim(&course, id).await?.taken_out());
    }

    let undone = act_many(
        &course,
        &[&first, &second],
        ClaimActionRequestDto::Undo,
        revision,
    )
    .await?;

    assert_eq!(undone, revision + 1);
    assert_eq!(bodies(&course).await?, before.0);
    for id in [&first, &second] {
        assert!(claim(&course, id).await?.taken_out());
    }
    Ok(())
}

#[tokio::test]
async fn putting_many_sentences_back_where_they_were_writes_each_where_it_stood() -> Result<()> {
    let course = published().await?;
    let first = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let second = taken_out_as(&course, "other", OTHER_CUT, OTHER_CUT, 1, None).await?;

    let revision = ledger(&course).await?.revision.0;
    act_many(&course, &[&first, &second], put_back(None, None), revision).await?;

    let after = bodies(&course).await?;
    assert!(after[0].starts_with(&format!("{GOOD} {CUT}")), "{after:?}");
    assert!(after[1].contains(OTHER_CUT), "{after:?}");
    Ok(())
}

#[tokio::test]
async fn a_batch_put_back_with_one_sentence_whose_place_is_unknown_writes_nothing() -> Result<()> {
    let course = published().await?;
    let first = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let lost = taken_out_as(&course, "lost", OTHER_CUT, OTHER_CUT, 9, None).await?;
    let before = written(&course).await?;

    let error = act_many(&course, &[&first, &lost], put_back(None, None), before.1)
        .await
        .unwrap_err();

    assert!(error.to_string().contains(CHOOSE), "{error}");
    assert_eq!(written(&course).await?, before);
    Ok(())
}

#[tokio::test]
async fn a_batch_put_back_cannot_carry_text_for_one_sentence() -> Result<()> {
    let course = published().await?;
    let first = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let before = written(&course).await?;

    let error = act_many(&course, &[&first], put_back(Some(WRITTEN), None), before.1)
        .await
        .unwrap_err();

    assert!(
        matches!(&error, AppError::InvalidInput(message) if message == "Choose one statement for that."),
        "{error:?}"
    );
    assert_eq!(written(&course).await?, before);
    Ok(())
}

const ALPHA: &str = "Alpha comes first.";
const BRAVO: &str = "Bravo comes second.";
const CHARLIE: &str = "Charlie comes third.";
const DELTA: &str = "Delta comes fourth.";
const ECHO: &str = "Echo comes last.";

fn original_order() -> String {
    [ALPHA, BRAVO, CHARLIE, DELTA, ECHO].join(" ")
}

/// Bravo, Charlie and Delta cut from the first section one after the other:
/// each cut recorded the neighbours the earlier cuts left it.
async fn adjacent_cuts(course: &Course) -> Result<[ClaimId; 3]> {
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(format!("{ALPHA} {ECHO}"))
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;
    let mut ids = Vec::new();
    for (id, sentence, before, after) in [
        ("bravo", BRAVO, ALPHA, CHARLIE),
        ("charlie", CHARLIE, ALPHA, DELTA),
        ("delta", DELTA, ALPHA, ECHO),
    ] {
        let cut = Some((Some(before), Some(after)));
        ids.push(taken_out_as(course, id, sentence, sentence, 0, cut).await?);
    }
    Ok(ids.try_into().unwrap())
}

/// Whether the sentences of `body` that are in the original are in its order.
fn in_original_order(body: &str) -> bool {
    let at: Vec<usize> = [ALPHA, BRAVO, CHARLIE, DELTA, ECHO]
        .iter()
        .filter_map(|sentence| body.find(sentence))
        .collect();
    at.windows(2).all(|pair| pair[0] < pair[1])
}

#[tokio::test]
async fn putting_adjacent_cut_sentences_back_in_one_batch_restores_the_original_whatever_the_order(
) -> Result<()> {
    for order in [[0, 1, 2], [2, 0, 1], [1, 2, 0], [2, 1, 0]] {
        let course = published().await?;
        let ids = adjacent_cuts(&course).await?;
        let revision = ledger(&course).await?.revision.0;
        let chosen: Vec<&ClaimId> = order.iter().map(|index| &ids[*index]).collect();

        act_many(&course, &chosen, put_back(None, None), revision).await?;

        assert_eq!(
            bodies(&course).await?[0],
            original_order(),
            "batch {order:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn putting_adjacent_cut_sentences_back_one_at_a_time_keeps_their_order_in_every_order(
) -> Result<()> {
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let course = published().await?;
        let ids = adjacent_cuts(&course).await?;
        for index in order {
            act(&course, &ids[index], put_back(None, None)).await?;
            let body = bodies(&course).await?[0].clone();
            assert!(in_original_order(&body), "{order:?}: {body}");
        }
        assert_eq!(
            bodies(&course).await?[0],
            original_order(),
            "one at a time {order:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_cut_sentence_whose_recorded_neighbours_have_a_sentence_between_them_is_not_placed_by_guessing(
) -> Result<()> {
    let course = published().await?;
    let ids = adjacent_cuts(&course).await?;
    let mine = "The user wrote this one.";
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(format!("{ALPHA} {mine} {ECHO}"))
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;
    let before = written(&course).await?;

    let error = act(&course, &ids[1], put_back(None, None))
        .await
        .unwrap_err();

    assert!(
        matches!(&error, AppError::InvalidInput(message) if message == CHOOSE),
        "{error:?}"
    );
    assert_eq!(written(&course).await?, before);
    Ok(())
}

/// Gives the stored report an old finding for `claim`, as the report held
/// while the machine's cut of its sentence stood.
async fn give_finding(course: &Course, claim: &ClaimId, reason: &str) -> Result<()> {
    let mut stored: LessonVerificationReport = serde_json::from_str(&report(course).await?)?;
    stored.findings.push(Finding {
        unit: 0,
        quote: CUT.into(),
        statement: CUT.into(),
        verdict: ClaimVerdict::Unsupported,
        reason: reason.into(),
        evidence: Vec::new(),
        supporting_quote: None,
        resolution: None,
        claim_id: Some(claim.0.clone()),
        trust: None,
        label: None,
        class: None,
        two_pass: Default::default(),
    });
    sqlx::query(
        "UPDATE learning_lesson_verifications SET report_json=? WHERE program_id=? AND lesson_id=?",
    )
    .bind(serde_json::to_string(&stored)?)
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .execute(&course.pool)
    .await?;
    Ok(())
}

async fn findings_of(course: &Course) -> Result<Vec<(Option<String>, String)>> {
    let stored: LessonVerificationReport = serde_json::from_str(&report(course).await?)?;
    Ok(stored
        .findings
        .into_iter()
        .map(|finding| (finding.claim_id, finding.reason))
        .collect())
}

#[tokio::test]
async fn putting_a_sentence_back_replaces_the_findings_of_every_claim_that_stood_on_it_and_undo_brings_them_back_where_they_stood(
) -> Result<()> {
    let course = published().await?;
    let first = taken_out(&course, "cut", 0, Some((Some(GOOD), None))).await?;
    let second = taken_out_as(
        &course,
        "also",
        "Rows are not read all at once.",
        CUT,
        0,
        Some((Some(GOOD), None)),
    )
    .await?;
    give_finding(&course, &first, "Taken out: first.").await?;
    give_finding(&course, &second, "Taken out: second.").await?;
    let before = findings_of(&course).await?;

    act(&course, &first, put_back(None, None)).await?;

    let after = findings_of(&course).await?;
    assert!(
        after
            .iter()
            .all(|(_, reason)| !reason.starts_with("Taken out")),
        "{after:?}"
    );
    let own: Vec<_> = after
        .iter()
        .filter(|(id, _)| id.as_deref() == Some(first.0.as_str()))
        .collect();
    assert_eq!(own.len(), 1, "{after:?}");

    act(&course, &second, ClaimActionRequestDto::Undo).await?;

    assert_eq!(findings_of(&course).await?, before);
    Ok(())
}

#[tokio::test]
async fn a_cut_recorded_in_another_section_does_not_make_this_ones_place_ambiguous() -> Result<()> {
    let course = published().await?;
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(format!("{ALPHA} {ECHO}"))
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;
    let bravo = taken_out_as(
        &course,
        "bravo",
        BRAVO,
        BRAVO,
        0,
        Some((Some(ALPHA), Some(ECHO))),
    )
    .await?;
    let zulu = "Zulu comes elsewhere.";
    taken_out_as(
        &course,
        "zulu",
        zulu,
        zulu,
        1,
        Some((Some(ECHO), Some(ALPHA))),
    )
    .await?;

    act(&course, &bravo, put_back(None, None)).await?;

    assert_eq!(bodies(&course).await?[0], [ALPHA, BRAVO, ECHO].join(" "));
    Ok(())
}
