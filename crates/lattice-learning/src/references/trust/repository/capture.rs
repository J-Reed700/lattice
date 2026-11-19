use super::super::capture::{compare_all_saved, Comparison, SavedTexts, VersionText};
use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;
use sqlx::{Row, SqliteConnection};

/// One saved or about-to-be-saved text of a course.
pub(in crate::references::trust) struct CourseText {
    pub source_id: String,
    pub version_id: String,
    pub text: String,
}

pub(in crate::references::trust) async fn active_texts(
    database: &Database,
    program_id: &str,
) -> Result<Vec<CourseText>> {
    let mut read = database.begin_read().await?;
    let texts = active_texts_in(&mut read, program_id).await?;
    read.commit().await?;
    Ok(texts)
}

/// [`compare_saved`](super::super::capture::compare_saved) for versions saved
/// together: each is compared with the ones before it too, as it would be if
/// they were saved in turn.
pub(crate) async fn compare_all(
    database: &Database,
    program_id: &str,
    versions: &[VersionText<'_>],
) -> Result<Vec<Comparison>> {
    let saved = SavedTexts(active_texts(database, program_id).await?);
    compare_all_saved(saved, versions).await
}

pub(in crate::references::trust) async fn active_texts_in(
    conn: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<CourseText>> {
    let saved = sqlx::query(
        "SELECT l.id AS source_id, v.id, v.full_text FROM learning_source_library l JOIN learning_source_versions v ON v.program_id=l.program_id AND v.id=l.active_version_id WHERE l.program_id=? AND l.deleted_at IS NULL",
    )
    .bind(program_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(saved
        .iter()
        .map(|row| CourseText {
            source_id: row.get("source_id"),
            version_id: row.get("id"),
            text: row.get("full_text"),
        })
        .collect())
}

pub(in crate::references::trust) async fn signals_json(
    conn: &mut SqliteConnection,
    program_id: &str,
    version_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT signals_json FROM source_provenance WHERE program_id=? AND source_version_id=?",
    )
    .bind(program_id)
    .bind(version_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::from)
}

pub(in crate::references::trust) async fn other_active_version_ids(
    conn: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT v.id FROM learning_source_library l JOIN learning_source_versions v ON v.program_id=l.program_id AND v.id=l.active_version_id WHERE l.program_id=? AND l.deleted_at IS NULL AND l.id<>?",
    )
    .bind(program_id)
    .bind(source_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::from)
}

pub(in crate::references::trust) async fn version_text(
    connection: &mut SqliteConnection,
    program_id: &str,
    version_id: &str,
) -> Result<String> {
    sqlx::query_scalar("SELECT full_text FROM learning_source_versions WHERE program_id=? AND id=?")
        .bind(program_id)
        .bind(version_id)
        .fetch_one(connection)
        .await
        .map_err(AppError::from)
}

pub(in crate::references::trust) async fn captured_version_of(
    conn: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
    url: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT v.id FROM learning_source_library l JOIN learning_source_versions v ON v.program_id=l.program_id AND v.id=l.active_version_id WHERE l.program_id=? AND l.deleted_at IS NULL AND l.id<>? AND (v.resolved_url=? OR v.requested_url=?) ORDER BY v.acquired_at, v.id LIMIT 1",
    )
    .bind(program_id)
    .bind(source_id)
    .bind(url)
    .bind(url)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::from)
}
