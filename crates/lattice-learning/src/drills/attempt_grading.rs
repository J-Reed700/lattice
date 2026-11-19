//! Grading a submitted attempt: the judge's assessment of the findings against the key, the score and the ratings that
//! follow from it, and, when the learner wrote a fix, what came of the fix. One transaction stores the grade and
//! completes the job. A grader that gives no assessment leaves the attempt as one that could not be graded, with the
//! error kept; a stopped job leaves it submitted.

use super::attempts::{self, Attempt, Graded};
use super::fix_grading::{grade_fix, FixCase};
use super::grader::{Review, ReviewGrader};
use super::jobs::GRADING_STEPS;
use super::material::DrillMaterial;
use super::proof::Tools;
use super::repository::StoredDrill;
use super::schedules;
use crate::persistence::now;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{
    lesson_rating, ratings, score, validate, IssueRating, PatternRating,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, WriteTx};
use lattice_core::shared::runtime::jobs::{JobContext, JobOutcome, JobStore};
use tokio_util::sync::CancellationToken;

/// Grades attempts. The model, the material, the grader and the database are the grading's own; the job runtime's
/// context carries the progress and the cancellation.
pub(crate) struct AttemptGrading<'a> {
    llm: &'a dyn LLMPort,
    material: Box<dyn DrillMaterial + 'a>,
    grader: &'a dyn ReviewGrader,
    database: Database,
    cancel: CancellationToken,
}

impl<'a> AttemptGrading<'a> {
    /// Grading of attempts at drills of `material`.
    pub(crate) fn new(
        llm: &'a dyn LLMPort,
        material: Box<dyn DrillMaterial + 'a>,
        grader: &'a dyn ReviewGrader,
        database: &Database,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            llm,
            material,
            grader,
            database: database.clone(),
            cancel,
        }
    }

    /// Grades `attempt`, a submitted attempt at `stored`. `Committed` when the grade, or the reason there is none, is
    /// stored and the job completed with it; `Stopped` when the job was cancelled, with nothing stored.
    pub(crate) async fn run(
        &self,
        context: &JobContext,
        attempt: &Attempt,
        stored: &StoredDrill,
    ) -> Result<JobOutcome> {
        let findings = attempt.findings.as_deref().unwrap_or_default();
        let key = &stored.drill.key;
        if !step(context, 0, "Grading your review").await? {
            return Ok(JobOutcome::Stopped);
        }
        let review = Review {
            drill: &stored.drill,
            clean: &stored.clean_files,
            source: stored.source.as_ref(),
            findings,
        };
        let Some(assessed) = context.until_cancelled(self.grader.assess(&review)).await else {
            return Ok(JobOutcome::Stopped);
        };
        let assessment = match assessed {
            Ok(assessment) => assessment,
            Err(error) => return self.give_up(context, attempt, &error).await,
        };
        if let Err(fault) = validate(key.len(), findings.len(), &assessment) {
            let error = AppError::ModelAnswerUnusable(format!(
                "The assessment of the review contradicts itself: {fault:?}"
            ));
            return self.give_up(context, attempt, &error).await;
        }
        let scored = score(key, findings, &assessment);
        let rated = ratings(key, &assessment);
        let fix = match attempt.fix_files.as_deref() {
            None => None,
            Some(learner) => {
                if !step(context, 1, "Checking your fix").await? {
                    return Ok(JobOutcome::Stopped);
                }
                let case = FixCase {
                    drill: &stored.drill,
                    learner,
                    suite: stored.suite.as_deref(),
                    source: stored.source.as_ref(),
                    assessment: &assessment,
                };
                let tools = Tools {
                    llm: self.llm,
                    runner: self.material.runner(),
                    cancel: &self.cancel,
                };
                let Some(graded) = context
                    .until_cancelled(grade_fix(self.material.as_ref(), tools, &case))
                    .await
                else {
                    return Ok(JobOutcome::Stopped);
                };
                match graded {
                    Ok(fix) => Some(fix),
                    Err(error) => return self.give_up(context, attempt, &error).await,
                }
            }
        };
        if !step(context, 2, "Saving the grade").await? {
            return Ok(JobOutcome::Stopped);
        }
        let graded = Graded {
            assessment: &assessment,
            score: &scored,
            ratings: &rated,
            fix: fix.as_deref(),
            model: self.llm.model_name(),
        };
        let lesson = lesson_rating(&stored.drill.material, key, &assessment);
        save_grade(
            &self.database,
            context,
            attempt,
            stored,
            &graded,
            &rated,
            lesson,
        )
        .await
    }

    async fn give_up(
        &self,
        context: &JobContext,
        attempt: &Attempt,
        error: &AppError,
    ) -> Result<JobOutcome> {
        give_up(
            &self.database,
            context,
            &attempt.id,
            Some(self.llm.model_name()),
            error,
        )
        .await
    }
}

/// Stores the grade of `attempt`, a submitted attempt at `stored`, and the schedules it moves, and completes the job,
/// in one transaction.
async fn save_grade(
    database: &Database,
    context: &JobContext,
    attempt: &Attempt,
    stored: &StoredDrill,
    graded: &Graded<'_>,
    rated: &[IssueRating],
    lesson: Option<PatternRating>,
) -> Result<JobOutcome> {
    let mut transaction = database.begin_write().await?;
    let recorded =
        attempts::repository::record_graded_in(&mut transaction, &attempt.id, graded).await?;
    if recorded {
        schedules::record_in(&mut transaction, stored, rated, lesson, now()).await?;
    }
    let finished = finish_in(
        &mut transaction,
        context,
        recorded,
        &attempt.id,
        "Your review is graded",
    )
    .await?;
    match finished {
        Finish::Commit => {
            transaction.commit().await?;
            Ok(JobOutcome::Committed)
        }
        Finish::Leave(outcome) => {
            transaction.rollback().await?;
            Ok(outcome)
        }
    }
}

/// Stores that the attempt could not be graded, with why and the model that was tried, when one was reached, and
/// completes the job with it: the learner reads "Couldn't grade yet" and may ask again. `Stopped` when the job was
/// cancelled, which is no reason to give up on the review.
pub(crate) async fn give_up(
    database: &Database,
    context: &JobContext,
    attempt_id: &str,
    model: Option<&str>,
    error: &AppError,
) -> Result<JobOutcome> {
    if context.is_cancelled() {
        return Ok(JobOutcome::Stopped);
    }
    tracing::warn!(attempt = attempt_id, %error, "A drill attempt could not be graded");
    let mut transaction = database.begin_write().await?;
    let recorded = attempts::repository::record_ungraded_in(
        &mut transaction,
        attempt_id,
        model,
        &error.to_string(),
    )
    .await?;
    let finished = finish_in(
        &mut transaction,
        context,
        recorded,
        attempt_id,
        "The review could not be graded",
    )
    .await?;
    match finished {
        Finish::Commit => {
            transaction.commit().await?;
            Ok(JobOutcome::Committed)
        }
        Finish::Leave(outcome) => {
            transaction.rollback().await?;
            Ok(outcome)
        }
    }
}

/// What the caller does with the transaction that recorded the attempt's state.
enum Finish {
    /// The job is completed in it: commit.
    Commit,
    /// Roll it back and end the job with this outcome.
    Leave(JobOutcome),
}

/// Completes the job in the transaction that recorded the attempt's state. A state that was not recorded because the
/// attempt was no longer waiting for a grade is left as it is, and the job completes without it.
async fn finish_in(
    transaction: &mut WriteTx,
    context: &JobContext,
    recorded: bool,
    attempt_id: &str,
    message: &str,
) -> Result<Finish> {
    let result_ref = format!("attempt:{attempt_id}");
    if !recorded {
        return Ok(Finish::Leave(JobOutcome::Completed {
            result_ref,
            message: "The review was no longer waiting for a grade.".into(),
        }));
    }
    let completed = JobStore::complete_in(transaction, context.id(), &result_ref, message).await?;
    if !completed {
        return Ok(Finish::Leave(JobOutcome::Stopped));
    }
    Ok(Finish::Commit)
}

/// Saves the job's progress at step `index` of [`GRADING_STEPS`]. False once the job is no longer running.
async fn step(context: &JobContext, index: u32, what: &str) -> Result<bool> {
    let label = format!("{what} ({} of {GRADING_STEPS})", index + 1);
    context
        .progress(index.max(context.job().progress_current), &label, None)
        .await
}
