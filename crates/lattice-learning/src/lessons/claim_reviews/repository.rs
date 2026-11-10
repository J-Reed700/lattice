//! The claim ledger in SQLite: `learning_lesson_reviews`, `learning_lesson_claims`
//! and `learning_lesson_claim_events` (§7.1). Every append replays the claim's
//! stored log, validates the event against it in the domain, and writes the
//! event, the replayed state and the review's revision in one transaction.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §3, §7.2, §9.

use std::collections::HashMap;

use async_trait::async_trait;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, ClaimState, EventProvenance,
    Ledger, LessonReview, LessonReviewState, RecordedEvent, Revision, StopReason, UnitKind,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::driver::DriverPool;
use lattice_core::shared::persistence::Transactions;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::persistence::{db, decode, encode, now};

pub(super) mod blocks;
pub(super) mod checker_examples;
pub(super) mod questions;
pub(super) mod reviews;
pub(super) mod teaching_findings;

/// A passage as the review card shows it: what a check compared, with its
/// text, so the card still shows it after its source is gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownPassage {
    pub source_version_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub text: String,
    /// `lexical`, `library`, `execution`, ...: how the passage was found.
    pub retrieval_kind: String,
}

/// An event row's payload: the event and the provenance the columns lack.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredEvent {
    event: ClaimEvent,
    app_version: String,
    checker_model: Option<String>,
}

#[derive(Clone)]
pub struct ClaimReviewRepository {
    pool: SqlitePool,
}

impl ClaimReviewRepository {
    pub fn new(pool: impl DriverPool) -> Self {
        let pool = pool.sqlx_pool().clone();
        Self { pool }
    }

    /// The passages each claim's latest check compared, as shown on its card.
    pub async fn shown_passages(
        &self,
        lesson_id: &str,
    ) -> Result<HashMap<ClaimId, Vec<ShownPassage>>> {
        let mut read = self.pool.begin_read().await.map_err(db)?;
        let shown = shown_passages_in(&mut read, lesson_id).await?;
        read.commit().await.map_err(db)?;
        Ok(shown)
    }

    /// Replaces the passages a claim's card shows. Display only: the event
    /// log keeps what the check compared (`EvidenceSeen`), so this write does
    /// not advance the revision.
    pub async fn show_passages(
        &self,
        lesson_id: &str,
        claim_id: &ClaimId,
        passages: &[ShownPassage],
    ) -> Result<()> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        Self::show_passages_in(&mut connection, lesson_id, claim_id, passages).await
    }

    /// [`Self::show_passages`] inside the caller's unit of work.
    pub async fn show_passages_in(
        conn: &mut SqliteConnection,
        lesson_id: &str,
        claim_id: &ClaimId,
        passages: &[ShownPassage],
    ) -> Result<()> {
        sqlx::query(
            "UPDATE learning_lesson_claims SET evidence_json=? WHERE claim_id=? AND lesson_id=?",
        )
        .bind(encode(passages)?)
        .bind(&claim_id.0)
        .bind(lesson_id)
        .execute(&mut *conn)
        .await
        .map_err(db)?;
        Ok(())
    }

    /// The review row's last write by the job, as Unix milliseconds: what
    /// "changed since the job ended" is measured from (§6.2).
    pub async fn review_written_at(&self, lesson_id: &str) -> Result<Option<i64>> {
        sqlx::query_scalar("SELECT updated_at FROM learning_lesson_reviews WHERE lesson_id=?")
            .bind(lesson_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)
    }

    /// The body of the lesson's block at `ordinal`, when the lesson has one.
    pub async fn block_body(&self, lesson_id: &str, ordinal: usize) -> Result<Option<String>> {
        sqlx::query_scalar("SELECT body FROM learning_blocks WHERE lesson_id=? AND ordinal=?")
            .bind(lesson_id)
            .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)
    }
}

#[async_trait]
impl ClaimReviewStore for ClaimReviewRepository {
    async fn load(&self, lesson_id: &str) -> Result<Ledger> {
        let mut read = self.pool.begin_read().await.map_err(db)?;
        let ledger = load_in(&mut read, lesson_id).await?;
        read.commit().await.map_err(db)?;
        Ok(ledger)
    }

    async fn append(
        &self,
        lesson_id: &str,
        claim_id: &ClaimId,
        provenance: EventProvenance,
        event: ClaimEvent,
        expected_revision: Revision,
    ) -> Result<Revision> {
        let mut tx = self.pool.begin_write().await.map_err(db)?;
        check_revision(&mut tx, lesson_id, expected_revision).await?;
        append_in(&mut tx, lesson_id, claim_id, provenance, event).await?;
        bump_revision(&mut tx, lesson_id).await?;
        tx.commit().await.map_err(db)?;
        Ok(Revision(expected_revision.0 + 1))
    }

    async fn set_lesson_review(&self, lesson_id: &str, review: LessonReview) -> Result<Revision> {
        let stamp = now();
        let stop_reason = review.stop_reason.as_ref().map(encode).transpose()?;
        let revision: i64 = sqlx::query_scalar(
            "INSERT INTO learning_lesson_reviews(lesson_id,program_id,job_id,state,revision,draft_sha256,checker_model,stop_reason_json,opened_at,updated_at) \
             VALUES(?,?,?,?,0,?,?,?,?,?) \
             ON CONFLICT(lesson_id) DO UPDATE SET program_id=excluded.program_id,job_id=excluded.job_id,state=excluded.state,\
             revision=learning_lesson_reviews.revision+1,draft_sha256=excluded.draft_sha256,checker_model=excluded.checker_model,\
             stop_reason_json=excluded.stop_reason_json,updated_at=excluded.updated_at \
             RETURNING revision",
        )
        .bind(lesson_id)
        .bind(&review.program_id)
        .bind(&review.job_id)
        .bind(review.state.as_str())
        .bind(&review.draft_sha256)
        .bind(&review.checker_model)
        .bind(stop_reason)
        .bind(stamp)
        .bind(stamp)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        Ok(Revision(revision))
    }

    async fn hold_lesson_review(
        &self,
        lesson_id: &str,
        stop_reason: Option<StopReason>,
    ) -> Result<Option<Revision>> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        hold_in(&mut connection, lesson_id, stop_reason.as_ref()).await
    }
}

/// Sends the review to Needs review in one statement, keeping its other
/// fields; `None` keeps the stored stop reason. `None` back: no review row.
pub(super) async fn hold_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    stop_reason: Option<&StopReason>,
) -> Result<Option<Revision>> {
    let stop_reason = stop_reason.map(encode).transpose()?;
    let revision: Option<i64> = sqlx::query_scalar(
        "UPDATE learning_lesson_reviews SET state=?,stop_reason_json=COALESCE(?,stop_reason_json),\
         revision=revision+1,updated_at=? WHERE lesson_id=? RETURNING revision",
    )
    .bind(LessonReviewState::NeedsReview.as_str())
    .bind(stop_reason)
    .bind(now())
    .bind(lesson_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    Ok(revision.map(Revision))
}

/// [`ClaimReviewRepository::shown_passages`] inside the caller's unit of work.
pub(super) async fn shown_passages_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
) -> Result<HashMap<ClaimId, Vec<ShownPassage>>> {
    let rows =
        sqlx::query("SELECT claim_id,evidence_json FROM learning_lesson_claims WHERE lesson_id=?")
            .bind(lesson_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
    rows.iter()
        .map(|row| {
            Ok((
                ClaimId(row.get("claim_id")),
                decode(row.get::<String, _>("evidence_json").as_str())?,
            ))
        })
        .collect()
}

/// The refusal of a write made against an older revision (§9): the caller
/// reloads and acts again.
pub(super) fn revision_conflict() -> AppError {
    AppError::InvalidState("This statement changed; reload and act again.".into())
}

/// Refuses unless the lesson's review is at `expected` on `connection`.
pub(super) async fn check_revision(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    expected: Revision,
) -> Result<()> {
    let revision: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM learning_lesson_reviews WHERE lesson_id=?")
            .bind(lesson_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
    let revision = revision.ok_or_else(|| {
        AppError::InvalidState("This lesson has no statement review to record into.".into())
    })?;
    if revision != expected.0 {
        return Err(revision_conflict());
    }
    Ok(())
}

pub(super) async fn bump_revision(
    connection: &mut SqliteConnection,
    lesson_id: &str,
) -> Result<()> {
    sqlx::query("UPDATE learning_lesson_reviews SET revision=revision+1 WHERE lesson_id=?")
        .bind(lesson_id)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// Writes one event of one claim on `connection`, without the revision
/// check or its advance: the caller's transaction owns both.
pub(super) async fn append_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    claim_id: &ClaimId,
    provenance: EventProvenance,
    event: ClaimEvent,
) -> Result<()> {
    let owner: Option<String> =
        sqlx::query_scalar("SELECT lesson_id FROM learning_lesson_claims WHERE claim_id=?")
            .bind(&claim_id.0)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
    if owner.as_deref().is_some_and(|owner| owner != lesson_id) {
        return Err(AppError::InvalidInput(
            "That statement belongs to another lesson.".into(),
        ));
    }
    let mut entry = match owner {
        Some(_) => {
            let events = claim_events(connection, claim_id).await?;
            ClaimLedgerEntry::from_events(claim_id.clone(), events)?
        }
        None => match &event {
            ClaimEvent::Extracted {
                location,
                statement,
            } => ClaimLedgerEntry::extracted(
                claim_id.clone(),
                provenance.clone(),
                location.clone(),
                statement.clone(),
            )?,
            _ => {
                return Err(AppError::NotFound(
                    "No statement with that id in this lesson.".into(),
                ))
            }
        },
    };
    let ordinal = if owner.is_some() {
        entry.append(provenance.clone(), event.clone())?
    } else {
        0
    };
    upsert_claim(connection, lesson_id, &entry).await?;
    sqlx::query("INSERT INTO learning_lesson_claim_events(claim_id,ordinal,actor,kind,payload_json,job_id,at) VALUES(?,?,?,?,?,?,?)")
        .bind(&claim_id.0)
        .bind(i64::from(ordinal))
        .bind(provenance.actor.as_str())
        .bind(event.kind_name())
        .bind(encode(&StoredEvent {
            event,
            app_version: provenance.app_version,
            checker_model: provenance.checker_model,
        })?)
        .bind(provenance.job_id)
        .bind(provenance.at)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

pub(crate) async fn load_in(connection: &mut SqliteConnection, lesson_id: &str) -> Result<Ledger> {
    let review = sqlx::query("SELECT program_id,job_id,state,revision,draft_sha256,checker_model,stop_reason_json FROM learning_lesson_reviews WHERE lesson_id=?")
        .bind(lesson_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
    let Some(review) = review else {
        return Ok(Ledger {
            lesson_id: lesson_id.into(),
            review: None,
            revision: Revision(0),
            claims: Vec::new(),
        });
    };
    let state: String = review.get("state");
    let stop_reason: Option<String> = review.get("stop_reason_json");
    let lesson_review = LessonReview {
        program_id: review.get("program_id"),
        job_id: review.get("job_id"),
        state: LessonReviewState::from_column(&state)
            .ok_or_else(|| AppError::Database(format!("Unknown lesson review state {state}")))?,
        draft_sha256: review.get("draft_sha256"),
        checker_model: review.get("checker_model"),
        stop_reason: stop_reason
            .as_deref()
            .map(decode::<StopReason>)
            .transpose()?,
    };
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT claim_id FROM learning_lesson_claims WHERE lesson_id=? ORDER BY unit_index,rowid",
    )
    .bind(lesson_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    let rows = sqlx::query("SELECT e.claim_id,e.ordinal,e.actor,e.payload_json,e.job_id,e.at FROM learning_lesson_claim_events e JOIN learning_lesson_claims c ON c.claim_id=e.claim_id WHERE c.lesson_id=? ORDER BY e.claim_id,e.ordinal")
        .bind(lesson_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
    let mut logs: HashMap<String, Vec<RecordedEvent>> = HashMap::new();
    for row in &rows {
        logs.entry(row.get("claim_id"))
            .or_default()
            .push(recorded(row)?);
    }
    let claims = ids
        .into_iter()
        .map(|id| {
            let events = logs.remove(&id).unwrap_or_default();
            ClaimLedgerEntry::from_events(ClaimId(id), events).map_err(AppError::from)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Ledger {
        lesson_id: lesson_id.into(),
        review: Some(lesson_review),
        revision: Revision(review.get("revision")),
        claims,
    })
}

async fn claim_events(
    connection: &mut SqliteConnection,
    claim_id: &ClaimId,
) -> Result<Vec<RecordedEvent>> {
    let rows = sqlx::query("SELECT claim_id,ordinal,actor,payload_json,job_id,at FROM learning_lesson_claim_events WHERE claim_id=? ORDER BY ordinal")
        .bind(&claim_id.0)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
    rows.iter().map(recorded).collect()
}

fn recorded(row: &sqlx::sqlite::SqliteRow) -> Result<RecordedEvent> {
    let actor: String = row.get("actor");
    let stored: StoredEvent = decode(row.get::<String, _>("payload_json").as_str())?;
    let ordinal: i64 = row.get("ordinal");
    Ok(RecordedEvent {
        ordinal: u32::try_from(ordinal)
            .map_err(|_| AppError::Database(format!("Invalid claim event ordinal {ordinal}")))?,
        provenance: EventProvenance {
            actor: Actor::from_column(&actor)
                .ok_or_else(|| AppError::Database(format!("Unknown claim event actor {actor}")))?,
            at: row.get("at"),
            job_id: row.get("job_id"),
            app_version: stored.app_version,
            checker_model: stored.checker_model,
        },
        event: stored.event,
    })
}

/// Writes the claim row's listing columns from its replayed log. The
/// passages shown are written separately ([`ClaimReviewRepository::show_passages`]).
async fn upsert_claim(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    entry: &ClaimLedgerEntry,
) -> Result<()> {
    let ClaimLocation {
        unit_index,
        unit_kind,
        field,
        quote,
        span,
    } = entry.location();
    let (verdict_kind, reason) = match entry.state() {
        ClaimState::Open { kind, reason, .. } | ClaimState::Queued { kind, reason } => {
            (Some(kind.as_str()), Some(reason.as_str()))
        }
        _ => (entry.latest_failure().map(|kind| kind.as_str()), None),
    };
    sqlx::query(
        "INSERT INTO learning_lesson_claims(claim_id,lesson_id,unit_index,unit_kind,field,quote,span_start,span_end,statement,state_json,evidence_fingerprint,verdict_kind,reason,evidence_json,updated_at) \
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,'[]',?) \
         ON CONFLICT(claim_id) DO UPDATE SET unit_index=excluded.unit_index,unit_kind=excluded.unit_kind,field=excluded.field,\
         quote=excluded.quote,span_start=excluded.span_start,span_end=excluded.span_end,statement=excluded.statement,\
         state_json=excluded.state_json,evidence_fingerprint=excluded.evidence_fingerprint,verdict_kind=excluded.verdict_kind,\
         reason=excluded.reason,updated_at=excluded.updated_at",
    )
    .bind(&entry.claim_id().0)
    .bind(lesson_id)
    .bind(i64::try_from(*unit_index).unwrap_or(i64::MAX))
    .bind(UnitKind::as_str(*unit_kind))
    .bind(field)
    .bind(quote)
    .bind(span.map(|(start, _)| i64::try_from(start).unwrap_or(i64::MAX)))
    .bind(span.map(|(_, end)| i64::try_from(end).unwrap_or(i64::MAX)))
    .bind(entry.statement())
    .bind(encode(entry.state())?)
    .bind(entry.basis().map(|basis| basis.fingerprint.0.clone()))
    .bind(verdict_kind)
    .bind(reason)
    .bind(now())
    .execute(&mut *connection)
    .await
    .map_err(db)?;
    Ok(())
}
