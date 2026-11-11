use super::list_conversations_impl;
use crate::features::conversation::chat::ports::ChatStorage;
use crate::features::conversation::chat::test_runtime::FakeRuntime;
use lattice_core::shared::error::AppError;

async fn runtime_with(count: usize) -> FakeRuntime {
    let runtime = FakeRuntime::default();
    let service = runtime.conversation_service();
    for index in 0..count {
        service
            .create_conversation(format!("Chat {index}"), None)
            .await
            .unwrap();
    }
    runtime
}

#[tokio::test]
async fn listing_conversations_returns_as_many_as_the_caller_asks_for() {
    let runtime = runtime_with(150).await;

    let all = list_conversations_impl(&runtime, Some(150), None)
        .await
        .unwrap();
    assert_eq!(all.len(), 150);
    let unlimited = list_conversations_impl(&runtime, None, None).await.unwrap();
    assert_eq!(unlimited.len(), 150);
    let two = list_conversations_impl(&runtime, Some(2), Some(1))
        .await
        .unwrap();
    assert_eq!(two.len(), 2);
}

#[tokio::test]
async fn listing_conversations_rejects_a_limit_below_one_and_a_negative_offset() {
    let runtime = runtime_with(3).await;

    assert!(matches!(
        list_conversations_impl(&runtime, Some(0), None).await,
        Err(AppError::InvalidInput(_))
    ));
    assert!(matches!(
        list_conversations_impl(&runtime, None, Some(-1)).await,
        Err(AppError::InvalidInput(_))
    ));
}
