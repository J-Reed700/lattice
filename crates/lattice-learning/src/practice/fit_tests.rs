//! Tests for which kinds of practice work a course has.

use super::*;
use crate::practice::practice_fit::{
    basis_of, check_practice_fit, fits, practice_fit, CourseOutline, OutlineLesson, OutlineModule,
};
use crate::practice::session::fake_kind::{fake_kind, ScriptedModel};
use crate::practice::session::kind::{Kind, Kinds};
use crate::practice_repository::KindVerdict;
use lattice_core::domain::learning::practice::work::KindId;
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

fn answer(verdicts: &[(&str, bool)]) -> String {
    let kinds: Vec<_> = verdicts
        .iter()
        .map(|(kind, asks)| {
            json!({"kind": kind, "reason": reason_of(kind), "courseAsksForIt": asks})
        })
        .collect();
    json!({ "kinds": kinds }).to_string()
}

fn scripted(answers: &[String]) -> ScriptedModel {
    ScriptedModel::new(answers.iter().map(String::as_str))
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
    .map_err(db)
}

type FullRow = (String, String, i64, String, String, i64);

async fn full_rows(pool: &SqlitePool, program_id: &str) -> Result<Vec<FullRow>> {
    sqlx::query_as(
        "SELECT kind,basis,fits,reason,model,decided_at FROM learning_practice_fits WHERE program_id=? ORDER BY kind",
    )
    .bind(program_id)
    .fetch_all(pool)
    .await
    .map_err(db)
}

async fn change_outline(pool: &SqlitePool, lesson_id: &str) -> Result<()> {
    sqlx::query("UPDATE learning_lessons SET title='A different lesson' WHERE id=?")
        .bind(lesson_id)
        .execute(pool)
        .await
        .map_err(db)?;
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
async fn two_kinds_are_decided_in_one_request_and_stored_on_one_basis() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let kinds = Kinds::new([kind("rope", ROPE), kind("beads", BEADS)])?;
    let model = scripted(&[answer(&[("beads", true), ("rope", false)])]);

    let checked = check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;

    assert_eq!(model.requests().len(), 1);
    let listing = format!("\n\nKinds of work:\n- beads: {BEADS}\n- rope: {ROPE}");
    let sent = model.texts();
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
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let kinds = Kinds::new([kind("beads", BEADS), kind("rope", ROPE)])?;
    let model = scripted(&[
        answer(&[("beads", true), ("rope", true)]),
        answer(&[("beads", false), ("rope", false)]),
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
    assert_eq!(model.requests().len(), 1);

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
    assert_eq!(model.requests().len(), 2);
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
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let reworded = "Beads, worded anew. A course asks for it when learners thread beads.";
    let model = scripted(&[
        answer(&[("beads", true), ("rope", true)]),
        answer(&[("beads", true), ("rope", true)]),
        answer(&[("beads", true), ("rope", true), ("shelf", true)]),
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
    assert_eq!(model.requests().len(), 2);
    let sent = model.texts();
    assert!(
        sent.get(1)
            .is_some_and(|text| text.contains(&format!("- beads: {reworded}"))),
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
    assert_eq!(model.requests().len(), 3);
    let sent = model.texts();
    assert!(
        sent.get(2)
            .is_some_and(|text| text.contains(&format!("- shelf: {SHELF}"))),
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
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
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

    let model = scripted(&[answer(&[("beads", false), ("rope", true)])]);
    let checked = check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;
    assert_eq!(model.requests().len(), 1);
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
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let kinds = Kinds::new([wanted("beads", BEADS), wanted("rope", ROPE)])?;
    let model = scripted(&[]);

    let checked = check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;

    assert_eq!(model.requests().len(), 0);
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

fn left_out() -> String {
    answer(&[("beads", true)])
}

fn named_twice() -> String {
    answer(&[("beads", true), ("beads", false)])
}

fn blank_reason() -> String {
    json!({"kinds": [
        {"kind": "beads", "reason": "Strung on a cord.", "courseAsksForIt": true},
        {"kind": "rope", "reason": "  ", "courseAsksForIt": false},
    ]})
    .to_string()
}

#[tokio::test]
async fn an_answer_that_breaks_its_shape_is_refused_twice_naming_the_kind() -> Result<()> {
    for (bad, named) in [
        (left_out(), "rope"),
        (named_twice(), "beads"),
        (blank_reason(), "rope"),
    ] {
        let pool = active_memory_pool().await?;
        let (program, _lesson) = setup(&pool).await?;
        let repo = LearningPracticeRepository::new(pool.clone());
        let kinds = Kinds::new([kind("beads", BEADS), kind("rope", ROPE)])?;
        let model = scripted(&[bad.clone(), bad]);

        let failed = check_practice_fit(&repo, &kinds, &model, &program.summary.id).await;

        assert!(
            matches!(&failed, Err(AppError::ModelAnswerUnusable(reason)) if reason.contains(named)),
            "{failed:?}"
        );
        assert_eq!(model.requests().len(), 2);
        assert!(rows(&pool, &program.summary.id).await?.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn a_refused_answer_is_asked_again_and_the_second_answer_is_stored() -> Result<()> {
    for bad in [left_out(), named_twice(), blank_reason()] {
        let pool = active_memory_pool().await?;
        let (program, _lesson) = setup(&pool).await?;
        let repo = LearningPracticeRepository::new(pool.clone());
        let kinds = Kinds::new([kind("beads", BEADS), kind("rope", ROPE)])?;
        let model = scripted(&[bad, answer(&[("beads", true), ("rope", false)])]);

        let checked = check_practice_fit(&repo, &kinds, &model, &program.summary.id).await?;

        assert_eq!(model.requests().len(), 2);
        assert_eq!(
            checked.kinds,
            vec![
                decided("beads", KindFit::Fits),
                decided("rope", KindFit::DoesNotFit)
            ]
        );
        assert_eq!(rows(&pool, &program.summary.id).await?.len(), 2);
    }
    Ok(())
}

#[tokio::test]
async fn fits_gives_exactly_the_kinds_that_fit() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let kinds = Kinds::new([
        wanted("beads", BEADS),
        kind("rope", ROPE),
        kind("shelf", SHELF),
    ])?;
    let model = scripted(&[answer(&[("beads", true), ("rope", true), ("shelf", false)])]);
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
    let program = crate::tests::fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let program_id = program.summary.id.clone();
    let repo = LearningPracticeRepository::new(pool.clone());
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
        .await
        .map_err(db)?;
    sqlx::query("DELETE FROM learning_programs WHERE id=?")
        .bind(&program_id)
        .execute(&pool)
        .await
        .map_err(db)?;
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
