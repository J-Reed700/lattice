//! Planning and recording removal of a sentence in a published lesson.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{
    span_of, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, EventProvenance, UnitKind,
    UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::WriteTx;
#[cfg(test)]
use lattice_core::shared::text::segmentation::sentences;

use super::actions::{in_section_body, ONLY_SECTION};
use super::published_edit::{placed, update_report, user_sentence_in, Recorded, CHANGED};
use super::repository::append_in;
use crate::content_verification::TakenOut;
#[cfg(test)]
use lattice_core::shared::text::same_text::common_prefix;

/// Planned removal of a sentence (with optional creation of an ad-hoc sentence claim).
pub(super) struct RemovePlan {
    pub unit: usize,
    pub old_body: String,
    pub range: (usize, usize),
    pub new_body: String,
    pub removed_text: String,
    pub target: Option<ClaimId>,
    pub removed: Vec<ClaimId>,
    pub sentence_claim: Option<String>,
}

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

#[cfg(test)]
pub(super) type Removal = (String, Option<String>, Option<String>);

#[cfg(test)]
pub(super) fn removal_text(old: &str, new: &str) -> Option<Removal> {
    let at = common_prefix(old, new);
    let kept = new.len().checked_sub(at)?;
    let end = old.len().checked_sub(kept)?;
    let text = old.get(at..end.max(at))?;
    if format!("{}{}{}", new.get(..at)?, text, new.get(at..)?) != old || text.is_empty() {
        return None;
    }
    let spans = sentences(new);
    let mut before = spans.iter().rposition(|&(_, end)| end <= at);
    let mut after = spans
        .iter()
        .position(|&(start, _)| start >= at)
        .unwrap_or(spans.len());
    loop {
        let start = before.and_then(|i| spans.get(i)).map_or(0, |span| span.0);
        let stop = spans.get(after).map_or(new.len(), |span| span.1);
        let follows = new.get(start..at).filter(|context| !context.is_empty());
        let precedes = new.get(at..stop).filter(|context| !context.is_empty());
        let joined = format!("{}{}", follows.unwrap_or(""), precedes.unwrap_or(""));
        let alone = follows.is_none() || precedes.is_none();
        if alone || span_of(new, &joined).is_some() {
            return Some((
                text.to_owned(),
                follows.map(str::to_owned),
                precedes.map(str::to_owned),
            ));
        }
        before = before.and_then(|i| i.checked_sub(1));
        after = (after + 1).min(spans.len());
    }
}

fn only_marker(prefix: &str) -> bool {
    prefix
        .trim_matches(|c: char| {
            c.is_whitespace()
                || matches!(c, '-' | '*' | '+' | '>' | '#' | '.' | ')')
                || c.is_ascii_digit()
        })
        .is_empty()
}

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

fn overlapping_live(
    claims: &[ClaimLedgerEntry],
    target: Option<&ClaimId>,
    unit: usize,
    range: (usize, usize),
    body: &str,
) -> Vec<ClaimId> {
    claims
        .iter()
        .filter(|entry| {
            target.is_some_and(|id| entry.claim_id() == id)
                || (in_section_body(entry)
                    && entry.location().unit_index == unit
                    && !entry.state().is_gone()
                    && placed(entry, body)
                        .is_some_and(|(start, end)| start < range.1 && range.0 < end))
        })
        .map(|entry| entry.claim_id().clone())
        .collect()
}

/// Plans removal of the sentence holding `claim_id` in `body`.
pub(super) fn plan(
    claims: &[ClaimLedgerEntry],
    claim_id: &ClaimId,
    body: &str,
) -> Result<RemovePlan> {
    let target = claims
        .iter()
        .find(|entry| entry.claim_id() == claim_id)
        .ok_or_else(|| AppError::NotFound("No statement with that id in this lesson.".into()))?;
    if !in_section_body(target) {
        return Err(AppError::InvalidInput(ONLY_SECTION.into()));
    }
    let unit = target.location().unit_index;
    let range = placed(target, body).ok_or_else(|| AppError::InvalidInput(CHANGED.into()))?;
    let removed = overlapping_live(claims, Some(claim_id), unit, range, body);
    let new_body = without_sentence(body, range);
    let removed_text = body.get(range.0..range.1).unwrap_or_default().to_owned();
    Ok(RemovePlan {
        unit,
        old_body: body.to_owned(),
        range,
        new_body,
        removed_text,
        target: Some(claim_id.clone()),
        removed,
        sentence_claim: None,
    })
}

/// Plans removal of `sentence` (from a teaching note) when no claim stands on it.
pub(super) fn plan_sentence(
    claims: &[ClaimLedgerEntry],
    unit: usize,
    sentence: &str,
    body: &str,
) -> Result<RemovePlan> {
    let range = span_of(body, sentence).ok_or_else(|| AppError::InvalidInput(CHANGED.into()))?;
    let removed = overlapping_live(claims, None, unit, range, body);
    let new_body = without_sentence(body, range);
    Ok(RemovePlan {
        unit,
        old_body: body.to_owned(),
        range,
        new_body,
        removed_text: sentence.to_owned(),
        target: None,
        removed,
        sentence_claim: Some(sentence.to_owned()),
    })
}

/// Writes the removal events and updates the verification report inside `tx`.
pub(super) async fn record_in(
    tx: &mut WriteTx,
    program_id: &str,
    lesson_id: &str,
    removal: &RemovePlan,
    user: &EventProvenance,
) -> Result<Recorded> {
    let mut created = Vec::new();
    let mut targets = Vec::new();
    let primary = if let Some(sentence) = &removal.sentence_claim {
        let id = ClaimId::generate();
        let location = ClaimLocation {
            unit_index: removal.unit,
            unit_kind: UnitKind::Block,
            field: "body".into(),
            quote: sentence.clone(),
            span: Some(removal.range),
        };
        user_sentence_in(tx, lesson_id, &id, location, user).await?;
        created.push(id.clone());
        targets.push(id.clone());
        id
    } else {
        removal
            .target
            .clone()
            .ok_or_else(|| AppError::InternalError("Missing removal target claim".into()))?
    };
    for id in &removal.removed {
        if !targets.contains(id) {
            targets.push(id.clone());
        }
    }
    let machine = user.as_machine();
    let mut acted = Vec::with_capacity(targets.len());
    for id in &targets {
        let text_removed = ClaimEvent::TextRemoved {
            text: removal.removed_text.clone(),
            follows: None,
            precedes: None,
        };
        append_in(tx, lesson_id, id, machine.clone(), text_removed).await?;
        let removed_event = ClaimEvent::UserAction {
            action: UserAction::Remove,
            note: None,
        };
        let ordinal = append_in(tx, lesson_id, id, user.clone(), removed_event).await?;
        acted.push((id.clone(), ordinal));
    }
    let ids: HashSet<String> = targets.iter().map(|id| id.0.clone()).collect();
    update_report(tx, program_id, lesson_id, |report| {
        report.taken_out(&TakenOut {
            unit: removal.unit,
            old_body: &removal.old_body,
            range: removal.range,
            new_body: &removal.new_body,
            removed: &ids,
            claim: &primary,
        })
    })
    .await?;
    Ok(Recorded { acted, created })
}
