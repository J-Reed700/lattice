//! A lesson's sections and its stored verification report.

use std::collections::HashMap;

use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::driver::DriverPool;
use sqlx::{Row, SqliteConnection};

use crate::persistence::db;

fn ordinal(unit: usize) -> i64 {
    i64::try_from(unit).unwrap_or(i64::MAX)
}

pub(in crate::lessons::claim_reviews) async fn section_body(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    unit: usize,
) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT body FROM learning_blocks WHERE lesson_id=? AND ordinal=?")
        .bind(lesson_id)
        .bind(ordinal(unit))
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)
}

pub(in crate::lessons::claim_reviews) async fn section_title_body(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    unit: usize,
) -> Result<Option<(String, String)>> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT title, body FROM learning_blocks WHERE lesson_id=? AND ordinal=?",
    )
    .bind(lesson_id)
    .bind(ordinal(unit))
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)
}

pub(in crate::lessons::claim_reviews) async fn section_count(
    connection: &mut SqliteConnection,
    lesson_id: &str,
) -> Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM learning_blocks WHERE lesson_id=?")
        .bind(lesson_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(db)
}

pub(in crate::lessons::claim_reviews) async fn write_section_body(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    unit: usize,
    body: &str,
) -> Result<()> {
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=?")
        .bind(body)
        .bind(lesson_id)
        .bind(ordinal(unit))
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

pub(in crate::lessons::claim_reviews) async fn write_section(
    connection: &mut SqliteConnection,
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
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// The published sections' text, by section index.
pub(in crate::lessons::claim_reviews) async fn section_bodies(
    pool: &impl DriverPool,
    lesson_id: &str,
) -> Result<HashMap<usize, String>> {
    let pool = pool.sqlx_pool();
    let rows = sqlx::query("SELECT ordinal, body FROM learning_blocks WHERE lesson_id=?")
        .bind(lesson_id)
        .fetch_all(pool)
        .await
        .map_err(db)?;
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

pub(in crate::lessons::claim_reviews) async fn lesson_in_course(
    pool: &impl DriverPool,
    program_id: &str,
    lesson_id: &str,
) -> Result<bool> {
    let pool = pool.sqlx_pool();
    let in_course: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM learning_lessons WHERE id=? AND program_id=?")
            .bind(lesson_id)
            .bind(program_id)
            .fetch_optional(pool)
            .await
            .map_err(db)?;
    Ok(in_course.is_some())
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
    .map_err(db)
}

pub(in crate::lessons::claim_reviews) async fn write_verification_report(
    connection: &mut SqliteConnection,
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
    .execute(&mut *connection)
    .await
    .map_err(db)?;
    Ok(())
}
