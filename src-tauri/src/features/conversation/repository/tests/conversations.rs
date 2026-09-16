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
