use super::*;

/// The active version of a live source, with what the dedupe compares.
pub(in super::super) struct LiveSource {
    pub saved: SavedSource,
    pub kind: String,
    pub origin: String,
    pub requested_url: Option<String>,
    pub content_sha256: String,
}

fn chosen_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ChosenVersion> {
    Ok(ChosenVersion {
        source_id: row.get("source_id"),
        version_id: row.get("version_id"),
        text: row.get("full_text"),
        added_by: parse_added_by(row.get("added_by"))?,
        selection_note: row.get("selection_note"),
    })
}

pub(in super::super) async fn live_sources(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<Vec<LiveSource>> {
    let rows = sqlx::query("SELECT s.id AS source_id,s.kind,s.origin,s.requested_url AS source_url,s.added_by,v.id AS version_id,v.title,v.requested_url,v.resolved_url,v.content_sha256,v.snapshot_hash FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL ORDER BY s.created_at,s.id")
        .bind(program_id)
        .fetch_all(pool)
        .await
        .map_err(db)?;
    rows.into_iter()
        .map(|row| {
            let requested_url: Option<String> = row
                .get::<Option<String>, _>("requested_url")
                .or_else(|| row.get("source_url"));
            let resolved_url: Option<String> = row.get("resolved_url");
            Ok(LiveSource {
                saved: SavedSource {
                    source_id: row.get("source_id"),
                    version_id: row.get("version_id"),
                    title: row.get("title"),
                    url: resolved_url.or_else(|| requested_url.clone()),
                    added_by: parse_added_by(row.get("added_by"))?,
                    snapshot_hash: row.get("snapshot_hash"),
                },
                kind: row.get("kind"),
                origin: row.get("origin"),
                requested_url,
                content_sha256: row.get("content_sha256"),
            })
        })
        .collect()
}

pub(in super::super) async fn chosen_versions(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<Vec<ChosenVersion>> {
    sqlx::query("SELECT s.id AS source_id,v.id AS version_id,v.full_text,s.added_by,s.selection_note FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL ORDER BY s.created_at,s.id")
        .bind(program_id)
        .fetch_all(pool)
        .await
        .map_err(db)?
        .iter()
        .map(chosen_from_row)
        .collect()
}

pub(in super::super) async fn chosen_version(
    pool: &SqlitePool,
    program_id: &str,
    version_id: &str,
) -> Result<Option<ChosenVersion>> {
    sqlx::query("SELECT l.id AS source_id,v.id AS version_id,v.full_text,l.added_by,l.selection_note FROM learning_source_versions v JOIN learning_source_library l ON l.id=v.source_id AND l.program_id=v.program_id WHERE v.program_id=? AND v.id=?")
        .bind(program_id)
        .bind(version_id)
        .fetch_optional(pool)
        .await
        .map_err(db)?
        .map(|row| chosen_from_row(&row))
        .transpose()
}

pub(in super::super) async fn set_snapshot_hash(
    pool: &SqlitePool,
    program_id: &str,
    version_id: &str,
    sha256: &str,
) -> Result<()> {
    sqlx::query("UPDATE learning_source_versions SET snapshot_hash=? WHERE program_id=? AND id=?")
        .bind(sha256)
        .bind(program_id)
        .bind(version_id)
        .execute(pool)
        .await
        .map_err(db)?;
    Ok(())
}
