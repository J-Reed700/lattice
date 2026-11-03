//! What ends the loop that prepares a job's lessons (claim classes design
//! §3.2): a source added meanwhile never sends a prepared lesson round again,
//! and a lesson is prepared again once, not without end.
use super::claim_review_flow_tests::Waiting;
use super::*;

/// A lesson of the fixture, verified against `sources` and bound to them.
async fn staged(
    model: &Model,
    id: &str,
    sources: &[LearningSourceDto],
) -> Result<(String, PreparedLearningLesson)> {
    let mut candidate = candidate(GOOD);
    candidate["blocks"][0]["sourceIds"] = json!([]);
    let report = verify(model, &candidate, sources).await?;
    let mut prepared = PreparedLearningLesson {
        verification: None,
        blocks: serde_json::from_value(candidate["blocks"].clone())?,
        questions: vec![],
        keys: vec![],
    };
    prepared.verification = Some(report.bind(id, &prepared)?);
    Ok((id.to_owned(), prepared))
}

#[tokio::test]
async fn a_one_lesson_job_whose_research_saves_a_page_prepares_the_lesson_once() -> Result<()> {
    let model = Model::new();
    let before = vec![source()];
    let lesson = staged(&model, "lesson", &before).await?;
    // Research saved a page while the lesson was prepared: the program's
    // sources are one more than the lesson's report is bound to.
    let mut after = before.clone();
    after.push(source());

    let mut rounds = Rounds::default();

    assert!(!rounds.again(&[lesson], &after, &model)?);
    Ok(())
}

#[tokio::test]
async fn a_second_lessons_research_does_not_send_the_first_lesson_round_again() -> Result<()> {
    let model = Model::new();
    let base = vec![source()];
    let first = staged(&model, "first", &base).await?;
    // The second lesson's research saves a page: the second is bound to it, the
    // first was prepared before it existed.
    let mut grown = base.clone();
    grown.push(source());
    let second = staged(&model, "second", &grown).await?;

    let mut rounds = Rounds::default();

    assert!(!rounds.again(&[first, second], &grown, &model)?);
    Ok(())
}

#[tokio::test]
async fn a_lesson_whose_bound_source_changed_is_prepared_again_once_and_not_a_third_time(
) -> Result<()> {
    let model = Model::new();
    let mut sources = vec![source()];
    let lesson = staged(&model, "lesson", &sources).await?;
    let mut rounds = Rounds::default();
    assert!(!rounds.again(&[lesson.clone()], &sources, &model)?);

    // A source the report is bound to changes: the lesson is prepared again.
    sources[0].excerpt.push_str(" Revised.");
    assert!(rounds.again(&[lesson], &sources, &model)?);

    // The second preparation stands while the source stays as it is.
    let second = staged(&model, "lesson", &sources).await?;
    assert!(!rounds.again(&[second.clone()], &sources, &model)?);

    // It changes a second time in the same job: no third preparation; the job
    // ends saying so.
    sources[0].excerpt.push_str(" Revised again.");
    let error = rounds
        .again(&[second], &sources, &model)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("prepared again and still cannot be published")
            && error.contains("changed while it was being prepared"),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn a_bound_source_that_is_gone_sends_the_lesson_round_like_one_that_changed() -> Result<()> {
    let model = Model::new();
    let sources = vec![source(), source()];
    let lesson = staged(&model, "lesson", &sources).await?;

    let mut rounds = Rounds::default();

    assert!(rounds.again(&[lesson], &sources[1..], &model)?);
    Ok(())
}

#[test]
fn a_publication_that_finds_a_source_changed_is_tried_once_more_and_then_fails_as_it_found_it() {
    let found = || {
        AppError::InvalidInput(
            "A verified source is no longer active; verify this lesson again.".into(),
        )
    };
    let mut rounds = Rounds::default();

    assert!(rounds.publishing_failed(found()).is_ok());
    let error = rounds.publishing_failed(found()).unwrap_err();

    assert!(sources_changed(&error));
}

#[tokio::test]
async fn a_source_added_after_the_lesson_was_verified_does_not_stop_it_publishing() -> Result<()> {
    let course = Waiting::course().await?;
    let model = Model::new();
    let sources = course.references().await?;
    let (_, prepared) = staged(&model, &course.lesson_id, &sources).await?;
    // Research, or the user, saves another page.
    install_source(
        &course.pool,
        &course.program_id,
        "A page on another subject.",
    )
    .await?;

    let mut tx = course
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    persist_report(&mut tx, &course.program_id, &course.lesson_id, &prepared).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

#[tokio::test]
async fn a_source_the_report_is_bound_to_changing_still_stops_publication() -> Result<()> {
    let course = Waiting::course().await?;
    let model = Model::new();
    let sources = course.references().await?;
    let (_, mut prepared) = staged(&model, &course.lesson_id, &sources).await?;
    if let Some(report) = prepared.verification.as_mut() {
        report.sources[0].sha256.push('x');
    }

    let mut tx = course
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let error = persist_report(&mut tx, &course.program_id, &course.lesson_id, &prepared)
        .await
        .unwrap_err();

    assert!(sources_changed(&error), "{error}");
    Ok(())
}
