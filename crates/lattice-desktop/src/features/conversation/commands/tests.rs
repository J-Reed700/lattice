use super::list_conversations_impl;
use crate::features::conversation::chat::ports::ChatStorage;
use crate::features::conversation::chat::test_runtime::FakeRuntime;

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
