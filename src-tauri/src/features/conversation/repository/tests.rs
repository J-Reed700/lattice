//! Shared fixtures for the conversation repository tests.

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

mod conversations;
mod document_references;
mod fork;
mod messages;
mod pruning;

async fn create_test_pool() -> SqlitePool {
    SqlitePoolOptions::new().connect(":memory:").await.unwrap()
}

async fn setup_schema(pool: &SqlitePool) {
    sqlx::query(
        r#"
            CREATE TABLE conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                model_name TEXT NOT NULL,
                system_prompt TEXT,
                space_id TEXT NOT NULL DEFAULT 'space_general',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                message_count INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE conversation_spaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL
            );

            INSERT INTO conversation_spaces (id, name) VALUES ('space_general', 'General');

            CREATE TABLE conversation_messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
                content TEXT NOT NULL,
                tokens INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                metadata TEXT,
                status TEXT NOT NULL DEFAULT 'completed',
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE conversation_message_bookmarks (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                title TEXT,
                note TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE (conversation_id, message_id)
            );

            CREATE TABLE conversation_web_sources (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                url TEXT NOT NULL,
                normalized_url TEXT NOT NULL,
                title TEXT,
                excerpt TEXT,
                relevance_score REAL,
                added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE (conversation_id, normalized_url)
            );

            CREATE TABLE conversation_documents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL,
                document_id TEXT NOT NULL,
                chunk_id TEXT,
                relevance_score REAL,
                added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
                UNIQUE(conversation_id, chunk_id)
            );

            CREATE TABLE document_space_memberships (
                document_id TEXT NOT NULL,
                space_id TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (document_id, space_id)
            );
            "#,
    )
    .execute(pool)
    .await
    .unwrap();
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
    sqlx::query(
        r#"
            INSERT INTO conversation_messages
                (id, conversation_id, role, content, tokens, created_at, metadata, status)
            VALUES (?, ?, ?, ?, ?, ?, NULL, 'completed')
            "#,
    )
    .bind(id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(tokens)
    .bind(created_at)
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
             ORDER BY created_at ASC, rowid ASC",
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
