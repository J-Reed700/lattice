//! The folder index's own SQLite store.
//!
//! Each indexed folder gets a directory under `<data_dir>/folder-index/`,
//! named for its root, holding `chunks.db` and the vectors file. Nothing goes
//! into `lattice.db`: backups copy that file whole and would carry every
//! indexed repository, and a separate store cannot leak into library search.
//!
//! The directories are the registry. Each `chunks.db` records its root, the
//! embedding identity its vectors were made with and when it was last opened,
//! so finding a parent's index or the least recently used one is a read of at
//! most a handful of small databases.

use super::chunker::Chunk;
use crate::shared::{AppError, Result};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Connection, Row, SqliteConnection, SqlitePool};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

pub const DB_FILE: &str = "chunks.db";
const SCHEMA_VERSION: &str = "1";

pub const META_ROOT: &str = "root";
pub const META_IDENTITY: &str = "identity";
pub const META_LAST_OPENED: &str = "last_opened";
const META_VERSION: &str = "version";

/// `AUTOINCREMENT` so a chunk id is never handed out twice: the vectors file
/// is saved in batches, and an id reused after a crash would put a new
/// passage behind an old passage's vector.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS files (
    path TEXT PRIMARY KEY,
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    embedded INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    text TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS chunks_path ON chunks(path);
CREATE VIRTUAL TABLE IF NOT EXISTS passages_fts USING fts5(
    text, path UNINDEXED, content='chunks', content_rowid='id',
    tokenize="unicode61 tokenchars '_'"
);
CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO passages_fts(rowid, text, path) VALUES (new.id, new.text, new.path);
END;
CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO passages_fts(passages_fts, rowid, text, path) VALUES ('delete', old.id, old.text, old.path);
END;
"#;

/// `path` is `?1` or sits under it; `''` matches everything.
const UNDER: &str = "(?1 = '' OR path = ?1 OR substr(path, 1, length(?1) + 1) = ?1 || '/')";

fn db_error(action: &str) -> impl Fn(sqlx::Error) -> AppError + '_ {
    move |error| AppError::Database(format!("Folder index: failed to {action}: {error}"))
}

/// The index directory for a canonical root: the first 16 hex digits of the
/// root's SHA-256, so the name says nothing about the folder.
pub fn index_dir(base: &Path, root: &Path) -> PathBuf {
    let digest = Sha256::digest(root.as_os_str().as_encoded_bytes());
    let name: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    base.join(name)
}

/// The vectors file for one embedding identity, beside `chunks.db`.
pub fn vectors_path(dir: &Path, identity: &str) -> PathBuf {
    dir.join(format!(
        "vectors-{}.usearch",
        identity.replace([':', '/'], "-")
    ))
}

/// Milliseconds since the epoch, strictly increasing within the process, so
/// two folders opened in the same millisecond still have an order.
pub fn next_open_stamp() -> i64 {
    static LAST: AtomicI64 = AtomicI64::new(0);
    let now = chrono::Utc::now().timestamp_millis();
    let mut previous = LAST.load(Ordering::SeqCst);
    loop {
        let stamp = now.max(previous + 1);
        match LAST.compare_exchange(previous, stamp, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return stamp,
            Err(actual) => previous = actual,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub size: i64,
    pub mtime: i64,
    pub content_hash: String,
    pub embedded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkRow {
    pub id: i64,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    /// As indexed: the line reference heading, then the lines.
    pub text: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub files_total: u32,
    pub files_indexed: u32,
    pub chunks: u32,
}

pub struct FolderStore {
    dir: PathBuf,
    pool: SqlitePool,
}

impl FolderStore {
    /// Opens (creating if needed) the store in `dir`.
    pub async fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|error| {
            AppError::FileStorage(format!(
                "Folder index: could not create {}: {error}",
                dir.display()
            ))
        })?;
        let options = SqliteConnectOptions::new()
            .filename(dir.join(DB_FILE))
            .create_if_missing(true)
            .busy_timeout(Duration::from_secs(5))
            .pragma("journal_mode", "WAL")
            .pragma("synchronous", "NORMAL");
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await
            .map_err(db_error("open chunks.db"))?;
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

    /// Every file row under `prefix` (`""` for all), keyed by path.
    pub async fn files_under(&self, prefix: &str) -> Result<HashMap<String, FileRow>> {
        let rows = sqlx::query(&format!(
            "SELECT path, size, mtime, content_hash, embedded FROM files WHERE {UNDER}"
        ))
        .bind(prefix)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("read files"))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("path"),
                    FileRow {
                        size: row.get("size"),
                        mtime: row.get("mtime"),
                        content_hash: row.get("content_hash"),
                        embedded: row.get::<i64, _>("embedded") != 0,
                    },
                )
            })
            .collect())
    }

    /// Same content under a new size or modification time: nothing to redo.
    pub async fn touch_file(&self, path: &str, size: i64, mtime: i64) -> Result<()> {
        sqlx::query("UPDATE files SET size = ?, mtime = ? WHERE path = ?")
            .bind(size)
            .bind(mtime)
            .bind(path)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db_error("update a file"))
    }

    /// Replaces a file's chunks and marks it for embedding (or done, when it
    /// has none). Returns the ids of the chunks it replaced, whose vectors the
    /// caller removes.
    pub async fn replace_file(
        &self,
        path: &str,
        size: i64,
        mtime: i64,
        content_hash: &str,
        chunks: &[Chunk],
    ) -> Result<Vec<i64>> {
        let mut tx = self.pool.begin().await.map_err(db_error("begin"))?;
        let old: Vec<i64> = sqlx::query_scalar("SELECT id FROM chunks WHERE path = ?")
            .bind(path)
            .fetch_all(&mut *tx)
            .await
            .map_err(db_error("read chunks"))?;
        sqlx::query("DELETE FROM chunks WHERE path = ?")
            .bind(path)
            .execute(&mut *tx)
            .await
            .map_err(db_error("delete chunks"))?;
        for chunk in chunks {
            sqlx::query(
                "INSERT INTO chunks (path, start_line, end_line, text) VALUES (?, ?, ?, ?)",
            )
            .bind(path)
            .bind(chunk.start_line)
            .bind(chunk.end_line)
            .bind(chunk.indexed_text(path))
            .execute(&mut *tx)
            .await
            .map_err(db_error("insert a chunk"))?;
        }
        sqlx::query(
            "INSERT INTO files (path, size, mtime, content_hash, embedded) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(path) DO UPDATE SET size = excluded.size, mtime = excluded.mtime,
                 content_hash = excluded.content_hash, embedded = excluded.embedded",
        )
        .bind(path)
        .bind(size)
        .bind(mtime)
        .bind(content_hash)
        .bind(i64::from(chunks.is_empty()))
        .execute(&mut *tx)
        .await
        .map_err(db_error("write a file"))?;
        tx.commit().await.map_err(db_error("commit"))?;
        Ok(old)
    }

    /// Forgets the files at `paths`. Returns their chunk ids.
    pub async fn remove_files(&self, paths: &[String]) -> Result<Vec<i64>> {
        let mut tx = self.pool.begin().await.map_err(db_error("begin"))?;
        let mut removed = Vec::new();
        for path in paths {
            let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM chunks WHERE path = ?")
                .bind(path)
                .fetch_all(&mut *tx)
                .await
                .map_err(db_error("read chunks"))?;
            removed.extend(ids);
            sqlx::query("DELETE FROM chunks WHERE path = ?")
                .bind(path)
                .execute(&mut *tx)
                .await
                .map_err(db_error("delete chunks"))?;
            sqlx::query("DELETE FROM files WHERE path = ?")
                .bind(path)
                .execute(&mut *tx)
                .await
                .map_err(db_error("delete a file"))?;
        }
        tx.commit().await.map_err(db_error("commit"))?;
        Ok(removed)
    }

    /// Forgets `prefix` and everything under it (a removed directory).
    pub async fn remove_under(&self, prefix: &str) -> Result<Vec<i64>> {
        let paths: Vec<String> =
            sqlx::query_scalar(&format!("SELECT path FROM files WHERE {UNDER}"))
                .bind(prefix)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error("read files"))?;
        self.remove_files(&paths).await
    }

    /// Files under `prefix` whose chunks are not all in the vectors file yet.
    pub async fn pending_files(&self, prefix: &str) -> Result<Vec<String>> {
        sqlx::query_scalar(&format!(
            "SELECT path FROM files WHERE embedded = 0 AND {UNDER} ORDER BY path"
        ))
        .bind(prefix)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("read pending files"))
    }

    pub async fn chunks_of(&self, path: &str) -> Result<Vec<ChunkRow>> {
        let rows = sqlx::query(
            "SELECT id, path, start_line, end_line, text FROM chunks WHERE path = ? ORDER BY start_line",
        )
        .bind(path)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("read chunks"))?;
        Ok(rows.into_iter().map(chunk_row).collect())
    }

    pub async fn chunks_by_ids(&self, ids: &[i64]) -> Result<Vec<ChunkRow>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "SELECT id, path, start_line, end_line, text FROM chunks WHERE id IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(db_error("read chunks"))?;
        Ok(rows.into_iter().map(chunk_row).collect())
    }

    /// Their chunks are in the saved vectors file.
    pub async fn mark_embedded(&self, paths: &[String]) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db_error("begin"))?;
        for path in paths {
            sqlx::query("UPDATE files SET embedded = 1 WHERE path = ?")
                .bind(path)
                .execute(&mut *tx)
                .await
                .map_err(db_error("mark a file embedded"))?;
        }
        tx.commit().await.map_err(db_error("commit"))
    }

    /// The vectors are gone (a new embedding model): every file with chunks
    /// is embedded again. Chunks and hashes stay.
    pub async fn reset_embedded(&self) -> Result<()> {
        sqlx::query(
            "UPDATE files SET embedded = 0 WHERE path IN (SELECT DISTINCT path FROM chunks)",
        )
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error("reset embedded flags"))
    }

    pub async fn counts(&self, prefix: &str) -> Result<Counts> {
        let files = sqlx::query(&format!(
            "SELECT COUNT(*) AS total, COALESCE(SUM(embedded), 0) AS indexed FROM files WHERE {UNDER}"
        ))
        .bind(prefix)
        .fetch_one(&self.pool)
        .await
        .map_err(db_error("count files"))?;
        let chunks: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM chunks WHERE {UNDER}"))
            .bind(prefix)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error("count chunks"))?;
        let as_u32 = |value: i64| u32::try_from(value).unwrap_or(u32::MAX);
        Ok(Counts {
            files_total: as_u32(files.get("total")),
            files_indexed: as_u32(files.get("indexed")),
            chunks: as_u32(chunks),
        })
    }

    /// Whether anything under `prefix` has been chunked.
    pub async fn has_chunks(&self, prefix: &str) -> Result<bool> {
        sqlx::query_scalar::<_, i64>(&format!(
            "SELECT EXISTS (SELECT 1 FROM chunks WHERE {UNDER})"
        ))
        .bind(prefix)
        .fetch_one(&self.pool)
        .await
        .map(|exists| exists != 0)
        .map_err(db_error("read chunks"))
    }

    /// Paths of the files under `prefix`, the scope a dense search keeps to.
    pub async fn paths_under(&self, prefix: &str) -> Result<Vec<String>> {
        sqlx::query_scalar(&format!("SELECT path FROM files WHERE {UNDER}"))
            .bind(prefix)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error("read files"))
    }

    /// Chunk ids for an FTS5 `MATCH` expression under `prefix`, best `bm25`
    /// first.
    pub async fn full_text(
        &self,
        expression: &str,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<i64>> {
        sqlx::query_scalar(
            "SELECT c.id FROM passages_fts JOIN chunks c ON c.id = passages_fts.rowid
             WHERE passages_fts MATCH ?2
               AND (?1 = '' OR c.path = ?1 OR substr(c.path, 1, length(?1) + 1) = ?1 || '/')
             ORDER BY bm25(passages_fts) LIMIT ?3",
        )
        .bind(prefix)
        .bind(expression)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("search the full-text index"))
    }
}

fn chunk_row(row: sqlx::sqlite::SqliteRow) -> ChunkRow {
    ChunkRow {
        id: row.get("id"),
        path: row.get("path"),
        start_line: u32::try_from(row.get::<i64, _>("start_line")).unwrap_or(0),
        end_line: u32::try_from(row.get::<i64, _>("end_line")).unwrap_or(0),
        text: row.get("text"),
    }
}

/// One index directory as the registry sees it.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub dir: PathBuf,
    /// `None` for a directory whose `chunks.db` is missing or unreadable.
    pub root: Option<PathBuf>,
    pub last_opened: i64,
}

/// Every index directory under `base`. Each `chunks.db` is read through a
/// short-lived connection of its own, so the open folder's pool is untouched.
pub async fn list_indexes(base: &Path) -> Vec<IndexEntry> {
    let Ok(entries) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut indexes = Vec::new();
    for entry in entries.filter_map(|entry| entry.ok()) {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let (root, last_opened) = read_registry_meta(&dir).await.unwrap_or((None, 0));
        indexes.push(IndexEntry {
            dir,
            root,
            last_opened,
        });
    }
    indexes
}

async fn read_registry_meta(dir: &Path) -> Option<(Option<PathBuf>, i64)> {
    let db = dir.join(DB_FILE);
    if !db.exists() {
        return None;
    }
    let options = SqliteConnectOptions::new()
        .filename(&db)
        .busy_timeout(Duration::from_secs(2));
    let mut connection = SqliteConnection::connect_with(&options).await.ok()?;
    let rows = sqlx::query("SELECT key, value FROM meta WHERE key IN (?, ?)")
        .bind(META_ROOT)
        .bind(META_LAST_OPENED)
        .fetch_all(&mut connection)
        .await
        .ok();
    let _ = connection.close().await;
    let mut root = None;
    let mut last_opened = 0;
    for row in rows? {
        let key: String = row.get("key");
        let value: String = row.get("value");
        match key.as_str() {
            META_ROOT => root = Some(PathBuf::from(value)),
            META_LAST_OPENED => last_opened = value.parse().unwrap_or(0),
            _ => {}
        }
    }
    Some((root, last_opened))
}
