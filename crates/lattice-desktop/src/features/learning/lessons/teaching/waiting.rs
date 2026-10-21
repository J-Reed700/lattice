//! A lesson job's side of the teaching findings that wait for the user
//! (`claim_reviews::teaching_findings`). Outside a job (tests, the live
//! checks) nothing is stored.
use std::collections::BTreeSet;

use serde_json::Value;

use crate::features::learning::lessons::claim_reviews::teaching_findings::TeachingFindingsRepository;
use crate::features::learning::review_findings::{FindingKey, ReviewFinding, SetAside};
use lattice_core::shared::error::Result;

fn repository() -> Option<(TeachingFindingsRepository, String, String)> {
    crate::features::learning::lesson_drafts::scope().map(|scope| {
        (
            TeachingFindingsRepository::new(scope.pool),
            scope.lesson_id,
            scope.job_id,
        )
    })
}

/// The findings the user already has for this text: waiting, or kept as
/// written, for parts that have not changed since.
pub(super) async fn held(candidate: &Value) -> Result<BTreeSet<FindingKey>> {
    match repository() {
        Some((store, lesson_id, _)) => store.held(&lesson_id, candidate).await,
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
pub(in crate::features::learning) async fn record(
    candidate: &Value,
    findings: Vec<ReviewFinding>,
) -> Result<()> {
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
    store
        .record(&lesson_id, &job_id, candidate, &set_aside, false)
        .await
}
