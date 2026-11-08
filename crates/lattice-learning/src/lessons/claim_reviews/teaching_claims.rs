//! The claims a teaching note's sentence stands on, so the user can take the
//! sentence out or add a source for it from the note. A waiting note names the
//! live claims whose sentence in the published section is exactly the note's;
//! a note resolved by removing the sentence names the claims that removal ended,
//! which taking it back needs. Draft lessons have none.

use std::collections::HashMap;

use lattice_core::domain::learning::claim_review::{Actor, ClaimLedgerEntry, ClaimState, Ledger};
use lattice_core::shared::error::Result;
use sqlx::{Row, SqlitePool};

use super::actions::in_section_body;
use super::published_edit::{gone, placed};
use super::teaching_findings::{
    TeachingFindingDto, TeachingFindingSiteDto, TeachingFindingStateDto, TeachingResolutionDto,
};
use crate::persistence::db;

/// The published sections' text, by section index.
pub(super) async fn section_bodies(
    pool: &SqlitePool,
    lesson_id: &str,
) -> Result<HashMap<usize, String>> {
    let rows = sqlx::query("SELECT ordinal, body FROM learning_blocks WHERE lesson_id=?")
        .bind(lesson_id)
        .fetch_all(pool)
        .await
        .map_err(db)?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            Some((
                usize::try_from(row.get::<i64, _>("ordinal")).ok()?,
                row.get::<String, _>("body"),
            ))
        })
        .collect())
}

/// Fills in the claim ids of each section note that is about a sentence.
pub(super) fn name_claims(
    ledger: &Ledger,
    bodies: &HashMap<usize, String>,
    findings: &mut [TeachingFindingDto],
) {
    for finding in findings {
        let (Some(sentence), Some(unit), TeachingFindingSiteDto::Section) = (
            finding.sentence.as_deref().map(str::trim),
            finding
                .unit_index
                .and_then(|unit| usize::try_from(unit).ok()),
            finding.site,
        ) else {
            continue;
        };
        let in_unit = |entry: &&ClaimLedgerEntry| {
            in_section_body(entry) && entry.location().unit_index == unit
        };
        let removed = finding.state == TeachingFindingStateDto::Resolved
            && finding.resolution == Some(TeachingResolutionDto::Removed);
        finding.claim_ids = match (finding.state, removed) {
            (TeachingFindingStateDto::Resolved, false) => Vec::new(),
            (_, true) => ledger
                .claims
                .iter()
                .filter(in_unit)
                .filter(|entry| {
                    matches!(
                        entry.state(),
                        ClaimState::Removed {
                            by: Actor::User,
                            ..
                        }
                    ) && entry.location().quote.trim() == sentence
                })
                .map(|entry| entry.claim_id().0.clone())
                .collect(),
            _ => {
                let Some(body) = bodies.get(&unit) else {
                    continue;
                };
                ledger
                    .claims
                    .iter()
                    .filter(in_unit)
                    .filter(|entry| !gone(entry))
                    .filter(|entry| {
                        placed(entry, body)
                            .and_then(|(start, end)| body.get(start..end))
                            .is_some_and(|text| text.trim() == sentence)
                    })
                    .map(|entry| entry.claim_id().0.clone())
                    .collect()
            }
        };
    }
}
