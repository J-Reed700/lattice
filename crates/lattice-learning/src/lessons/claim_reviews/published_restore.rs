//! A sentence put back into a published lesson, the original put in place of
//! a rewrite, and the undo of either (or of an edit): the section's text, the
//! ledger's events and the stored report change in one transaction, so a
//! refusal writes nothing.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{
    span_of, Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState, EventProvenance, Revision,
    UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::actions::{in_section_body, ONLY_SECTION};
use super::published_edit::CHANGED;
use super::repository::{append_in, bump_revision, load_in, revision_conflict};
use crate::content_verification::{
    insert_sentence, located, remove_sentence, LessonVerificationReport, WrittenBack,
};
use crate::persistence::db;

const BY_HAND: &str = "The section changed after this; undo it by hand.";
const PUT_BACK_REASON: &str = "You put this sentence back; it was not checked against the sources.";
const RESTORED_REASON: &str = "You restored the original sentence; it was not checked again.";

/// What the user puts into the section's text.
pub(super) enum Restoring<'a> {
    /// The sentence the machine took out, at `place`.
    PutBack,
    /// The original in place of the machine's rewrite.
    Original { original: &'a str, rewrite: &'a str },
}

pub(super) struct Restore<'a> {
    pub lesson_id: &'a str,
    pub claim_id: &'a ClaimId,
    pub restoring: Restoring<'a>,
    /// The user's event on the claim: `UserAction::PutBack` or
    /// `UserAction::RestoreOriginal`.
    pub event: ClaimEvent,
    pub expected_revision: Revision,
    pub user: EventProvenance,
}

fn gone(entry: &ClaimLedgerEntry) -> bool {
    matches!(
        entry.state(),
        ClaimState::Removed { .. } | ClaimState::Edited { .. }
    )
}

/// Where the claim's sentence is in `body`: the span it recorded when the
/// text there is still the sentence, else the sentence's only occurrence.
fn placed(entry: &ClaimLedgerEntry, body: &str) -> Option<(usize, usize)> {
    let (sentence, span) = located(entry.location(), entry.statement(), body);
    span.filter(|&(start, end)| body.get(start..end) == Some(sentence.as_str()))
        .or_else(|| span_of(body, &sentence))
}

async fn body_of(tx: &mut Transaction<'_, Sqlite>, lesson_id: &str, unit: usize) -> Result<String> {
    sqlx::query_scalar("SELECT body FROM learning_blocks WHERE lesson_id=? AND ordinal=?")
        .bind(lesson_id)
        .bind(i64::try_from(unit).unwrap_or(i64::MAX))
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::InvalidInput(CHANGED.into()))
}

async fn write_body(
    tx: &mut Transaction<'_, Sqlite>,
    lesson_id: &str,
    unit: usize,
    body: &str,
) -> Result<()> {
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=?")
        .bind(body)
        .bind(lesson_id)
        .bind(i64::try_from(unit).unwrap_or(i64::MAX))
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    Ok(())
}

/// Applies `change` to the stored report of the lesson, when it has one.
async fn with_report(
    tx: &mut Transaction<'_, Sqlite>,
    program_id: &str,
    lesson_id: &str,
    change: impl FnOnce(&mut LessonVerificationReport),
) -> Result<()> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_optional(&mut **tx)
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
    .execute(&mut **tx)
    .await
    .map_err(db)?;
    Ok(())
}

pub(super) async fn apply(pool: &SqlitePool, restore: Restore<'_>) -> Result<Revision> {
    let _ = (pool, restore);
    Err(AppError::InternalError("not built yet".into()))
}

/// The claims an undo of `target`'s last user event takes back, with the
/// text that goes back into the section and the one it replaces.
struct Undoing {
    unit: usize,
    /// The section's text once the undo is written.
    then: String,
    /// Claims whose last user event is undone with the claim's.
    undone: Vec<(ClaimId, u32)>,
    /// A claim an edit made that goes out with the edit.
    successor: Option<ClaimId>,
}

fn last_user_event(entry: &ClaimLedgerEntry) -> Option<(u32, &ClaimEvent)> {
    let ordinal = entry.undo_target()?;
    entry
        .events()
        .iter()
        .find(|record| record.ordinal == ordinal)
        .map(|record| (ordinal, &record.event))
}

/// Whether the claim's undo target is one a published lesson undoes: the
/// user's edit, put back or restore of the sentence, or the source that
/// came with a put back.
pub(super) fn undoable_in_published(entry: &ClaimLedgerEntry) -> bool {
    let _ = entry;
    false
}

/// Takes back the claim's last user event on a published lesson.
pub(super) async fn undo(
    pool: &SqlitePool,
    lesson_id: &str,
    claim_id: &ClaimId,
    expected_revision: Revision,
    user: EventProvenance,
) -> Result<Revision> {
    let _ = (pool, lesson_id, claim_id, expected_revision, user);
    Err(AppError::InternalError("not built yet".into()))
}
