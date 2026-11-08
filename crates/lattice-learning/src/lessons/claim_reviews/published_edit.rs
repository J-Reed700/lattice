//! The user's edit of a statement in a published lesson: their sentence takes
//! the place of the old one in the section's text, every claim that stood on
//! the replaced text gives way to one claim for the new sentence, which they
//! confirm, and the stored report follows, so the learner view reads the new
//! sentence as confirmed by them. [`apply_in`] runs inside the caller's
//! transaction, which owns the revision check and its advance, so a refusal
//! writes nothing.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{
    span_of, Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, ClaimState,
    EventProvenance, HumanBinding, UnitKind, UnverifiedReason, UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

use super::actions::{in_section_body, ONLY_SECTION};
use super::repository::{append_in, load_in};
use crate::content_verification::{located, LessonVerificationReport, WrittenOver};
use crate::persistence::db;

pub(super) const CHANGED: &str =
    "This statement changed since you opened it; reload and try again.";
pub(super) const UNDO_BLOCKED: &str =
    "This section changed after that, so it can't be undone on its own. Edit the section to change it back.";
pub(super) const BLANK: &str = "Write the sentence you want in its place.";

pub(super) struct PublishedEdit<'a> {
    pub lesson_id: &'a str,
    pub claim_id: &'a ClaimId,
    /// The text the edit replaces, as the draft's Edit reads it: the claim's
    /// sentence as the lesson writes it, or other text of the section.
    pub before: &'a str,
    pub after: &'a str,
    pub user: EventProvenance,
}

/// Where the claim's sentence is in `body`: the span it recorded when the
/// text there is still the sentence, else the sentence's only occurrence.
pub(super) fn placed(entry: &ClaimLedgerEntry, body: &str) -> Option<(usize, usize)> {
    let (sentence, span) = located(entry.location(), entry.statement(), body);
    span.filter(|&(start, end)| body.get(start..end) == Some(sentence.as_str()))
        .or_else(|| span_of(body, &sentence))
}

pub(super) fn gone(entry: &ClaimLedgerEntry) -> bool {
    matches!(
        entry.state(),
        ClaimState::Removed { .. } | ClaimState::Edited { .. }
    )
}

/// The section's text as the lesson holds it now.
pub(super) async fn section_body(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    unit: usize,
) -> Result<String> {
    sqlx::query_scalar("SELECT body FROM learning_blocks WHERE lesson_id=? AND ordinal=?")
        .bind(lesson_id)
        .bind(i64::try_from(unit).unwrap_or(i64::MAX))
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::InvalidInput(CHANGED.into()))
}

pub(super) async fn write_section_body(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    unit: usize,
    body: &str,
) -> Result<()> {
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=?")
        .bind(body)
        .bind(lesson_id)
        .bind(i64::try_from(unit).unwrap_or(i64::MAX))
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// Lets `change` rewrite the lesson's stored verification report and stores
/// the result with its new hash; a lesson with no stored report is left as it
/// is.
pub(super) async fn update_report(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    change: impl FnOnce(&mut LessonVerificationReport),
) -> Result<()> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    let Some(stored) = stored else {
        return Ok(());
    };
    let mut report: LessonVerificationReport = serde_json::from_str(&stored)?;
    change(&mut report);
    sqlx::query(
        "UPDATE learning_lesson_verifications SET content_sha256=?,report_json=? WHERE program_id=? AND lesson_id=?",
    )
    .bind(report.content_sha256())
    .bind(serde_json::to_string(&report)?)
    .bind(program_id)
    .bind(lesson_id)
    .execute(&mut *connection)
    .await
    .map_err(db)?;
    Ok(())
}

/// Writes the edit on `connection`; the claims that stood on the replaced
/// text are the ones it returns.
pub(super) async fn apply_in(
    connection: &mut SqliteConnection,
    edit: PublishedEdit<'_>,
) -> Result<HashSet<ClaimId>> {
    let PublishedEdit {
        lesson_id,
        claim_id,
        before,
        after,
        user,
    } = edit;
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
    if after.trim().is_empty() {
        return Err(AppError::InvalidInput(BLANK.into()));
    }
    let unit = target.location().unit_index;
    let body = section_body(connection, lesson_id, unit).await?;
    let changed = || AppError::InvalidInput(CHANGED.into());
    let sentence = placed(target, &body).ok_or_else(changed)?;
    let range = if body.get(sentence.0..sentence.1) == Some(before) {
        sentence
    } else {
        span_of(&body, before).ok_or_else(changed)?
    };
    let replaced: Vec<&ClaimLedgerEntry> = ledger
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
    let mut new_body = body.clone();
    new_body.replace_range(range.0..range.1, after);

    let successor = ClaimId::generate();
    let location = ClaimLocation {
        unit_index: unit,
        unit_kind: UnitKind::Block,
        field: "body".into(),
        quote: after.into(),
        span: Some((range.0, range.0 + after.len())),
    };
    let machine = EventProvenance {
        actor: Actor::Machine,
        ..user.clone()
    };
    for (id, provenance, event) in [
        (
            &successor,
            &machine,
            ClaimEvent::Extracted {
                location,
                statement: after.into(),
            },
        ),
        (
            &successor,
            &machine,
            ClaimEvent::PublishedUnverified {
                reason: UnverifiedReason::Unchecked,
            },
        ),
        (
            &successor,
            &user,
            ClaimEvent::UserAction {
                action: UserAction::Confirm {
                    bound: HumanBinding::new(after, Vec::new()),
                },
                note: None,
            },
        ),
    ] {
        append_in(connection, lesson_id, id, provenance.clone(), event).await?;
    }
    for entry in &replaced {
        let edited = ClaimEvent::UserAction {
            action: UserAction::Edit {
                before: before.into(),
                after: after.into(),
            },
            note: None,
        };
        let superseded = ClaimEvent::Superseded {
            successor: Some(successor.clone()),
            by: Actor::User,
        };
        for (provenance, event) in [(&user, edited), (&machine, superseded)] {
            append_in(
                connection,
                lesson_id,
                entry.claim_id(),
                provenance.clone(),
                event,
            )
            .await?;
        }
    }

    write_section_body(connection, lesson_id, unit, &new_body).await?;
    let ids: HashSet<String> = replaced
        .iter()
        .map(|entry| entry.claim_id().0.clone())
        .collect();
    update_report(connection, &review.program_id, lesson_id, |report| {
        report.written_over(&WrittenOver {
            unit,
            old_body: &body,
            range,
            new_body: &new_body,
            replaced: &ids,
            after,
            successor: &successor,
            at: user.at,
        })
    })
    .await?;
    Ok(replaced
        .iter()
        .map(|entry| entry.claim_id().clone())
        .collect())
}
