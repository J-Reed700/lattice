//! Attachments left by deleted conversations.

use std::collections::HashSet;
use std::future::Future;

use crate::infrastructure::storage::{FileIntents, FileStore};
use lattice_core::application::ports::document_scope::DocumentScopePort;
use lattice_core::shared::error::{AppError, Result};

/// Delete, through `delete`, every attachment whose conversation's delete
/// recorded it as let go of and whose conversation is gone, and clear each
/// record once its document is gone or claimed by a live conversation again.
/// Returns how many were found and how many deleted.
pub async fn reconcile_orphaned_attachments<F, Fut>(
    scope: &dyn DocumentScopePort,
    intents: &FileIntents,
    delete: F,
) -> Result<(usize, usize)>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let records = intents.pending(FileStore::Attachment).await?;
    let orphans = scope.orphaned_conversation_owned_documents().await?;
    let mut done: HashSet<String> = records
        .iter()
        .map(|record| record.name.clone())
        .filter(|name| !orphans.contains(name))
        .collect();
    let mut deleted = 0usize;
    for document_id in &orphans {
        match delete(document_id.clone()).await {
            Ok(()) => {
                deleted += 1;
                done.insert(document_id.clone());
            }
            Err(AppError::NotFound(_)) => {
                done.insert(document_id.clone());
            }
            Err(error) => {
                tracing::warn!(%error, document_id, "Could not delete an orphaned attachment")
            }
        }
    }
    let settled: Vec<i64> = records
        .iter()
        .filter(|record| done.contains(&record.name))
        .map(|record| record.id)
        .collect();
    intents.forget(&settled).await?;
    Ok((orphans.len(), deleted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::conversation::repository::ConversationRepository;
    use crate::infrastructure::persistence::repositories::document_scope::SqliteDocumentScope;
    use sqlx::SqlitePool;

    async fn migrated_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn conversation(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO conversations (id, title, space_id, created_at, updated_at) \
             VALUES (?, 'A chat', 'space_general', '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn attachment(pool: &SqlitePool, id: &str, owner: &str) {
        sqlx::query(
            "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum, \
             owner_conversation_id) VALUES (?, ?, 'a.txt', 1, '2026-11-01T00:00:00Z', 'abc', ?)",
        )
        .bind(id)
        .bind(format!("/library/{id}.txt"))
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn documents(pool: &SqlitePool) -> Vec<String> {
        sqlx::query_scalar("SELECT id FROM documents ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    /// A tangent goes with its parent by cascade; its attachments, and a nested
    /// tangent's, are orphans of that delete too.
    #[tokio::test]
    async fn a_deleted_chats_tangents_leave_orphans_too() {
        let pool = migrated_pool().await;
        conversation(&pool, "chat").await;
        for (tangent, parent) in [("tangent", "chat"), ("nested", "tangent")] {
            sqlx::query(
                "INSERT INTO conversations (id, title, space_id, created_at, updated_at,                  tangent_parent_id) VALUES (?, 'A tangent', 'space_general',                  '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z', ?)",
            )
            .bind(tangent)
            .bind(parent)
            .execute(&pool)
            .await
            .unwrap();
        }
        attachment(&pool, "in-tangent", "tangent").await;
        attachment(&pool, "in-nested", "nested").await;
        ConversationRepository::new(pool.clone())
            .delete("chat")
            .await
            .unwrap();
        let intents = FileIntents::new(pool.clone());

        let delete = |id: String| {
            let pool = pool.clone();
            async move {
                sqlx::query("DELETE FROM documents WHERE id = ?")
                    .bind(id)
                    .execute(&pool)
                    .await?;
                Ok::<(), AppError>(())
            }
        };
        let (found, deleted) = reconcile_orphaned_attachments(
            &SqliteDocumentScope::new(pool.clone()),
            &intents,
            delete,
        )
        .await
        .unwrap();

        assert_eq!((found, deleted), (2, 2));
        assert!(documents(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn a_deleted_chats_leftover_attachment_goes_and_a_lost_rows_does_not() {
        let pool = migrated_pool().await;
        conversation(&pool, "deleted-chat").await;
        attachment(&pool, "leftover", "deleted-chat").await;
        // A conversation row that went without the app's delete.
        attachment(&pool, "row-lost", "lost-chat").await;
        ConversationRepository::new(pool.clone())
            .delete("deleted-chat")
            .await
            .unwrap();
        let intents = FileIntents::new(pool.clone());

        let delete = |id: String| {
            let pool = pool.clone();
            async move {
                sqlx::query("DELETE FROM documents WHERE id = ?")
                    .bind(id)
                    .execute(&pool)
                    .await?;
                Ok::<(), AppError>(())
            }
        };
        let (found, deleted) = reconcile_orphaned_attachments(
            &SqliteDocumentScope::new(pool.clone()),
            &intents,
            delete,
        )
        .await
        .unwrap();

        assert_eq!((found, deleted), (1, 1));
        assert_eq!(documents(&pool).await, ["row-lost"]);
        assert!(intents
            .pending(FileStore::Attachment)
            .await
            .unwrap()
            .is_empty());
    }
}
