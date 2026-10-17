//! The `chat_images` rows (§7.5). A row is written before its file, leased
//! to the turn's `request_id` with a NULL `message_id`, and attached to the
//! answer when it persists. Refs are allocated per request inside the
//! insert, so two `find_image` calls in one turn never collide.

use std::collections::HashSet;

use chrono::Utc;
use lattice_core::application::contracts::images::{ChatImageDto, ImageOrigin};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use lattice_core::shared::error::{AppError, Result};

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

const COLUMNS: &str = "id, conversation_id, request_id, message_id, ref AS image_ref, ext, mime, \
    sha256, byte_size, width, height, caption, source_url, source_title, image_url, origin, \
    license, attribution, created_at";

/// Rows in the order the answer numbered them.
const BY_REF: &str = "ORDER BY CAST(substr(ref, 5) AS INTEGER)";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ImageRow {
    pub id: String,
    pub conversation_id: String,
    pub request_id: String,
    pub message_id: Option<String>,
    /// The `ref` column: `img-N`.
    pub image_ref: String,
    pub ext: String,
    pub mime: String,
    pub sha256: String,
    pub byte_size: i64,
    pub width: i64,
    pub height: i64,
    pub caption: String,
    pub source_url: String,
    pub source_title: Option<String>,
    pub image_url: String,
    pub origin: String,
    pub license: Option<String>,
    pub attribution: Option<String>,
    pub created_at: String,
}

impl ImageRow {
    /// `<id>.<ext>`: the file name and the `lattice-image` path.
    pub fn file_name(&self) -> String {
        format!("{}.{}", self.id, self.ext)
    }

    pub fn to_dto(&self) -> ChatImageDto {
        ChatImageDto {
            id: self.id.clone(),
            r#ref: self.image_ref.clone(),
            file_name: self.file_name(),
            mime: self.mime.clone(),
            width: u32::try_from(self.width).unwrap_or(0),
            height: u32::try_from(self.height).unwrap_or(0),
            byte_size: u64::try_from(self.byte_size).unwrap_or(0),
            caption: self.caption.clone(),
            source_url: self.source_url.clone(),
            source_title: self.source_title.clone(),
            // The column's CHECK admits only the two stored values.
            origin: ImageOrigin::parse(&self.origin).unwrap_or(ImageOrigin::Page),
            license: self.license.clone(),
            attribution: self.attribution.clone(),
        }
    }
}

/// A row about to be written; its ref is allocated by the insert.
#[derive(Debug, Clone)]
pub struct NewImageRow {
    pub id: String,
    pub conversation_id: String,
    pub request_id: String,
    pub ext: String,
    pub mime: String,
    pub sha256: String,
    pub byte_size: u64,
    pub width: u32,
    pub height: u32,
    pub caption: String,
    pub source_url: String,
    pub source_title: Option<String>,
    pub image_url: String,
    pub origin: ImageOrigin,
    pub license: Option<String>,
    pub attribution: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImageRepository {
    pool: SqlitePool,
}

impl ImageRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts `row` leased to its request and returns the ref it was given:
    /// one past the request's highest, in the same statement, so concurrent
    /// calls in one turn cannot be handed the same ref.
    pub async fn insert(&self, row: &NewImageRow) -> Result<String> {
        sqlx::query_scalar(
            "INSERT INTO chat_images (id, conversation_id, request_id, message_id, ref, ext, \
             mime, sha256, byte_size, width, height, caption, source_url, source_title, \
             image_url, origin, license, attribution, created_at) \
             SELECT ?, ?, ?, NULL, \
                    'img-' || (COALESCE(MAX(CAST(substr(ref, 5) AS INTEGER)), 0) + 1), \
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ? \
             FROM chat_images WHERE request_id = ? \
             RETURNING ref",
        )
        .bind(&row.id)
        .bind(&row.conversation_id)
        .bind(&row.request_id)
        .bind(&row.ext)
        .bind(&row.mime)
        .bind(&row.sha256)
        .bind(i64::try_from(row.byte_size).unwrap_or(i64::MAX))
        .bind(i64::from(row.width))
        .bind(i64::from(row.height))
        .bind(&row.caption)
        .bind(&row.source_url)
        .bind(&row.source_title)
        .bind(&row.image_url)
        .bind(row.origin.as_str())
        .bind(&row.license)
        .bind(&row.attribution)
        .bind(Utc::now().to_rfc3339())
        .bind(&row.request_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    /// A copy of row `source_id` under `id`, attached to `message_id` in
    /// `conversation_id` and keeping its ref and all it describes. `None`
    /// when the source row is gone.
    pub async fn insert_copy(
        &self,
        source_id: &str,
        id: &str,
        conversation_id: &str,
        request_id: &str,
        message_id: &str,
    ) -> Result<Option<ImageRow>> {
        sqlx::query_as(&format!(
            "INSERT INTO chat_images (id, conversation_id, request_id, message_id, ref, ext, \
             mime, sha256, byte_size, width, height, caption, source_url, source_title, \
             image_url, origin, license, attribution, created_at) \
             SELECT ?, ?, ?, ?, ref, ext, mime, sha256, byte_size, width, height, caption, \
                    source_url, source_title, image_url, origin, license, attribution, ? \
             FROM chat_images WHERE id = ? \
             RETURNING {COLUMNS}"
        ))
        .bind(id)
        .bind(conversation_id)
        .bind(request_id)
        .bind(message_id)
        .bind(Utc::now().to_rfc3339())
        .bind(source_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    pub async fn get(&self, id: &str) -> Result<Option<ImageRow>> {
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM chat_images WHERE id = ?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)
    }

    /// Gives the request's leased rows whose ref is in `refs` to
    /// `message_id`, and returns every row of the request that message now
    /// holds.
    pub async fn attach(
        &self,
        request_id: &str,
        message_id: &str,
        refs: &[String],
    ) -> Result<Vec<ImageRow>> {
        if !refs.is_empty() {
            let mut update: QueryBuilder<Sqlite> =
                QueryBuilder::new("UPDATE chat_images SET message_id = ");
            update
                .push_bind(message_id)
                .push(" WHERE request_id = ")
                .push_bind(request_id)
                .push(" AND message_id IS NULL AND ref IN (");
            let mut list = update.separated(", ");
            for r in refs {
                list.push_bind(r);
            }
            list.push_unseparated(")");
            update.build().execute(&self.pool).await.map_err(db)?;
        }
        sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM chat_images WHERE request_id = ? AND message_id = ? {BY_REF}"
        ))
        .bind(request_id)
        .bind(message_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }

    /// The request's rows no message holds.
    pub async fn unattached(&self, request_id: &str) -> Result<Vec<ImageRow>> {
        sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM chat_images WHERE request_id = ? AND message_id IS NULL {BY_REF}"
        ))
        .bind(request_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }

    pub async fn for_message(&self, message_id: &str) -> Result<Vec<ImageRow>> {
        sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM chat_images WHERE message_id = ? {BY_REF}"
        ))
        .bind(message_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }

    pub async fn for_conversation(&self, conversation_id: &str) -> Result<Vec<ImageRow>> {
        sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM chat_images WHERE conversation_id = ? ORDER BY created_at"
        ))
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }

    pub async fn delete_ids(&self, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut delete: QueryBuilder<Sqlite> =
            QueryBuilder::new("DELETE FROM chat_images WHERE id IN (");
        let mut list = delete.separated(", ");
        for id in ids {
            list.push_bind(id);
        }
        list.push_unseparated(")");
        let done = delete.build().execute(&self.pool).await.map_err(db)?;
        Ok(done.rows_affected())
    }

    /// `(id, ext)` of every row: the files the sweep must keep.
    pub async fn live_files(&self) -> Result<HashSet<(String, String)>> {
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, ext FROM chat_images")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(rows.into_iter().collect())
    }

    /// Rows no message holds. Used only by the startup sweep: a running turn
    /// cannot survive a restart, so no age bound is needed and none is used.
    pub async fn orphans_without_message(&self) -> Result<Vec<ImageRow>> {
        sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM chat_images WHERE message_id IS NULL"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }
}
