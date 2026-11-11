//! A learner's attempt at a drill (docs/design/2026-11-04-code-review-drills.md sections 1 and 4). It opens when a drill
//! is taken from the bank, is submitted with the learner's findings and, when they wrote one, their fix, and is graded
//! by the `learning.drill_grading` job. The grade, the model that gave it and the reason it could not be given are in
//! the attempt's row.

use super::bank::{self, Shelf};
use super::build::ask::normalise;
use super::jobs::DRILL_GRADING;
use super::repository::{self as drill_repository, StoredDrill};
use crate::persistence::db;
use lattice_core::domain::learning::drills::{
    Assessment, DrillFile, Finding, IssueFix, IssueRating, Score,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, Transactions};
use serde::{Deserialize, Serialize};

pub(crate) mod repository;
pub(crate) use repository::recent_in;

/// How a drill is practised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AttemptMode {
    Practice,
    Interview,
    Rapid,
}

impl AttemptMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Practice => "practice",
            Self::Interview => "interview",
            Self::Rapid => "rapid",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "practice" => Ok(Self::Practice),
            "interview" => Ok(Self::Interview),
            "rapid" => Ok(Self::Rapid),
            other => Err(AppError::Database(format!("Unknown attempt mode {other}"))),
        }
    }
}

/// Where an attempt stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    /// The learner is reviewing.
    Open,
    /// Handed in, waiting for its grade.
    Submitted,
    Graded,
    /// The grader gave no assessment; a regrade tries again.
    CouldntGrade,
}

impl AttemptStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Submitted => "submitted",
            Self::Graded => "graded",
            Self::CouldntGrade => "couldnt_grade",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "open" => Ok(Self::Open),
            "submitted" => Ok(Self::Submitted),
            "graded" => Ok(Self::Graded),
            "couldnt_grade" => Ok(Self::CouldntGrade),
            other => Err(AppError::Database(format!(
                "Unknown attempt status {other}"
            ))),
        }
    }
}

/// An attempt as its row keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    pub id: String,
    pub drill_id: String,
    pub mode: AttemptMode,
    pub started_at: i64,
    pub submitted_at: Option<i64>,
    /// As the client measured it.
    pub time_spent_ms: Option<i64>,
    pub findings: Option<Vec<Finding>>,
    /// The learner's files when they wrote a fix: the drill's files with theirs in place.
    pub fix_files: Option<Vec<DrillFile>>,
    pub status: AttemptStatus,
    pub assessment: Option<Assessment>,
    pub score: Option<Score>,
    pub ratings: Option<Vec<IssueRating>>,
    /// What came of the fix for each key issue; kept only with a fix.
    pub fix: Option<Vec<IssueFix>>,
    /// The model that graded, or tried to, as the server names it.
    pub grader_model: Option<String>,
    pub graded_at: Option<i64>,
    /// Why the grader gave no assessment.
    pub error: Option<String>,
    /// How many times a grade was asked for again.
    pub regrades: u32,
}

/// What a learner hands in.
#[derive(Debug, Clone, PartialEq)]
pub struct Submission {
    pub findings: Vec<Finding>,
    /// The files the learner changed or added, when they wrote a fix; the drill's files stand for the rest.
    pub fix_files: Option<Vec<DrillFile>>,
    pub time_spent_ms: i64,
}

/// An attempt and the grading job waiting for it, or none when the attempt is no longer waiting.
#[derive(Debug, Clone, PartialEq)]
pub struct Queued {
    pub attempt: Attempt,
    /// The caller tells the job runtime once the job is committed.
    pub job: Option<String>,
}

/// An attempt just opened, the drill it is on, and the build queued to follow that drill, when one was.
#[derive(Debug, Clone, PartialEq)]
pub struct Begun {
    pub attempt: Attempt,
    pub drill: StoredDrill,
    /// The build queued to follow it, when one was; the caller tells the job runtime once the job is committed.
    pub next_build: Option<String>,
}

/// Takes the oldest ready drill of `shelf` built from a lesson, from `lesson` when one is given, and opens an attempt
/// on it, in one transaction. `None` when no such drill is ready.
pub async fn start(
    database: &Database,
    shelf: Shelf<'_>,
    lesson: Option<&str>,
    mode: AttemptMode,
) -> Result<Option<Begun>> {
    let mut transaction = database.begin_write().await.map_err(db)?;
    let Some(taken) = bank::take_in(&mut transaction, shelf, lesson).await? else {
        transaction.commit().await.map_err(db)?;
        return Ok(None);
    };
    let attempt = repository::open_in(&mut transaction, &taken.drill.drill.id, mode).await?;
    transaction.commit().await.map_err(db)?;
    Ok(Some(Begun {
        attempt,
        drill: taken.drill,
        next_build: taken.next_build,
    }))
}

/// Hands in an open attempt: its findings and fix are stored, and the job that grades it is queued, in one
/// transaction. Asking again with what was already handed in finds the same job; asking with anything else is refused.
/// A finding is refused only when it has no text or its lines are none of the drill's, and a fix that leaves every
/// file of the drill as it was is no fix.
pub async fn submit(database: &Database, id: &str, submission: Submission) -> Result<Queued> {
    let mut transaction = database.begin_write().await.map_err(db)?;
    let attempt = repository::find_in(&mut transaction, id)
        .await?
        .ok_or_else(not_found)?;
    let drill = drill_repository::find_in(&mut transaction, &attempt.drill_id)
        .await?
        .ok_or_else(|| AppError::InternalError("The attempt's drill is not there.".into()))?;
    let submission = checked(submission, &drill.drill.files)?;
    let queued = match attempt.status {
        AttemptStatus::Open => {
            repository::submit_in(&mut transaction, id, &submission).await?;
            let attempt = repository::find_in(&mut transaction, id)
                .await?
                .ok_or_else(not_found)?;
            let job = repository::queue_grading_in(&mut transaction, &attempt).await?;
            Queued {
                attempt,
                job: Some(job),
            }
        }
        _ if attempt.findings.as_ref() == Some(&submission.findings)
            && attempt.fix_files == submission.fix_files =>
        {
            let job = match attempt.status {
                AttemptStatus::Submitted => {
                    Some(repository::queue_grading_in(&mut transaction, &attempt).await?)
                }
                _ => None,
            };
            Queued { attempt, job }
        }
        _ => {
            return Err(AppError::InvalidState(
                "This review was already handed in.".into(),
            ))
        }
    };
    transaction.commit().await.map_err(db)?;
    Ok(queued)
}

/// Asks for the grade of an attempt that could not be graded again: it waits for a new grading job, and the error of
/// the last try is cleared. Asking while it already waits finds that job. An attempt that is open or graded is refused.
pub async fn regrade(database: &Database, id: &str) -> Result<Queued> {
    let mut transaction = database.begin_write().await.map_err(db)?;
    let attempt = repository::find_in(&mut transaction, id)
        .await?
        .ok_or_else(not_found)?;
    let attempt = match attempt.status {
        AttemptStatus::CouldntGrade => {
            repository::regrade_in(&mut transaction, id).await?;
            repository::find_in(&mut transaction, id)
                .await?
                .ok_or_else(not_found)?
        }
        AttemptStatus::Submitted => attempt,
        AttemptStatus::Open | AttemptStatus::Graded => {
            return Err(AppError::InvalidState(
                "Only a review that could not be graded can be graded again.".into(),
            ))
        }
    };
    let job = repository::queue_grading_in(&mut transaction, &attempt).await?;
    transaction.commit().await.map_err(db)?;
    Ok(Queued {
        attempt,
        job: Some(job),
    })
}

/// The attempt, and the drill it is on.
pub async fn get(database: &Database, id: &str) -> Result<(Attempt, StoredDrill)> {
    let mut tx = database.begin_read().await.map_err(db)?;
    let attempt = repository::find_in(&mut tx, id)
        .await?
        .ok_or_else(not_found)?;
    let drill = drill_repository::find_in(&mut tx, &attempt.drill_id)
        .await?
        .ok_or_else(|| AppError::InternalError("The attempt's drill is not there.".into()))?;
    tx.commit().await.map_err(db)?;
    Ok((attempt, drill))
}

/// The attempt, and the drill it is on, when the attempt is submitted and waits for its grade. `None` when it is gone
/// or in any other state.
pub(crate) async fn waiting(
    database: &Database,
    id: &str,
) -> Result<Option<(Attempt, StoredDrill)>> {
    match get(database, id).await {
        Ok((attempt, drill)) if attempt.status == AttemptStatus::Submitted => {
            Ok(Some((attempt, drill)))
        }
        Ok(_) | Err(AppError::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// What a grade writes into the attempt's row.
pub(crate) struct Graded<'a> {
    pub assessment: &'a Assessment,
    pub score: &'a Score,
    pub ratings: &'a [IssueRating],
    /// Kept exactly when the attempt has a fix.
    pub fix: Option<&'a [IssueFix]>,
    /// The model that graded, as the server names it.
    pub model: &'a str,
}

/// The operation ID of the job that grades an attempt after it has been asked for again `regrades` times.
pub(crate) fn operation_id(attempt: &str, regrades: u32) -> String {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_OID,
        format!("{DRILL_GRADING}:{attempt}:{regrades}").as_bytes(),
    )
    .to_string()
}

/// The submission with its fix as the learner's files, or the reason it is refused.
fn checked(mut submission: Submission, drill: &[DrillFile]) -> Result<Submission> {
    if submission.time_spent_ms < 0 {
        return Err(AppError::InvalidInput(
            "The time spent cannot be negative.".into(),
        ));
    }
    check_findings(&submission.findings, drill)?;
    submission.fix_files = submission
        .fix_files
        .map(|sent| learner_files(drill, sent))
        .filter(|files| files.as_slice() != drill);
    Ok(submission)
}

/// The drill's files with the files the learner sent in place of theirs, by path, and the ones the drill has no file
/// for after them. Line endings are the ones of a drill's files.
fn learner_files(drill: &[DrillFile], sent: Vec<DrillFile>) -> Vec<DrillFile> {
    let mut files = drill.to_vec();
    for file in sent {
        let file = DrillFile {
            text: normalise(&file.text),
            path: file.path,
        };
        match files.iter_mut().find(|kept| kept.path == file.path) {
            Some(kept) => *kept = file,
            None => files.push(file),
        }
    }
    files
}

/// Refuses a finding with no text, and one whose lines are none of the drill's.
fn check_findings(findings: &[Finding], drill: &[DrillFile]) -> Result<()> {
    for (position, finding) in findings.iter().enumerate() {
        let number = position + 1;
        if finding.text.trim().is_empty() {
            return Err(AppError::InvalidInput(format!(
                "Finding {number} has no text."
            )));
        }
        let Some(span) = &finding.span else {
            continue;
        };
        let Some(file) = drill.iter().find(|file| file.path == span.path) else {
            return Err(AppError::InvalidInput(format!(
                "Finding {number} is on {}, which is not a file of the drill.",
                span.path
            )));
        };
        let lines = file.text.lines().count();
        let names_a_line = span.start >= 1
            && span.start <= span.end
            && usize::try_from(span.start).is_ok_and(|start| start <= lines);
        if !names_a_line {
            return Err(AppError::InvalidInput(format!(
                "Finding {number} is on lines {} to {} of {}, which has {lines} lines.",
                span.start, span.end, span.path
            )));
        }
    }
    Ok(())
}

fn not_found() -> AppError {
    AppError::NotFound("This review is not there.".into())
}
