//! The records the sweeps delete by: `file_intents` rows. A sweep removes a
//! file only when a row here names it, never because no other row does.

use futures::FutureExt;
use std::path::Path;
use std::sync::OnceLock;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Transactions;

/// Which files a row names, and what its `name` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStore {
    /// A library blob: its SHA-256.
    Library,
    /// A frozen page: its snapshot id.
    WebSnapshot,
    /// A chat picture: `<id>.<ext>`.
    ChatImage,
    /// A chat's attachment: its document id.
    Attachment,
}

impl FileStore {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::WebSnapshot => "web_snapshot",
            Self::ChatImage => "chat_image",
            Self::Attachment => "attachment",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentKind {
    /// About to write the file, before anything references it.
    Import,
    /// The row that referenced the file is gone.
    Release,
}

impl IntentKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Release => "release",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "import" => Some(Self::Import),
            "release" => Some(Self::Release),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIntent {
    pub id: i64,
    pub kind: IntentKind,
    pub name: String,
    /// Written by this process: an import it names may still be running.
    pub this_run: bool,
}

/// This process's id in `file_intents.run_id`.
pub fn run_id() -> &'static str {
    // global-state: the process's own id, as its pid is; each intent it records
    // must carry the one value.
    static RUN_ID: OnceLock<String> = OnceLock::new();
    RUN_ID.get_or_init(|| uuid::Uuid::new_v4().to_string())
}

/// SQL recording a release of `name` for every row `from` selects, for a
/// caller's own transaction. Bind [`run_id`] first, then `from`'s parameters.
pub fn record_releases_sql(store: FileStore, name: &str, from: &str) -> String {
    format!(
        "INSERT INTO file_intents (store, kind, name, run_id) \
         SELECT '{}', 'release', {name}, ? FROM {from}",
        store.as_str()
    )
}

#[derive(Debug, Clone)]
pub struct FileIntents {
    pool: SqlitePool,
}

impl FileIntents {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn record(&self, store: FileStore, kind: IntentKind, name: &str) -> Result<i64> {
        Ok(sqlx::query_scalar::<_, i64>(
            "INSERT INTO file_intents (store, kind, name, run_id) VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(store.as_str())
        .bind(kind.as_str())
        .bind(name)
        .bind(run_id())
        .fetch_one(&self.pool)
        .boxed()
        .await?)
    }

    /// Every row for `store`, oldest first.
    pub async fn pending(&self, store: FileStore) -> Result<Vec<FileIntent>> {
        let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
            "SELECT id, kind, name, run_id FROM file_intents WHERE store = ? ORDER BY id",
        )
        .bind(store.as_str())
        .fetch_all(&self.pool)
        .boxed()
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, kind, name, run)| {
                Some(FileIntent {
                    id,
                    kind: IntentKind::parse(&kind)?,
                    this_run: run == run_id(),
                    name,
                })
            })
            .collect())
    }

    /// The ids of the releases recorded for `name`.
    pub async fn releases_of(&self, store: FileStore, name: &str) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT id FROM file_intents WHERE store = ? AND kind = 'release' AND name = ?",
        )
        .bind(store.as_str())
        .bind(name)
        .fetch_all(&self.pool)
        .boxed()
        .await?)
    }

    pub async fn forget(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin_write().await?;
        for id in ids {
            sqlx::query("DELETE FROM file_intents WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Forget the releases recorded for each of `names`.
    pub async fn forget_releases(&self, store: FileStore, names: &[String]) -> Result<()> {
        if names.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin_write().await?;
        for name in names {
            sqlx::query(
                "DELETE FROM file_intents WHERE store = ? AND kind = 'release' AND name = ?",
            )
            .bind(store.as_str())
            .bind(name)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

/// Drop every record in the database file at `path`, which a restore is about
/// to put in place: they describe what another process meant to do to files
/// that the database being replaced may since have claimed again. A database
/// from before the table existed has none.
pub async fn forget_all_in(path: &Path) -> Result<()> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Delete)
        .foreign_keys(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let outcome = forget_all(&pool).await;
    pool.close().await;
    outcome
}

async fn forget_all(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin_write().await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'file_intents')",
    )
    .fetch_one(&mut *tx)
    .boxed()
    .await?;
    if exists {
        sqlx::query("DELETE FROM file_intents")
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn migrated_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn a_record_is_kept_until_it_is_forgotten_and_knows_its_run() {
        let pool = migrated_pool().await;
        let intents = FileIntents::new(pool.clone());
        let ours = intents
            .record(FileStore::Library, IntentKind::Import, "a")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO file_intents (store, kind, name, run_id) \
             VALUES ('library', 'release', 'b', 'an earlier run')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let pending = intents.pending(FileStore::Library).await.unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(
            (pending[0].id, pending[0].kind, pending[0].this_run),
            (ours, IntentKind::Import, true)
        );
        assert_eq!(
            (pending[1].kind, pending[1].this_run),
            (IntentKind::Release, false)
        );
        assert!(intents
            .pending(FileStore::ChatImage)
            .await
            .unwrap()
            .is_empty());

        intents.forget(&[ours]).await.unwrap();
        intents
            .forget_releases(FileStore::Library, &["b".to_string()])
            .await
            .unwrap();
        assert!(intents
            .pending(FileStore::Library)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_database_file_loses_its_records_and_one_without_the_table_is_left_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("restored.db");
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        FileIntents::new(pool.clone())
            .record(FileStore::ChatImage, IntentKind::Release, "x.png")
            .await
            .unwrap();
        pool.close().await;

        forget_all_in(&path).await.unwrap();

        let pool = SqlitePoolOptions::new()
            .connect_with(SqliteConnectOptions::new().filename(&path))
            .await
            .unwrap();
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_intents")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0);
        pool.close().await;

        let bare = dir.path().join("older.db");
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&bare)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query("CREATE TABLE documents (id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        forget_all_in(&bare).await.unwrap();
    }

    #[tokio::test]
    async fn forgetting_all_waits_for_a_writer_that_drops_the_table() {
        let dir = tempfile::tempdir().unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(crate::tests::sqlite::file_options(dir.path().join("t.db")))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let mut writer = pool.begin_write().await.unwrap();
        sqlx::query("DROP TABLE file_intents")
            .execute(&mut *writer)
            .await
            .unwrap();
        let forgetting = tokio::spawn({
            let pool = pool.clone();
            async move { forget_all(&pool).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        writer.commit().await.unwrap();

        forgetting.await.unwrap().unwrap();
    }
}
