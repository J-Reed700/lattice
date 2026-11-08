//! The user's removal of a statement in a published lesson: its sentence goes
//! out of the section's text with the whitespace around it, every claim that
//! stood on that sentence ends removed by the user, and the stored report
//! follows. [`remove_in`] runs inside the caller's transaction, which owns the
//! revision check and its advance, so a refusal writes nothing.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{
    span_of, Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, EventProvenance,
    UnitKind, UnverifiedReason, UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::segmentation::sentences;
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

/// What a removal took out: the text, and the whole sentences that end
/// (`follows`) and start (`precedes`) where it was taken from.
pub(super) type Removal = (String, Option<String>, Option<String>);

/// What `without_sentence` took out of `old` to leave `new`, and where: the
/// text itself, and the sentence-aligned text of `new` before and after the
/// junction (`None` when it was taken from the start or the end), widened by a
/// sentence each way until the two together occur once. `None` when the two do
/// not differ by one deletion.
pub(super) fn removal_text(old: &str, new: &str) -> Option<Removal> {
    let at = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
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
        .ok_or_else(|| AppError::NotFound("This lesson has no statement review.".into()))?;
    let target = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == claim_id)
        .ok_or_else(|| AppError::NotFound("No statement with that id in this lesson.".into()))?;
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
    let taken = removal_text(&body, &new_body);
    for entry in &removed {
        if let Some((text, follows, precedes)) = &taken {
            let event = ClaimEvent::TextRemoved {
                text: text.clone(),
                follows: follows.clone(),
                precedes: precedes.clone(),
            };
            let machine = EventProvenance {
                actor: Actor::Machine,
                ..user.clone()
            };
            append_in(connection, lesson_id, entry.claim_id(), machine, event).await?;
        }
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
            claim: claim_id,
        })
    })
    .await?;
    Ok(removed
        .iter()
        .map(|entry| entry.claim_id().clone())
        .collect())
}

/// Takes `sentence`, which no claim stands on exactly, out of the section on
/// `connection`: a claim for it is made as an edit makes its successor, then
/// removed through [`remove_in`], which ends every claim that overlaps it.
/// Returns the claim made and the claims the removal ended.
pub(super) async fn remove_sentence_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    unit: usize,
    sentence: &str,
    user: &EventProvenance,
) -> Result<(ClaimId, HashSet<ClaimId>)> {
    let body = section_body(connection, lesson_id, unit).await?;
    let span = span_of(&body, sentence).ok_or_else(|| AppError::InvalidInput(CHANGED.into()))?;
    let claim_id = ClaimId::generate();
    let location = ClaimLocation {
        unit_index: unit,
        unit_kind: UnitKind::Block,
        field: "body".into(),
        quote: sentence.into(),
        span: Some(span),
    };
    let machine = EventProvenance {
        actor: Actor::Machine,
        ..user.clone()
    };
    for event in [
        ClaimEvent::Extracted {
            location,
            statement: sentence.into(),
        },
        ClaimEvent::PublishedUnverified {
            reason: UnverifiedReason::Unchecked,
        },
    ] {
        append_in(connection, lesson_id, &claim_id, machine.clone(), event).await?;
    }
    let ended = remove_in(connection, lesson_id, &claim_id, user).await?;
    Ok((claim_id, ended))
}
