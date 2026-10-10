//! The idempotency ledger behind every Learning command that takes an
//! operation ID. A client that retries sends the same ID; the ledger answers
//! with the first attempt's result instead of repeating the work, and refuses
//! an ID that was already spent on a different request.
//!
//! Callers look up and record inside their own transaction, so the ledger row
//! commits or rolls back with the work it describes.
use super::persistence::{db, decode, encode, now};
use crate::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Row, Sqlite};

/// One request, keyed by its client-supplied operation ID.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Operation<'a> {
    pub id: &'a str,
    /// The workflow that owns the ID, such as `canvas` or `recall`.
    pub scope: &'static str,
    pub kind: &'a str,
    /// The owning program. Deleting the program deletes its ledger rows, so
    /// only an existing program may be named here.
    pub program_id: Option<&'a str>,
    /// The entity the request targets, when the request names one up front.
    pub subject_id: Option<&'a str>,
    pub payload_hash: &'a str,
    /// What the caller is told when the ID was already used for other data.
    pub conflict: &'static str,
}

/// An operation ID already recorded for a different request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationConflict(pub &'static str);

impl From<OperationConflict> for AppError {
    fn from(conflict: OperationConflict) -> Self {
        AppError::InvalidInput(conflict.0.into())
    }
}

/// A ledger row as stored.
#[derive(Debug, Clone)]
pub(crate) struct RecordedOperation {
    pub scope: String,
    pub kind: String,
    pub program_id: Option<String>,
    pub subject_id: Option<String>,
    pub payload_hash: String,
    pub result_json: String,
}

impl Operation<'_> {
    /// Same ID and same request: a replay. Same ID and anything else: a conflict.
    pub(crate) fn check(
        &self,
        recorded: &RecordedOperation,
    ) -> std::result::Result<(), OperationConflict> {
        let same = recorded.scope == self.scope
            && recorded.kind == self.kind
            && recorded.program_id.as_deref() == self.program_id
            && recorded.subject_id.as_deref() == self.subject_id
            && recorded.payload_hash == self.payload_hash;
        if same {
            Ok(())
        } else {
            Err(OperationConflict(self.conflict))
        }
    }

    /// The stored result of an earlier identical request, or `None` when the
    /// ID is new.
    pub(crate) async fn replay<'e, T, E>(&self, executor: E) -> Result<Option<T>>
    where
        T: DeserializeOwned,
        E: sqlx::Executor<'e, Database = Sqlite>,
    {
        let Some(recorded) = find(executor, self.id).await? else {
            return Ok(None);
        };
        self.check(&recorded)?;
        decode(&recorded.result_json).map(Some)
    }

    /// Whether an identical request was already recorded, ignoring its result.
    pub(crate) async fn seen<'e, E>(&self, executor: E) -> Result<bool>
    where
        E: sqlx::Executor<'e, Database = Sqlite>,
    {
        Ok(self
            .replay::<serde::de::IgnoredAny, _>(executor)
            .await?
            .is_some())
    }

    /// Record this request and what it produced.
    pub(crate) async fn record<'e, T, E>(&self, executor: E, result: &T) -> Result<()>
    where
        T: Serialize + ?Sized,
        E: sqlx::Executor<'e, Database = Sqlite>,
    {
        sqlx::query("INSERT INTO learning_operations(operation_id,program_id,scope,kind,subject_id,payload_hash,result_json,created_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(self.id)
            .bind(self.program_id)
            .bind(self.scope)
            .bind(self.kind)
            .bind(self.subject_id)
            .bind(self.payload_hash)
            .bind(encode(result)?)
            .bind(now())
            .execute(executor)
            .await
            .map_err(db)?;
        Ok(())
    }
}

async fn find<'e, E>(executor: E, operation_id: &str) -> Result<Option<RecordedOperation>>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query("SELECT scope,kind,program_id,subject_id,payload_hash,result_json FROM learning_operations WHERE operation_id=?")
        .bind(operation_id)
        .fetch_optional(executor)
        .await
        .map_err(db)?;
    Ok(row.map(|row| RecordedOperation {
        scope: row.get("scope"),
        kind: row.get("kind"),
        program_id: row.get("program_id"),
        subject_id: row.get("subject_id"),
        payload_hash: row.get("payload_hash"),
        result_json: row.get("result_json"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::tests::pool;

    const CONFLICT: &str = "Operation ID was already used with different test data.";

    fn operation<'a>(kind: &'a str, payload_hash: &'a str) -> Operation<'a> {
        Operation {
            id: "6f9d1f8e-6d4b-4b8e-9a51-3c2f1e0d9b7a",
            scope: "canvas",
            kind,
            program_id: None,
            subject_id: Some("canvas-1"),
            payload_hash,
            conflict: CONFLICT,
        }
    }

    #[test]
    fn an_identical_request_is_a_replay_and_anything_else_conflicts() {
        let recorded = RecordedOperation {
            scope: "canvas".into(),
            kind: "save".into(),
            program_id: None,
            subject_id: Some("canvas-1".into()),
            payload_hash: "a".into(),
            result_json: "null".into(),
        };
        assert_eq!(operation("save", "a").check(&recorded), Ok(()));
        let conflict = Err(OperationConflict(CONFLICT));
        assert_eq!(operation("save", "b").check(&recorded), conflict);
        assert_eq!(operation("create", "a").check(&recorded), conflict);
        let other_subject = Operation {
            subject_id: Some("canvas-2"),
            ..operation("save", "a")
        };
        assert_eq!(other_subject.check(&recorded), conflict);
        let other_scope = Operation {
            scope: "source",
            ..operation("save", "a")
        };
        assert_eq!(other_scope.check(&recorded), conflict);
    }

    #[tokio::test]
    async fn the_ledger_returns_the_first_result_and_refuses_a_reused_id() -> Result<()> {
        let pool = pool().await?;
        let first = operation("save", "a");
        assert_eq!(first.replay::<i64, _>(&pool).await?, None);
        assert!(!first.seen(&pool).await?);
        let mut tx = pool.begin().await.map_err(db)?;
        first.record(&mut *tx, &7_i64).await?;
        tx.commit().await.map_err(db)?;
        assert_eq!(first.replay::<i64, _>(&pool).await?, Some(7));
        assert!(first.seen(&pool).await?);
        let reused = operation("save", "b").replay::<i64, _>(&pool).await;
        assert!(matches!(reused, Err(AppError::InvalidInput(message)) if message == CONFLICT));
        Ok(())
    }

    #[tokio::test]
    async fn a_rolled_back_transaction_leaves_no_ledger_row() -> Result<()> {
        let pool = pool().await?;
        let op = operation("save", "a");
        let mut tx = pool.begin().await.map_err(db)?;
        op.record(&mut *tx, &()).await?;
        tx.rollback().await.map_err(db)?;
        assert!(!op.seen(&pool).await?);
        Ok(())
    }
}
