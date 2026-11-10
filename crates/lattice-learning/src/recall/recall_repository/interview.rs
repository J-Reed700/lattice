use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqliteConnection};

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

/// An interview activity as a drill reads it.
pub(in crate::recall) struct InterviewActivity {
    pub title: String,
    pub kind: String,
    pub status: String,
}

pub(in crate::recall) async fn activity(
    conn: &mut SqliteConnection,
    program_id: &str,
    activity_id: &str,
) -> Result<Option<InterviewActivity>> {
    let row = sqlx::query(
        "SELECT title,kind,status FROM learning_practical_activities WHERE program_id=? AND id=?",
    )
    .bind(program_id)
    .bind(activity_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(db)?;
    Ok(row.map(|row| InterviewActivity {
        title: row.get("title"),
        kind: row.get("kind"),
        status: row.get("status"),
    }))
}

/// The ids of the course's ready interview activities, oldest first.
pub(in crate::recall) async fn ready_activity_ids(
    conn: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT id FROM learning_practical_activities WHERE program_id=? AND kind='interview' AND status='ready' ORDER BY created_at,id",
    )
    .bind(program_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(db)
}

/// The path and content of each of the activity's files at `paths`.
pub(in crate::recall) async fn files(
    conn: &mut SqliteConnection,
    activity_id: &str,
    paths: [&str; 2],
) -> Result<Vec<(String, String)>> {
    let files = sqlx::query(
        "SELECT path,content FROM learning_practical_files WHERE activity_id=? AND path IN (?,?)",
    )
    .bind(activity_id)
    .bind(paths[0])
    .bind(paths[1])
    .fetch_all(&mut *conn)
    .await
    .map_err(db)?;
    Ok(files
        .iter()
        .map(|file| (file.get("path"), file.get("content")))
        .collect())
}

/// A source version's title and link.
pub(in crate::recall) async fn source_title_and_link(
    conn: &mut SqliteConnection,
    program_id: &str,
    version_id: &str,
) -> Result<Option<(String, Option<String>)>> {
    let row = sqlx::query("SELECT title,COALESCE(resolved_url,requested_url) url FROM learning_source_versions WHERE program_id=? AND id=?")
        .bind(program_id)
        .bind(version_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(db)?;
    Ok(row.map(|row| (row.get("title"), row.get("url"))))
}
