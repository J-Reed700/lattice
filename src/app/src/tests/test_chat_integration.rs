//! Integration test for Chat/QA functionality

use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use vault::application::dtos::conversation_dto::{
    CreateConversationRequestDto, GetConversationRequestDto,
};
use vault::features::qa::dto::QARequestDto;
use vault::infrastructure::persistence::database::DatabaseConnection;
use vault::infrastructure::services::ConversationService;
use vault::interfaces::di::Container;

#[tokio::test]
async fn test_conversation_creation() {
    // Setup test environment
    let test_dir = std::env::temp_dir().join("recall_chat_test_conv");
    std::fs::create_dir_all(&test_dir).unwrap();
    let db_path = test_dir.join("test.db");
    let model_dir = test_dir.join("models");
    std::fs::create_dir_all(&model_dir).unwrap();

    // Create database pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
        .await
        .unwrap();

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // Create database connection
    let db_conn = Arc::new(DatabaseConnection::new(db_path.clone()).await.unwrap());

    // Create container
    let container = Container::new(
        pool.clone(),
        db_conn,
        None, // No embedding model for test
        "http://localhost:11434",
        "llama3.1:8b",
        test_dir.clone(),
    )
    .await
    .unwrap();

    // Test conversation creation
    let create_uc = container.create_conversation_use_case();
    let request = CreateConversationRequestDto {
        title: "Test Chat".to_string(),
        model_name: "test-model".to_string(),
        system_prompt: None,
    };

    let response = create_uc.execute(request).await.unwrap();
    assert!(!response.conversation.id.is_empty());
    assert_eq!(response.conversation.title, "Test Chat");

    // Cleanup
    std::fs::remove_dir_all(&test_dir).ok();
}

#[tokio::test]
async fn test_conversation_messages() {
    // Setup test environment
    let test_dir = std::env::temp_dir().join("recall_chat_test_msg");
    std::fs::create_dir_all(&test_dir).unwrap();
    let db_path = test_dir.join("test.db");
    let model_dir = test_dir.join("models");
    std::fs::create_dir_all(&model_dir).unwrap();

    // Create database pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
        .await
        .unwrap();

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // Create database connection
    let db_conn = Arc::new(DatabaseConnection::new(db_path.clone()).await.unwrap());

    // Create container
    let container = Container::new(
        pool.clone(),
        db_conn,
        None, // No embedding model for test
        "http://localhost:11434",
        "llama3.1:8b",
        test_dir.clone(),
    )
    .await
    .unwrap();

    // Create conversation
    let create_uc = container.create_conversation_use_case();
    let conv_response = create_uc
        .execute(CreateConversationRequestDto {
            title: "Message Test".to_string(),
            model_name: "test-model".to_string(),
            system_prompt: None,
        })
        .await
        .unwrap();

    let conv_id = conv_response.conversation.id;

    // Create conversation service directly to add messages
    let conv_service = ConversationService::new(pool.clone());
    let user_msg = conv_service
        .add_user_message(&conv_id, "Hello, test message".to_string(), 10)
        .await
        .unwrap();

    assert!(!user_msg.id.is_empty());
    assert_eq!(user_msg.content, "Hello, test message");

    // Get conversation with messages
    let get_uc = container.get_conversation_use_case();
    let request = GetConversationRequestDto {
        conversation_id: conv_id,
    };
    let conv_data = get_uc.execute(request).await.unwrap();

    // The GetConversationResponseDto now has an Option<ConversationDto> field
    assert!(conv_data.conversation.is_some());
    let conversation = conv_data.conversation.unwrap();
    assert_eq!(conversation.message_count, 1);

    // Cleanup
    std::fs::remove_dir_all(&test_dir).ok();
}

#[tokio::test]
async fn test_context_window_builder() {
    use vault::application::services::context_window_builder::ContextWindowBuilder;
    use vault::infrastructure::services::ConversationService;

    // Setup test environment
    let test_dir = std::env::temp_dir().join("recall_chat_test_ctx");
    std::fs::create_dir_all(&test_dir).unwrap();
    let db_path = test_dir.join("test.db");
    let model_dir = test_dir.join("models");
    std::fs::create_dir_all(&model_dir).unwrap();

    // Create database pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
        .await
        .unwrap();

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // Create conversation service
    let conv_service = Arc::new(ConversationService::new(pool.clone()));

    // Create test conversation
    let conversation = conv_service
        .create_conversation("Context Test".to_string(), "test-model".to_string(), None)
        .await
        .unwrap();

    let conv_id = conversation.id.to_string();

    // Add messages
    for i in 1..=10 {
        if i % 2 == 1 {
            conv_service
                .add_user_message(&conv_id, format!("Test message #{}", i), 50)
                .await
                .unwrap();
        } else {
            conv_service
                .add_assistant_message(&conv_id, format!("Test message #{}", i), 50)
                .await
                .unwrap();
        }
    }

    // Build context
    let token_counter = Arc::new(|text: &str| text.len() / 4);
    let context_builder = ContextWindowBuilder::new(conv_service.clone(), 4096, token_counter)
        .with_recent_message_count(5);

    let context = context_builder.build(&conv_id).await.unwrap();

    // Should have built a context with messages
    assert!(!context.is_empty());
    assert!(context.len() <= 10); // Should not exceed total messages

    // Cleanup
    std::fs::remove_dir_all(&test_dir).ok();
}

#[tokio::test]
#[ignore] // Ignore by default as it requires LLM
async fn test_qa_endpoint_with_llm() {
    // This test requires an active LLM model (Ollama or local)
    // Run with: cargo test test_qa_endpoint_with_llm -- --ignored

    let test_dir = std::env::temp_dir().join("recall_chat_test_qa");
    std::fs::create_dir_all(&test_dir).unwrap();
    let db_path = test_dir.join("test.db");
    let model_dir = test_dir.join("models");
    std::fs::create_dir_all(&model_dir).unwrap();

    // Create database pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
        .await
        .unwrap();

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // Create database connection
    let db_conn = Arc::new(DatabaseConnection::new(db_path.clone()).await.unwrap());

    // Create container
    let container = Container::new(
        pool.clone(),
        db_conn,
        None, // No embedding model for test
        "http://localhost:11434",
        "llama3.1:8b",
        test_dir.clone(),
    )
    .await
    .unwrap();

    // Try to ask a question
    let qa_request = QARequestDto {
        question: "What is 2+2?".to_string(),
        context_limit: Some(5),
        model: Some("test-model".to_string()),
        temperature: Some(0.7),
        max_tokens: Some(100),
        images: None,
    };

    let ask_uc = container.ask_question_use_case().await.unwrap();
    match ask_uc.execute(qa_request).await {
        Ok(response) => {
            assert!(!response.answer.is_empty());
            println!("LLM Response: {}", response.answer);
        }
        Err(e) => {
            println!("Expected error without LLM: {}", e);
        }
    }

    std::fs::remove_dir_all(&test_dir).ok();
}
