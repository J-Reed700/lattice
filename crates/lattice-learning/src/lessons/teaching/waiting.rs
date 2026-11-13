//! A lesson job's side of the teaching findings that wait for the user
//! (`claim_reviews::teaching_findings`). Outside a job (tests, the live
//! checks) nothing is stored.
use serde_json::Value;
use std::collections::BTreeSet;

use crate::lessons::claim_reviews::teaching_findings::TeachingFindingsRepository;
use crate::review_findings::{Held, ReviewFinding, SetAside};
use lattice_core::shared::error::Result;

fn repository() -> Option<(TeachingFindingsRepository, String, String)> {
    crate::lesson_drafts::scope().map(|scope| {
        (
            TeachingFindingsRepository::new(scope.database),
            scope.lesson_id,
            scope.job_id,
        )
    })
}

/// The findings the user already has for this text: waiting, or kept as
/// written, for parts that have not changed since.
pub(crate) async fn held(candidate: &Value) -> Result<Held> {
    match repository() {
        Some((store, lesson_id, _)) => store.held(&lesson_id, candidate).await,
        None => Ok(Held::default()),
    }
}

/// The text hashes of the questions whose findings wait for the user.
pub(crate) async fn question_texts() -> Result<BTreeSet<String>> {
    match repository() {
        Some((store, lesson_id, _)) => store.waiting_question_texts(&lesson_id).await,
        None => Ok(BTreeSet::new()),
    }
}

/// A completed review's outcome: what it set aside waits for the user, and a
/// finding it no longer raises was fixed and goes.
pub(super) async fn settle(candidate: &Value, set_aside: &[SetAside]) -> Result<()> {
    match repository() {
        Some((store, lesson_id, job_id)) => {
            store
                .record(&lesson_id, &job_id, candidate, set_aside, true)
                .await
        }
        None => Ok(()),
    }
}

/// Findings found once the review is over, on the lesson as it publishes;
/// the findings already waiting stay.
pub(crate) async fn record(candidate: &Value, findings: Vec<ReviewFinding>) -> Result<()> {
    let Some((store, lesson_id, job_id)) = repository() else {
        return Ok(());
    };
    if findings.is_empty() {
        return Ok(());
    }
    tracing::info!(
        findings = findings.len(),
        "Lesson rules the repairs did not settle wait for the user"
    );
    let set_aside: Vec<_> = findings
        .into_iter()
        .map(|finding| SetAside {
            finding,
            repairs: 0,
        })
        .collect();
    for aside in &set_aside {
        crate::lessons::trace::decided_finding(
            crate::lessons::trace::Stage::TeachingReview,
            &aside.finding,
            "waits for the user",
            "found once the review was over, on the lesson as it publishes",
        );
    }
    store
        .record(&lesson_id, &job_id, candidate, &set_aside, false)
        .await
}
