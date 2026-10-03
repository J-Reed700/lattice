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

/// Deleting a conversation must take every kind of conversation-scoped data
/// with it: transcript, citations and their permanent page snapshots,
/// bookmarks, summary, memory ledger and vectors, and journal links. The
/// schema promises this with cascading foreign keys; this test holds it to
/// that promise on the real migration, table by table.
#[tokio::test]
async fn test_delete_conversation_cascades_to_every_related_table() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let conversation_id = seed_thread(&pool).await;

    seed_documents(&pool, &["doc-1"]).await;
    sqlx::query("INSERT INTO conversation_documents (conversation_id, document_id) VALUES (?, ?)")
        .bind(&conversation_id)
        .bind("doc-1")
        .execute(&pool)
        .await
        .unwrap();
    repo.store_conversation_web_source_snapshot(
        conversation_id.clone(),
        "https://example.com/archived".into(),
        Some("Archived page".into()),
        "The full article text, archived beside the citation.".into(),
        false,
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversation_message_bookmarks (id, conversation_id, message_id) \
         VALUES ('bm-1', ?, 'm1')",
    )
    .bind(&conversation_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversation_summaries (id, conversation_id, summary_text, up_to_message_id, \
         original_message_count, original_tokens, summary_tokens, compression_ratio) \
         VALUES ('sum-1', ?, 'summary', 'm1', 1, 10, 5, 0.5)",
    )
    .bind(&conversation_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversation_memory_vectors (id, conversation_id, message_id, role, content, \
         embedding, dimension, embedding_model) \
         VALUES ('vec-1', ?, 'm1', 'user', 'first question', X'0000', 1, 'test-model')",
    )
    .bind(&conversation_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO conversation_memory_state (conversation_id) VALUES (?)")
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO conversation_memory_items (id, conversation_id, kind, label, \
         created_at_sequence, changed_at_sequence) \
         VALUES ('mi-1', ?, 'established_fact', 'a sourced fact', 1, 1)",
    )
    .bind(&conversation_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversation_memory_evidence (item_id, ordinal, message_id, sequence, role, \
         start_byte, end_byte, content_digest, purpose) \
         VALUES ('mi-1', 0, 'm1', 1, 'user', 0, 5, 'sha256:test', 'assertion')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversation_memory_events (id, conversation_id, operation_id, \
         memory_revision, operation) VALUES ('me-1', ?, 'op-1', 1, 'compact')",
    )
    .bind(&conversation_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO journals (id, name) VALUES ('journal-1', 'Journal')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO journal_conversation_entries (journal_space_id, conversation_id) \
         VALUES ('journal-1', ?)",
    )
    .bind(&conversation_id)
    .execute(&pool)
    .await
    .unwrap();

    repo.delete(&conversation_id).await.unwrap();

    for table in [
        "conversation_messages",
        "conversation_documents",
        "conversation_web_sources",
        "conversation_message_bookmarks",
        "conversation_summaries",
        "conversation_memory_vectors",
        "conversation_memory_state",
        "conversation_memory_items",
        "conversation_memory_events",
        "journal_conversation_entries",
    ] {
        let remaining: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE conversation_id = ?"
        ))
        .bind(&conversation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            remaining, 0,
            "{table} kept rows after the conversation died"
        );
    }
    // Evidence is keyed by item and message, not by conversation; the cascade
    // reaches it through both parents.
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM conversation_memory_evidence WHERE item_id = 'mi-1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "conversation_memory_evidence kept rows after the conversation died"
    );
    // Shared tables are not conversation data: the document and the journal
    // themselves must survive.
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents WHERE id = 'doc-1'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM journals WHERE id = 'journal-1'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn test_delete_conversation_preserves_space_memberships() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    seed_documents(&pool, &["doc-1"]).await;
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
    seed_documents(&pool, &["doc-shared"]).await;
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

/// An Explorer thread's folder is stored on the row and comes back on every
/// projection the frontend lists or opens it through; clearing it unbinds.
#[tokio::test]
async fn the_explorer_root_round_trips_through_the_conversation_projections() {
    use crate::features::conversation::space_dto::ListConversationsExplorerQueryDto;
    use crate::features::explorer::repository::{conversation_root, set_conversation_root};

    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let conversation = repo
        .create_conversation("Folder", "model", None)
        .await
        .unwrap();
    let id = conversation.id.to_string();

    set_conversation_root(&pool, &id, Some("/Users/me/Code/project"))
        .await
        .unwrap();
    assert_eq!(
        conversation_root(&pool, &id).await.unwrap().as_deref(),
        Some("/Users/me/Code/project")
    );
    let projected = repo.project_conversation(&conversation).await.unwrap();
    assert_eq!(
        projected.explorer_root.as_deref(),
        Some("/Users/me/Code/project")
    );
    let listed = repo
        .list_conversations_explorer(ListConversationsExplorerQueryDto {
            space_id: None,
            query: None,
            saved_only: None,
            bookmarked_only: None,
            pinned_only: None,
            has_message_bookmarks: None,
            include_archived: None,
            limit: None,
            offset: None,
        })
        .await
        .unwrap();
    assert_eq!(
        listed.conversations[0].explorer_root.as_deref(),
        Some("/Users/me/Code/project")
    );

    set_conversation_root(&pool, &id, None).await.unwrap();
    assert_eq!(conversation_root(&pool, &id).await.unwrap(), None);
    assert!(set_conversation_root(&pool, "missing", Some("/x"))
        .await
        .is_err());
}
