//! Forking a conversation into a sibling thread.

use super::*;
use crate::features::conversation::repository::ConversationRepository;

#[tokio::test]
async fn test_fork_copies_messages_up_to() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let (new_id, copied) = repo
        .fork(
            &conversation_id,
            Some("m2"),
            "conv-branch",
            "Thread · branch",
        )
        .await
        .unwrap();

    assert_eq!(new_id, "conv-branch");
    assert_eq!(copied, 2);

    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        role: String,
        content: String,
        tokens: i64,
        status: String,
    }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT id, role, content, tokens, status FROM conversation_messages \
             WHERE conversation_id = ? ORDER BY created_at ASC, rowid ASC",
    )
    .bind("conv-branch")
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].role, "user");
    assert_eq!(rows[0].content, "first question");
    assert_eq!(rows[0].tokens, 1);
    assert_eq!(rows[0].status, "completed");
    assert_eq!(rows[1].content, "first answer");
    assert!(
        rows.iter().all(|r| r.id != "m1" && r.id != "m2"),
        "copies get fresh ids"
    );

    let (title, space_id) = sqlx::query_as::<_, (String, String)>(
        "SELECT title, space_id FROM conversations WHERE id = ?",
    )
    .bind("conv-branch")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(title.ends_with("· branch"));
    assert_eq!(space_id, "space_general");
    assert_eq!(conversation_counts(&pool, "conv-branch").await, (2, 3));
}

#[tokio::test]
async fn test_fork_copies_all_when_none() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let (_, copied) = repo
        .fork(&conversation_id, None, "conv-branch-all", "Thread · branch")
        .await
        .unwrap();

    assert_eq!(copied, 4);
    assert_eq!(message_ids(&pool, "conv-branch-all").await.len(), 4);
}

#[tokio::test]
async fn test_fork_unknown_anchor_copies_nothing() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let (_, copied) = repo
        .fork(
            &conversation_id,
            Some("does-not-exist"),
            "conv-branch-empty",
            "Thread · branch",
        )
        .await
        .unwrap();

    assert_eq!(
        copied, 0,
        "an unknown anchor must not be read as 'copy everything'"
    );
}

#[tokio::test]
async fn test_fork_copies_linked_documents_and_web_sources() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    seed_documents(&pool, &["doc-1"]).await;
    let conversation_id = seed_thread(&pool).await;

    sqlx::query(
            "INSERT INTO conversation_documents (conversation_id, document_id, chunk_id, relevance_score, added_at) \
             VALUES (?, 'doc-1', 'chunk-1', 0.9, '2026-09-01T10:00:00Z')",
        )
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
            "INSERT INTO conversation_web_sources (id, conversation_id, url, normalized_url, title, excerpt, relevance_score, added_at) \
             VALUES ('ws-1', ?, 'https://example.com/a', 'example.com/a', 'A', 'excerpt', 0.5, '2026-09-01T10:00:00Z')",
        )
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .unwrap();

    let repo = ConversationRepository::new(pool.clone());
    repo.fork(
        &conversation_id,
        None,
        "conv-branch-links",
        "Thread · branch",
    )
    .await
    .unwrap();

    let doc_ids = sqlx::query_scalar::<_, String>(
        "SELECT document_id FROM conversation_documents WHERE conversation_id = ?",
    )
    .bind("conv-branch-links")
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(doc_ids, vec!["doc-1"]);

    let web_ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM conversation_web_sources WHERE conversation_id = ?",
    )
    .bind("conv-branch-links")
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(web_ids.len(), 1);
    assert_ne!(web_ids[0], "ws-1", "the copy gets a fresh primary key");
}

#[tokio::test]
async fn test_fork_does_not_touch_original() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let before = conversation_counts(&pool, &conversation_id).await;

    let repo = ConversationRepository::new(pool.clone());
    repo.fork(
        &conversation_id,
        Some("m2"),
        "conv-branch-2",
        "Thread · branch",
    )
    .await
    .unwrap();

    assert_eq!(conversation_counts(&pool, &conversation_id).await, before);
    assert_eq!(
        message_ids(&pool, &conversation_id).await,
        vec!["m1", "m2", "m3", "m4"]
    );
}

/// A branch has to say where it came from, so the sidebar and the chat header
/// can offer the way back.
#[tokio::test]
async fn a_branch_records_the_conversation_and_turn_it_was_taken_from() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    repo.fork(
        &conversation_id,
        Some("m2"),
        "conv-lineage",
        "Thread · branch",
    )
    .await
    .unwrap();

    let lineage = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT forked_from_conversation_id, forked_from_message_id \
         FROM conversations WHERE id = ?",
    )
    .bind("conv-lineage")
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(lineage.0.as_deref(), Some(conversation_id.as_str()));
    assert_eq!(lineage.1.as_deref(), Some("m2"));
}

/// Branching the whole thread names the parent but no turn: there is no single
/// point it was taken at.
#[tokio::test]
async fn a_whole_thread_branch_records_the_parent_and_no_turn() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    repo.fork(&conversation_id, None, "conv-whole", "Thread · branch")
        .await
        .unwrap();

    let lineage = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT forked_from_conversation_id, forked_from_message_id \
         FROM conversations WHERE id = ?",
    )
    .bind("conv-whole")
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(lineage.0.as_deref(), Some(conversation_id.as_str()));
    assert_eq!(lineage.1, None);
}

/// An anchor that is not in the parent copies no messages, so it must not be
/// recorded either — a lineage link to a turn that is not there is worse than
/// none.
#[tokio::test]
async fn an_unknown_anchor_records_no_turn() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let (_, copied) = repo
        .fork(
            &conversation_id,
            Some("not-a-message"),
            "conv-dangling",
            "Thread · branch",
        )
        .await
        .unwrap();

    assert_eq!(copied, 0);
    let lineage = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT forked_from_conversation_id, forked_from_message_id \
         FROM conversations WHERE id = ?",
    )
    .bind("conv-dangling")
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(lineage.0.as_deref(), Some(conversation_id.as_str()));
    assert_eq!(lineage.1, None);
}
