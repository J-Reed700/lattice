use super::Operation;
use crate::persistence::{db, now};
use lattice_core::shared::error::Result;
use sqlx::{Row, Sqlite};

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

pub(super) async fn insert<'e, E>(
    executor: E,
    operation: &Operation<'_>,
    result_json: &str,
) -> Result<()>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    sqlx::query("INSERT INTO learning_operations(operation_id,program_id,scope,kind,subject_id,payload_hash,result_json,created_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(operation.id)
        .bind(operation.program_id)
        .bind(operation.scope)
        .bind(operation.kind)
        .bind(operation.subject_id)
        .bind(operation.payload_hash)
        .bind(result_json)
        .bind(now())
        .execute(executor)
        .await
        .map_err(db)?;
    Ok(())
}

pub(super) async fn delete<'e, E>(executor: E, operation: &Operation<'_>) -> Result<()>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    sqlx::query("DELETE FROM learning_operations WHERE operation_id=? AND scope=? AND kind=? AND payload_hash=?")
        .bind(operation.id)
        .bind(operation.scope)
        .bind(operation.kind)
        .bind(operation.payload_hash)
        .execute(executor)
        .await
        .map_err(db)?;
    Ok(())
}

pub(super) async fn find<'e, E>(
    executor: E,
    operation_id: &str,
) -> Result<Option<RecordedOperation>>
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
