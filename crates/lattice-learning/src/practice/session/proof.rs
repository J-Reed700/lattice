//! The proof of a drafted exercise, run before it is shown, the same for every kind. The reference must be right;
//! each misreading must fail a case and run; the form derives the start by code; the task has one right answer.

use crate::persistence::hash;
use crate::practice::session::kind::{Established, Kind};
use lattice_core::domain::learning::practice::work::{
    CaseStatus, Draft, Exercise, Form, Misreading, Piece,
};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

/// What a proof found.
#[derive(Debug, Clone, PartialEq)]
pub enum Proof {
    Proven(Exercise),
    /// Why the draft cannot stand, written for the model that drafted it.
    Unproven(String),
}

/// Proves a draft. A draft that does not hold is `Proof::Unproven`; a form the kind does not support, a missing
/// starter or a cancelled build is an error.
pub async fn prove(kind: &Kind, draft: Draft, cancel: CancellationToken) -> Result<Proof> {
    if !kind.supports(draft.form) {
        return Err(AppError::InvalidState(format!(
            "The kind {} does not support the form {:?}.",
            kind.id.as_str(),
            draft.form
        )));
    }

    ensure_running(&cancel)?;
    let (cases, proof) = match kind.truth.establish(&draft, cancel.clone()).await? {
        Established::Right { cases, proof } => (cases, proof),
        Established::Wrong(reason) => return Ok(Proof::Unproven(reason)),
    };

    let mut kept = Vec::new();
    let mut notes = Vec::new();
    for misreading in &draft.misreadings {
        ensure_running(&cancel)?;
        let report = kind
            .checker
            .check(&draft.brief, &misreading.piece, &cases, cancel.clone())
            .await?;
        let killed_by: Vec<String> = report
            .results
            .iter()
            .filter(|result| {
                matches!(
                    result.status,
                    CaseStatus::Failed | CaseStatus::Error | CaseStatus::TimedOut
                )
            })
            .map(|result| result.case_id.clone())
            .collect();
        let never_runs = !report.results.is_empty()
            && report
                .results
                .iter()
                .all(|result| result.status == CaseStatus::Error);
        if killed_by.is_empty() {
            notes.push(format!(
                "Misreading {} passes every case, so nothing tells it from the reference.",
                misreading.id
            ));
        } else if never_runs {
            notes.push(format!(
                "Misreading {} raises on every case, so it is not a plausible reading.",
                misreading.id
            ));
        } else {
            kept.push(Misreading {
                killed_by,
                ..misreading.clone()
            });
        }
    }

    let needed = kind.misreadings_needed.max(draft.form.misreadings_needed());
    if kept.len() < needed {
        return Ok(Proof::Unproven(format!(
            "Only {} of the {} misreadings fail a case and run; {needed} are needed. {} Write misreadings that are plausible wrong readings of the statement, and add cases that tell each one from the reference.",
            kept.len(),
            draft.misreadings.len(),
            notes.join(" ")
        )));
    }

    let (start, planted) = match start_of(kind, &draft, &kept)? {
        Start::Piece(start, planted) => (start, planted),
        Start::Unproven(reason) => return Ok(Proof::Unproven(reason)),
    };

    let exercise = Exercise {
        kind: kind.id.clone(),
        form: draft.form,
        brief: draft.brief,
        reference: draft.reference,
        misreadings: kept,
        cases,
        start,
        planted,
        proof,
    };

    ensure_running(&cancel)?;
    if let Some(reason) = kind.truth.unambiguous(&exercise, cancel).await? {
        return Ok(Proof::Unproven(reason));
    }
    Ok(Proof::Proven(exercise))
}

enum Start {
    Piece(Piece, Option<String>),
    Unproven(String),
}

/// The starting piece the form derives, and for Repair the misreading it is.
fn start_of(kind: &Kind, draft: &Draft, kept: &[Misreading]) -> Result<Start> {
    match draft.form {
        Form::Build => {
            let starter = draft.starter.clone().ok_or_else(|| {
                AppError::InvalidState("A Build draft carries no starting piece.".into())
            })?;
            Ok(Start::Piece(starter, None))
        }
        Form::Repair => {
            let mut most: Option<&Misreading> = None;
            for misreading in kept {
                match most {
                    Some(best) if best.killed_by.len() >= misreading.killed_by.len() => {}
                    _ => most = Some(misreading),
                }
            }
            Ok(match most {
                Some(planted) => Start::Piece(planted.piece.clone(), Some(planted.id.clone())),
                None => Start::Unproven("No misreading is left to start a repair from.".into()),
            })
        }
        Form::Complete => {
            let gaps = kind.gaps.as_ref().ok_or_else(|| missing_role("gaps"))?;
            if draft.asks.is_empty() {
                return Ok(Start::Unproven(
                    "The draft names no part to empty. Name the parts the learner is to fill in."
                        .into(),
                ));
            }
            let start = gaps.emptied(&draft.brief, &draft.reference, &draft.asks)?;
            Ok(Start::Piece(start, None))
        }
        Form::Arrange => {
            let order = kind.order.as_ref().ok_or_else(|| missing_role("order"))?;
            let seed = seed_of(&draft.reference)?;
            let start = order.shuffled(&draft.brief, &draft.reference, seed)?;
            Ok(Start::Piece(start, None))
        }
    }
}

fn missing_role(role: &str) -> AppError {
    AppError::InvalidState(format!("The kind has no {role} role."))
}

/// A seed that depends only on the reference: the first eight bytes of its hash.
fn seed_of(reference: &Piece) -> Result<u64> {
    let digest = hash(reference)?;
    let head = digest
        .get(..16)
        .ok_or_else(|| AppError::InvalidState("The reference hash is too short.".into()))?;
    u64::from_str_radix(head, 16)
        .map_err(|error| AppError::InvalidState(format!("The reference hash is not hex: {error}")))
}

fn ensure_running(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    Ok(())
}

fn cancelled() -> AppError {
    AppError::InvalidState("The practice build was cancelled.".into())
}
