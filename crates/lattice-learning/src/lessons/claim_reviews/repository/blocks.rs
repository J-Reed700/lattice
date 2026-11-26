//! A lesson's sections and its stored verification report.

use lattice_core::shared::error::AppError;
use std::collections::HashMap;

use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::{Database, WriteTx};
use sqlx::{Row, SqliteConnection};

fn ordinal(unit: usize) -> i64 {
    i64::try_from(unit).unwrap_or(i64::MAX)
}

/// A lesson block joined with its course and any statement review state.
pub(crate) struct SectionRow {
    pub program_id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub review_state: Option<String>,
}

pub(crate) async fn section_row(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    unit: usize,
) -> Result<Option<SectionRow>> {
    let Ok(unit_ordinal) = i64::try_from(unit) else {
        return Ok(None);
    };
    let row = sqlx::query(
        "SELECT l.program_id, b.kind, b.title, b.body, r.state AS review_state \
         FROM learning_blocks b \
         JOIN learning_lessons l ON l.id=b.lesson_id \
         LEFT JOIN learning_lesson_reviews r ON r.lesson_id=b.lesson_id \
         WHERE b.lesson_id=? AND b.ordinal=?",
    )
    .bind(lesson_id)
    .bind(unit_ordinal)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|row| SectionRow {
        program_id: row.get("program_id"),
        kind: row.get("kind"),
        title: row.get("title"),
        body: row.get("body"),
        review_state: row.get("review_state"),
    }))
}

pub(crate) async fn section_count(
    connection: &mut SqliteConnection,
    lesson_id: &str,
) -> Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM learning_blocks WHERE lesson_id=?")
        .bind(lesson_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(AppError::from)
}

pub(crate) async fn write_section(
    tx: &mut WriteTx,
    lesson_id: &str,
    unit: usize,
    title: &str,
    body: &str,
) -> Result<()> {
    sqlx::query("UPDATE learning_blocks SET title=?, body=? WHERE lesson_id=? AND ordinal=?")
        .bind(title)
        .bind(body)
        .bind(lesson_id)
        .bind(ordinal(unit))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// The published sections' text, by section index, within an open read connection.
pub(in crate::lessons::claim_reviews) async fn section_bodies_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
) -> Result<HashMap<usize, String>> {
    let rows = sqlx::query("SELECT ordinal, body FROM learning_blocks WHERE lesson_id=?")
        .bind(lesson_id)
        .fetch_all(&mut *connection)
        .await?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            Some((
                usize::try_from(row.get::<i64, _>("ordinal")).ok()?,
                row.get::<String, _>("body"),
            ))
        })
        .collect())
}

/// The published sections' text, by section index.
pub(in crate::lessons::claim_reviews) async fn section_bodies(
    database: &Database,
    lesson_id: &str,
) -> Result<HashMap<usize, String>> {
    let mut read = database.begin_read().await?;
    let bodies = section_bodies_in(&mut *read, lesson_id).await?;
    read.commit().await?;
    Ok(bodies)
}

/// The lesson's stored verification report, as JSON.
pub(in crate::lessons::claim_reviews) async fn verification_report(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(AppError::from)
}

pub(in crate::lessons::claim_reviews) async fn write_verification_report(
    tx: &mut WriteTx,
    program_id: &str,
    lesson_id: &str,
    content_sha256: &str,
    report_json: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE learning_lesson_verifications SET content_sha256=?,report_json=? WHERE program_id=? AND lesson_id=?",
    )
    .bind(content_sha256)
    .bind(report_json)
    .bind(program_id)
    .bind(lesson_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
