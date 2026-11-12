//! The `learning_checker_examples` table.

use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, WriteTx};
use sqlx::Row;

use crate::lessons::claim_reviews::checker_examples::{CheckerExample, Label, LabelSource};
use crate::persistence::{decode, encode};

pub(in crate::lessons::claim_reviews) async fn insert(
    database: &Database,
    example: &CheckerExample,
) -> Result<()> {
    let mut tx = database.begin_write().await?;
    insert_in(&mut tx, example).await?;
    tx.commit().await.map_err(AppError::from)
}

pub(in crate::lessons::claim_reviews) async fn insert_in(
    tx: &mut WriteTx,
    example: &CheckerExample,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_checker_examples(id,program_id,lesson_id,claim_id,checker_model,context_window,policy,claim,passages_json,local_verdict,label,label_source,at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&example.id)
        .bind(&example.program_id)
        .bind(&example.lesson_id)
        .bind(&example.claim_id)
        .bind(&example.checker_model)
        .bind(example.context_window)
        .bind(&example.policy)
        .bind(&example.claim)
        .bind(encode(&example.passages)?)
        .bind(&example.local_verdict)
        .bind(example.label.as_str())
        .bind(example.label_source.as_str())
        .bind(example.at)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(in crate::lessons::claim_reviews) async fn delete_for_claim(
    database: &Database,
    claim_id: &str,
    source: LabelSource,
) -> Result<()> {
    let mut tx = database.begin_write().await?;
    delete_for_claim_in(&mut tx, claim_id, source).await?;
    tx.commit().await.map_err(AppError::from)
}

pub(in crate::lessons::claim_reviews) async fn delete_for_claim_in(
    tx: &mut WriteTx,
    claim_id: &str,
    source: LabelSource,
) -> Result<()> {
    sqlx::query("DELETE FROM learning_checker_examples WHERE claim_id=? AND label_source=?")
        .bind(claim_id)
        .bind(source.as_str())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(in crate::lessons::claim_reviews) async fn list(
    database: &Database,
    program_id: Option<&str>,
) -> Result<Vec<CheckerExample>> {
    let mut read = database.begin_read().await?;
    let rows = sqlx::query("SELECT * FROM learning_checker_examples WHERE ?1 IS NULL OR program_id=?1 ORDER BY at,rowid")
        .bind(program_id)
        .fetch_all(&mut *read)
        .await?;
    read.commit().await?;
    rows.iter()
        .map(|row| {
            let label: String = row.get("label");
            let source: String = row.get("label_source");
            Ok(CheckerExample {
                id: row.get("id"),
                program_id: row.get("program_id"),
                lesson_id: row.get("lesson_id"),
                claim_id: row.get("claim_id"),
                checker_model: row.get("checker_model"),
                context_window: row.get("context_window"),
                policy: row.get("policy"),
                claim: row.get("claim"),
                passages: decode(row.get::<String, _>("passages_json").as_str())?,
                local_verdict: row.get("local_verdict"),
                label: Label::from_column(&label)
                    .ok_or_else(|| AppError::Database(format!("Unknown label {label}")))?,
                label_source: LabelSource::from_column(&source)
                    .ok_or_else(|| AppError::Database(format!("Unknown label source {source}")))?,
                at: row.get("at"),
            })
        })
        .collect()
}
