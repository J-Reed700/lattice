//! SQLite-backed document scope persistence shared by import workflows.

use lattice_core::application::ports::document_scope::DocumentScopePort;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::{Database, WriteTx};

// Bound JSON encoding and SQLite's per-statement work without one worker
// round trip per document. All batches still share a single transaction.
const SCOPE_BATCH_SIZE: usize = 500;

pub struct SqliteDocumentScope {
    db: Database,
}
impl SqliteDocumentScope {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl DocumentScopePort for SqliteDocumentScope {
    async fn space_exists(&self, id: &str) -> Result<bool> {
        let mut read = self.db.begin_read().await?;
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM conversation_spaces WHERE id = ?)",
        )
        .bind(id)
        .fetch_one(&mut *read)
        .await?;
        read.commit().await?;
        Ok(exists)
    }
    async fn conversation_exists(&self, id: &str) -> Result<bool> {
        let mut read = self.db.begin_read().await?;
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?)",
        )
        .bind(id)
        .fetch_one(&mut *read)
        .await?;
        read.commit().await?;
        Ok(exists)
    }
    async fn assign_documents(&self, document_ids: &[String], space_id: &str) -> Result<()> {
        if document_ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.begin_write().await?;
        assign_documents_in(&mut tx, document_ids, space_id).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn set_conversation_owner(
        &self,
        document_ids: &[String],
        conversation_id: Option<&str>,
    ) -> Result<()> {
        if document_ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.begin_write().await?;
        set_conversation_owner_in(&mut tx, document_ids, conversation_id).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn file_documents(
        &self,
        document_ids: &[String],
        owner_conversation_id: Option<&str>,
        space_id: Option<&str>,
    ) -> Result<()> {
        if document_ids.is_empty() || (owner_conversation_id.is_none() && space_id.is_none()) {
            return Ok(());
        }
        let mut tx = self.db.begin_write().await?;
        if let Some(conversation) = owner_conversation_id {
            set_conversation_owner_in(&mut tx, document_ids, Some(conversation)).await?;
        }
        if let Some(space) = space_id {
            assign_documents_in(&mut tx, document_ids, space).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn conversation_owner(&self, document_id: &str) -> Result<Option<String>> {
        let mut read = self.db.begin_read().await?;
        let owner = sqlx::query_scalar::<_, Option<String>>(
            "SELECT owner_conversation_id FROM documents WHERE id = ?",
        )
        .bind(document_id)
        .fetch_optional(&mut *read)
        .await?
        .flatten();
        read.commit().await?;
        Ok(owner)
    }

    async fn documents_owned_by_conversation(&self, conversation_id: &str) -> Result<Vec<String>> {
        let mut read = self.db.begin_read().await?;
        let owned = sqlx::query_scalar::<_, String>(
            "SELECT id FROM documents WHERE owner_conversation_id = ? ORDER BY rowid",
        )
        .bind(conversation_id)
        .fetch_all(&mut *read)
        .await?;
        read.commit().await?;
        Ok(owned)
    }

    async fn orphaned_conversation_owned_documents(&self) -> Result<Vec<String>> {
        let mut read = self.db.begin_read().await?;
        let orphaned = sqlx::query_scalar::<_, String>(
            "SELECT d.id FROM documents d
             WHERE d.owner_conversation_id IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM conversations c WHERE c.id = d.owner_conversation_id)
               AND EXISTS (SELECT 1 FROM file_intents i
                           WHERE i.store = 'attachment' AND i.kind = 'release' AND i.name = d.id)
             ORDER BY d.rowid",
        )
        .fetch_all(&mut *read)
        .await?;
        read.commit().await?;
        Ok(orphaned)
    }
}

async fn assign_documents_in(
    tx: &mut WriteTx,
    document_ids: &[String],
    space_id: &str,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    for ids in document_ids.chunks(SCOPE_BATCH_SIZE) {
        let scope = serde_json::to_string(ids)?;
        sqlx::query(
            "INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at)
             SELECT value, ?, ? FROM json_each(?)",
        )
        .bind(space_id)
        .bind(&now)
        .bind(scope)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn set_conversation_owner_in(
    tx: &mut WriteTx,
    document_ids: &[String],
    conversation_id: Option<&str>,
) -> Result<()> {
    for ids in document_ids.chunks(SCOPE_BATCH_SIZE) {
        let scope = serde_json::to_string(ids)?;
        sqlx::query(
            "UPDATE documents SET owner_conversation_id = ?
             WHERE id IN (SELECT value FROM json_each(?))",
        )
        .bind(conversation_id)
        .bind(scope)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn bulk_repository() -> (SqliteDocumentScope, SqlitePool, Vec<String>) {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
        let db = Database::new(pool.clone());
        let mut ids: Vec<_> = (0..1001).map(|index| format!("doc-{index}")).collect();
        ids.push("quoted\"; --\n雪".into());
        sqlx::query(
            "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum)
             SELECT value, '/bulk/' || value, value, 1, '2026-10-25T00:00:00Z', 'checksum'
             FROM json_each(?)",
        )
        .bind(serde_json::to_string(&ids).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        (SqliteDocumentScope::new(db), pool, ids)
    }

    #[tokio::test]
    async fn bulk_scope_changes_use_bounded_statement_counts() {
        use crate::tests::common::query_counts::count_writes;

        let (repo, pool, ids) = bulk_repository().await;
        let mut repeated = ids.clone();
        repeated.push(ids.first().unwrap().clone());
        let (assigned, writes) =
            count_writes(repo.assign_documents(&repeated, "space_general")).await;
        assigned.unwrap();
        assert!(
            (1..=4).contains(&writes),
            "bulk assignment used {writes} statements"
        );
        repo.assign_documents(&repeated, "space_general")
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_space_memberships")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, ids.len() as i64);

        // Missing document IDs remain a no-op for ownership updates.
        repeated.push("missing".into());
        let (owned, writes) =
            count_writes(repo.set_conversation_owner(&repeated, Some("chat"))).await;
        owned.unwrap();
        assert!(
            (1..=4).contains(&writes),
            "bulk ownership used {writes} statements"
        );
        assert_eq!(
            repo.documents_owned_by_conversation("chat").await.unwrap(),
            ids
        );
        repo.set_conversation_owner(&repeated, None).await.unwrap();
        assert!(repo
            .documents_owned_by_conversation("chat")
            .await
            .unwrap()
            .is_empty());

        let (empty, writes) = count_writes(repo.assign_documents(&[], "missing-space")).await;
        empty.unwrap();
        assert_eq!(writes, 0);
    }

    #[tokio::test]
    async fn later_batch_failure_rolls_back_all_scope_changes() {
        let (repo, pool, mut ids) = bulk_repository().await;
        ids.push("missing".into());
        assert!(repo.assign_documents(&ids, "space_general").await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_space_memberships")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            count, 0,
            "a later foreign-key failure must undo earlier batches"
        );

        sqlx::raw_sql(
            "UPDATE documents SET owner_conversation_id = 'prior' WHERE id = 'doc-0';
             CREATE TRIGGER fail_late_owner BEFORE UPDATE OF owner_conversation_id ON documents
             WHEN NEW.id = 'doc-1000' AND NEW.owner_conversation_id = 'chat'
             BEGIN SELECT RAISE(ABORT, 'injected ownership failure'); END;",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(repo
            .set_conversation_owner(&ids, Some("chat"))
            .await
            .is_err());
        assert!(repo
            .documents_owned_by_conversation("chat")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            repo.conversation_owner("doc-0").await.unwrap().as_deref(),
            Some("prior")
        );
        let failures: Vec<sqlx::sqlite::SqliteRow> = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(failures.is_empty());
    }

    #[tokio::test]
    async fn scope_validation_and_atomic_idempotent_assignment() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql("CREATE TABLE conversation_spaces (id TEXT PRIMARY KEY);
            CREATE TABLE conversations (id TEXT PRIMARY KEY);
            CREATE TABLE documents (id TEXT PRIMARY KEY, owner_conversation_id TEXT);
            INSERT INTO documents VALUES ('first', NULL), ('second', NULL);
            CREATE TABLE document_space_memberships (document_id TEXT, space_id TEXT, created_at TEXT, PRIMARY KEY(document_id, space_id));
            INSERT INTO conversation_spaces VALUES ('space'); INSERT INTO conversations VALUES ('chat');
            CREATE TRIGGER fail_second BEFORE INSERT ON document_space_memberships WHEN NEW.document_id = 'second' BEGIN SELECT RAISE(ABORT, 'injected failure'); END;")
            .execute(&pool).await.unwrap();
        let db = Database::new(pool.clone());
        let repo = SqliteDocumentScope::new(db);
        assert!(repo.space_exists("space").await.unwrap());
        assert!(!repo.space_exists("missing").await.unwrap());
        assert!(repo.conversation_exists("chat").await.unwrap());
        assert!(!repo.conversation_exists("missing").await.unwrap());
        let ids = vec!["first".into(), "second".into()];
        assert!(repo.assign_documents(&ids, "space").await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_space_memberships")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        sqlx::query("DROP TRIGGER fail_second")
            .execute(&pool)
            .await
            .unwrap();
        repo.assign_documents(&ids, "space").await.unwrap();
        repo.assign_documents(&ids, "space").await.unwrap();
        repo.assign_documents(&[], "space").await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_space_memberships")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    /// A conversation row a migration or a restored backup lost is not a
    /// conversation the user deleted: its attachments stay.
    #[tokio::test]
    async fn a_conversation_row_gone_with_no_record_of_its_delete_orphans_nothing() {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum, \
             owner_conversation_id) \
             VALUES ('attached', '/library/a.txt', 'a.txt', 1, '2026-11-01T00:00:00Z', 'abc', 'lost-chat')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let repo = SqliteDocumentScope::new(Database::new(pool));

        assert!(repo
            .orphaned_conversation_owned_documents()
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn conversation_ownership_is_stamped_cleared_and_swept() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(
            "CREATE TABLE conversations (id TEXT PRIMARY KEY);
             CREATE TABLE documents (id TEXT PRIMARY KEY, owner_conversation_id TEXT);
             CREATE TABLE file_intents (store TEXT, kind TEXT, name TEXT);
             INSERT INTO conversations VALUES ('chat');
             INSERT INTO documents VALUES ('filed', NULL), ('attached', NULL), ('stray', NULL);
             INSERT INTO file_intents VALUES ('attachment', 'release', 'stray');",
        )
        .execute(&pool)
        .await
        .unwrap();
        let db = Database::new(pool.clone());
        let repo = SqliteDocumentScope::new(db);

        repo.set_conversation_owner(&["attached".into()], Some("chat"))
            .await
            .unwrap();
        repo.set_conversation_owner(&["stray".into()], Some("deleted-chat"))
            .await
            .unwrap();
        assert_eq!(
            repo.documents_owned_by_conversation("chat").await.unwrap(),
            vec!["attached".to_string()]
        );
        // A live conversation's attachments are not orphans; a deleted one's are.
        assert_eq!(
            repo.orphaned_conversation_owned_documents().await.unwrap(),
            vec!["stray".to_string()]
        );

        // "Add to library" releases the row without touching anything else.
        repo.set_conversation_owner(&["attached".into()], None)
            .await
            .unwrap();
        assert!(repo
            .documents_owned_by_conversation("chat")
            .await
            .unwrap()
            .is_empty());
        repo.set_conversation_owner(&[], Some("chat"))
            .await
            .unwrap();
    }
}
