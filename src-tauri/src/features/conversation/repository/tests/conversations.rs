//! Conversation CRUD, aggregates and counts.

use super::*;
use crate::domain::conversation::MessageRole;
use crate::features::conversation::repository::ConversationRepository;

#[tokio::test]
async fn test_create_conversation() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool);

    let conversation = repo
        .create_conversation(
            "Test Chat",
            "claude-sonnet-4-5-20250929",
            Some("Be helpful"),
        )
        .await
        .unwrap();

    assert_eq!(conversation.title, "Test Chat");
    assert_eq!(conversation.model_name, "claude-sonnet-4-5-20250929");
    assert_eq!(conversation.system_prompt, Some("Be helpful".to_string()));
    assert_eq!(conversation.message_count, 0);
    assert_eq!(conversation.total_tokens, 0);
}

#[tokio::test]
async fn test_find_by_id() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool);

    let created = repo
        .create_conversation("Test", "model", None)
        .await
        .unwrap();

    let found = repo
        .find_by_id(&created.id.to_string())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, created.id);
    assert_eq!(found.title, "Test");
}
#[tokio::test]
async fn test_find_aggregate_by_id() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool);

    let conversation = repo
        .create_conversation("Test", "model", None)
        .await
        .unwrap();

    repo.add_message(
        &conversation.id.to_string(),
        MessageRole::User,
        "Hello",
        5,
        None,
    )
    .await
    .unwrap();

    let aggregate = repo
        .find_aggregate_by_id(&conversation.id.to_string())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(aggregate.title(), "Test");
    assert_eq!(aggregate.messages().len(), 1);
    assert_eq!(aggregate.messages()[0].content, "Hello");
}
#[tokio::test]
async fn test_update_title() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool);

    let conversation = repo
        .create_conversation("Old Title", "model", None)
        .await
        .unwrap();

    repo.update_title(&conversation.id.to_string(), "New Title")
        .await
        .unwrap();

    let updated = repo
        .find_by_id(&conversation.id.to_string())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.title, "New Title");
}

#[tokio::test]
async fn test_delete_conversation() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool);

    let conversation = repo
        .create_conversation("Test", "model", None)
        .await
        .unwrap();

    repo.delete(&conversation.id.to_string()).await.unwrap();

    let found = repo.find_by_id(&conversation.id.to_string()).await.unwrap();

    assert!(found.is_none());
}

#[tokio::test]
async fn test_delete_conversation_preserves_space_memberships() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let conversation = repo
        .create_conversation("Test", "model", None)
        .await
        .unwrap();

    repo.add_document_reference(&conversation.id.to_string(), "doc-1", None, Some(0.9))
        .await
        .unwrap();

    let before_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_space_memberships WHERE document_id = 'doc-1' AND space_id = 'space_general'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before_count, 1);

    repo.delete(&conversation.id.to_string()).await.unwrap();

    let after_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_space_memberships WHERE document_id = 'doc-1' AND space_id = 'space_general'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after_count, 1);
}

#[tokio::test]
async fn test_delete_conversation_keeps_membership_when_other_space_conversation_uses_doc() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let conversation_a = repo
        .create_conversation("Conversation A", "model", None)
        .await
        .unwrap();
    let conversation_b = repo
        .create_conversation("Conversation B", "model", None)
        .await
        .unwrap();

    repo.add_document_reference(
        &conversation_a.id.to_string(),
        "doc-shared",
        None,
        Some(0.7),
    )
    .await
    .unwrap();
    repo.add_document_reference(
        &conversation_b.id.to_string(),
        "doc-shared",
        None,
        Some(0.8),
    )
    .await
    .unwrap();

    repo.delete(&conversation_a.id.to_string()).await.unwrap();

    let membership_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_space_memberships WHERE document_id = 'doc-shared' AND space_id = 'space_general'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(membership_count, 1);
}

#[tokio::test]
async fn test_find_all() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool);

    repo.create_conversation("Conv 1", "model", None)
        .await
        .unwrap();
    repo.create_conversation("Conv 2", "model", None)
        .await
        .unwrap();
    repo.create_conversation("Conv 3", "model", None)
        .await
        .unwrap();

    let all = repo.find_all(None, None).await.unwrap();
    assert_eq!(all.len(), 3);

    let limited = repo.find_all(Some(2), None).await.unwrap();
    assert_eq!(limited.len(), 2);
}

#[tokio::test]
async fn test_count_and_exists() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool);

    assert_eq!(repo.count().await.unwrap(), 0);

    let conversation = repo
        .create_conversation("Test", "model", None)
        .await
        .unwrap();

    assert_eq!(repo.count().await.unwrap(), 1);
    assert!(repo.exists(&conversation.id.to_string()).await.unwrap());
    assert!(!repo.exists("non-existent").await.unwrap());
}

#[tokio::test]
async fn test_upsert_summary_keeps_one_row_per_conversation() {
    use crate::domain::conversation::CompactionRecord;
    use crate::shared::domain_types::ConversationId;
    use std::str::FromStr;

    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let conversation = repo
        .create_conversation("Compacted", "model", None)
        .await
        .unwrap();
    let conversation_id = conversation.id.to_string();
    seed_message(
        &pool,
        &conversation_id,
        "msg-1",
        "user",
        "first",
        10,
        "2026-01-01T00:00:00Z",
    )
    .await;

    let record = |summary: &str, summary_tokens: i64| CompactionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: ConversationId::from_str(&conversation_id).unwrap(),
        summary_text: summary.to_string(),
        up_to_message_id: "msg-1".to_string(),
        original_message_count: 1,
        original_tokens: 10,
        summary_tokens,
        compression_ratio: summary_tokens as f64 / 10.0,
        created_at: chrono::Utc::now(),
    };

    let first = record("first summary", 5);
    repo.upsert_summary(&first).await.unwrap();
    let stored_first = repo.get_summary(&conversation_id).await.unwrap().unwrap();

    // The row must carry the record's own id and timestamp: callers are handed
    // the record, so a row keyed by anything else is a row they cannot find.
    assert_eq!(stored_first.id, first.id);
    assert_eq!(
        stored_first.created_at.timestamp(),
        first.created_at.timestamp()
    );
    let first_id = stored_first.id;

    // Re-compacting replaces the summary in place: callers read one row, so
    // `get_summary` never has to choose between competing summaries.
    repo.upsert_summary(&record("second summary", 4))
        .await
        .unwrap();

    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_summaries WHERE conversation_id = ?")
            .bind(&conversation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(rows, 1, "a conversation must keep at most one summary");

    let stored = repo.get_summary(&conversation_id).await.unwrap().unwrap();
    assert_eq!(stored.id, first_id, "the summary id stays stable");
    assert_eq!(stored.summary_text, "second summary");
    assert_eq!(stored.summary_tokens, 4);
}
