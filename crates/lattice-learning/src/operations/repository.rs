use super::Operation;
use crate::persistence::{decode, encode, now};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::WriteTx;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Row, SqliteConnection};

impl Operation<'_> {
    /// The stored result of an earlier identical request, or `None` when the
    /// ID is new.
    pub(crate) async fn replay<T>(&self, connection: &mut SqliteConnection) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let Some(recorded) = find(connection, self.id).await? else {
            return Ok(None);
        };
        self.check(&recorded)?;
        decode(&recorded.result_json).map(Some)
    }

    /// Whether an identical request was already recorded, ignoring its result.
    pub(crate) async fn seen(&self, connection: &mut SqliteConnection) -> Result<bool> {
        Ok(self
            .replay::<serde::de::IgnoredAny>(connection)
            .await?
            .is_some())
    }

    /// Record this request and what it produced.
    pub(crate) async fn record<T>(&self, tx: &mut WriteTx, result: &T) -> Result<()>
    where
        T: Serialize + ?Sized,
    {
        insert(tx, self, &encode(result)?).await
    }

    /// Drop this identical request's row once what it produced is gone, so
    /// the same ID can do the work again inside the caller's transaction. A
    /// row recorded for any other request is left alone.
    pub(crate) async fn forget(&self, tx: &mut WriteTx) -> Result<()> {
        delete(tx, self).await
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

pub(super) async fn insert(
    tx: &mut WriteTx,
    operation: &Operation<'_>,
    result_json: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_operations(operation_id,program_id,scope,kind,subject_id,payload_hash,result_json,created_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(operation.id)
        .bind(operation.program_id)
        .bind(operation.scope)
        .bind(operation.kind)
        .bind(operation.subject_id)
        .bind(operation.payload_hash)
        .bind(result_json)
        .bind(now())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(super) async fn delete(tx: &mut WriteTx, operation: &Operation<'_>) -> Result<()> {
    sqlx::query("DELETE FROM learning_operations WHERE operation_id=? AND scope=? AND kind=? AND payload_hash=?")
        .bind(operation.id)
        .bind(operation.scope)
        .bind(operation.kind)
        .bind(operation.payload_hash)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(super) async fn find(
    connection: &mut SqliteConnection,
    operation_id: &str,
) -> Result<Option<RecordedOperation>> {
    let row = sqlx::query("SELECT scope,kind,program_id,subject_id,payload_hash,result_json FROM learning_operations WHERE operation_id=?")
        .bind(operation_id)
        .fetch_optional(connection)
        .await?;
    Ok(row.map(|row| RecordedOperation {
        scope: row.get("scope"),
        kind: row.get("kind"),
        program_id: row.get("program_id"),
        subject_id: row.get("subject_id"),
        payload_hash: row.get("payload_hash"),
        result_json: row.get("result_json"),
    }))
}
