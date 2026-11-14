#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::tests::{loose, model, shaped_lesson, sources};
use super::*;
use crate::lessons::claim_reviews::teaching_findings::TeachingFindingsRepository;
use crate::lessons::shape;
use crate::review_findings::{
    site_sha256, FindingKind, FindingSite, PlanCheck, ReviewFinding, Rule, SetAside,
};
use lattice_core::shared::persistence::Database;

/// The shaped lesson's questions: practice at units 11 and 12, quiz at 13
/// and 14, test at 15 and 16.
fn waiting_on(lesson: &Value, questions: &[usize]) -> BTreeSet<String> {
    questions
        .iter()
        .map(|index| site_sha256(lesson, FindingSite::Question(*index)))
        .collect()
}

fn waiting_on_sections(lesson: &Value, sections: &[usize]) -> BTreeSet<String> {
    sections
        .iter()
        .map(|index| site_sha256(lesson, FindingSite::Section(*index)))
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

fn units_of(lesson: &Value, waiting: &BTreeSet<String>, cuts: &Cuts) -> Vec<usize> {
    chosen(lesson, waiting, cuts)
        .into_iter()
        .map(|(unit, _)| unit)
        .collect()
}

#[test]
fn a_section_whose_finding_waits_is_chosen_when_the_lesson_can_spare_it() {
    let lesson = shaped_lesson();
    let waiting = waiting_on_sections(&lesson, &[2]);
    assert_eq!(
        chosen(&lesson, &waiting, &Cuts::default()),
        [(2, "Not to be confused with".to_owned())]
    );
}

#[test]
fn a_worked_example_whose_finding_waits_goes_with_the_check_it_shows() {
    let lesson = shaped_lesson();
    let waiting = waiting_on_sections(&lesson, &[1]);
    assert_eq!(
        chosen(&lesson, &waiting, &Cuts::default()),
        [(1, "First example".to_owned()), (3, "Your turn".to_owned())]
    );
}

#[test]
fn sections_are_chosen_only_while_the_lesson_can_spare_them() {
    let lesson = shaped_lesson();
    let waiting = waiting_on_sections(&lesson, &[2, 7, 8, 9]);
    assert_eq!(units_of(&lesson, &waiting, &Cuts::default()), [2, 7, 8]);
}

#[test]
fn a_section_that_would_take_too_much_with_it_stays_and_a_later_one_still_goes() {
    let lesson = shaped_lesson();
    let mut cuts = Cuts::default();
    cuts.go(7, "Guided".into());
    cuts.go(8, "On your own".into());
    let waiting = waiting_on_sections(&lesson, &[1, 2]);
    assert_eq!(units_of(&lesson, &waiting, &cuts), [2]);
}

#[test]
fn sections_and_questions_are_chosen_in_unit_order() {
    let lesson = shaped_lesson();
    let mut waiting = waiting_on_sections(&lesson, &[2]);
    waiting.extend(waiting_on(&lesson, &[3]));
    assert_eq!(
        chosen(&lesson, &waiting, &Cuts::default()),
        [
            (2, "Not to be confused with".to_owned()),
            (14, "Q2?".to_owned())
        ]
    );
}

#[test]
fn the_first_explanation_is_never_chosen() {
    let lesson = shaped_lesson();
    let waiting = waiting_on_sections(&lesson, &[0]);
    assert!(chosen(&lesson, &waiting, &Cuts::default()).is_empty());
}

async fn record_waiting(
    database: &Database,
    (job, lesson_id): (&str, &str),
    lesson: &Value,
    finding: ReviewFinding,
) -> Result<()> {
    TeachingFindingsRepository::new(database.clone())
        .record(
            lesson_id,
            job,
            lesson,
            &[SetAside {
                finding,
                repairs: 2,
            }],
            true,
        )
        .await
}

#[tokio::test]
async fn a_section_finding_that_waits_sends_its_section_out() -> Result<()> {
    let pool = crate::test_support::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = crate::content_verification::test_support::setup(&db).await?;
    let lesson = shaped_lesson();
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(2)), "Alpha holds the line");
    record_waiting(&db, (&job, &lesson_id), &lesson, finding).await?;
    crate::lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        let mut cuts = Cuts::default();
        assert!(send_out_unsettled(&lesson, &mut cuts, shape::policy()).await?);
        assert_eq!(
            cuts.going.get(&2).map(String::as_str),
            Some("Not to be confused with")
        );
        assert!(cuts.unsettled.contains(&2));
        assert!(!send_out_unsettled(&lesson, &mut cuts, shape::policy()).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_check_that_goes_with_an_unsettled_section_comes_back_with_it() -> Result<()> {
    let pool = crate::test_support::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = crate::content_verification::test_support::setup(&db).await?;
    let lesson = shaped_lesson();
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(1)), "Alpha holds the line");
    record_waiting(&db, (&job, &lesson_id), &lesson, finding).await?;
    crate::lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        let mut cuts = Cuts::default();
        send_out_unsettled(&lesson, &mut cuts, shape::policy()).await?;
        assert_eq!(cuts.unsettled.iter().copied().collect::<Vec<_>>(), [1, 3]);
        assert_eq!(cuts.going.keys().copied().collect::<Vec<_>>(), [1, 3]);
        assert!(cuts.readmit());
        assert!(cuts.going.is_empty());
        Ok(())
    })
    .await
}

/// What `send_out_unsettled` sends out for each finding in turn, each the
/// only one recorded: its id, whether anything went, and the units that did.
async fn sent_out_for(findings: Vec<ReviewFinding>) -> Result<Vec<(String, bool, Vec<usize>)>> {
    let pool = crate::test_support::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = crate::content_verification::test_support::setup(&db).await?;
    let lesson = shaped_lesson();
    let mut sent = Vec::new();
    for finding in findings {
        let id = finding.id().as_str().to_owned();
        record_waiting(&db, (&job, &lesson_id), &lesson, finding).await?;
        let (any, going) = crate::lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
            let mut cuts = Cuts::default();
            let any = send_out_unsettled(&lesson, &mut cuts, shape::policy()).await?;
            Ok((any, cuts.going.keys().copied().collect::<Vec<_>>()))
        })
        .await?;
        sent.push((id, any, going));
    }
    Ok(sent)
}

#[tokio::test]
async fn a_finding_about_a_parts_own_text_sends_the_part_out_whatever_raised_it() -> Result<()> {
    let at = FindingSite::Section(2);
    let sent = sent_out_for(vec![
        ReviewFinding::reviewer(1, Some(at), "Alpha holds the line"),
        ReviewFinding::rule(Rule::Schema, FindingKind::Structure, at, "Alpha is off"),
        ReviewFinding::rule_about(
            Rule::SentenceRepeated,
            FindingKind::Teaching,
            at,
            "Alpha holds the line.",
            "Alpha repeats",
        ),
        ReviewFinding::plan(PlanCheck::Contradiction, "subject-1", at, "Alpha clashes"),
        ReviewFinding::plan(PlanCheck::Unseen, "subject-1", at, "Alpha is unseen"),
    ])
    .await?;
    for (id, _, going) in sent {
        assert_eq!(going, [2], "finding {id}");
    }
    Ok(())
}

#[tokio::test]
async fn a_finding_about_what_the_lesson_lacks_or_the_whole_lesson_sends_nothing_out() -> Result<()>
{
    let at = FindingSite::Section(2);
    let sent = sent_out_for(vec![
        ReviewFinding::plan(PlanCheck::Used, "subject-1", at, "Alpha is unused"),
        ReviewFinding::plan(PlanCheck::Planned, "item-1", at, "Alpha is unplanned"),
        ReviewFinding::reviewer(1, Some(FindingSite::Lesson), "Alpha holds the line"),
        ReviewFinding::reviewer(1, None, "Alpha holds the line"),
    ])
    .await?;
    for (id, any, going) in sent {
        assert!(!any, "finding {id}");
        assert!(going.is_empty(), "finding {id}");
    }
    Ok(())
}

#[tokio::test]
async fn a_waiting_section_finding_sends_the_section_out_of_the_lesson() -> Result<()> {
    let pool = crate::test_support::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = crate::content_verification::test_support::setup(&db).await?;
    let lesson = shaped_lesson();
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(2)), "Alpha holds the line");
    record_waiting(&db, (&job, &lesson_id), &lesson, finding).await?;
    let llm = model(&[]);
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    crate::lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
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
        assert_eq!(
            ledger.cuts.going.get(&2).map(String::as_str),
            Some("Not to be confused with")
        );
        assert!(ledger.cuts.unsettled.contains(&2));
        Ok(())
    })
    .await
}
