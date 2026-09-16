//! Pruning, truncation and taking back the last user turn.

use super::*;
use crate::features::conversation::repository::ConversationRepository;
use crate::shared::error::AppError;

#[tokio::test]
async fn test_truncate_after_exclusive() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let deleted = repo
        .truncate_after(&conversation_id, "m2", false)
        .await
        .unwrap();

    assert_eq!(deleted, 2);
    assert_eq!(message_ids(&pool, &conversation_id).await, vec!["m1", "m2"]);
    assert_eq!(conversation_counts(&pool, &conversation_id).await, (2, 3));
}
#[tokio::test]
async fn pruning_commits_messages_bookmarks_and_totals_together() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let id = seed_thread(&pool).await;
    seed_pruning_bookmarks(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let selected = vec!["m1".into(), "m2".into(), "m2".into(), "missing".into()];
    assert_eq!(repo.delete_messages(&id, &selected).await.unwrap(), 2);
    assert_eq!(message_ids(&pool, &id).await, vec!["m3", "m4"]);
    assert_eq!(conversation_counts(&pool, &id).await, (2, 12));
    assert_eq!(pruning_bookmark_ids(&pool).await, vec!["b2"]);
    assert_eq!(repo.delete_messages(&id, &selected).await.unwrap(), 0);
    assert_eq!(repo.delete_messages(&id, &[]).await.unwrap(), 0);
    assert_eq!(conversation_counts(&pool, &id).await, (2, 12));
}

#[tokio::test]
async fn pruning_failure_rolls_back_and_allows_retry() {
    for operation in [
        "DELETE ON conversation_message_bookmarks",
        "DELETE ON conversation_messages",
        "UPDATE ON conversations",
    ] {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let id = seed_thread(&pool).await;
        seed_pruning_bookmarks(&pool).await;
        let repo = ConversationRepository::new(pool.clone());
        sqlx::query(&format!("CREATE TRIGGER reject_pruning BEFORE {operation} BEGIN SELECT RAISE(ABORT, 'injected pruning failure'); END"))
                .execute(&pool).await.unwrap();
        let result = repo.delete_messages(&id, &["m1".into(), "m2".into()]).await;
        assert!(
            matches!(result, Err(AppError::Database(message)) if message.contains("injected pruning failure"))
        );
        assert_eq!(message_ids(&pool, &id).await, vec!["m1", "m2", "m3", "m4"]);
        assert_eq!(conversation_counts(&pool, &id).await, (4, 15));
        assert_eq!(pruning_bookmark_ids(&pool).await, vec!["b1", "b2"]);
        sqlx::query("DROP TRIGGER reject_pruning")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(repo.delete_messages(&id, &["m1".into()]).await.unwrap(), 1);
        assert_eq!(conversation_counts(&pool, &id).await, (3, 14));
    }
}

#[tokio::test]
async fn pruning_cannot_delete_another_conversations_messages_or_bookmarks() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let id = seed_thread(&pool).await;
    seed_pruning_bookmarks(&pool).await;
    sqlx::query(
        "INSERT INTO conversations (id, title, model_name) VALUES ('other', 'Other', 'model')",
    )
    .execute(&pool)
    .await
    .unwrap();
    seed_message(
        &pool,
        "other",
        "foreign",
        "user",
        "keep",
        16,
        "2026-09-01T10:00:00Z",
    )
    .await;
    sqlx::query("INSERT INTO conversation_message_bookmarks (id, conversation_id, message_id) VALUES ('b3', 'other', 'foreign')")
            .execute(&pool).await.unwrap();
    let repo = ConversationRepository::new(pool.clone());
    assert_eq!(
        repo.delete_messages(&id, &["m1".into(), "foreign".into()])
            .await
            .unwrap(),
        1
    );
    assert_eq!(message_ids(&pool, "other").await, vec!["foreign"]);
    assert_eq!(conversation_counts(&pool, "other").await, (1, 16));
    assert_eq!(pruning_bookmark_ids(&pool).await, vec!["b2", "b3"]);
    assert_eq!(conversation_counts(&pool, &id).await, (3, 14));
}
#[tokio::test]
async fn test_truncate_after_inclusive() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let deleted = repo
        .truncate_after(&conversation_id, "m2", true)
        .await
        .unwrap();

    assert_eq!(deleted, 3);
    assert_eq!(message_ids(&pool, &conversation_id).await, vec!["m1"]);
    assert_eq!(conversation_counts(&pool, &conversation_id).await, (1, 1));
}

#[tokio::test]
async fn test_truncate_after_deletes_orphaned_bookmarks() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;

    for (bookmark_id, message_id) in [("b1", "m2"), ("b2", "m4")] {
        sqlx::query(
                "INSERT INTO conversation_message_bookmarks (id, conversation_id, message_id, created_at) \
                 VALUES (?, ?, ?, '2026-09-01T11:00:00Z')",
            )
            .bind(bookmark_id)
            .bind(&conversation_id)
            .bind(message_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    let repo = ConversationRepository::new(pool.clone());
    repo.truncate_after(&conversation_id, "m2", false)
        .await
        .unwrap();

    let surviving =
        sqlx::query_scalar::<_, String>("SELECT message_id FROM conversation_message_bookmarks")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(
        surviving,
        vec!["m2"],
        "only the surviving message keeps its bookmark"
    );
}

#[tokio::test]
async fn test_truncate_after_same_second_ordering() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;

    sqlx::query(
            "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
             VALUES ('conv-2', 'Tied', 'model', 'space_general', '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

    // All three share an identical timestamp; only rowid separates them.
    seed_message(
        &pool,
        "conv-2",
        "t1",
        "user",
        "a",
        1,
        "2026-09-01T10:00:00Z",
    )
    .await;
    seed_message(
        &pool,
        "conv-2",
        "t2",
        "assistant",
        "b",
        1,
        "2026-09-01T10:00:00Z",
    )
    .await;
    seed_message(
        &pool,
        "conv-2",
        "t3",
        "user",
        "c",
        1,
        "2026-09-01T10:00:00Z",
    )
    .await;

    let repo = ConversationRepository::new(pool.clone());
    let deleted = repo.truncate_after("conv-2", "t1", false).await.unwrap();

    assert_eq!(deleted, 2);
    assert_eq!(message_ids(&pool, "conv-2").await, vec!["t1"]);
}

#[tokio::test]
async fn test_take_last_user_turn_removes_trailing_assistants_only() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let taken = repo
        .take_last_user_turn(&conversation_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(taken.0, "second question");
    assert_eq!(taken.1, 4);
    assert_eq!(
        message_ids(&pool, &conversation_id).await,
        vec!["m1", "m2"],
        "the earlier user→assistant prefix is untouched"
    );
    assert_eq!(conversation_counts(&pool, &conversation_id).await, (2, 3));
}

#[tokio::test]
async fn test_take_last_user_turn_none_when_no_user_message() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;

    sqlx::query(
            "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
             VALUES ('conv-3', 'System only', 'model', 'space_general', '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
    seed_message(
        &pool,
        "conv-3",
        "s1",
        "system",
        "be helpful",
        1,
        "2026-09-01T10:00:00Z",
    )
    .await;

    let repo = ConversationRepository::new(pool.clone());
    assert!(repo.take_last_user_turn("conv-3").await.unwrap().is_none());
    assert_eq!(message_ids(&pool, "conv-3").await, vec!["s1"]);
}
