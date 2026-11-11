//! A deleted source reopens the statements that rest on it. `reopen_cause`
//! says which: a claim a check resolved by one of its versions, and a
//! confirmation bound to one. A library change like a trust change, it
//! calls no model and writes in the caller's transaction.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{
    cited_source_versions, end_state, reopen_cause, ClaimEvent, ClaimId, ClaimLedgerEntry, Ledger,
    LessonReview, LessonReviewState, NewInformation, ReopenCause, SourceChange,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, Transactions, WriteTx};

use super::repository::{append_in, bump_revision, check_revision, load_in, reviews};
use super::trust_changed::machine;
use crate::persistence::{db, now};

/// Reopens what the versions of the deleted `source_id` closed, in every
/// lesson review of the program, on `connection`.
pub(crate) async fn reopen_for_deleted_source(
    connection: &mut WriteTx,
    program_id: &str,
    source_id: &str,
) -> Result<()> {
    let versions = reviews::source_versions(connection, program_id, source_id).await?;
    let lessons = reviews::reviewed_lessons(connection, program_id).await?;
    for lesson_id in lessons {
        let ledger = load_in(connection, &lesson_id).await?;
        let Some(review) = &ledger.review else {
            continue;
        };
        let planned = reopens(&ledger.claims, &versions);
        if !planned.is_empty() {
            write(connection, &lesson_id, review, &planned).await?;
        }
    }
    Ok(())
}

/// Reopens in `ledger`'s review what a source deleted before deleting
/// reopened anything left closed. True when the caller should read the
/// ledger again: something was written, or the review moved meanwhile and
/// nothing was.
pub(super) async fn reopen_lost(
    database: &Database,
    program_id: &str,
    ledger: &Ledger,
) -> Result<bool> {
    let Some(review) = ledger
        .review
        .as_ref()
        .filter(|review| review.program_id == program_id)
    else {
        return Ok(false);
    };
    let mut read = database.begin_read().await.map_err(db)?;
    let deleted = reviews::deleted_versions_in(&mut read, program_id).await?;
    read.commit().await.map_err(db)?;
    let planned = reopens(&ledger.claims, &deleted);
    if planned.is_empty() {
        return Ok(false);
    }
    let mut tx = database.begin_write().await.map_err(db)?;
    match check_revision(&mut tx, &ledger.lesson_id, ledger.revision).await {
        Ok(()) => {}
        Err(AppError::InvalidState(_)) => return Ok(true),
        Err(error) => return Err(error),
    }
    write(&mut tx, &ledger.lesson_id, review, &planned).await?;
    tx.commit().await.map_err(db)?;
    Ok(true)
}

/// The reopen each claim gets when the `deleted` versions are gone.
fn reopens(claims: &[ClaimLedgerEntry], deleted: &HashSet<String>) -> Vec<(ClaimId, ReopenCause)> {
    claims
        .iter()
        .filter_map(|entry| {
            let cause = cited_source_versions(entry)
                .into_iter()
                .filter(|version| deleted.contains(*version))
                .find_map(|version| {
                    reopen_cause(
                        entry.state(),
                        &NewInformation::SourceChanged(SourceChange::Deleted(version.to_owned())),
                    )
                })?;
            Some((entry.claim_id().clone(), cause))
        })
        .collect()
}

/// Appends the reopens and advances the review's revision once. A published
/// lesson left with a statement to look at is held for review and stays
/// published, as a trust change holds it.
async fn write(
    connection: &mut WriteTx,
    lesson_id: &str,
    review: &LessonReview,
    planned: &[(ClaimId, ReopenCause)],
) -> Result<()> {
    for (claim, cause) in planned {
        let event = ClaimEvent::Reopened {
            cause: cause.clone(),
        };
        append_in(connection, lesson_id, claim, machine(review), event).await?;
    }
    bump_revision(connection, lesson_id).await?;
    if review.state == LessonReviewState::Ready {
        let ledger = load_in(connection, lesson_id).await?;
        if end_state(&ledger.claims) == LessonReviewState::NeedsReview {
            reviews::mark_needs_review(connection, lesson_id, now()).await?;
        }
    }
    Ok(())
}
