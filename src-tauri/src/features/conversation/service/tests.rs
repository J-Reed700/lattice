use super::*;
use crate::features::conversation::repository::ConversationRepository;
use sqlx::SqlitePool;

#[tokio::test]
async fn invalid_input_never_reaches_repository() {
    use lattice_core::application::ports::conversation_repository::MockConversationRepositoryPort;
    // An unexpected call fails: validation must happen before persistence.
    let service = ConversationService::new(Arc::new(MockConversationRepositoryPort::new()));
    assert!(matches!(
        service.rename_conversation("chat", " ".into()).await,
        Err(AppError::InvalidInput(_))
    ));
    assert!(matches!(
        service.add_user_message("chat", " ".into(), 1).await,
        Err(AppError::InvalidInput(_))
    ));
    assert!(matches!(
        service.prune_conversation_to_limit("chat", 0).await,
        Err(AppError::InvalidInput(_))
    ));
}

#[tokio::test]
async fn clearing_prompt_is_forwarded_as_none() {
    use lattice_core::application::ports::conversation_repository::MockConversationRepositoryPort;
    let mut repository = MockConversationRepositoryPort::new();
    repository
        .expect_update_system_prompt()
        .withf(|id, prompt| id == "chat" && prompt.is_none())
        .times(1)
        .returning(|_, _| Ok(()));
    let service = ConversationService::new(Arc::new(repository));
    service.update_system_prompt("chat", None).await.unwrap();
}

#[tokio::test]
async fn repository_failure_is_preserved() {
    use lattice_core::application::ports::conversation_repository::MockConversationRepositoryPort;
    let mut repository = MockConversationRepositoryPort::new();
    repository
        .expect_update_message_status()
        .withf(|id, status| id == "message" && status == "failed")
        .times(1)
        .returning(|_, _| Err(AppError::Database("write failed".into())));
    let service = ConversationService::new(Arc::new(repository));
    assert!(
        matches!(service.update_message_status("message", "failed".into()).await,
        Err(AppError::Database(message)) if message == "write failed")
    );
}

#[tokio::test]
async fn missing_conversation_stops_pruning_before_write() {
    use lattice_core::application::ports::conversation_repository::MockConversationRepositoryPort;
    let mut repository = MockConversationRepositoryPort::new();
    repository
        .expect_load_aggregate()
        .withf(|id| id == "missing")
        .times(1)
        .returning(|_| Ok(None));
    let service = ConversationService::new(Arc::new(repository));
    assert!(matches!(
        service.prune_conversation_to_limit("missing", 10).await,
        Err(AppError::NotFound(_))
    ));
}

/// Create a test database pool from the real migration.
///
/// Deliberately not a hand-maintained subset of the schema: a copy drifts,
/// and a test that passes against a schema production does not have is
/// worse than no test. Adding `conversations.next_message_sequence` broke
/// every one of these while production was fine.
async fn create_test_pool() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // Several of these tests assert cascade behaviour, which SQLite only
    // enforces with this on.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    pool
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

/// Create test service with fresh database
async fn create_test_service() -> (ConversationService, SqlitePool) {
    let pool = create_test_pool().await;
    let service = ConversationService::new(Arc::new(ConversationRepository::new(pool.clone())));
    (service, pool)
}

#[tokio::test]
async fn complete_turn_is_atomic_and_rejects_replays() {
    use crate::features::conversation::ConversationServiceTrait;
    let (service, pool) = create_test_service().await;
    let conversation = service
        .create_conversation("Chat".into(), "model".into(), None)
        .await
        .unwrap();
    let id = conversation.id.to_string();
    let user = service
        .add_message_with_status(
            &id,
            MessageRole::User,
            "question".into(),
            2,
            "pending".into(),
        )
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER fail_assistant BEFORE INSERT ON conversation_messages WHEN NEW.role='assistant' BEGIN SELECT RAISE(ABORT, 'injected failure'); END")
        .execute(&pool).await.unwrap();
    assert!(service
        .complete_turn(&id, &user.id, "answer".into(), 3, None)
        .await
        .is_err());
    let snapshot = service.get_conversation(&id).await.unwrap().unwrap();
    assert_eq!(snapshot.messages().len(), 1);
    assert_eq!(snapshot.messages()[0].status, "pending");
    assert_eq!(snapshot.conversation().total_tokens, 2);
    sqlx::query("DROP TRIGGER fail_assistant")
        .execute(&pool)
        .await
        .unwrap();
    let assistant = service
        .complete_turn(&id, &user.id, "answer".into(), 3, Some("{}".into()))
        .await
        .unwrap();
    assert_eq!(assistant.metadata.as_deref(), Some("{}"));
    let snapshot = service.get_conversation(&id).await.unwrap().unwrap();
    assert_eq!(snapshot.messages().len(), 2);
    assert!(snapshot.messages().iter().all(|m| m.status == "completed"));
    // A late transport/read failure must not undo the committed turn.
    service.fail_pending_turn(&user.id).await.unwrap();
    assert_eq!(
        service
            .get_conversation(&id)
            .await
            .unwrap()
            .unwrap()
            .messages()[0]
            .status,
        "completed"
    );
    assert_eq!(snapshot.conversation().total_tokens, 5);
    assert!(matches!(
        service
            .complete_turn(&id, &user.id, "duplicate".into(), 3, None)
            .await,
        Err(AppError::InvalidState(_))
    ));
    assert_eq!(
        service
            .get_conversation(&id)
            .await
            .unwrap()
            .unwrap()
            .messages()
            .len(),
        2
    );
}

/// Count messages for a conversation (for CASCADE DELETE verification)
async fn count_messages(pool: &SqlitePool, conversation_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversation_messages WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

/// Count document references for a conversation (for CASCADE DELETE verification)
async fn count_document_refs(pool: &SqlitePool, conversation_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversation_documents WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

#[tokio::test]
async fn test_create_conversation_with_system_prompt() {
    let (service, _pool) = create_test_service().await;

    let result = service
        .create_conversation(
            "Test Chat".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            Some("You are a helpful assistant.".to_string()),
        )
        .await;

    let conversation = result.expect("create_conversation should succeed");
    assert_eq!(conversation.title, "Test Chat");
    assert_eq!(conversation.model_name, "claude-sonnet-4-5-20250929");
    assert_eq!(
        conversation.system_prompt,
        Some("You are a helpful assistant.".to_string())
    );
    assert_eq!(conversation.message_count, 0);
    assert_eq!(conversation.total_tokens, 0);
    assert!(!conversation.id.to_string().is_empty());
}

#[tokio::test]
async fn test_create_conversation_rejects_empty_title() {
    let (service, _pool) = create_test_service().await;

    let result = service
        .create_conversation(
            "".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await;

    assert!(result.is_err());
    match result {
        Err(AppError::InvalidInput(msg)) => {
            assert!(msg.contains("title") || msg.contains("empty"));
        }
        _ => panic!("Expected AppError::InvalidInput"),
    }
}

#[tokio::test]
async fn test_get_conversation_loads_aggregate() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test Conv".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            Some("System prompt".to_string()),
        )
        .await
        .expect("create should succeed");

    service
        .add_user_message(&conversation.id.to_string(), "Hello!".to_string(), 10)
        .await
        .expect("add_user_message should succeed");

    let result = service.get_conversation(&conversation.id.to_string()).await;

    let aggregate = result
        .expect("get_conversation should succeed")
        .expect("Conversation should exist");

    assert_eq!(aggregate.conversation().id, conversation.id);
    assert_eq!(aggregate.conversation().title, "Test Conv");
    assert_eq!(aggregate.message_count(), 1);
    assert_eq!(aggregate.messages().len(), 1);
    assert_eq!(aggregate.messages()[0].content, "Hello!");
}

#[tokio::test]
async fn test_get_conversation_returns_none_for_missing() {
    let (service, _pool) = create_test_service().await;

    let result = service.get_conversation("nonexistent-id").await;

    let option = result.expect("get_conversation should succeed");
    assert!(option.is_none());
}

#[tokio::test]
async fn test_list_conversations_with_pagination() {
    let (service, _pool) = create_test_service().await;

    for i in 0..5 {
        service
            .create_conversation(
                format!("Conv {}", i),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");
    }

    let page1 = service
        .list_conversations(Some(3), Some(0))
        .await
        .expect("list should succeed");
    let page2 = service
        .list_conversations(Some(3), Some(3))
        .await
        .expect("list should succeed");

    assert_eq!(page1.len(), 3);
    assert_eq!(page2.len(), 2);

    let page1_ids: Vec<_> = page1.iter().map(|c| c.id.clone()).collect();
    let page2_ids: Vec<_> = page2.iter().map(|c| c.id.clone()).collect();
    assert!(page1_ids.iter().all(|id| !page2_ids.contains(id)));
}

#[tokio::test]
async fn test_rename_conversation() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Old Title".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    service
        .rename_conversation(&conversation.id.to_string(), "New Title".to_string())
        .await
        .expect("rename should succeed");

    let updated = service
        .get_conversation(&conversation.id.to_string())
        .await
        .expect("get should succeed")
        .expect("Conversation should exist");

    assert_eq!(updated.conversation().title, "New Title");
}

#[tokio::test]
async fn test_add_user_message() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    let result = service
        .add_user_message(&conversation.id.to_string(), "Hello!".to_string(), 10)
        .await;

    let message = result.expect("add_user_message should succeed");
    assert_eq!(message.content, "Hello!");
    assert_eq!(message.tokens, 10);
    assert!(matches!(message.role, MessageRole::User));
    assert_eq!(message.conversation_id, conversation.id);
}

#[tokio::test]
async fn test_add_assistant_message() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    let result = service
        .add_assistant_message(&conversation.id.to_string(), "Hi there!".to_string(), 20)
        .await;

    let message = result.expect("add_assistant_message should succeed");
    assert_eq!(message.content, "Hi there!");
    assert_eq!(message.tokens, 20);
    assert!(matches!(message.role, MessageRole::Assistant));
}

#[tokio::test]
async fn setting_metadata_fields_keeps_the_other_keys() {
    use crate::features::conversation::ConversationServiceTrait;
    let (service, _pool) = create_test_service().await;
    let conversation = service
        .create_conversation("Test".to_string(), "model".to_string(), None)
        .await
        .expect("create should succeed");
    let message = service
        .add_assistant_message_with_metadata(
            &conversation.id.to_string(),
            "Answer".to_string(),
            5,
            Some(r#"{"sources":[1],"verification":{"enabled":true,"pending":true}}"#.to_string()),
        )
        .await
        .expect("add should succeed");

    ConversationServiceTrait::set_message_metadata_fields(
        &service,
        &message.id,
        vec![(
            "verification".to_string(),
            serde_json::json!({"enabled": true, "claimsEvaluated": 2}),
        )],
    )
    .await
    .expect("update should succeed");

    let aggregate = service
        .get_conversation(&conversation.id.to_string())
        .await
        .expect("load")
        .expect("present");
    let stored = aggregate
        .messages()
        .iter()
        .find(|m| m.id == message.id)
        .and_then(|m| m.metadata.clone())
        .expect("metadata");
    let stored: serde_json::Value = serde_json::from_str(&stored).expect("json");
    assert_eq!(stored["sources"], serde_json::json!([1]));
    assert_eq!(
        stored["verification"],
        serde_json::json!({"enabled": true, "claimsEvaluated": 2})
    );

    let missing = ConversationServiceTrait::set_message_metadata_fields(
        &service,
        "no-such-message",
        vec![("verification".to_string(), serde_json::json!({}))],
    )
    .await;
    assert!(missing.is_err());
}

#[tokio::test]
async fn test_add_assistant_message_with_metadata() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    let metadata = r#"{"sources": [{"title": "doc1", "score": 0.9}]}"#;

    let result = service
        .add_assistant_message_with_metadata(
            &conversation.id.to_string(),
            "Answer with sources".to_string(),
            50,
            Some(metadata.to_string()),
        )
        .await;

    let message = result.expect("add_assistant_message_with_metadata should succeed");
    assert_eq!(message.content, "Answer with sources");
    assert_eq!(message.tokens, 50);
    assert!(matches!(message.role, MessageRole::Assistant));
    assert_eq!(message.metadata, Some(metadata.to_string()));
}

#[tokio::test]
async fn test_add_message_rejects_empty_content() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    let result = service
        .add_user_message(&conversation.id.to_string(), "".to_string(), 0)
        .await;

    assert!(result.is_err());
    match result {
        Err(AppError::InvalidInput(msg)) => {
            assert!(msg.contains("content") || msg.contains("empty"));
        }
        _ => panic!("Expected AppError::InvalidInput"),
    }
}

#[tokio::test]
async fn test_add_message_to_nonexistent_conversation() {
    let (service, _pool) = create_test_service().await;

    let result = service
        .add_user_message("fake-conversation-id", "Orphan message".to_string(), 10)
        .await;

    assert!(result.is_err());
    match result {
        Err(AppError::Database(_)) | Err(AppError::NotFound(_)) => {}
        _ => panic!("Expected AppError::Database or AppError::NotFound"),
    }
}

#[tokio::test]
async fn test_prune_conversation_to_token_limit() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    for i in 0..5 {
        service
            .add_user_message(&conversation.id.to_string(), format!("Message {}", i), 100)
            .await
            .expect("add message should succeed");
    }

    service
        .prune_conversation_to_limit(&conversation.id.to_string(), 250)
        .await
        .expect("prune should succeed");

    let aggregate = service
        .get_conversation(&conversation.id.to_string())
        .await
        .expect("get should succeed")
        .expect("Conversation should exist");

    assert!(aggregate.total_tokens() <= 250);
    assert!(aggregate.message_count() >= 1); // At least 1 message preserved
}

#[tokio::test]
async fn test_prune_rejects_invalid_token_limit() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    let result = service
        .prune_conversation_to_limit(&conversation.id.to_string(), 0)
        .await;

    assert!(result.is_err());
    match result {
        Err(AppError::InvalidInput(msg)) => {
            assert!(msg.contains("limit") || msg.contains("positive"));
        }
        _ => panic!("Expected AppError::InvalidInput"),
    }
}

#[tokio::test]
async fn test_delete_conversation_cascades_to_messages() {
    let (service, pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test Conv".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    let conv_id = conversation.id.to_string();

    service
        .add_user_message(&conv_id, "msg1".to_string(), 10)
        .await
        .expect("add message should succeed");
    service
        .add_user_message(&conv_id, "msg2".to_string(), 10)
        .await
        .expect("add message should succeed");
    service
        .add_user_message(&conv_id, "msg3".to_string(), 10)
        .await
        .expect("add message should succeed");

    let before_count = count_messages(&pool, &conv_id).await;
    assert_eq!(before_count, 3);

    service
        .delete_conversation(&conv_id)
        .await
        .expect("delete should succeed");

    let conv_result = service
        .get_conversation(&conv_id)
        .await
        .expect("get should succeed");
    assert!(conv_result.is_none(), "Conversation should be deleted");

    // 2. **CRITICAL**: Verify messages cascade deleted
    let after_count = count_messages(&pool, &conv_id).await;
    assert_eq!(after_count, 0, "All messages must cascade delete");
}

#[tokio::test]
async fn test_delete_conversation_cascades_to_document_references() {
    let (service, pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test Conv".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    let conv_id = conversation.id.to_string();
    seed_documents(&pool, &["doc-123", "doc-789"]).await;

    service
        .add_document_reference(
            &conv_id,
            "doc-123".to_string(),
            Some("chunk-456".to_string()),
            Some(0.95),
        )
        .await
        .expect("add reference should succeed");
    service
        .add_document_reference(
            &conv_id,
            "doc-789".to_string(),
            Some("chunk-012".to_string()),
            Some(0.88),
        )
        .await
        .expect("add reference should succeed");

    let before_count = count_document_refs(&pool, &conv_id).await;
    assert_eq!(before_count, 2);

    service
        .delete_conversation(&conv_id)
        .await
        .expect("delete should succeed");

    let after_count = count_document_refs(&pool, &conv_id).await;
    assert_eq!(
        after_count, 0,
        "All document references must cascade delete"
    );
}

#[tokio::test]
async fn test_delete_nonexistent_conversation() {
    let (service, _pool) = create_test_service().await;

    let result = service.delete_conversation("nonexistent-id").await;

    assert!(result.is_err());
    match result {
        Err(AppError::NotFound(_)) => {}
        _ => panic!("Expected AppError::NotFound"),
    }
}

#[tokio::test]
async fn test_orphaned_messages_prevented_by_foreign_key() {
    let (service, _pool) = create_test_service().await;

    let result = service
        .add_user_message(
            "fake-conversation-id",
            "Orphaned message attempt".to_string(),
            10,
        )
        .await;

    assert!(result.is_err());
    match result {
        Err(AppError::Database(msg)) | Err(AppError::NotFound(msg)) => {
            let msg_lower = msg.to_lowercase();
            assert!(
                msg_lower.contains("foreign key")
                    || msg_lower.contains("constraint")
                    || msg_lower.contains("not found")
            );
        }
        _ => {
            panic!("Expected AppError::Database or AppError::NotFound with foreign key message")
        }
    }
}

#[tokio::test]
async fn test_update_system_prompt() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            Some("Old prompt".to_string()),
        )
        .await
        .expect("create should succeed");

    service
        .update_system_prompt(&conversation.id.to_string(), Some("New prompt".to_string()))
        .await
        .expect("update should succeed");

    let updated = service
        .get_conversation(&conversation.id.to_string())
        .await
        .expect("get should succeed")
        .expect("Conversation should exist");

    assert_eq!(
        updated.conversation().system_prompt,
        Some("New prompt".to_string())
    );
}

#[tokio::test]
async fn test_update_system_prompt_to_none() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            Some("Old prompt".to_string()),
        )
        .await
        .expect("create should succeed");

    service
        .update_system_prompt(&conversation.id.to_string(), None)
        .await
        .expect("update should succeed");

    let updated = service
        .get_conversation(&conversation.id.to_string())
        .await
        .expect("get should succeed")
        .expect("Conversation should exist");

    assert_eq!(updated.conversation().system_prompt, None);
}

#[tokio::test]
async fn test_add_document_reference() {
    let (service, pool) = create_test_service().await;
    seed_documents(&pool, &["doc-123"]).await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    service
        .add_document_reference(
            &conversation.id.to_string(),
            "doc-123".to_string(),
            Some("chunk-456".to_string()),
            Some(0.95),
        )
        .await
        .expect("add reference should succeed");

    let aggregate = service
        .get_conversation(&conversation.id.to_string())
        .await
        .expect("get should succeed")
        .expect("Conversation should exist");

    assert_eq!(aggregate.document_context().len(), 1);
    assert_eq!(
        aggregate.document_context()[0].document_id.as_str(),
        "doc-123"
    );
}

#[tokio::test]
async fn test_get_conversation_includes_document_references() {
    let (service, pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    let conv_id = conversation.id.to_string();
    seed_documents(&pool, &["doc-1", "doc-2"]).await;

    service
        .add_document_reference(
            &conv_id,
            "doc-1".to_string(),
            Some("chunk-1".to_string()),
            Some(0.9),
        )
        .await
        .expect("add reference should succeed");
    service
        .add_document_reference(
            &conv_id,
            "doc-2".to_string(),
            Some("chunk-2".to_string()),
            Some(0.8),
        )
        .await
        .expect("add reference should succeed");

    let result = service.get_conversation(&conv_id).await;

    let aggregate = result
        .expect("get should succeed")
        .expect("Conversation should exist");

    assert_eq!(aggregate.document_context().len(), 2);
}

#[tokio::test]
async fn test_two_phase_commit_pattern() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    let conv_id = conversation.id.to_string();

    let result = service
        .add_message_with_status(
            &conv_id,
            MessageRole::User,
            "Question?".to_string(),
            10,
            "pending".to_string(),
        )
        .await;

    let message = result.expect("add_message_with_status should succeed");
    assert_eq!(message.status, "pending");

    service
        .update_message_status(&message.id.to_string(), "completed".to_string())
        .await
        .expect("update_message_status should succeed");

    let aggregate = service
        .get_conversation(&conv_id)
        .await
        .expect("get should succeed")
        .expect("Conversation should exist");

    let updated_msg = &aggregate.messages()[0];
    assert_eq!(updated_msg.status, "completed");
}

#[tokio::test]
async fn test_two_phase_commit_failure() {
    let (service, _pool) = create_test_service().await;

    let conversation = service
        .create_conversation(
            "Test".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            None,
        )
        .await
        .expect("create should succeed");

    let conv_id = conversation.id.to_string();

    let message = service
        .add_message_with_status(
            &conv_id,
            MessageRole::User,
            "Question?".to_string(),
            10,
            "pending".to_string(),
        )
        .await
        .expect("add_message_with_status should succeed");

    service
        .update_message_status(&message.id.to_string(), "failed".to_string())
        .await
        .expect("update_message_status should succeed");

    let aggregate = service
        .get_conversation(&conv_id)
        .await
        .expect("get should succeed")
        .expect("Conversation should exist");

    let updated_msg = &aggregate.messages()[0];
    assert_eq!(updated_msg.status, "failed");
}

#[tokio::test]
async fn test_update_message_status_nonexistent_message() {
    let (service, _pool) = create_test_service().await;

    let result = service
        .update_message_status("fake-message-id", "completed".to_string())
        .await;

    assert!(result.is_err());
    match result {
        Err(AppError::NotFound(_)) => {}
        _ => panic!("Expected AppError::NotFound"),
    }
}
