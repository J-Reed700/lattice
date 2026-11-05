//! Stage 4: each mutation goes through its proof strategy. A mutation that is not proven is dropped, with why. The
//! drill's files and spans are the baseline with only the proven edits applied.

use super::outputs::{Baseline, DroppedMutation, Planted, Proved, ProvenMutation, Trail};
use super::DrillBuild;
use crate::features::learning::drills::proof::{ProofCase, ProofVerdict, Tools};
use crate::features::learning::drills::spec::DrillSpec;
use lattice_core::domain::learning::drills::{apply, pattern, DrillFile, Edit};
use lattice_core::shared::error::{AppError, Result};

impl DrillBuild<'_> {
    /// Stage 4. Every mutation proven or dropped, and the drill's files and spans with the proven ones applied.
    pub(in crate::features::learning) async fn prove(
        &self,
        spec: &DrillSpec,
        baseline: &Baseline,
        planted: &Planted,
    ) -> Result<Proved> {
        let tools = Tools {
            llm: self.llm,
            runner: self.material.runner(),
            cancel: &self.cancel,
        };
        let mut proven: Vec<ProvenMutation> = Vec::new();
        let mut dropped: Vec<DroppedMutation> = Vec::new();
        for (position, mutation) in planted.mutations.iter().enumerate() {
            let Some(known) = pattern(&mutation.pattern) else {
                dropped.push(DroppedMutation {
                    mutation: mutation.clone(),
                    why: format!("{} is not a pattern of the catalogue.", mutation.pattern),
                    trail: Trail::default(),
                });
                continue;
            };
            let others: Vec<Edit> = planted
                .mutations
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != position)
                .map(|(_, other)| other.edit.clone())
                .collect();
            let (mutated, _) = apply(&baseline.files, std::slice::from_ref(&mutation.edit))
                .map_err(|fault| {
                    AppError::InvalidData(format!(
                        "The edit for {} no longer applies to the code: {fault:?}",
                        mutation.pattern
                    ))
                })?;
            let case = ProofCase {
                language: spec.material.language().unwrap_or_default(),
                clean: &baseline.files,
                mutation,
                mutated: &mutated,
                pattern: known,
                suite: baseline.suite.as_deref(),
                others: &others,
                source: baseline.source.as_ref(),
            };
            let proof_strategy = self.material.proof_strategy(known, mutation, tools);
            match proof_strategy.prove(&case).await? {
                ProofVerdict::Proven {
                    proof,
                    mutation,
                    trail,
                } => proven.push(ProvenMutation {
                    mutation,
                    proof,
                    trail,
                }),
                ProofVerdict::Unproven { why, trail } => dropped.push(DroppedMutation {
                    mutation: mutation.clone(),
                    why,
                    trail,
                }),
            }
        }
        settle(&baseline.files, proven, dropped)
    }
}

/// The drill's files and spans: the baseline with only the proven edits made. A proven mutation whose change cannot
/// be undone once the others are left out, because its changed text is then in the code twice, is dropped too.
fn settle(
    clean: &[DrillFile],
    mut proven: Vec<ProvenMutation>,
    mut dropped: Vec<DroppedMutation>,
) -> Result<Proved> {
    loop {
        let edits: Vec<Edit> = proven
            .iter()
            .map(|item| item.mutation.edit.clone())
            .collect();
        let (files, spans) = apply(clean, &edits).map_err(|fault| {
            AppError::InvalidData(format!(
                "The proven edits do not apply together to the code: {fault:?}"
            ))
        })?;
        let irreversible = proven.iter().position(|item| {
            let fix = item.mutation.fix();
            apply(&files, std::slice::from_ref(&fix)).is_err()
        });
        let Some(position) = irreversible else {
            return Ok(Proved {
                proven,
                dropped,
                files,
                spans,
            });
        };
        let gone = proven.remove(position);
        dropped.push(DroppedMutation {
            mutation: gone.mutation,
            why: "With the unproven changes left out, its changed text is in the code more than once, so the change cannot be undone with certainty.".into(),
            trail: gone.trail,
        });
    }
}
