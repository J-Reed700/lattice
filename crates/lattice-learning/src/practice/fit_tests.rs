//! Tests for which kinds of practice work a course has.

use super::*;
use crate::practice::practice_fit::{
    basis_of, check_practice_fit, fits, practice_fit, CourseOutline, Input, Open, OutlineLesson,
    OutlineModule, PracticeFit,
};
use crate::practice::session::fake_kind::fake_kind;
use crate::practice::session::kind::{Kind, Kinds};
use crate::practice_repository::KindVerdict;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::domain::learning::practice::work::KindId;
use lattice_core::shared::persistence::Database;
use serde_json::Value;
use std::collections::BTreeSet;

const BEADS: &str = "Beads. A course asks for it when learners string beads.";
const ROPE: &str = "Rope. A course asks for it when learners tie ropes.";
const SHELF: &str = "A shelf. A course asks for it when learners build shelves.";

fn kind(id: &str, words: &str) -> Kind {
    fake_kind(&["a"]).with_id(id).asked_when(words).build()
}

fn wanted(id: &str, words: &str) -> Kind {
    fake_kind(&["a"])
        .with_id(id)
        .asked_when(words)
        .wanted_outright()
        .build()
}

fn reason_of(kind: &str) -> String {
    format!("The reason for {kind}.")
}

/// The handle written before each kind a request lists, with what the kind says.
fn listed(prompt: &str) -> Vec<(String, String)> {
    prompt
        .lines()
        .filter_map(|line| {
            let (handle, words) = line.strip_prefix("- [")?.split_once("] ")?;
            Some((handle.to_owned(), words.to_owned()))
        })
        .collect()
}

fn entry_of(handle: &str, reason: &str, asks: bool) -> Value {
    json!({"of": handle, "reason": reason, "courseAsksForIt": if asks { "yes" } else { "no" }})
}

/// An answer with a verdict for each listed kind whose words name one of `verdicts`.
fn answer(prompt: &str, verdicts: &[(&str, bool)]) -> String {
    let entries: Vec<Value> = listed(prompt)
        .into_iter()
        .filter_map(|(handle, words)| {
            let words = words.to_lowercase();
            verdicts
                .iter()
                .find(|(kind, _)| words.contains(kind))
                .map(|(kind, asks)| entry_of(&handle, &reason_of(kind), *asks))
        })
        .collect();
    json!({ "entries": entries }).to_string()
}

fn replying(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

/// A model that answers the nth request with `reply(prompt, n)`.
fn scripted_by(reply: impl Fn(&str, usize) -> String + Send + Sync + 'static) -> ScriptedModel {
    ScriptedModel::new()
        .on_request::<PracticeFit>(move |sent, nth| replying(reply(&sent.prompt(), nth)))
}

/// A model that answers the nth request with the nth of `verdicts`, and fails a request after them.
fn scripted(verdicts: &'static [&'static [(&'static str, bool)]]) -> ScriptedModel {
    ScriptedModel::new().on_request::<PracticeFit>(move |sent, nth| match verdicts.get(nth) {
        Some(verdicts) => replying(answer(&sent.prompt(), verdicts)),
        None => Box::pin(std::future::ready(Err(AppError::InternalError(
            "Unexpected extra model call".into(),
        )))),
    })
}

fn requests(model: &ScriptedModel) -> Vec<CompletionRequest> {
    model.sent().into_iter().map(|sent| sent.request).collect()
}

fn texts(model: &ScriptedModel) -> Vec<String> {
    model.sent().iter().map(Sent::prompt).collect()
}

fn entry(kind: &str, fit: KindFit, reason: Option<&str>) -> PracticeKindFitDto {
    PracticeKindFitDto {
        kind: kind.to_owned(),
        fit,
        reason: reason.map(str::to_owned),
    }
}

fn decided(kind: &str, fit: KindFit) -> PracticeKindFitDto {
    entry(kind, fit, Some(&reason_of(kind)))
}

type Row = (String, String, i64, String);

async fn rows(pool: &SqlitePool, program_id: &str) -> Result<Vec<Row>> {
    sqlx::query_as(
        "SELECT kind,basis,fits,model FROM learning_practice_fits WHERE program_id=? ORDER BY kind",
    )
    .bind(program_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

type FullRow = (String, String, i64, String, String, i64);

async fn full_rows(pool: &SqlitePool, program_id: &str) -> Result<Vec<FullRow>> {
    sqlx::query_as(
        "SELECT kind,basis,fits,reason,model,decided_at FROM learning_practice_fits WHERE program_id=? ORDER BY kind",
    )
    .bind(program_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

async fn change_outline(pool: &SqlitePool, lesson_id: &str) -> Result<()> {
    sqlx::query("UPDATE learning_lessons SET title='A different lesson' WHERE id=?")
        .bind(lesson_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn outline(lesson_title: &str) -> CourseOutline {
    CourseOutline {
        title: "A course".into(),
        modules: vec![OutlineModule {
            title: "A module".into(),
            lessons: vec![OutlineLesson {
                title: lesson_title.into(),
                objective: "Explain the idea".into(),
            }],
        }],
    }
}

#[tokio::test]
async fn prompt_snapshot_practice_fit() -> Result<()> {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let pool = active_memory_pool().await?;
    let database = Database::new(pool.clone());
    let (program, _lesson) = setup(&pool, &database).await?;
    let repo = LearningPracticeRepository::new(database.clone());
    let kinds = Kinds::new([kind("rope", ROPE), kind("beads", BEADS)])?;
    let model = scripted(&[&[("beads", true), ("rope", false)]]);
    check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;
    snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "practice-fit",
        &requests(&model),
    );
    Ok(())
}

#[tokio::test]
async fn two_kinds_are_decided_in_one_request_and_stored_on_one_basis() -> Result<()> {
    let pool = active_memory_pool().await?;
    let database = Database::new(pool.clone());
    let (program, _lesson) = setup(&pool, &database).await?;
    let repo = LearningPracticeRepository::new(database.clone());
    let kinds = Kinds::new([kind("rope", ROPE), kind("beads", BEADS)])?;
    let model = scripted(&[&[("beads", true), ("rope", false)]]);

    let checked = check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;

    assert_eq!(model.sent().len(), 1);
    let listing = format!("\n\nKinds of work:\n- [k1] {BEADS}\n- [k2] {ROPE}");
    let sent = texts(&model);
    assert!(
        sent.first().is_some_and(|text| text.contains(&listing)),
        "{sent:?}"
    );
    assert_eq!(
        checked.kinds,
        vec![
            decided("beads", KindFit::Fits),
            decided("rope", KindFit::DoesNotFit)
        ]
    );
    let state = repo.fit_state(&program.summary.id).await?;
    let basis = basis_of(&state.outline, &kinds)?;
    assert_eq!(
        rows(&pool, &program.summary.id).await?,
        vec![
            ("beads".into(), basis.clone(), 1, "scripted-model".into()),
            ("rope".into(), basis, 0, "scripted-model".into()),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn deciding_again_sends_nothing_until_the_outline_changes() -> Result<()> {
    let pool = active_memory_pool().await?;
    let database = Database::new(pool.clone());
    let (program, lesson) = setup(&pool, &database).await?;
    let repo = LearningPracticeRepository::new(database.clone());
    let kinds = Kinds::new([kind("beads", BEADS), kind("rope", ROPE)])?;
    let model = scripted(&[
        &[("beads", true), ("rope", true)],
        &[("beads", false), ("rope", false)],
    ]);
    let first = check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;

    assert_eq!(
        practice_fit(&repo, &kinds, &program.summary.id).await?,
        first
    );
    assert_eq!(
        check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?,
        first
    );
    assert_eq!(model.sent().len(), 1);

    change_outline(&pool, &lesson).await?;
    let known = practice_fit(&repo, &kinds, &program.summary.id).await?;
    assert_eq!(
        known.kinds,
        vec![
            entry("beads", KindFit::Checking, None),
            entry("rope", KindFit::Checking, None)
        ]
    );
    let again = check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;
    assert_eq!(model.sent().len(), 2);
    assert_eq!(
        again.kinds,
        vec![
            decided("beads", KindFit::DoesNotFit),
            decided("rope", KindFit::DoesNotFit)
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_reworded_kind_and_an_added_kind_are_each_decided_again_in_one_request() -> Result<()> {
    let pool = active_memory_pool().await?;
    let database = Database::new(pool.clone());
    let (program, _lesson) = setup(&pool, &database).await?;
    let repo = LearningPracticeRepository::new(database.clone());
    let reworded = "Beads, worded anew. A course asks for it when learners thread beads.";
    let model = scripted(&[
        &[("beads", true), ("rope", true)],
        &[("beads", true), ("rope", true)],
        &[("beads", true), ("rope", true), ("shelf", true)],
    ]);
    let first = Kinds::new([kind("beads", BEADS), kind("rope", ROPE)])?;
    check_practice_fit(&repo, &first, &model, &program.summary.id).await?;

    let second = Kinds::new([kind("beads", reworded), kind("rope", ROPE)])?;
    let known = practice_fit(&repo, &second, &program.summary.id).await?;
    assert_eq!(
        known.kinds,
        vec![
            entry("beads", KindFit::Checking, None),
            entry("rope", KindFit::Checking, None)
        ]
    );
    check_practice_fit(&repo, &second, &model, &program.summary.id).await?;
    assert_eq!(model.sent().len(), 2);
    let sent = texts(&model);
    assert!(
        sent.get(1)
            .is_some_and(|text| text.contains(&format!("- [k1] {reworded}"))),
        "{sent:?}"
    );

    let third = Kinds::new([
        kind("beads", reworded),
        kind("rope", ROPE),
        kind("shelf", SHELF),
    ])?;
    let known = practice_fit(&repo, &third, &program.summary.id).await?;
    assert_eq!(known.kinds.len(), 3);
    assert!(known
        .kinds
        .iter()
        .all(|entry| entry.fit == KindFit::Checking));
    let checked = check_practice_fit(&repo, &third, &model, &program.summary.id).await?;
    assert_eq!(model.sent().len(), 3);
    let sent = texts(&model);
    assert!(
        sent.get(2)
            .is_some_and(|text| text.contains(&format!("- [k3] {SHELF}"))),
        "{sent:?}"
    );
    assert_eq!(
        checked.kinds,
        vec![
            decided("beads", KindFit::Fits),
            decided("rope", KindFit::Fits),
            decided("shelf", KindFit::Fits),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_kind_the_profile_wants_fits_without_a_reason_even_when_its_row_says_no() -> Result<()> {
    let pool = active_memory_pool().await?;
    let database = Database::new(pool.clone());
    let (program, _lesson) = setup(&pool, &database).await?;
    let repo = LearningPracticeRepository::new(database.clone());
    let kinds = Kinds::new([wanted("beads", BEADS), kind("rope", ROPE)])?;
    let state = repo.fit_state(&program.summary.id).await?;
    let basis = basis_of(&state.outline, &kinds)?;
    repo.record_fits(
        &program.summary.id,
        &basis,
        &[KindVerdict {
            kind: "beads".into(),
            fits: false,
            reason: "A stored no.".into(),
        }],
        "test",
        1,
    )
    .await?;

    let known = practice_fit(&repo, &kinds, &program.summary.id).await?;
    assert_eq!(
        known.kinds,
        vec![
            entry("beads", KindFit::Fits, None),
            entry("rope", KindFit::Checking, None)
        ]
    );

    let model = scripted(&[&[("beads", false), ("rope", true)]]);
    let checked = check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;
    assert_eq!(model.sent().len(), 1);
    assert!(
        texts(&model).iter().all(|text| !text.contains(BEADS)),
        "a kind the profile wants is not asked"
    );
    assert_eq!(
        checked.kinds,
        vec![
            entry("beads", KindFit::Fits, None),
            decided("rope", KindFit::Fits)
        ]
    );
    let stored = rows(&pool, &program.summary.id).await?;
    assert_eq!(
        stored.iter().map(|row| row.2).collect::<Vec<_>>(),
        vec![0, 1]
    );
    Ok(())
}

#[tokio::test]
async fn when_every_kind_is_wanted_nothing_is_asked_and_nothing_stored() -> Result<()> {
    let pool = active_memory_pool().await?;
    let database = Database::new(pool.clone());
    let (program, _lesson) = setup(&pool, &database).await?;
    let repo = LearningPracticeRepository::new(database.clone());
    let kinds = Kinds::new([wanted("beads", BEADS), wanted("rope", ROPE)])?;
    let model = scripted(&[]);

    let checked = check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;

    assert_eq!(model.sent().len(), 0);
    assert_eq!(
        checked.kinds,
        vec![
            entry("beads", KindFit::Fits, None),
            entry("rope", KindFit::Fits, None)
        ]
    );
    assert_eq!(
        practice_fit(&repo, &kinds, &program.summary.id).await?,
        checked
    );
    assert!(rows(&pool, &program.summary.id).await?.is_empty());
    Ok(())
}

async fn beads_and_rope() -> Result<(SqlitePool, LearningPracticeRepository, String, Kinds)> {
    let pool = active_memory_pool().await?;
    let database = Database::new(pool.clone());
    let (program, _lesson) = setup(&pool, &database).await?;
    let repo = LearningPracticeRepository::new(database.clone());
    let kinds = Kinds::new([kind("beads", BEADS), kind("rope", ROPE)])?;
    Ok((pool, repo, program.summary.id, kinds))
}

/// A first answer for the listed kinds that `first` makes of them, then answers for whatever is listed.
fn then_everything(first: fn(&[(String, String)]) -> Vec<Value>) -> ScriptedModel {
    scripted_by(move |prompt, nth| {
        if nth == 0 {
            json!({ "entries": first(&listed(prompt)) }).to_string()
        } else {
            answer(prompt, &[("beads", true), ("rope", false)])
        }
    })
}

#[tokio::test]
async fn a_kind_the_answer_leaves_out_is_asked_again_alone_and_the_others_stand() -> Result<()> {
    let (pool, repo, program_id, kinds) = beads_and_rope().await?;
    let model = then_everything(|kinds| {
        kinds
            .iter()
            .filter(|(_, words)| words.starts_with("Beads"))
            .map(|(handle, _)| entry_of(handle, &reason_of("beads"), true))
            .collect()
    });

    let checked = check_practice_fit(&repo, &kinds, &model, &program_id).await?;

    let sent = texts(&model);
    assert_eq!(sent.len(), 2);
    assert!(sent
        .get(1)
        .is_some_and(|text| text.contains(ROPE) && !text.contains(BEADS)));
    assert_eq!(
        checked.kinds,
        vec![
            decided("beads", KindFit::Fits),
            decided("rope", KindFit::DoesNotFit)
        ]
    );
    assert_eq!(rows(&pool, &program_id).await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_blank_reason_is_corrected_for_that_kind_alone() -> Result<()> {
    let (_pool, repo, program_id, kinds) = beads_and_rope().await?;
    let model = then_everything(|kinds| {
        kinds
            .iter()
            .map(|(handle, words)| {
                let reason = if words.starts_with("Rope") {
                    "  ".to_owned()
                } else {
                    reason_of("beads")
                };
                entry_of(handle, &reason, true)
            })
            .collect()
    });

    let checked = check_practice_fit(&repo, &kinds, &model, &program_id).await?;

    let sent = texts(&model);
    assert_eq!(sent.len(), 2);
    assert!(sent
        .get(1)
        .is_some_and(|text| text.contains("The reason for this kind is empty")));
    assert_eq!(
        checked.kinds,
        vec![
            decided("beads", KindFit::Fits),
            decided("rope", KindFit::DoesNotFit)
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_kind_answered_twice_differently_is_asked_again_alone() -> Result<()> {
    let (_pool, repo, program_id, kinds) = beads_and_rope().await?;
    let model = then_everything(|kinds| {
        let mut entries = Vec::new();
        for (handle, words) in kinds {
            entries.push(entry_of(handle, "A reason.", true));
            if words.starts_with("Beads") {
                entries.push(entry_of(handle, "A reason.", false));
            }
        }
        entries
    });

    let checked = check_practice_fit(&repo, &kinds, &model, &program_id).await?;

    let sent = texts(&model);
    assert_eq!(sent.len(), 2);
    assert!(sent
        .get(1)
        .is_some_and(|text| text.contains(BEADS) && !text.contains(ROPE)));
    assert_eq!(
        checked.kinds,
        vec![
            decided("beads", KindFit::Fits),
            entry("rope", KindFit::Fits, Some("A reason."))
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_kind_the_model_never_answers_stays_being_checked_and_is_the_only_one_asked_next_time(
) -> Result<()> {
    let (pool, repo, program_id, kinds) = beads_and_rope().await?;
    let silent = scripted_by(|prompt, _| answer(prompt, &[("beads", true)]));

    let checked = check_practice_fit(&repo, &kinds, &silent, &program_id).await?;

    assert_eq!(
        checked.kinds,
        vec![
            decided("beads", KindFit::Fits),
            entry("rope", KindFit::Checking, None)
        ]
    );
    assert_eq!(rows(&pool, &program_id).await?.len(), 1);

    let later = scripted(&[&[("rope", false)]]);
    let checked = check_practice_fit(&repo, &kinds, &later, &program_id).await?;

    let sent = texts(&later);
    assert_eq!(sent.len(), 1);
    assert!(sent
        .first()
        .is_some_and(|text| text.contains(ROPE) && !text.contains(BEADS)));
    assert_eq!(
        checked.kinds,
        vec![
            decided("beads", KindFit::Fits),
            decided("rope", KindFit::DoesNotFit)
        ]
    );
    assert_eq!(rows(&pool, &program_id).await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_leaves_every_kind_being_checked() -> Result<()> {
    let (pool, repo, program_id, kinds) = beads_and_rope().await?;
    let model = scripted_by(|_, _| "No verdicts here.".to_owned());

    let checked = check_practice_fit(&repo, &kinds, &model, &program_id).await?;

    assert!(checked
        .kinds
        .iter()
        .all(|entry| entry.fit == KindFit::Checking));
    assert!(rows(&pool, &program_id).await?.is_empty());
    Ok(())
}

#[test]
fn the_fit_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Input {
        outline: marks.told(CourseOutline {
            title: "Harbour tides".into(),
            modules: vec![OutlineModule {
                title: "Reading the almanac".into(),
                lessons: vec![OutlineLesson {
                    title: "The gate opens".into(),
                    objective: "Say when the gate opens.".into(),
                }],
            }],
        }),
        kinds: vec![Open {
            id: marks.reference("kind-key-0001".to_owned()),
            asked_when: marks.told("A kiln. A course asks for it when learners fire bowls.".into()),
        }],
    };
    holds::<PracticeFit>(&input, &marks);
}

#[tokio::test]
async fn fits_gives_exactly_the_kinds_that_fit() -> Result<()> {
    let pool = active_memory_pool().await?;
    let database = Database::new(pool.clone());
    let (program, lesson) = setup(&pool, &database).await?;
    let repo = LearningPracticeRepository::new(database.clone());
    let kinds = Kinds::new([
        wanted("beads", BEADS),
        kind("rope", ROPE),
        kind("shelf", SHELF),
    ])?;
    let model = scripted(&[&[("beads", true), ("rope", true), ("shelf", false)]]);
    let only =
        |ids: &[&str]| -> BTreeSet<KindId> { ids.iter().map(|id| KindId::new(id)).collect() };

    assert_eq!(
        fits(&repo, &kinds, &program.summary.id).await?,
        only(&["beads"])
    );

    check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;
    assert_eq!(
        fits(&repo, &kinds, &program.summary.id).await?,
        only(&["beads", "rope"])
    );

    change_outline(&pool, &lesson).await?;
    assert_eq!(
        fits(&repo, &kinds, &program.summary.id).await?,
        only(&["beads"])
    );
    Ok(())
}

#[tokio::test]
async fn recording_fits_replaces_the_courses_earlier_rows_and_skips_a_deleted_course() -> Result<()>
{
    let pool = active_memory_pool().await?;
    let database = Database::new(pool.clone());
    let program = crate::tests::fixture();
    LearningRepository::new(database.clone())
        .create(&program)
        .await?;
    let program_id = program.summary.id.clone();
    let repo = LearningPracticeRepository::new(database.clone());
    let verdict = |kind: &str, fits: bool, reason: &str| KindVerdict {
        kind: kind.into(),
        fits,
        reason: reason.into(),
    };
    repo.record_fits(
        &program_id,
        "basis-1",
        &[
            verdict("beads", true, "Yes."),
            verdict("rope", false, "No."),
        ],
        "model-a",
        10,
    )
    .await?;
    assert_eq!(
        full_rows(&pool, &program_id).await?,
        vec![
            (
                "beads".into(),
                "basis-1".into(),
                1,
                "Yes.".into(),
                "model-a".into(),
                10
            ),
            (
                "rope".into(),
                "basis-1".into(),
                0,
                "No.".into(),
                "model-a".into(),
                10
            ),
        ]
    );

    repo.record_fits(
        &program_id,
        "basis-2",
        &[verdict("beads", false, "Now no.")],
        "model-b",
        20,
    )
    .await?;
    assert_eq!(
        full_rows(&pool, &program_id).await?,
        vec![(
            "beads".into(),
            "basis-2".into(),
            0,
            "Now no.".into(),
            "model-b".into(),
            20
        )]
    );

    sqlx::query("DELETE FROM learning_practice_fits WHERE program_id=?")
        .bind(&program_id)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM learning_programs WHERE id=?")
        .bind(&program_id)
        .execute(&pool)
        .await?;
    repo.record_fits(
        &program_id,
        "basis-3",
        &[verdict("beads", true, "Back.")],
        "model-c",
        30,
    )
    .await?;
    assert!(full_rows(&pool, &program_id).await?.is_empty());
    Ok(())
}

#[test]
fn a_changed_outline_or_kind_changes_the_basis_and_the_same_ones_do_not() -> Result<()> {
    let kinds = Kinds::new([kind("beads", BEADS)])?;
    let first = basis_of(&outline("Hash maps"), &kinds)?;
    assert_eq!(first, basis_of(&outline("Hash maps"), &kinds)?);
    assert_ne!(first, basis_of(&outline("Binary search"), &kinds)?);
    let mut more = outline("Hash maps");
    more.modules.push(OutlineModule {
        title: "Another module".into(),
        lessons: Vec::new(),
    });
    assert_ne!(first, basis_of(&more, &kinds)?);
    let reworded = Kinds::new([kind("beads", ROPE)])?;
    assert_ne!(first, basis_of(&outline("Hash maps"), &reworded)?);
    let grown = Kinds::new([kind("beads", BEADS), kind("rope", ROPE)])?;
    assert_ne!(first, basis_of(&outline("Hash maps"), &grown)?);
    Ok(())
}
