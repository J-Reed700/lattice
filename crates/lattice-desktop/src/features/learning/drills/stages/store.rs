//! Stage 6: the drill's key is checked against its code, and one transaction writes the drill and completes the job,
//! so a cancelled job stores nothing and a stored drill never leaves its job running.

use super::outputs::{Baseline, BlindReview, CleanCode, CleanReview, Evidence, Planted, Proved};
use super::DrillBuild;
use crate::features::learning::drills::repository::{self, NewDrill};
use crate::features::learning::drills::spec::DrillSpec;
use crate::features::learning::persistence::db;
use lattice_core::domain::learning::drills::{check_key, Drill, KeyFault};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobContext, JobOutcome, JobStore};

/// What the stages made.
pub(in crate::features::learning) struct Built<'a> {
    pub baseline: &'a Baseline,
    pub code: &'a CleanCode,
    pub review: &'a CleanReview,
    pub planted: &'a Planted,
    pub proved: &'a Proved,
    pub blind: &'a BlindReview,
}

impl DrillBuild<'_> {
    /// Stage 6. `check_key` must pass, then one transaction writes the drill and completes the job. A key that ends
    /// empty stores nothing and fails the job with the reason. `Stopped` when the job was cancelled meanwhile, with
    /// nothing stored.
    pub(in crate::features::learning) async fn store(
        &self,
        context: &JobContext,
        spec: &DrillSpec,
        built: Built<'_>,
    ) -> Result<JobOutcome> {
        let mut key = built.proved.planted_key();
        key.extend(built.blind.extras.iter().cloned());
        let drill = Drill {
            id: built.baseline.drill_id.clone(),
            material: spec.material.clone(),
            title: built.baseline.title.clone(),
            files: built.proved.files.clone(),
            key,
        };
        if let Err(fault) = check_key(&built.baseline.files, &drill) {
            return Err(AppError::InvalidData(refusal(&fault, built.proved)));
        }
        let evidence = Evidence::new(
            built.code,
            built.review,
            built.planted,
            built.proved,
            built.blind,
        );
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        repository::insert_in(
            &mut transaction,
            &NewDrill {
                drill: &drill,
                clean_files: &built.baseline.files,
                spec,
                domain: &built.baseline.domain,
                suite: built.baseline.suite.as_deref(),
                runner: self.material.runner().map(|runner| runner.name()),
                model: self.llm.model_name(),
                evidence: &evidence,
            },
        )
        .await?;
        let completed = JobStore::complete_in(
            &mut transaction,
            context.id(),
            &format!("drill:{}", drill.id),
            "The drill is ready",
        )
        .await?;
        if !completed {
            transaction.rollback().await.map_err(db)?;
            return Ok(JobOutcome::Stopped);
        }
        transaction.commit().await.map_err(db)?;
        Ok(JobOutcome::Committed)
    }
}

/// Why a key is refused, in words that name what was dropped when nothing was left.
fn refusal(fault: &KeyFault, proved: &Proved) -> String {
    match fault {
        KeyFault::EmptyKey if !proved.dropped.is_empty() => {
            let reasons: Vec<String> = proved
                .dropped
                .iter()
                .map(|dropped| format!("{}: {}", dropped.mutation.pattern, dropped.why))
                .collect();
            format!(
                "No planted defect could be proven, so the drill has no key. {}",
                reasons.join(" ")
            )
        }
        KeyFault::EmptyKey => "The drill has no key: no defect was planted.".to_owned(),
        other => format!("The drill's key does not agree with its code: {other:?}"),
    }
}
