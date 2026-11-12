//! What the tutor reads and binds in the database.

use super::binding::CourseBinding;
use super::cards::DueCard;
use crate::persistence::db;
use futures::stream::BoxStream;
use futures::{StreamExt, TryStreamExt};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, WriteTx};
use sqlx::{Row, SqliteConnection};

pub(super) async fn program_status(
    conn: &mut SqliteConnection,
    program_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT status FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(db)
}

pub(super) async fn lesson_in_program(
    conn: &mut SqliteConnection,
    lesson_id: &str,
    program_id: &str,
) -> Result<bool> {
    let owned: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM learning_lessons WHERE id=? AND program_id=?")
            .bind(lesson_id)
            .bind(program_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(db)?;
    Ok(owned.is_some())
}

/// Binds a conversation to a course; `false` when there is no such chat or it
/// is a folder's thread.
pub(super) async fn bind_course(
    tx: &mut WriteTx,
    conversation_id: &str,
    program_id: &str,
    lesson_id: Option<&str>,
) -> Result<bool> {
    let bound = sqlx::query(
        "UPDATE conversations SET program_id=?, lesson_id=? WHERE id=? AND explorer_root IS NULL",
    )
    .bind(program_id)
    .bind(lesson_id)
    .bind(conversation_id)
    .execute(&mut **tx)
    .await
    .map_err(db)?;
    Ok(bound.rows_affected() > 0)
}

pub(super) async fn conversation_course(
    database: &Database,
    conversation_id: &str,
) -> Result<Option<CourseBinding>> {
    let mut read = database.begin_read().await.map_err(db)?;
    let row = sqlx::query("SELECT program_id, lesson_id FROM conversations WHERE id=?")
        .bind(conversation_id)
        .fetch_optional(&mut *read)
        .await
        .map_err(db)?;
    read.commit().await.map_err(db)?;
    Ok(row.and_then(|row| {
        let program_id: Option<String> = row.get("program_id");
        program_id.map(|program_id| CourseBinding {
            program_id,
            lesson_id: row.get("lesson_id"),
        })
    }))
}

/// The completed tutor replies of a chat, newest first.
pub(super) fn assistant_replies<'a>(
    conn: &'a mut SqliteConnection,
    conversation_id: &'a str,
) -> BoxStream<'a, Result<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT content FROM conversation_messages WHERE conversation_id=? AND role='assistant' AND status='completed' ORDER BY sequence DESC",
    )
    .bind(conversation_id)
    .fetch(conn)
    .map_err(db)
    .boxed()
}

pub(super) async fn due_count(database: &Database, program_id: &str, at: i64) -> Result<i64> {
    let mut read = database.begin_read().await.map_err(db)?;
    let due = sqlx::query_scalar(
        "SELECT count(*) FROM study_cards c JOIN learning_memory m ON m.deck_id=c.deck_id WHERE m.program_id=? AND c.due_at<=?",
    )
    .bind(program_id)
    .bind(at)
    .fetch_one(&mut *read)
    .await
    .map_err(db)?;
    read.commit().await.map_err(db)?;
    Ok(due)
}

/// The deck's cards due at `at`, soonest first.
pub(super) async fn due_cards(
    database: &Database,
    program_id: &str,
    at: i64,
) -> Result<Vec<DueCard>> {
    let mut read = database.begin_read().await.map_err(db)?;
    let rows = sqlx::query(
        "SELECT c.id,c.question,c.options_json,COALESCE(l.module_id,c.topic) group_key FROM study_cards c JOIN learning_memory m ON m.deck_id=c.deck_id LEFT JOIN learning_card_origins o ON o.card_id=c.id AND o.program_id=m.program_id LEFT JOIN learning_lessons l ON l.id=o.lesson_id WHERE m.program_id=? AND c.due_at<=? ORDER BY c.due_at,c.id",
    )
    .bind(program_id)
    .bind(at)
    .fetch_all(&mut *read)
    .await
    .map_err(db)?;
    read.commit().await.map_err(db)?;
    rows.iter()
        .map(|row| {
            Ok(DueCard {
                id: row.get("id"),
                front: row.get("question"),
                options: serde_json::from_str(row.get("options_json")).map_err(|error| {
                    AppError::InvalidState(format!("Invalid recall options: {error}"))
                })?,
                group: row.get("group_key"),
            })
        })
        .collect()
}

/// A card of the course's deck, as a rating reads it.
pub(super) struct CardToRate {
    pub due_at: i64,
    pub review_count: i64,
    pub format: String,
    pub question: String,
}

pub(super) async fn card_to_rate(
    database: &Database,
    program_id: &str,
    card_id: &str,
) -> Result<Option<CardToRate>> {
    let mut read = database.begin_read().await.map_err(db)?;
    let row = sqlx::query(
        "SELECT c.due_at,c.review_count,c.format,c.question FROM study_cards c JOIN learning_memory m ON m.deck_id=c.deck_id WHERE c.id=? AND m.program_id=?",
    )
    .bind(card_id)
    .bind(program_id)
    .fetch_optional(&mut *read)
    .await
    .map_err(db)?;
    read.commit().await.map_err(db)?;
    Ok(row.map(|row| CardToRate {
        due_at: row.get("due_at"),
        review_count: row.get("review_count"),
        format: row.get("format"),
        question: row.get("question"),
    }))
}

pub(super) async fn card_due_at(database: &Database, card_id: &str) -> Result<i64> {
    let mut read = database.begin_read().await.map_err(db)?;
    let due_at = sqlx::query_scalar("SELECT due_at FROM study_cards WHERE id=?")
        .bind(card_id)
        .fetch_one(&mut *read)
        .await
        .map_err(db)?;
    read.commit().await.map_err(db)?;
    Ok(due_at)
}

/// A source version's title and saved text.
pub(super) async fn source_text(
    database: &Database,
    program_id: &str,
    version_id: &str,
) -> Result<Option<(String, String)>> {
    let mut read = database.begin_read().await.map_err(db)?;
    let row = sqlx::query(
        "SELECT title, full_text FROM learning_source_versions WHERE program_id=? AND id=?",
    )
    .bind(program_id)
    .bind(version_id)
    .fetch_optional(&mut *read)
    .await
    .map_err(db)?;
    read.commit().await.map_err(db)?;
    Ok(row.map(|row| (row.get("title"), row.get("full_text"))))
}
