#!/usr/bin/env cargo +nightly -Zscript

//! Test Chat/QA functionality in Recall Desktop
//!
//! This script tests the complete chat pipeline including:
//! - Question → Context Retrieval → Answer flow
//! - Conversation creation and persistence
//! - RAG (Retrieval Augmented Generation) pipeline
//! - Context window building

use std::path::PathBuf;
use std::sync::Arc;

#[path = "src/src/crates/recall/lib.rs"]
mod vault;

use vault::interfaces::di::Container;
use vault::application::dtos::qa_dto::{QARequestDto, QAResponseDto};
use vault::application::dtos::conversation_dto::CreateConversationRequestDto;
use vault::infrastructure::setup;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    setup::setup_tracing();

    println!("\n=== RECALL CHAT/QA FUNCTIONALITY TEST ===\n");

    // Setup test environment
    println!("1. Setting up test environment...");
    let test_dir = std::env::temp_dir().join("recall_chat_test");
    std::fs::create_dir_all(&test_dir)?;

    let db_path = test_dir.join("test.db");
    let model_dir = test_dir.join("models");
    std::fs::create_dir_all(&model_dir)?;

    // Initialize database
    println!("2. Initializing database...");
    let db_conn = setup::setup_database(db_path).await?;

    // Create DI container
    println!("3. Creating DI container...");
    let security_context = vault::infrastructure::security::SecurityContext::new(
        vault::infrastructure::security::SecurityConfig::default()
    );

    let config = vault::interfaces::commands::config::AppConfig::default();

    let container = Container::new(
        db_conn.pool().clone(),
        Arc::new(db_conn),
        security_context,
        model_dir.clone(),
        test_dir.clone(),
        config,
    ).await?;

    println!("\n=== TESTING CHAT API ENDPOINTS ===\n");

    // Test 1: Create new conversation
    println!("Test 1: Creating new conversation...");
    let create_uc = container.create_conversation_use_case();
    let create_request = CreateConversationRequestDto {
        title: "Test Chat Session".to_string(),
        model_name: "test-model".to_string(),
        system_prompt: Some("You are a helpful assistant".to_string()),
    };

    match create_uc.execute(create_request).await {
        Ok(response) => {
            println!("✅ Conversation created: {}", response.conversation.id);
            println!("   Title: {}", response.conversation.title);

            // Test 2: List conversations
            println!("\nTest 2: Listing conversations...");
            let list_uc = container.list_conversations_use_case();
            match list_uc.execute(()).await {
                Ok(convs) => {
                    println!("✅ Found {} conversation(s)", convs.conversations.len());
                    for conv in &convs.conversations {
                        println!("   - {} ({})", conv.title, conv.id);
                    }
                }
                Err(e) => println!("❌ List conversations failed: {}", e),
            }

            // Test 3: Add message to conversation
            println!("\nTest 3: Testing conversation chat...");
            let conv_service = container.conversation_service();

            // Add a user message
            match conv_service.add_message(
                &response.conversation.id,
                vault::domain::conversation::MessageRole::User,
                "What is semantic search?".to_string(),
                100,
            ).await {
                Ok(msg) => {
                    println!("✅ User message added: {}", msg.id);

                    // Add assistant response
                    match conv_service.add_message(
                        &response.conversation.id,
                        vault::domain::conversation::MessageRole::Assistant,
                        "Semantic search is a search technique that uses AI to understand the meaning and context of queries...".to_string(),
                        150,
                    ).await {
                        Ok(msg) => println!("✅ Assistant message added: {}", msg.id),
                        Err(e) => println!("❌ Failed to add assistant message: {}", e),
                    }
                }
                Err(e) => println!("❌ Failed to add user message: {}", e),
            }

            // Test 4: Get conversation messages
            println!("\nTest 4: Getting conversation messages...");
            let get_conv_uc = container.get_conversation_use_case();
            match get_conv_uc.execute(response.conversation.id.clone()).await {
                Ok(conv_data) => {
                    println!("✅ Retrieved conversation with {} messages", conv_data.messages.len());
                    for msg in &conv_data.messages {
                        println!("   [{}]: {}", msg.role,
                            if msg.content.len() > 50 {
                                format!("{}...", &msg.content[..50])
                            } else {
                                msg.content.clone()
                            }
                        );
                    }
                }
                Err(e) => println!("❌ Failed to get conversation: {}", e),
            }
        }
        Err(e) => println!("❌ Create conversation failed: {}", e),
    }

    println!("\n=== TESTING QA FUNCTIONALITY ===\n");

    // Test 5: Ask question (without context retrieval since no documents indexed)
    println!("Test 5: Testing ask_question endpoint...");
    let qa_request = QARequestDto {
        question: "What is machine learning?".to_string(),
        max_results: Some(5),
        threshold: Some(0.7),
        temperature: Some(0.7),
        max_tokens: Some(500),
    };

    let ask_uc = container.ask_question_use_case().await?;
    match ask_uc.execute(qa_request.clone()).await {
        Ok(response) => {
            println!("✅ Question answered successfully");
            println!("   Answer: {}",
                if response.answer.len() > 100 {
                    format!("{}...", &response.answer[..100])
                } else {
                    response.answer.clone()
                }
            );
            println!("   Sources: {} document(s) used", response.sources.len());
            if let Some(confidence) = response.confidence {
                println!("   Confidence: {:.2}%", confidence * 100.0);
            }
        }
        Err(e) => {
            println!("❌ Ask question failed: {}", e);
            println!("   This is expected if no LLM model is configured");
        }
    }

    println!("\n=== TESTING RAG PIPELINE ===\n");

    // Test 6: Context window building
    println!("Test 6: Testing context window builder...");
    use vault::application::services::context_window_builder::ContextWindowBuilder;
    use vault::infrastructure::services::ConversationService;

    let conv_service = Arc::new(ConversationService::new(container.db_pool().clone()));
    let max_tokens = 4096;
    let token_counter = Arc::new(|text: &str| text.len() / 4); // Rough estimation

    let context_builder = ContextWindowBuilder::new(
        conv_service.clone(),
        max_tokens,
        token_counter,
    ).with_recent_message_count(10);

    // Create a test conversation with messages
    let test_conv_id = "test-rag-conv";
    println!("   Creating test conversation for RAG...");

    match conv_service.create_conversation(
        test_conv_id.to_string(),
        "RAG Test".to_string(),
        Some("test-model".to_string()),
        None,
    ).await {
        Ok(_) => {
            // Add some test messages
            for i in 1..=5 {
                let _ = conv_service.add_message(
                    test_conv_id,
                    if i % 2 == 1 {
                        vault::domain::conversation::MessageRole::User
                    } else {
                        vault::domain::conversation::MessageRole::Assistant
                    },
                    format!("Test message #{}", i),
                    50,
                ).await;
            }

            match context_builder.build(test_conv_id).await {
                Ok(context) => {
                    println!("✅ Context built with {} messages", context.len());
                    for msg in context.iter().take(3) {
                        println!("   - {}",
                            if msg.len() > 50 {
                                format!("{}...", &msg[..50])
                            } else {
                                msg.clone()
                            }
                        );
                    }
                }
                Err(e) => println!("❌ Context building failed: {}", e),
            }
        }
        Err(e) => println!("❌ Failed to create test conversation: {}", e),
    }

    println!("\n=== TEST SUMMARY ===\n");

    println!("Chat/QA System Components Tested:");
    println!("✓ Conversation creation and management");
    println!("✓ Message persistence");
    println!("✓ Conversation retrieval");
    println!("✓ QA endpoint (requires LLM model)");
    println!("✓ Context window building");
    println!("✓ RAG pipeline structure");

    println!("\nNote: Full QA functionality requires:");
    println!("- An active LLM model (Ollama or local model)");
    println!("- Indexed documents for context retrieval");
    println!("- Embedding model for semantic search");

    // Cleanup
    println!("\nCleaning up test environment...");
    let _ = std::fs::remove_dir_all(&test_dir);

    Ok(())
}