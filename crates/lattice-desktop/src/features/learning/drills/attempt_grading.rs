//! Grading a submitted attempt: the judge's assessment of the findings against the key, the score and the ratings that
//! follow from it, and, when the learner wrote a fix, what came of the fix. One transaction stores the grade and
//! completes the job. A grader that gives no assessment leaves the attempt as one that could not be graded, with the
//! error kept; a stopped job leaves it submitted.

use super::attempts::{self, Attempt, Graded};
use super::fix_grading::{grade_fix, FixCase};
use super::grader::{Review, ReviewGrader};
use super::jobs::GRADING_STEPS;
use super::proof::Tools;
use super::repository::StoredDrill;
use super::runner::CodeRunner;
use crate::features::learning::persistence::db;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{ratings, score, validate};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobContext, JobOutcome, JobStore};
use sqlx::{Sqlite, SqlitePool, Transaction};
use tokio_util::sync::CancellationToken;

/// Grades attempts. The model, the runner, the grader and the database are the grading's own; the job runtime's context
/// carries the progress and the cancellation.
pub(in crate::features::learning) struct AttemptGrading<'a> {
    llm: &'a dyn LLMPort,
    runner: Option<&'a dyn CodeRunner>,
    grader: &'a dyn ReviewGrader,
    pool: &'a SqlitePool,
    cancel: CancellationToken,
}

impl<'a> AttemptGrading<'a> {
    /// Grading that runs the learner's fix on `runner`, or, without one, judges every issue of it.
    pub(in crate::features::learning) fn new(
        llm: &'a dyn LLMPort,
        runner: Option<&'a dyn CodeRunner>,
        grader: &'a dyn ReviewGrader,
        pool: &'a SqlitePool,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            llm,
            runner,
            grader,
            pool,
            cancel,
        }
    }

    /// Grades `attempt`, a submitted attempt at `stored`. `Committed` when the grade, or the reason there is none, is
    /// stored and the job completed with it; `Stopped` when the job was cancelled, with nothing stored.
    pub(in crate::features::learning) async fn run(
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
                };
                let tools = Tools {
                    llm: self.llm,
                    runner: self.runner,
                    cancel: &self.cancel,
                };
                let Some(graded) = context.until_cancelled(grade_fix(tools, &case)).await else {
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
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let recorded = attempts::record_graded_in(&mut transaction, &attempt.id, &graded).await?;
        finish(
            transaction,
            context,
            recorded,
            &attempt.id,
            "Your review is graded",
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
            self.pool,
            context,
            &attempt.id,
            Some(self.llm.model_name()),
            error,
        )
        .await
    }
}

/// Stores that the attempt could not be graded, with why and the model that was tried, when one was reached, and
/// completes the job with it: the learner reads "Couldn't grade yet" and may ask again. `Stopped` when the job was
/// cancelled, which is no reason to give up on the review.
pub(in crate::features::learning) async fn give_up(
    pool: &SqlitePool,
    context: &JobContext,
    attempt_id: &str,
    model: Option<&str>,
    error: &AppError,
) -> Result<JobOutcome> {
    if context.is_cancelled() {
        return Ok(JobOutcome::Stopped);
    }
    tracing::warn!(attempt = attempt_id, %error, "A drill attempt could not be graded");
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    let recorded =
        attempts::record_ungraded_in(&mut transaction, attempt_id, model, &error.to_string())
            .await?;
    finish(
        transaction,
        context,
        recorded,
        attempt_id,
        "The review could not be graded",
    )
    .await
}

/// Completes the job in the transaction that recorded the attempt's state. A state that was not recorded because the
/// attempt was no longer waiting for a grade is left as it is, and the job completes without it.
async fn finish(
    mut transaction: Transaction<'static, Sqlite>,
    context: &JobContext,
    recorded: bool,
    attempt_id: &str,
    message: &str,
) -> Result<JobOutcome> {
    let result_ref = format!("attempt:{attempt_id}");
    if !recorded {
        transaction.rollback().await.map_err(db)?;
        return Ok(JobOutcome::Completed {
            result_ref,
            message: "The review was no longer waiting for a grade.".into(),
        });
    }
    let completed =
        JobStore::complete_in(&mut transaction, context.id(), &result_ref, message).await?;
    if !completed {
        transaction.rollback().await.map_err(db)?;
        return Ok(JobOutcome::Stopped);
    }
    transaction.commit().await.map_err(db)?;
    Ok(JobOutcome::Committed)
}

/// Saves the job's progress at step `index` of [`GRADING_STEPS`]. False once the job is no longer running.
async fn step(context: &JobContext, index: u32, what: &str) -> Result<bool> {
    let label = format!("{what} ({} of {GRADING_STEPS})", index + 1);
    context
        .progress(index.max(context.job().progress_current), &label, None)
        .await
}
