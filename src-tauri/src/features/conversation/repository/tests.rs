//! Shared fixtures for the conversation repository tests.

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

mod conversations;
mod document_references;
mod fork;
mod memory;
mod messages;
mod pruning;

async fn create_test_pool() -> SqlitePool {
    SqlitePoolOptions::new().connect(":memory:").await.unwrap()
}

/// Apply the real migration, not a hand-maintained copy of it.
///
/// These tests used to build their own abbreviated schema. That drifts: adding
/// `conversations.next_message_sequence` broke every one of them while
/// production was fine, and the reverse — a test passing against a schema
/// production does not have — is the failure that actually costs something.
/// The migration is small and the run is a few milliseconds.
async fn setup_schema(pool: &SqlitePool) {
    sqlx::migrate!("./migrations").run(pool).await.unwrap();
    // The memory-invalidation triggers depend on FK cascade behaviour, and the
    // conversation/message relationship is what several of these tests assert.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(pool)
        .await
        .unwrap();
}

/// Insert the `documents` rows a fixture is about to reference.
///
/// `conversation_documents.document_id` really does have a foreign key to
/// `documents`, and these tests now run with enforcement on, so a linked
/// document has to exist. Seeding it is also more honest than the old
/// unenforced schema: a reference to a document that was never indexed is not
/// a state the application can reach.
async fn seed_documents(pool: &SqlitePool, ids: &[&str]) {
    for id in ids {
        sqlx::query(
            "INSERT OR IGNORE INTO documents \
                (id, file_path, file_name, size_bytes, modified_at, checksum) \
             VALUES (?, ?, ?, 1, '2026-09-01T10:00:00Z', ?)",
        )
        .bind(id)
        .bind(format!("/fixtures/{id}.md"))
        .bind(format!("{id}.md"))
        .bind(format!("checksum-{id}"))
        .execute(pool)
        .await
        .unwrap();
    }
}

/// Insert a message at a controlled `created_at` so ordering is testable.
async fn seed_message(
    pool: &SqlitePool,
    conversation_id: &str,
    id: &str,
    role: &str,
    content: &str,
    tokens: i64,
    created_at: &str,
) {
    // Sequence and digest are written the way production writes them, so these
    // fixtures are valid evidence sources and order by sequence like real rows.
    let sequence: i64 =
        sqlx::query_scalar("SELECT next_message_sequence FROM conversations WHERE id = ?")
            .bind(conversation_id)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query(
        r#"
            INSERT INTO conversation_messages
                (id, conversation_id, role, content, tokens, created_at, metadata, status,
                 sequence, content_digest)
            VALUES (?, ?, ?, ?, ?, ?, NULL, 'completed', ?, ?)
            "#,
    )
    .bind(id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(tokens)
    .bind(created_at)
    .bind(sequence)
    .bind(crate::domain::conversation_memory::compute_digest(content))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE conversations SET next_message_sequence = ? WHERE id = ?")
        .bind(sequence + 1)
        .bind(conversation_id)
        .execute(pool)
        .await
        .unwrap();

    sqlx::query(
        r#"
            UPDATE conversations
            SET message_count = message_count + 1, total_tokens = total_tokens + ?
            WHERE id = ?
            "#,
    )
    .bind(tokens)
    .bind(conversation_id)
    .execute(pool)
    .await
    .unwrap();
}
async fn message_ids(pool: &SqlitePool, conversation_id: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM conversation_messages WHERE conversation_id = ? \
             ORDER BY sequence ASC, created_at ASC, rowid ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .unwrap()
}
async fn conversation_counts(pool: &SqlitePool, conversation_id: &str) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT message_count, total_tokens FROM conversations WHERE id = ?",
    )
    .bind(conversation_id)
    .fetch_one(pool)
    .await
    .unwrap()
}
/// A four-message thread: user → assistant → user → assistant.
async fn seed_thread(pool: &SqlitePool) -> String {
    sqlx::query(
            "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
             VALUES ('conv-1', 'Thread', 'model', 'space_general', '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

    seed_message(
        pool,
        "conv-1",
        "m1",
        "user",
        "first question",
        1,
        "2026-09-01T10:00:00Z",
    )
    .await;
    seed_message(
        pool,
        "conv-1",
        "m2",
        "assistant",
        "first answer",
        2,
        "2026-09-01T10:00:01Z",
    )
    .await;
    seed_message(
        pool,
        "conv-1",
        "m3",
        "user",
        "second question",
        4,
        "2026-09-01T10:00:02Z",
    )
    .await;
    seed_message(
        pool,
        "conv-1",
        "m4",
        "assistant",
        "second answer",
        8,
        "2026-09-01T10:00:03Z",
    )
    .await;

    "conv-1".to_string()
}
async fn seed_pruning_bookmarks(pool: &SqlitePool) {
    sqlx::query("INSERT INTO conversation_message_bookmarks (id, conversation_id, message_id) VALUES ('b1', 'conv-1', 'm1'), ('b2', 'conv-1', 'm4')")
            .execute(pool).await.unwrap();
}

async fn pruning_bookmark_ids(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar("SELECT id FROM conversation_message_bookmarks ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}
