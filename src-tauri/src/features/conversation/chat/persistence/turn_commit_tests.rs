use super::*;
use crate::features::conversation::{
    repository::ConversationRepository, service::ConversationService, ConversationServiceTrait,
};
use lattice_core::domain::conversation::MessageRole;

async fn service_with_pending_question() -> (
    Arc<dyn ConversationServiceTrait>,
    sqlx::SqlitePool,
    String,
    String,
) {
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let service: Arc<dyn ConversationServiceTrait> = Arc::new(ConversationService::new(Arc::new(
        ConversationRepository::new(pool.clone()),
    )));
    let conversation = service
        .create_conversation("Chat".into(), None)
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
    (service, pool, id, user.id)
}

async fn status_of(pool: &sqlx::SqlitePool, id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM conversation_messages WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn failed_question_retains_request_identity_and_attachments() {
    let (service, pool, id, _) = service_with_pending_question().await;
    let llm: Arc<dyn lattice_core::application::ports::LLMPort> =
        Arc::new(crate::features::llm::engine::factory::MockLLMPort::new());
    let (user_id, _) = persist_user_message_pending(
        &service,
        &id,
        "request-one",
        "same question",
        &["notes.pdf".into()],
        &["document-one".into()],
        &llm,
    )
    .await
    .unwrap();
    mark_user_message_failed(&service, &user_id).await;
    let raw: String = sqlx::query_scalar("SELECT metadata FROM conversation_messages WHERE id=?")
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(metadata["requestId"], "request-one");
    assert_eq!(metadata["attachments"], serde_json::json!(["notes.pdf"]));
    assert_eq!(
        metadata["attachmentDocumentIds"],
        serde_json::json!(["document-one"])
    );
    assert_eq!(status_of(&pool, &user_id).await, "failed");
}

#[tokio::test]
async fn a_commit_that_fails_leaves_the_question_failed_not_pending() {
    let (service, pool, id, user_id) = service_with_pending_question().await;
    sqlx::query("CREATE TRIGGER fail_assistant BEFORE INSERT ON conversation_messages WHEN NEW.role='assistant' BEGIN SELECT RAISE(ABORT, 'injected failure'); END")
        .execute(&pool).await.unwrap();

    let result = commit_turn(&service, &id, &user_id, "answer".into(), 3, None).await;

    assert!(result.is_err());
    assert_eq!(status_of(&pool, &user_id).await, "failed");
}

#[tokio::test]
async fn a_committed_turn_is_not_undone_by_a_failed_re_read() {
    let (service, pool, id, user_id) = service_with_pending_question().await;
    let answer = commit_turn(&service, &id, &user_id, "answer".into(), 3, None)
        .await
        .unwrap();

    let messages = turn_transcript(
        Err(AppError::Database("database is locked".into())),
        &user_id,
        "question",
        2,
        &answer,
    );

    assert_eq!(status_of(&pool, &user_id).await, "completed");
    let roles: Vec<_> = messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, ["user", "assistant"]);
    assert_eq!(messages[1].id, answer.id);
    assert_eq!(messages[1].content, "answer");
    assert_eq!(messages[0].status, "completed");
}
