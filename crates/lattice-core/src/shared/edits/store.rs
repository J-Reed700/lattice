//! SQLite persistence for `content_edits` and `content_edit_receipts`.

use std::collections::HashMap;

use serde_json::Value;
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, Sqlite, SqliteConnection};

use super::types::{
    ContentAddress, ContentEditOutcome, ContentEditQuery, ContentRef, EditOp, EditOrigin,
};
use crate::shared::error::Result;
use crate::shared::persistence::WriteTx;

#[derive(Debug, Clone)]
pub(super) struct StoredEditRow {
    pub id: String,
    pub kind: String,
    pub content_key: String,
    pub scope_feature: String,
    pub scope_container: String,
    pub seq: i64,
    pub origin: EditOrigin,
    pub target: Option<ContentAddress>,
    pub ops: Vec<EditOp>,
    pub inverse: Vec<EditOp>,
    pub before_revision: String,
    pub after_revision: String,
    pub reverts_edit_id: Option<String>,
    pub request_sha256: String,
    pub outcome: Option<ContentEditOutcome>,
    pub created_at: i64,
}

pub(super) struct NewEditRow<'a> {
    pub id: &'a str,
    pub content: &'a ContentRef,
    pub scope_feature: &'a str,
    pub scope_container: &'a str,
    pub seq: i64,
    pub origin: &'a EditOrigin,
    pub target: Option<&'a ContentAddress>,
    pub ops: &'a [EditOp],
    pub inverse: &'a [EditOp],
    pub before_revision: &'a str,
    pub after_revision: &'a str,
    pub reverts_edit_id: Option<&'a str>,
    pub request_sha256: &'a str,
    pub app_version: &'a str,
    pub created_at: i64,
}

const COLUMNS: &str = "id, kind, content_key, scope_feature, scope_container, seq, origin_json, \
    target_json, ops_json, inverse_json, before_revision, after_revision, reverts_edit_id, \
    request_sha256, outcome_json, created_at";

fn parse_row(row: &SqliteRow) -> Result<StoredEditRow> {
    let origin: EditOrigin = serde_json::from_str(&row.get::<String, _>("origin_json"))?;
    let target: Option<ContentAddress> = row
        .get::<Option<String>, _>("target_json")
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?;
    let ops: Vec<EditOp> = serde_json::from_str(&row.get::<String, _>("ops_json"))?;
    let inverse: Vec<EditOp> = serde_json::from_str(&row.get::<String, _>("inverse_json"))?;
    let outcome: Option<ContentEditOutcome> = row
        .get::<Option<String>, _>("outcome_json")
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?;
    Ok(StoredEditRow {
        id: row.get("id"),
        kind: row.get("kind"),
        content_key: row.get("content_key"),
        scope_feature: row.get("scope_feature"),
        scope_container: row.get("scope_container"),
        seq: row.get("seq"),
        origin,
        target,
        ops,
        inverse,
        before_revision: row.get("before_revision"),
        after_revision: row.get("after_revision"),
        reverts_edit_id: row.get("reverts_edit_id"),
        request_sha256: row.get("request_sha256"),
        outcome,
        created_at: row.get("created_at"),
    })
}

pub(super) async fn find_edit_by_id(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<StoredEditRow>> {
    let sql = format!("SELECT {COLUMNS} FROM content_edits WHERE id = ?");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(connection)
        .await?;
    row.as_ref().map(parse_row).transpose()
}

pub(super) async fn load_entity_edits(
    connection: &mut SqliteConnection,
    content: &ContentRef,
) -> Result<Vec<StoredEditRow>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM content_edits WHERE kind = ? AND content_key = ? ORDER BY seq ASC"
    );
    let rows = sqlx::query(&sql)
        .bind(&content.kind)
        .bind(&content.key)
        .fetch_all(connection)
        .await?;
    rows.iter().map(parse_row).collect()
}

pub(super) async fn next_seq(
    connection: &mut SqliteConnection,
    content: &ContentRef,
) -> Result<i64> {
    let seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM content_edits WHERE kind = ? AND content_key = ?",
    )
    .bind(&content.kind)
    .bind(&content.key)
    .fetch_one(connection)
    .await?;
    Ok(seq)
}

pub(super) async fn insert_edit(tx: &mut WriteTx, row: &NewEditRow<'_>) -> Result<()> {
    let origin_json = serde_json::to_string(row.origin)?;
    let target_json = row.target.map(serde_json::to_string).transpose()?;
    let ops_json = serde_json::to_string(row.ops)?;
    let inverse_json = serde_json::to_string(row.inverse)?;
    sqlx::query(
        "INSERT INTO content_edits (id, kind, content_key, scope_feature, scope_container, seq, \
         origin_json, target_json, ops_json, inverse_json, before_revision, after_revision, \
         reverts_edit_id, request_sha256, outcome_json, app_version, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)",
    )
    .bind(row.id)
    .bind(&row.content.kind)
    .bind(&row.content.key)
    .bind(row.scope_feature)
    .bind(row.scope_container)
    .bind(row.seq)
    .bind(origin_json)
    .bind(target_json)
    .bind(ops_json)
    .bind(inverse_json)
    .bind(row.before_revision)
    .bind(row.after_revision)
    .bind(row.reverts_edit_id)
    .bind(row.request_sha256)
    .bind(row.app_version)
    .bind(row.created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn set_outcome(
    tx: &mut WriteTx,
    edit_id: &str,
    outcome: &ContentEditOutcome,
) -> Result<()> {
    let outcome_json = serde_json::to_string(outcome)?;
    sqlx::query("UPDATE content_edits SET outcome_json = ? WHERE id = ?")
        .bind(outcome_json)
        .bind(edit_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(super) async fn insert_receipt(
    tx: &mut WriteTx,
    edit_id: &str,
    reactor: &str,
    receipt: &Value,
) -> Result<()> {
    let receipt_json = serde_json::to_string(receipt)?;
    sqlx::query(
        "INSERT INTO content_edit_receipts (edit_id, reactor, receipt_json) VALUES (?, ?, ?)",
    )
    .bind(edit_id)
    .bind(reactor)
    .bind(receipt_json)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn receipts_for_edit(
    connection: &mut SqliteConnection,
    edit_id: &str,
) -> Result<HashMap<String, Value>> {
    let rows =
        sqlx::query("SELECT reactor, receipt_json FROM content_edit_receipts WHERE edit_id = ?")
            .bind(edit_id)
            .fetch_all(connection)
            .await?;
    let mut receipts = HashMap::with_capacity(rows.len());
    for row in rows {
        let reactor: String = row.get("reactor");
        let receipt: Value = serde_json::from_str(&row.get::<String, _>("receipt_json"))?;
        receipts.insert(reactor, receipt);
    }
    Ok(receipts)
}

pub(super) async fn receipt_for_edit_and_reactor(
    connection: &mut SqliteConnection,
    edit_id: &str,
    reactor: &str,
) -> Result<Option<Value>> {
    let json_text: Option<String> = sqlx::query_scalar(
        "SELECT receipt_json FROM content_edit_receipts WHERE edit_id = ? AND reactor = ?",
    )
    .bind(edit_id)
    .bind(reactor)
    .fetch_optional(connection)
    .await?;
    json_text
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(Into::into)
}

pub(super) async fn query_edits(
    connection: &mut SqliteConnection,
    query: &ContentEditQuery,
) -> Result<Vec<StoredEditRow>> {
    let mut builder: QueryBuilder<'_, Sqlite> = QueryBuilder::new("SELECT ");
    builder.push(COLUMNS);
    builder.push(" FROM content_edits WHERE 1 = 1");

    if let Some(scope) = &query.scope {
        builder.push(" AND scope_feature = ");
        builder.push_bind(&scope.feature);
        builder.push(" AND scope_container = ");
        builder.push_bind(&scope.container);
    }

    if !query.contents.is_empty() {
        builder.push(" AND (");
        for (index, content) in query.contents.iter().enumerate() {
            if index > 0 {
                builder.push(" OR ");
            }
            builder.push("(kind = ");
            builder.push_bind(&content.kind);
            builder.push(" AND content_key = ");
            builder.push_bind(&content.key);
            builder.push(")");
        }
        builder.push(")");
    }

    if let Some(since) = query.since {
        builder.push(" AND created_at >= ");
        builder.push_bind(since);
    }

    builder.push(" ORDER BY created_at DESC, seq DESC, rowid DESC");
    let rows = builder.build().fetch_all(connection).await?;
    rows.iter().map(parse_row).collect()
}
