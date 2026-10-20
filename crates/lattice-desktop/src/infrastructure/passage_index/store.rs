//! The passage store: one `passages.db` per index directory.
//!
//! A document is whatever its owner keys it by (a path relative to a folder,
//! a source version), with a stamp the owner compares to decide whether to
//! read it again, the hash of what was chunked, and whether all its passages
//! are in the saved vectors file. A passage is a span of its document in the
//! owner's units and the text that is indexed. The full-text index follows
//! the passages through triggers, and a vocabulary table over it gives each
//! term's passage count.
//!
//! The schema version is recorded in `meta`. A store made in a directory
//! that has none, or one recorded under another version, starts from an
//! empty directory: passage ids restart with a new store, and a vectors file
//! left beside it would put old vectors behind new passages.

use super::chunking::NewPassage;
use lattice_core::shared::{AppError, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Connection, Row, SqliteConnection, SqlitePool};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DB_FILE: &str = "passages.db";
/// The tables' shape. A store recorded under another version is rebuilt.
const SCHEMA_VERSION: &str = "2";
const META_VERSION: &str = "version";
/// The embedding identity the vectors file beside the store was made with;
/// `vectors` reads and writes it.
pub const META_IDENTITY: &str = "identity";

/// `AUTOINCREMENT` so a passage id is never handed out twice: the vectors
/// file is saved in batches, and an id reused after a crash would put a new
/// passage behind an old passage's vector. `span_start`/`span_end` are the
/// owner's units; the store never reads them.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS index_documents (
    key TEXT PRIMARY KEY,
    stamp TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    embedded INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS passages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    document TEXT NOT NULL,
    span_start INTEGER NOT NULL,
    span_end INTEGER NOT NULL,
    text TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS passages_document ON passages(document);
CREATE VIRTUAL TABLE IF NOT EXISTS passages_fts USING fts5(
    text, document UNINDEXED, content='passages', content_rowid='id',
    tokenize="unicode61 tokenchars '_'"
);
CREATE VIRTUAL TABLE IF NOT EXISTS passages_vocab USING fts5vocab(passages_fts, 'row');
CREATE TRIGGER IF NOT EXISTS passages_ai AFTER INSERT ON passages BEGIN
    INSERT INTO passages_fts(rowid, text, document) VALUES (new.id, new.text, new.document);
END;
CREATE TRIGGER IF NOT EXISTS passages_ad AFTER DELETE ON passages BEGIN
    INSERT INTO passages_fts(passages_fts, rowid, text, document)
    VALUES ('delete', old.id, old.text, old.document);
END;
"#;

/// `column` is `?1` or sits under it as a `/`-separated key; `''` matches
/// everything.
fn under(column: &str) -> String {
    format!("(?1 = '' OR {column} = ?1 OR substr({column}, 1, length(?1) + 1) = ?1 || '/')")
}

/// A pool that timed out is a busy store, not a broken one: the index job
/// waits and resumes from its checkpoints instead of ending as failed.
pub(super) fn db_error(action: &str) -> impl Fn(sqlx::Error) -> AppError + '_ {
    move |error| match error {
        sqlx::Error::PoolTimedOut => AppError::ServiceNotAvailable(format!(
            "Passage index: busy, could not {action}; saved work is retained"
        )),
        error => AppError::Database(format!("Passage index: failed to {action}: {error}")),
    }
}

/// What the store holds for one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The owner's cheap check for "changed since": a folder's `size:mtime`.
    pub stamp: String,
    pub content_hash: String,
    /// Every passage of it is in the saved vectors file.
    pub embedded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    pub id: i64,
    pub document: String,
    pub start: usize,
    pub end: usize,
    /// As indexed.
    pub text: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub documents_total: u32,
    pub documents_embedded: u32,
    pub passages_total: u32,
    /// Passages of documents marked embedded: in the saved vectors file.
    pub passages_embedded: u32,
}

/// Documents and passages under `prefix`. Every passage has its document's
/// row (both are written in one transaction), so the join drops nothing.
pub(super) async fn read_counts(
    connection: &mut SqliteConnection,
    prefix: &str,
) -> std::result::Result<Counts, sqlx::Error> {
    let documents = sqlx::query(&format!(
        "SELECT COUNT(*) AS total, COALESCE(SUM(embedded), 0) AS embedded FROM index_documents
         WHERE {}",
        under("key")
    ))
    .bind(prefix)
    .fetch_one(&mut *connection)
    .await?;
    let passages = sqlx::query(&format!(
        "SELECT COUNT(*) AS total, COALESCE(SUM(d.embedded), 0) AS embedded
         FROM passages p JOIN index_documents d ON d.key = p.document WHERE {}",
        under("p.document")
    ))
    .bind(prefix)
    .fetch_one(&mut *connection)
    .await?;
    let as_u32 = |value: i64| u32::try_from(value).unwrap_or(u32::MAX);
    Ok(Counts {
        documents_total: as_u32(documents.get("total")),
        documents_embedded: as_u32(documents.get("embedded")),
        passages_total: as_u32(passages.get("total")),
        passages_embedded: as_u32(passages.get("embedded")),
    })
}

pub(super) async fn read_meta(
    connection: &mut SqliteConnection,
) -> std::result::Result<HashMap<String, String>, sqlx::Error> {
    let rows = sqlx::query("SELECT key, value FROM meta")
        .fetch_all(&mut *connection)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get("key"), row.get("value")))
        .collect())
}

/// A short-lived connection to `dir`'s store; `None` when it has none or it
/// will not open. Reading through one leaves a live store's pool untouched.
pub(super) async fn connect(dir: &Path) -> Option<SqliteConnection> {
    let db = dir.join(DB_FILE);
    if !db.exists() {
        return None;
    }
    let options = SqliteConnectOptions::new()
        .filename(&db)
        .busy_timeout(Duration::from_secs(2));
    SqliteConnection::connect_with(&options).await.ok()
}

/// The store file of the index layout before this one, which a rebuild
/// removes with its journal.
const EARLIER_DB_FILE: &str = "chunks.db";

/// Whether a file named `name` belongs to an index: the store or the earlier
/// layout's, their journals, or a vectors file made against either.
fn index_file(name: &str) -> bool {
    name.starts_with(DB_FILE)
        || name.starts_with(EARLIER_DB_FILE)
        || name.starts_with(super::vectors::PREFIX)
}

/// Removes the index's own files directly in `dir`: the store, its journal
/// and any vectors file made against it. Anything else in the directory is
/// left alone, so a wrong directory never loses a file the index did not
/// write.
fn clear_directory(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let owned = entry.file_name().to_str().is_some_and(index_file);
        if owned && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

async fn connect_pool(dir: &Path) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(dir.join(DB_FILE))
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5))
        .pragma("journal_mode", "WAL")
        .pragma("synchronous", "NORMAL");
    SqlitePoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .map_err(db_error("open passages.db"))
}

/// Whether the store in `pool` holds tables of another shape than this one.
async fn other_version(pool: &SqlitePool) -> Result<bool> {
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(pool)
    .await
    .map_err(db_error("read the schema"))?;
    if tables == 0 {
        return Ok(false);
    }
    let version: Option<String> = sqlx::query_scalar("SELECT value FROM meta WHERE key = ?")
        .bind(META_VERSION)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    Ok(version.as_deref() != Some(SCHEMA_VERSION))
}

pub struct PassageStore {
    dir: PathBuf,
    pool: SqlitePool,
}

impl PassageStore {
    /// Opens (creating if needed) the store in `dir`. A directory without a
    /// store, or with one of another version, is emptied first.
    pub async fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|error| {
            AppError::FileStorage(format!(
                "Passage index: could not create {}: {error}",
                dir.display()
            ))
        })?;
        if !dir.join(DB_FILE).exists() {
            clear_directory(dir);
        }
        let mut pool = connect_pool(dir).await?;
        if other_version(&pool).await? {
            tracing::info!(dir = %dir.display(), "Passage index: schema changed; rebuilding");
            pool.close().await;
            clear_directory(dir);
            pool = connect_pool(dir).await?;
        }
        sqlx::raw_sql(SCHEMA)
            .execute(&pool)
            .await
            .map_err(db_error("create the schema"))?;
        let store = Self {
            dir: dir.to_path_buf(),
            pool,
        };
        store.set_meta(META_VERSION, SCHEMA_VERSION).await?;
        Ok(store)
    }

    /// A store that lives in memory for as long as it is open: no directory,
    /// no vectors beside it, nothing on disk to clean up. For passages
    /// searched by text for a moment, such as a collection built from sources
    /// a caller already holds. One connection that never retires, because
    /// each connection to an in-memory database is a database of its own.
    pub async fn memory() -> Result<Self> {
        let options = SqliteConnectOptions::new().in_memory(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await
            .map_err(db_error("open an in-memory store"))?;
        sqlx::raw_sql(SCHEMA)
            .execute(&pool)
            .await
            .map_err(db_error("create the schema"))?;
        let store = Self {
            dir: PathBuf::new(),
            pool,
        };
        store.set_meta(META_VERSION, SCHEMA_VERSION).await?;
        Ok(store)
    }

    /// The index directory; empty for a [`Self::memory`] store.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    pub async fn meta(&self, key: &str) -> Result<Option<String>> {
        sqlx::query_scalar::<_, String>("SELECT value FROM meta WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error("read meta"))
    }

    pub async fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO meta (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error("write meta"))
    }

    pub async fn delete_meta(&self, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM meta WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error("write meta"))
    }

    /// Every document under `prefix` (`""` for all), keyed by its key.
    pub async fn documents(&self, prefix: &str) -> Result<HashMap<String, Document>> {
        let rows = sqlx::query(&format!(
            "SELECT key, stamp, content_hash, embedded FROM index_documents WHERE {}",
            under("key")
        ))
        .bind(prefix)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("read documents"))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("key"),
                    Document {
                        stamp: row.get("stamp"),
                        content_hash: row.get("content_hash"),
                        embedded: row.get::<i64, _>("embedded") != 0,
                    },
                )
            })
            .collect())
    }

    /// Keys of the documents under `prefix`: a search's scope, or what a
    /// removed folder takes with it.
    pub async fn keys_under(&self, prefix: &str) -> Result<Vec<String>> {
        sqlx::query_scalar(&format!(
            "SELECT key FROM index_documents WHERE {} ORDER BY key",
            under("key")
        ))
        .bind(prefix)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("read documents"))
    }

    /// Same content under a new stamp: nothing to redo.
    pub async fn touch(&self, key: &str, stamp: &str) -> Result<()> {
        sqlx::query("UPDATE index_documents SET stamp = ? WHERE key = ?")
            .bind(stamp)
            .bind(key)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error("update a document"))
    }

    /// Replaces a document's passages and marks it for embedding (or done,
    /// when it has none). Returns the ids of the passages it replaced, whose
    /// vectors the caller removes.
    pub async fn replace(
        &self,
        key: &str,
        stamp: &str,
        content_hash: &str,
        passages: &[NewPassage],
    ) -> Result<Vec<i64>> {
        let mut tx = self.pool.begin().await.map_err(db_error("begin"))?;
        let old: Vec<i64> = sqlx::query_scalar("SELECT id FROM passages WHERE document = ?")
            .bind(key)
            .fetch_all(&mut *tx)
            .await
            .map_err(db_error("read passages"))?;
        sqlx::query("DELETE FROM passages WHERE document = ?")
            .bind(key)
            .execute(&mut *tx)
            .await
            .map_err(db_error("delete passages"))?;
        for passage in passages {
            sqlx::query(
                "INSERT INTO passages (document, span_start, span_end, text) VALUES (?, ?, ?, ?)",
            )
            .bind(key)
            .bind(i64::try_from(passage.start).unwrap_or(i64::MAX))
            .bind(i64::try_from(passage.end).unwrap_or(i64::MAX))
            .bind(&passage.text)
            .execute(&mut *tx)
            .await
            .map_err(db_error("insert a passage"))?;
        }
        sqlx::query(
            "INSERT INTO index_documents (key, stamp, content_hash, embedded) VALUES (?, ?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET stamp = excluded.stamp,
                 content_hash = excluded.content_hash, embedded = excluded.embedded",
        )
        .bind(key)
        .bind(stamp)
        .bind(content_hash)
        .bind(i64::from(passages.is_empty()))
        .execute(&mut *tx)
        .await
        .map_err(db_error("write a document"))?;
        tx.commit().await.map_err(db_error("commit"))?;
        Ok(old)
    }

    /// Forgets the documents at `keys`. Returns their passage ids.
    pub async fn remove(&self, keys: &[String]) -> Result<Vec<i64>> {
        let mut tx = self.pool.begin().await.map_err(db_error("begin"))?;
        let mut removed = Vec::new();
        for key in keys {
            let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM passages WHERE document = ?")
                .bind(key)
                .fetch_all(&mut *tx)
                .await
                .map_err(db_error("read passages"))?;
            removed.extend(ids);
            sqlx::query("DELETE FROM passages WHERE document = ?")
                .bind(key)
                .execute(&mut *tx)
                .await
                .map_err(db_error("delete passages"))?;
            sqlx::query("DELETE FROM index_documents WHERE key = ?")
                .bind(key)
                .execute(&mut *tx)
                .await
                .map_err(db_error("delete a document"))?;
        }
        tx.commit().await.map_err(db_error("commit"))?;
        Ok(removed)
    }

    /// Documents under `prefix` whose passages are not all in the vectors
    /// file yet, in key order.
    pub async fn pending(&self, prefix: &str) -> Result<Vec<String>> {
        sqlx::query_scalar(&format!(
            "SELECT key FROM index_documents WHERE embedded = 0 AND {} ORDER BY key",
            under("key")
        ))
        .bind(prefix)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("read pending documents"))
    }

    /// A document's passages in span order.
    pub async fn passages_of(&self, key: &str) -> Result<Vec<Passage>> {
        let rows = sqlx::query(
            "SELECT id, document, span_start, span_end, text FROM passages
             WHERE document = ? ORDER BY span_start, id",
        )
        .bind(key)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("read passages"))?;
        Ok(rows.into_iter().map(passage).collect())
    }

    /// The passages with these ids that still exist, in no particular order.
    pub async fn by_ids(&self, ids: &[i64]) -> Result<Vec<Passage>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = serde_json::to_string(ids)?;
        let rows = sqlx::query(
            "SELECT id, document, span_start, span_end, text FROM passages
             WHERE id IN (SELECT value FROM json_each(?))",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("read passages"))?;
        Ok(rows.into_iter().map(passage).collect())
    }

    /// Their passages are in the saved vectors file.
    pub async fn mark_embedded(&self, keys: &[String]) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db_error("begin"))?;
        for key in keys {
            sqlx::query("UPDATE index_documents SET embedded = 1 WHERE key = ?")
                .bind(key)
                .execute(&mut *tx)
                .await
                .map_err(db_error("mark a document embedded"))?;
        }
        tx.commit().await.map_err(db_error("commit"))
    }

    /// The vectors are gone (a new embedding model): every document with
    /// passages is embedded again. Passages and hashes stay.
    pub async fn reset_embedded(&self) -> Result<()> {
        sqlx::query(
            "UPDATE index_documents SET embedded = 0
             WHERE key IN (SELECT DISTINCT document FROM passages)",
        )
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error("reset embedded flags"))
    }

    /// Passage ids for an FTS5 `MATCH` expression, best `bm25` first, with
    /// their `bm25` (lower is better), confined to the documents in `scope`
    /// when one is given.
    pub async fn full_text(
        &self,
        expression: &str,
        scope: Option<&HashSet<String>>,
        limit: usize,
    ) -> Result<Vec<(i64, f64)>> {
        let scope = scope.map(serde_json::to_string).transpose()?;
        let rows = sqlx::query(
            "SELECT p.id AS id, bm25(passages_fts) AS score
             FROM passages_fts JOIN passages p ON p.id = passages_fts.rowid
             WHERE passages_fts MATCH ?1
               AND (?2 IS NULL OR p.document IN (SELECT value FROM json_each(?2)))
             ORDER BY score, p.id LIMIT ?3",
        )
        .bind(expression)
        .bind(scope)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("search the full-text index"))?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("id"), row.get("score")))
            .collect())
    }

    pub async fn counts(&self, prefix: &str) -> Result<Counts> {
        let mut connection = self.pool.acquire().await.map_err(db_error("count"))?;
        read_counts(&mut connection, prefix)
            .await
            .map_err(db_error("count documents and passages"))
    }

    /// Whether anything under `prefix` has passages.
    pub async fn has_passages(&self, prefix: &str) -> Result<bool> {
        sqlx::query_scalar::<_, i64>(&format!(
            "SELECT EXISTS (SELECT 1 FROM passages WHERE {})",
            under("document")
        ))
        .bind(prefix)
        .fetch_one(&self.pool)
        .await
        .map(|exists| exists != 0)
        .map_err(db_error("read passages"))
    }

    /// How many passages hold each of `terms`, from the full-text index's
    /// vocabulary. A term is matched as the index tokenised it (lower case);
    /// one in no passage is absent from the map.
    pub async fn term_counts(&self, terms: &[String]) -> Result<HashMap<String, usize>> {
        if terms.is_empty() {
            return Ok(HashMap::new());
        }
        let terms = serde_json::to_string(terms)?;
        let rows = sqlx::query(
            "SELECT term, doc FROM passages_vocab WHERE term IN (SELECT value FROM json_each(?))",
        )
        .bind(terms)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("read the vocabulary"))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let count: i64 = row.get("doc");
                (row.get("term"), usize::try_from(count).unwrap_or(0))
            })
            .collect())
    }
}

fn passage(row: SqliteRow) -> Passage {
    let span = |column: &str| usize::try_from(row.get::<i64, _>(column)).unwrap_or(0);
    Passage {
        id: row.get("id"),
        document: row.get("document"),
        start: span("span_start"),
        end: span("span_end"),
        text: row.get("text"),
    }
}
