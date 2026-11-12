//! The idempotency ledger behind every Learning command that takes an
//! operation ID. A client that retries sends the same ID; the ledger answers
//! with the first attempt's result instead of repeating the work, and refuses
//! an ID that was already spent on a different request.
//!
//! Callers look up and record inside their own transaction, so the ledger row
//! commits or rolls back with the work it describes.
use self::repository::RecordedOperation;
use lattice_core::shared::error::AppError;

mod repository;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::pool;
    use lattice_core::shared::error::Result;
    use lattice_core::shared::persistence::Database;

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
        let db = Database::new(pool().await?);
        let first = operation("save", "a");
        let mut read = db.begin_read().await?;
        assert_eq!(first.replay::<i64>(&mut read).await?, None);
        assert!(!first.seen(&mut read).await?);
        drop(read);
        let mut tx = db.begin_write().await?;
        first.record(&mut tx, &7_i64).await?;
        tx.commit().await?;
        let mut read = db.begin_read().await?;
        assert_eq!(first.replay::<i64>(&mut read).await?, Some(7));
        assert!(first.seen(&mut read).await?);
        let reused = operation("save", "b").replay::<i64>(&mut read).await;
        assert!(matches!(reused, Err(AppError::InvalidInput(message)) if message == CONFLICT));
        Ok(())
    }

    #[tokio::test]
    async fn forgetting_frees_the_id_for_the_same_request_only() -> Result<()> {
        let db = Database::new(pool().await?);
        let first = operation("save", "a");
        let mut tx = db.begin_write().await?;
        first.record(&mut tx, &1_i64).await?;
        operation("save", "b").forget(&mut tx).await?;
        assert_eq!(first.replay::<i64>(&mut tx).await?, Some(1));
        first.forget(&mut tx).await?;
        assert!(!first.seen(&mut tx).await?);
        first.record(&mut tx, &2_i64).await?;
        assert_eq!(first.replay::<i64>(&mut tx).await?, Some(2));
        Ok(())
    }

    #[tokio::test]
    async fn a_rolled_back_transaction_leaves_no_ledger_row() -> Result<()> {
        let db = Database::new(pool().await?);
        let op = operation("save", "a");
        let mut tx = db.begin_write().await?;
        op.record(&mut tx, &()).await?;
        tx.rollback().await?;
        let mut read = db.begin_read().await?;
        assert!(!op.seen(&mut read).await?);
        Ok(())
    }
}
