//! Appending and reading messages.

use super::*;
use crate::domain::conversation::MessageRole;
use crate::features::conversation::repository::ConversationRepository;

#[tokio::test]
async fn test_add_message() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = ConversationRepository::new(pool);

    let conversation = repo
        .create_conversation("Test", "model", None)
        .await
        .unwrap();

    let message = repo
        .add_message(
            &conversation.id.to_string(),
            MessageRole::User,
            "Hello!",
            10,
            None,
        )
        .await
        .unwrap();

    assert_eq!(message.content, "Hello!");
    assert_eq!(message.tokens, 10);

    let updated = repo
        .find_by_id(&conversation.id.to_string())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.message_count, 1);
    assert_eq!(updated.total_tokens, 10);
}

#[tokio::test]
async fn test_get_messages() {
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
        "First",
        5,
        None,
    )
    .await
    .unwrap();

    repo.add_message(
        &conversation.id.to_string(),
        MessageRole::Assistant,
        "Second",
        10,
        None,
    )
    .await
    .unwrap();

    let messages = repo
        .get_messages(&conversation.id.to_string())
        .await
        .unwrap();

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "First");
    assert_eq!(messages[1].content, "Second");
}
