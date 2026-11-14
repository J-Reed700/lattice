#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::tests::{loose, model, shaped_lesson, sources};
use super::*;
use crate::lessons::claim_reviews::teaching_findings::TeachingFindingsRepository;
use crate::lessons::shape;
use crate::review_findings::{site_sha256, FindingSite, ReviewFinding, SetAside};
use lattice_core::shared::persistence::Database;

/// The shaped lesson's questions: practice at units 11 and 12, quiz at 13
/// and 14, test at 15 and 16.
fn waiting_on(lesson: &Value, questions: &[usize]) -> BTreeSet<String> {
    questions
        .iter()
        .map(|index| site_sha256(lesson, FindingSite::Question(*index)))
        .collect()
}

fn chosen(lesson: &Value, waiting: &BTreeSet<String>, cuts: &Cuts) -> Vec<(usize, String)> {
    unsettled_units(lesson, waiting, cuts, shape::policy())
}

#[test]
fn a_question_whose_finding_waits_is_chosen_when_the_lesson_can_spare_it() {
    let lesson = shaped_lesson();
    let waiting = waiting_on(&lesson, &[3]);
    assert_eq!(
        chosen(&lesson, &waiting, &Cuts::default()),
        [(14, "Q2?".to_owned())]
    );
}

#[test]
fn of_two_waiting_questions_of_one_kind_only_the_first_is_chosen() {
    let lesson = shaped_lesson();
    let waiting = waiting_on(&lesson, &[0, 1]);
    assert_eq!(
        chosen(&lesson, &waiting, &Cuts::default()),
        [(11, "P1?".to_owned())]
    );
}

#[test]
fn a_hash_that_matches_no_question_chooses_nothing() {
    let lesson = shaped_lesson();
    let waiting = BTreeSet::from(["not a question's hash".to_owned()]);
    assert!(chosen(&lesson, &waiting, &Cuts::default()).is_empty());
}

#[test]
fn a_question_already_going_is_not_chosen_again() {
    let lesson = shaped_lesson();
    let mut cuts = Cuts::default();
    cuts.go(13, "Q1?".into());
    let waiting = waiting_on(&lesson, &[2, 4]);
    assert_eq!(chosen(&lesson, &waiting, &cuts), [(15, "T1?".to_owned())]);
}

#[test]
fn readmit_puts_back_only_the_unsettled_units() {
    let mut cuts = Cuts::default();
    cuts.go(0, "First idea".into());
    cuts.go(12, "P2?".into());
    cuts.go_unsettled(11, "P1?".into());
    assert!(cuts.going.contains_key(&11));
    assert!(cuts.readmit());
    assert_eq!(cuts.going.keys().copied().collect::<Vec<_>>(), [0, 12]);
    assert!(cuts.unsettled.is_empty());
    assert!(!cuts.readmit());
}

#[test]
fn keep_current_drops_an_unsettled_unit_whose_label_changed() {
    let lesson = shaped_lesson();
    let mut cuts = Cuts::default();
    cuts.go_unsettled(11, "P1?".into());
    cuts.go_unsettled(13, "An older prompt?".into());
    cuts.keep_current(&lesson);
    assert_eq!(cuts.going.keys().copied().collect::<Vec<_>>(), [11]);
    assert_eq!(cuts.unsettled.iter().copied().collect::<Vec<_>>(), [11]);
}

#[test]
fn cuts_saved_without_the_unsettled_field_load() {
    let saved = json!({"taken":{},"going":{"11":"P1?"},"mended":{}});
    let cuts: std::result::Result<Cuts, _> = serde_json::from_value(saved);
    assert!(cuts.is_ok(), "{:?}", cuts.err());
    let cuts = cuts.unwrap();
    assert!(cuts.unsettled.is_empty());
    assert!(cuts.going.contains_key(&11));
}

#[tokio::test]
async fn a_waiting_question_finding_sends_the_question_out_of_the_lesson() -> Result<()> {
    let pool = crate::test_support::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = crate::content_verification::test_support::setup(&db).await?;
    let database = db.clone();
    let lesson = shaped_lesson();
    let finding =
        ReviewFinding::reviewer(1, Some(FindingSite::Question(2)), "Alpha holds the line");
    TeachingFindingsRepository::new(database.clone())
        .record(
            &lesson_id,
            &job,
            &lesson,
            &[SetAside {
                finding,
                repairs: 3,
            }],
            true,
        )
        .await?;
    let llm = model(&[]);
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    crate::lesson_drafts::run(&database, &repo, &job, &lesson_id, async {
        let mut ledger = ClaimLedger::open(&llm, "{}").await?;
        let settled = settle(
            &llm,
            ("{}", loose(), 4_000),
            (&references, &[]),
            &lesson.to_string(),
            &mut ledger,
        )
        .await?;
        assert!(matches!(settled, Settled::Again(_)));
        assert_eq!(ledger.cuts.going.get(&13).map(String::as_str), Some("Q1?"));
        assert!(ledger.cuts.unsettled.contains(&13));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_floor_failure_with_an_unsettled_question_going_puts_it_back() -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    ledger.cuts.go_unsettled(11, "P1?".into());
    ledger.cuts.go(12, "P2?".into());
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    let settled = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &shaped_lesson().to_string(),
        &mut ledger,
    )
    .await?;

    assert!(matches!(settled, Settled::Again(_)));
    assert_eq!(ledger.cuts.going.keys().copied().collect::<Vec<_>>(), [12]);
    assert!(ledger.cuts.unsettled.is_empty());
    Ok(())
}

#[test]
fn with_every_question_waiting_one_question_of_each_kind_stays() {
    let lesson = shaped_lesson();
    let waiting = waiting_on(&lesson, &[0, 1, 2, 3, 4, 5]);
    let units: Vec<usize> = chosen(&lesson, &waiting, &Cuts::default())
        .into_iter()
        .map(|(unit, _)| unit)
        .collect();
    assert_eq!(units, [11, 13, 15]);
}

#[tokio::test]
async fn a_finding_that_waits_after_the_review_sends_its_question_out() -> Result<()> {
    let pool = crate::test_support::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = crate::content_verification::test_support::setup(&db).await?;
    let database = db.clone();
    let lesson = shaped_lesson();
    let finding =
        ReviewFinding::reviewer(1, Some(FindingSite::Question(2)), "Alpha holds the line");
    TeachingFindingsRepository::new(database.clone())
        .record(
            &lesson_id,
            &job,
            &lesson,
            &[SetAside {
                finding,
                repairs: 3,
            }],
            true,
        )
        .await?;
    crate::lesson_drafts::run(&database, &repo, &job, &lesson_id, async {
        let mut cuts = Cuts::default();
        assert!(send_out_unsettled(&lesson, &mut cuts, shape::policy()).await?);
        assert_eq!(cuts.going.get(&13).map(String::as_str), Some("Q1?"));
        assert!(cuts.unsettled.contains(&13));
        assert!(!send_out_unsettled(&lesson, &mut cuts, shape::policy()).await?);
        Ok(())
    })
    .await
}
