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
use sqlx::{SqliteConnection, SqlitePool};

use super::repository::{append_in, bump_revision, check_revision, load_in};
use super::trust_changed::machine;
use crate::persistence::{db, now};

/// Reopens what the versions of the deleted `source_id` closed, in every
/// lesson review of the program, on `connection`.
pub(crate) async fn reopen_for_deleted_source(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<()> {
    let versions: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT id FROM learning_source_versions WHERE program_id=? AND source_id=?",
    )
    .bind(program_id)
    .bind(source_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?
    .into_iter()
    .collect();
    let lessons: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT lesson_id FROM learning_lesson_reviews WHERE program_id=? ORDER BY lesson_id",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
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
    pool: &SqlitePool,
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
    let deleted = deleted_versions(pool, program_id).await?;
    let planned = reopens(&ledger.claims, &deleted);
    if planned.is_empty() {
        return Ok(false);
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    match check_revision(&mut tx, &ledger.lesson_id, ledger.revision).await {
        Ok(()) => {}
        Err(AppError::InvalidState(_)) => return Ok(true),
        Err(error) => return Err(error),
    }
    write(&mut tx, &ledger.lesson_id, review, &planned).await?;
    tx.commit().await.map_err(db)?;
    Ok(true)
}

/// The versions of the program's deleted sources.
pub(super) async fn deleted_versions(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<HashSet<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT v.id FROM learning_source_versions v \
         JOIN learning_source_library s ON s.program_id=v.program_id AND s.id=v.source_id \
         WHERE v.program_id=? AND s.deleted_at IS NOT NULL",
    )
    .bind(program_id)
    .fetch_all(pool)
    .await
    .map_err(db)?
    .into_iter()
    .collect())
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
    connection: &mut SqliteConnection,
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
            sqlx::query(
                "UPDATE learning_lesson_reviews SET state=?,updated_at=? WHERE lesson_id=?",
            )
            .bind(LessonReviewState::NeedsReview.as_str())
            .bind(now())
            .bind(lesson_id)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
        }
    }
    Ok(())
}
