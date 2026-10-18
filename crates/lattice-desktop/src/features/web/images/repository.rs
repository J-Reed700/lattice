//! The `chat_images` rows (§7.5). A row is written before its file, leased
//! to the turn's `request_id` with a NULL `message_id`, and attached to the
//! answer when it persists. Refs are allocated per request inside the
//! insert, so two `find_image` calls in one turn never collide.
//!
//! A picture the reader attaches is a row too (`origin = 'attachment'`, no
//! source or image URL), leased to a composer draft (`draft:<id>`) until the
//! user message that carries it claims it and its refs become `att-1…`.

use std::collections::HashSet;

use chrono::Utc;
use lattice_core::application::contracts::images::{ChatImageDto, ImageOrigin};
use lattice_core::application::ports::chat_images::DRAFT_REQUEST_PREFIX;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use lattice_core::shared::error::{AppError, Result};

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

const COLUMNS: &str = "id, conversation_id, request_id, message_id, ref AS image_ref, ext, mime, \
    sha256, byte_size, width, height, caption, source_url, source_title, image_url, origin, \
    license, attribution, original_name, created_at";

/// The ref prefix of a found picture (`img-N`).
pub const FOUND_REF_PREFIX: &str = "img-";

/// The ref prefix of an attached picture (`att-N`). The same length as
/// [`FOUND_REF_PREFIX`], so [`BY_REF`] orders both.
pub const ATTACHED_REF_PREFIX: &str = "att-";

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
    pub source_url: Option<String>,
    pub source_title: Option<String>,
    pub image_url: Option<String>,
    pub origin: String,
    pub license: Option<String>,
    pub attribution: Option<String>,
    pub original_name: Option<String>,
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
            // The column's CHECK admits only the stored values.
            origin: ImageOrigin::parse(&self.origin).unwrap_or(ImageOrigin::Page),
            license: self.license.clone(),
            attribution: self.attribution.clone(),
            original_name: self.original_name.clone(),
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
    pub source_url: Option<String>,
    pub source_title: Option<String>,
    pub image_url: Option<String>,
    pub origin: ImageOrigin,
    pub license: Option<String>,
    pub attribution: Option<String>,
    pub original_name: Option<String>,
}

impl NewImageRow {
    /// `att-` for an attached picture, `img-` for one an answer found.
    fn ref_prefix(&self) -> &'static str {
        if self.origin == ImageOrigin::Attachment {
            ATTACHED_REF_PREFIX
        } else {
            FOUND_REF_PREFIX
        }
    }
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
    /// one past the request's highest of the same kind (`img-` or `att-`), in
    /// the same statement, so concurrent calls in one turn cannot be handed
    /// the same ref.
    pub async fn insert(&self, row: &NewImageRow) -> Result<String> {
        let prefix = row.ref_prefix();
        sqlx::query_scalar(
            "INSERT INTO chat_images (id, conversation_id, request_id, message_id, ref, ext, \
             mime, sha256, byte_size, width, height, caption, source_url, source_title, \
             image_url, origin, license, attribution, original_name, created_at) \
             SELECT ?, ?, ?, NULL, \
                    ? || (COALESCE(MAX(CAST(substr(ref, 5) AS INTEGER)), 0) + 1), \
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ? \
             FROM chat_images WHERE request_id = ? AND substr(ref, 1, 4) = ? \
             RETURNING ref",
        )
        .bind(&row.id)
        .bind(&row.conversation_id)
        .bind(&row.request_id)
        .bind(prefix)
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
        .bind(&row.original_name)
        .bind(Utc::now().to_rfc3339())
        .bind(&row.request_id)
        .bind(prefix)
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
             image_url, origin, license, attribution, original_name, created_at) \
             SELECT ?, ?, ?, ?, ref, ext, mime, sha256, byte_size, width, height, caption, \
                    source_url, source_title, image_url, origin, license, attribution, \
                    original_name, ? \
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

    /// A copy of the conversation's row `source_id` under `id`, leased to the
    /// draft `request_id` as `att-1`, keeping all it describes. `None` when
    /// the conversation has no such row.
    pub async fn insert_draft_copy(
        &self,
        source_id: &str,
        id: &str,
        conversation_id: &str,
        request_id: &str,
    ) -> Result<Option<ImageRow>> {
        sqlx::query_as(&format!(
            "INSERT INTO chat_images (id, conversation_id, request_id, message_id, ref, ext, \
             mime, sha256, byte_size, width, height, caption, source_url, source_title, \
             image_url, origin, license, attribution, original_name, created_at) \
             SELECT ?, conversation_id, ?, NULL, '{ATTACHED_REF_PREFIX}1', ext, mime, sha256, \
                    byte_size, width, height, caption, source_url, source_title, image_url, \
                    origin, license, attribution, original_name, ? \
             FROM chat_images WHERE id = ? AND conversation_id = ? \
             RETURNING {COLUMNS}"
        ))
        .bind(id)
        .bind(request_id)
        .bind(Utc::now().to_rfc3339())
        .bind(source_id)
        .bind(conversation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    /// The conversation's rows among `ids`, in the order `ids` names them.
    pub async fn in_conversation(
        &self,
        conversation_id: &str,
        ids: &[String],
    ) -> Result<Vec<ImageRow>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut select: QueryBuilder<Sqlite> = QueryBuilder::new(format!(
            "SELECT {COLUMNS} FROM chat_images WHERE conversation_id = "
        ));
        select.push_bind(conversation_id).push(" AND id IN (");
        let mut list = select.separated(", ");
        for id in ids {
            list.push_bind(id);
        }
        list.push_unseparated(")");
        let rows: Vec<ImageRow> = select
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(in_order(rows, ids))
    }

    /// Gives the conversation's rows among `ids` that no other message holds
    /// to `message_id` under `request_id`, numbered `att-1…` in the order
    /// given, and returns them in that order. One transaction: refs move
    /// through a per-row placeholder first, so renumbering never collides
    /// with the request's `UNIQUE (request_id, ref)`.
    pub async fn claim(
        &self,
        conversation_id: &str,
        ids: &[String],
        message_id: &str,
        request_id: &str,
    ) -> Result<Vec<ImageRow>> {
        let candidates = self.in_conversation(conversation_id, ids).await?;
        let claimable: Vec<String> = candidates
            .into_iter()
            .filter(|row| {
                row.message_id
                    .as_deref()
                    .is_none_or(|held| held == message_id)
            })
            .map(|row| row.id)
            .collect();
        if claimable.is_empty() {
            return Ok(Vec::new());
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        for id in &claimable {
            sqlx::query(
                "UPDATE chat_images SET message_id = ?, request_id = ?, ref = 'claim:' || id \
                 WHERE id = ? AND (message_id IS NULL OR message_id = ?)",
            )
            .bind(message_id)
            .bind(request_id)
            .bind(id)
            .bind(message_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        for (index, id) in claimable.iter().enumerate() {
            sqlx::query("UPDATE chat_images SET ref = ? WHERE id = ? AND message_id = ?")
                .bind(format!("{ATTACHED_REF_PREFIX}{}", index + 1))
                .bind(id)
                .bind(message_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        let rows = self.in_conversation(conversation_id, &claimable).await?;
        Ok(rows
            .into_iter()
            .filter(|row| row.message_id.as_deref() == Some(message_id))
            .collect())
    }

    /// The conversation's draft rows among `ids` that no message holds.
    pub async fn unclaimed_drafts(
        &self,
        conversation_id: &str,
        ids: &[String],
    ) -> Result<Vec<ImageRow>> {
        Ok(self
            .in_conversation(conversation_id, ids)
            .await?
            .into_iter()
            .filter(|row| {
                row.message_id.is_none() && row.request_id.starts_with(DRAFT_REQUEST_PREFIX)
            })
            .collect())
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
    /// and a composer's draft cannot survive a restart, so no age bound is
    /// needed beyond the sweep's own start (see `ChatImageGc::sweep`).
    pub async fn orphans_without_message(&self) -> Result<Vec<ImageRow>> {
        sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM chat_images WHERE message_id IS NULL"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }
}

/// `rows` in the order `ids` names them; ids with no row are skipped.
fn in_order(mut rows: Vec<ImageRow>, ids: &[String]) -> Vec<ImageRow> {
    let mut ordered = Vec::with_capacity(rows.len());
    for id in ids {
        if let Some(at) = rows.iter().position(|row| &row.id == id) {
            ordered.push(rows.swap_remove(at));
        }
    }
    ordered
}
