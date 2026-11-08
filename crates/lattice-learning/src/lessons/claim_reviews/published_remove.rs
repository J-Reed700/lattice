//! The user's removal of a statement in a published lesson: its sentence goes
//! out of the section's text with the whitespace around it, every claim that
//! stood on that sentence ends removed by the user, and the stored report
//! follows. [`remove_in`] runs inside the caller's transaction, which owns the
//! revision check and its advance, so a refusal writes nothing.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{
    ClaimEvent, ClaimId, ClaimLedgerEntry, EventProvenance, UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

use super::actions::{in_section_body, ONLY_SECTION};
use super::published_edit::{
    gone, placed, section_body, update_report, write_section_body, CHANGED,
};
use super::repository::{append_in, load_in};
use crate::content_verification::TakenOut;

/// `body` with the sentence at `range` taken out, and the whitespace around
/// it: no double space, no empty list item, no extra blank line is left.
pub(super) fn without_sentence(body: &str, range: (usize, usize)) -> String {
    let (Some(before), Some(after)) = (body.get(..range.0), body.get(range.1..)) else {
        return body.to_owned();
    };
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    let prefix = before.get(line_start..).unwrap_or_default();
    let (suffix, rest) = match after.find('\n') {
        Some(at) => (
            after.get(..at).unwrap_or_default(),
            after.get(at + 1..).unwrap_or_default(),
        ),
        None => (after, ""),
    };
    if suffix.trim().is_empty() && only_marker(prefix) {
        let head = before.get(..line_start).unwrap_or_default();
        return without_line(head, rest);
    }
    let kept_before = before.trim_end();
    let kept_after = after.trim_start();
    let leading = before.get(kept_before.len()..).unwrap_or_default();
    let trailing = after
        .get(..after.len().saturating_sub(kept_after.len()))
        .unwrap_or_default();
    if kept_before.is_empty() {
        return kept_after.to_owned();
    }
    if kept_after.is_empty() {
        return kept_before.to_owned();
    }
    let separator = if trailing.contains('\n') && !leading.contains('\n') {
        trailing
    } else {
        leading
    };
    format!("{kept_before}{separator}{kept_after}")
}

/// Whether a line's text before the sentence is only the marker of a list
/// item, quotation or heading, or nothing.
fn only_marker(prefix: &str) -> bool {
    prefix
        .trim_matches(|c: char| {
            c.is_whitespace()
                || matches!(c, '-' | '*' | '+' | '>' | '#' | '.' | ')')
                || c.is_ascii_digit()
        })
        .is_empty()
}

/// The text on either side of a line that goes whole: `head` before it and
/// `rest` after its line break. A blank line is kept between paragraphs, not
/// doubled, and none is left at either end.
fn without_line(head: &str, rest: &str) -> String {
    if rest.is_empty() {
        return head.trim_end_matches(['\r', '\n']).to_owned();
    }
    let lead = rest
        .len()
        .saturating_sub(rest.trim_start_matches(['\r', '\n']).len());
    let blank_after = rest.get(..lead).is_some_and(|run| run.contains('\n'));
    let blank_before = head.is_empty()
        || head
            .get(head.trim_end_matches(['\r', '\n']).len()..)
            .is_some_and(|run| run.matches('\n').count() >= 2);
    let rest = if blank_before && blank_after {
        rest.trim_start_matches(['\r', '\n'])
    } else {
        rest
    };
    format!("{head}{rest}")
}

/// Writes the removal on `connection`: the claim's sentence out of its
/// section, every claim on it ended removed by `user`, the report updated.
/// The claims it ended are the ones it returns.
pub(super) async fn remove_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    claim_id: &ClaimId,
    user: &EventProvenance,
) -> Result<HashSet<ClaimId>> {
    let ledger = load_in(connection, lesson_id).await?;
    let review = ledger
        .review
        .as_ref()
        .ok_or_else(|| AppError::NotFound("This lesson has no claim review.".into()))?;
    let target = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == claim_id)
        .ok_or_else(|| AppError::NotFound("No claim with that id in this lesson.".into()))?;
    if !in_section_body(target) {
        return Err(AppError::InvalidInput(ONLY_SECTION.into()));
    }
    let unit = target.location().unit_index;
    let body = section_body(connection, lesson_id, unit).await?;
    let range = placed(target, &body).ok_or_else(|| AppError::InvalidInput(CHANGED.into()))?;
    let removed: Vec<&ClaimLedgerEntry> = ledger
        .claims
        .iter()
        .filter(|entry| {
            entry.claim_id() == claim_id
                || (in_section_body(entry)
                    && entry.location().unit_index == unit
                    && !gone(entry)
                    && placed(entry, &body)
                        .is_some_and(|(start, end)| start < range.1 && range.0 < end))
        })
        .collect();
    let new_body = without_sentence(&body, range);
    for entry in &removed {
        let event = ClaimEvent::UserAction {
            action: UserAction::Remove,
            note: None,
        };
        append_in(connection, lesson_id, entry.claim_id(), user.clone(), event).await?;
    }
    write_section_body(connection, lesson_id, unit, &new_body).await?;
    let ids: HashSet<String> = removed
        .iter()
        .map(|entry| entry.claim_id().0.clone())
        .collect();
    update_report(connection, &review.program_id, lesson_id, |report| {
        report.taken_out(&TakenOut {
            unit,
            old_body: &body,
            range,
            new_body: &new_body,
            removed: &ids,
        })
    })
    .await?;
    Ok(removed
        .iter()
        .map(|entry| entry.claim_id().clone())
        .collect())
}
