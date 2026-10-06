use super::*;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::space_dto::ListConversationsExplorerQueryDto;
use crate::features::conversation::tangent_dto::CreateTangentRequestDto;

fn request(conversation_id: &str, message_id: &str) -> CreateTangentRequestDto {
    CreateTangentRequestDto {
        conversation_id: conversation_id.into(),
        message_id: message_id.into(),
        selected_text: "first answer".into(),
    }
}

#[tokio::test]
async fn tangent_captures_context_without_changing_parent_or_entering_lists() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let before = repo.find_by_id(&parent).await.unwrap().unwrap();
    let tangent = repo.create_tangent(request(&parent, "m2")).await.unwrap();
    assert_eq!(tangent.parent_conversation_id, parent);
    assert_eq!(tangent.context_message_count, 2);
    let messages = repo.get_messages(&tangent.conversation_id).await.unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "first question");
    assert!(messages[1].content.contains("selected this passage"));
    assert!(messages[1].content.ends_with("> first answer"));
    assert_eq!(
        message_ids(&pool, &parent).await,
        vec!["m1", "m2", "m3", "m4"]
    );
    let after = repo.find_by_id(&parent).await.unwrap().unwrap();
    assert_eq!(before.updated_at, after.updated_at);
    assert_eq!((after.message_count, after.total_tokens), (4, 15));
    assert_eq!(
        repo.get_messages(&parent).await.unwrap()[1].content,
        "first answer"
    );
    assert_eq!(repo.find_all(None, None).await.unwrap().len(), 1);
    assert_eq!(repo.count().await.unwrap(), 1);
    let listed = repo
        .list_conversations_explorer(
            serde_json::from_value::<ListConversationsExplorerQueryDto>(serde_json::json!({}))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.conversations.len(), 1);
    let reopened = ConversationRepository::new(pool);
    assert_eq!(
        reopened.list_tangents(&parent).await.unwrap()[0].selected_text,
        "first answer"
    );
}

#[tokio::test]
async fn tangent_promotion_keeps_transcript_and_survives_parent_deletion() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let promoted = repo.create_tangent(request(&parent, "m2")).await.unwrap();
    let child = repo.create_tangent(request(&parent, "m4")).await.unwrap();
    seed_message(
        &pool,
        &promoted.conversation_id,
        "tangent-question",
        "user",
        "Why?",
        3,
        "2026-10-06T01:00:00Z",
    )
    .await;
    let ids = message_ids(&pool, &promoted.conversation_id).await;
    repo.promote_tangent(&promoted.conversation_id)
        .await
        .unwrap();
    repo.promote_tangent(&promoted.conversation_id)
        .await
        .unwrap();
    assert_eq!(repo.find_all(None, None).await.unwrap().len(), 2);
    assert_eq!(repo.list_tangents(&parent).await.unwrap().len(), 1);
    assert_eq!(message_ids(&pool, &promoted.conversation_id).await, ids);
    repo.delete(&parent).await.unwrap();
    assert!(repo
        .find_by_id(&child.conversation_id)
        .await
        .unwrap()
        .is_none());
    assert!(repo
        .get_messages(&child.conversation_id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(message_ids(&pool, &promoted.conversation_id).await, ids);
}

#[tokio::test]
async fn tangent_anchor_deletion_retains_quote_and_nested_tangents_share_parent() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let tangent = repo.create_tangent(request(&parent, "m2")).await.unwrap();
    seed_message(
        &pool,
        &tangent.conversation_id,
        "followup",
        "assistant",
        "A tangent answer",
        4,
        "2026-10-06T01:00:00Z",
    )
    .await;
    let nested = repo
        .create_tangent(request(&tangent.conversation_id, "followup"))
        .await
        .unwrap();
    assert_eq!(nested.parent_conversation_id, parent);
    assert_eq!(nested.source_conversation_id, tangent.conversation_id);
    repo.truncate_after(&parent, "m1", false).await.unwrap();
    assert_eq!(repo.list_tangents(&parent).await.unwrap().len(), 2);
    assert_eq!(
        repo.list_tangents(&parent)
            .await
            .unwrap()
            .iter()
            .find(|t| t.conversation_id == tangent.conversation_id)
            .unwrap()
            .selected_text,
        "first answer"
    );
}

#[tokio::test]
async fn invalid_tangent_requests_cannot_create_orphan_conversations() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let other = repo
        .create_conversation("Other", "model", None)
        .await
        .unwrap();
    let cases = [
        request(&parent, "unknown"),
        request(&parent, "m1"),
        request(&other.id.to_string(), "m2"),
        CreateTangentRequestDto {
            selected_text: "  ".into(),
            ..request(&parent, "m2")
        },
        CreateTangentRequestDto {
            selected_text: "🪴".repeat(8001),
            ..request(&parent, "m2")
        },
    ];
    for case in cases {
        assert!(repo.create_tangent(case).await.is_err());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    sqlx::query("CREATE TRIGGER reject_tangent_copy BEFORE INSERT ON conversation_messages BEGIN SELECT RAISE(ABORT, 'copy failed'); END").execute(&pool).await.unwrap();
    assert!(repo.create_tangent(request(&parent, "m2")).await.is_err());
    assert!(repo.list_tangents(&parent).await.unwrap().is_empty());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn explorer_tangent_preserves_folder_prompt_and_sources() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    sqlx::query("UPDATE conversations SET explorer_root = '/project', system_prompt = 'Keep answers concise' WHERE id = ?").bind(&parent).execute(&pool).await.unwrap();
    seed_documents(&pool, &["source"]).await;
    let repo = ConversationRepository::new(pool.clone());
    repo.add_document_reference(&parent, "source", None, Some(0.9))
        .await
        .unwrap();
    let tangent = repo.create_tangent(request(&parent, "m2")).await.unwrap();
    let detail = repo
        .find_by_id(&tangent.conversation_id)
        .await
        .unwrap()
        .unwrap();
    let projected = repo.project_conversation(&detail).await.unwrap();
    assert_eq!(projected.explorer_root.as_deref(), Some("/project"));
    assert_eq!(
        projected.tangent_parent_id.as_deref(),
        Some(parent.as_str())
    );
    assert_eq!(
        projected.system_prompt.as_deref(),
        Some("Keep answers concise")
    );
    assert_eq!(
        repo.get_document_references(&tangent.conversation_id)
            .await
            .unwrap()
            .len(),
        1
    );
    let counts = crate::features::explorer::repository::thread_counts(&pool)
        .await
        .unwrap();
    assert_eq!(counts.get("/project"), Some(&1));
    assert_eq!(
        crate::features::explorer::repository::folder_threads(&pool, "/project")
            .await
            .unwrap(),
        vec![parent]
    );
    repo.promote_tangent(&tangent.conversation_id)
        .await
        .unwrap();
    let counts = crate::features::explorer::repository::thread_counts(&pool)
        .await
        .unwrap();
    assert_eq!(counts.get("/project"), Some(&2));
}
