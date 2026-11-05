//! `SourceChecked`: a planted error in an explanation is proven by its source passage. The clean sentences the edit
//! replaces must be supported by the passage, with the exact words they rest on, and the sentences it puts in their
//! place must be contradicted or not supported by it, and the checker must be sure of both. Anything else drops the
//! mutation, as an unproven defect of code is dropped. A proven mutation's severity is raised to the floor its pattern
//! and proof set.

use super::{SourceCheck, SourceSays};
use crate::features::learning::drills::build::Trail;
use crate::features::learning::drills::proof::{unproven, ProofCase, ProofStrategy, ProofVerdict};
use async_trait::async_trait;
use lattice_core::domain::learning::drills::{
    apply, severity_floor, DrillFile, Edit, Proof, SourceVerdict,
};
use lattice_core::shared::error::Result;

pub(super) struct SourceChecked<'a> {
    check: &'a dyn SourceCheck,
}

impl<'a> SourceChecked<'a> {
    pub(super) fn new(check: &'a dyn SourceCheck) -> Self {
        Self { check }
    }
}

#[async_trait]
impl ProofStrategy for SourceChecked<'_> {
    async fn prove(&self, case: &ProofCase<'_>) -> Result<ProofVerdict> {
        let mut trail = Trail::default();
        let Some(source) = case.source else {
            return Ok(unproven(
                "There is no source passage to check the change against.",
                trail,
            ));
        };
        let edit = &case.mutation.edit;
        let (Some(clean), Some(planted)) = (
            whole_lines(case.clean, &edit.path, &edit.before),
            whole_lines(case.mutated, &edit.path, &edit.after),
        ) else {
            return Ok(unproven(
                "The sentences the change touches are not in the explanation once.",
                trail,
            ));
        };
        let before = self.check.check(&clean, &source.text).await?;
        trail.call(
            "source check of the clean sentences",
            self.check.model_name(),
        );
        if !before.confident {
            return Ok(unproven(
                format!(
                    "The source check was not sure of the sentences the change replaces, so they are no clean baseline: {}",
                    before.reason
                ),
                trail,
            ));
        }
        let quote = match (before.says, before.quote) {
            (SourceSays::Supported, Some(quote)) => quote,
            (says, _) => {
                return Ok(unproven(
                    format!(
                        "The source passage {} the sentences the change replaces, so they are no clean baseline: {}",
                        says.says(),
                        before.reason
                    ),
                    trail,
                ))
            }
        };
        let after = self.check.check(&planted, &source.text).await?;
        trail.call(
            "source check of the planted sentences",
            self.check.model_name(),
        );
        if !after.confident {
            return Ok(unproven(
                format!(
                    "The source check was not sure of the changed sentences, so the change proves no error: {}",
                    after.reason
                ),
                trail,
            ));
        }
        let verdict = match after.says {
            SourceSays::Contradicted => SourceVerdict::Contradicted,
            SourceSays::Unsupported => SourceVerdict::Unsupported,
            SourceSays::Supported => {
                return Ok(unproven(
                    format!(
                        "The source passage supports the changed sentences too, so the change plants no error: {}",
                        after.reason
                    ),
                    trail,
                ))
            }
        };
        let proof = Proof::Sourced {
            quote,
            planted: verdict,
            reason: after.reason,
        };
        let mut mutation = case.mutation.clone();
        if let Some(floor) = severity_floor(case.pattern, &proof) {
            mutation.severity = mutation.severity.max(floor);
        }
        Ok(ProofVerdict::Proven {
            proof,
            mutation,
            trail,
        })
    }
}

/// The whole lines `text` stands on in the file at `path`, joined into one claim, when it is there once.
fn whole_lines(files: &[DrillFile], path: &str, text: &str) -> Option<String> {
    let identity = Edit {
        path: path.to_owned(),
        before: text.to_owned(),
        after: text.to_owned(),
    };
    let (_, spans) = apply(files, std::slice::from_ref(&identity)).ok()?;
    let span = spans.into_iter().next()?;
    let file = files.iter().find(|file| file.path == path)?;
    let first = usize::try_from(span.start).ok()?.checked_sub(1)?;
    let count = usize::try_from(span.end.checked_sub(span.start)?).ok()? + 1;
    let lines: Vec<&str> = file
        .text
        .lines()
        .skip(first)
        .take(count)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    (!lines.is_empty()).then(|| lines.join(" "))
}
