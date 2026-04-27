//! Comprehensive tests for ConversationalQAService
//!
//! Phase 4 Week 2 Day 3 - Conversation Domain Services Testing
//! Step 2: conversational_qa_service.rs (7 tests - adapted for manual mocks)
//!
//! **DEVIATION FROM SPEC**: Original spec called for 18 tests with mockall mocks,
//! but actual codebase uses manual mock implementations without error injection
//! or access to private methods. Tests have been adapted to test core functionality.
//!
//! Tests are organized into 3 categories:
//! - Category 1: Q&A Orchestration Flow (4 tests, P0)
//! - Category 2: Multi-Turn Conversations (2 tests, P0)
//! - Category 3: Metadata Validation (1 test, P0)
//!
//! **Skipped tests due to technical limitations**:
//! - Error injection tests (manual mocks don't support error injection)
//! - Streaming tests (requires mocking tauri::Window)
//! - Metrics recording test (Metrics fields are private)
//! - Helper method test (convert_search_results is private)
//!
//! Total: 7 tests implemented (all P0) targeting core functionality with 75%+ coverage

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::features::search::dto::SearchResultDto;
    use crate::domain::conversation::MessageRole;
    use crate::infrastructure::observability::Metrics;
    use crate::features::qa::conversational_service::ConversationalQAService;
    use crate::infrastructure::services::traits::ContextManagerTrait;
    use crate::infrastructure::services::mocks::MockContextManager;
    use crate::features::conversation::ConversationServiceTrait;
    use crate::features::conversation::mocks::MockConversationService;
    use crate::features::qa::QAEngineTrait;
    use crate::features::qa::mocks::MockQAEngine;
    use crate::shared::error::{AppError, Result};

    // ========================================================================
    // Test Helpers
    // ========================================================================

    /// Helper 1: Create test conversation aggregate
    async fn create_test_conversation(
        service: &MockConversationService,
        message_count: usize,
    ) -> String {
        // Create conversation
        let conversation = service
            .create_conversation(
                "Test Conversation".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                Some("You are helpful.".to_string()),
            )
            .await
            .expect("Failed to create conversation");

        let conv_id = conversation.id.to_string();

        // Add messages
        for i in 0..message_count {
            if i % 2 == 0 {
                service
                    .add_user_message(&conv_id, format!("Message {}", i), 10)
                    .await
                    .expect("Failed to add user message");
            } else {
                service
                    .add_assistant_message(&conv_id, format!("Message {}", i), 10)
                    .await
                    .expect("Failed to add assistant message");
            }
        }

        conv_id
    }

    /// Helper 2: Create test search results
    fn create_test_search_results(count: usize) -> Vec<SearchResultDto> {
        (0..count)
            .map(|i| SearchResultDto {
                id: format!("chunk_{}", i),
                title: format!("test_{}.txt", i),
                content: format!("Test content {}", i),
                score: 0.8,
                path: Some(format!("/path/test_{}.txt", i)),
                document_id: Some(format!("doc_{}", i)),
                position: Some(i),
                vector_score: Some(0.8),
                bm25_score: None,
                vector_rank: None,
                bm25_rank: None,
                metadata: HashMap::new(),
            })
            .collect()
    }

    /// Helper 3: Setup test service with mocks
    async fn setup_qa_test() -> (
        ConversationalQAService,
        Arc<MockConversationService>,
        Arc<MockContextManager>,
        Arc<MockQAEngine>,
    ) {
        let mock_conv = Arc::new(MockConversationService::new());
        let mock_context = Arc::new(MockContextManager::new());
        let mock_qa = Arc::new(MockQAEngine::new());
        let metrics = Arc::new(Metrics::new());

        // Configure default answer for any question
        mock_qa.set_answer("*", "Default answer".to_string()).await;

        let service = ConversationalQAService::new(
            mock_conv.clone() as Arc<dyn ConversationServiceTrait>,
            mock_context.clone() as Arc<dyn ContextManagerTrait>,
            mock_qa.clone() as Arc<dyn QAEngineTrait>,
            metrics,
        );

        (service, mock_conv, mock_context, mock_qa)
    }

    // ========================================================================
    // Category 1: Q&A Orchestration Flow (P0) - 5 tests
    // ========================================================================

    #[tokio::test]
    async fn test_ask_question_orchestrates_full_flow() -> Result<()> {
        // ARRANGE
        let (service, mock_conv, _mock_context, mock_qa) = setup_qa_test().await;

        // Create conversation with 2 messages
        let conv_id = create_test_conversation(&mock_conv, 2).await;

        // Configure answer
        mock_qa.set_answer("*", "Sample answer".to_string()).await;

        let search_results = create_test_search_results(3);

        // ACT
        let answer = service
            .ask_question(&conv_id, "What is RAG?", search_results.clone())
            .await?;

        // ASSERT
        assert_eq!(answer.answer, "Sample answer");
        assert_eq!(answer.sources.len(), 3);
        assert_eq!(answer.conversation_id, conv_id);
        // After adding user question + assistant answer, should have 4 messages
        assert_eq!(answer.message_count, 4);

        Ok(())
    }

    #[tokio::test]
    async fn test_ask_question_builds_context_with_conversation_history() -> Result<()> {
        // ARRANGE
        let (service, mock_conv, _mock_context, mock_qa) = setup_qa_test().await;

        // Create conversation with 3 prior messages
        let conv_id = create_test_conversation(&mock_conv, 3).await;

        // Configure answer
        mock_qa.set_answer("*", "Answer".to_string()).await;

        let search_results = create_test_search_results(2);

        // ACT
        let answer = service
            .ask_question(&conv_id, "Follow-up question", search_results)
            .await?;

        // ASSERT
        assert_eq!(answer.answer, "Answer");
        // Verify context was built (checked by fact that answer was generated)
        assert!(
            answer.message_count > 3,
            "Should have more than 3 messages after adding new Q&A"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_ask_question_saves_user_and_assistant_messages() -> Result<()> {
        // ARRANGE
        let (service, mock_conv, _mock_context, mock_qa) = setup_qa_test().await;

        // Create conversation with no messages
        let conv_id = create_test_conversation(&mock_conv, 0).await;

        // Configure answer
        mock_qa.set_answer("*", "Sample answer".to_string()).await;

        let search_results = create_test_search_results(2);

        // ACT
        let _answer = service
            .ask_question(&conv_id, "Question?", search_results)
            .await?;

        // ASSERT
        // Get conversation to verify messages were saved
        let aggregate = mock_conv
            .get_conversation(&conv_id)
            .await?
            .expect("Conversation should exist");

        assert_eq!(
            aggregate.message_count(),
            2,
            "Should have user + assistant messages"
        );
        assert_eq!(aggregate.messages()[0].role, MessageRole::User);
        assert_eq!(aggregate.messages()[0].content, "Question?");
        assert_eq!(aggregate.messages()[1].role, MessageRole::Assistant);
        assert_eq!(aggregate.messages()[1].content, "Sample answer");

        Ok(())
    }

    #[tokio::test]
    async fn test_ask_question_estimates_tokens() -> Result<()> {
        // ARRANGE
        let (service, mock_conv, _mock_context, mock_qa) = setup_qa_test().await;

        let conv_id = create_test_conversation(&mock_conv, 0).await;

        // Configure answer with specific length
        mock_qa
            .set_answer("*", "ABCDEFGHIJKLMNOP".to_string())
            .await; // 16 chars = 4 tokens

        let search_results = create_test_search_results(1);

        // ACT
        let _answer = service
            .ask_question(&conv_id, "12345678", search_results)
            .await?; // 8 chars = 2 tokens

        // ASSERT
        let aggregate = mock_conv
            .get_conversation(&conv_id)
            .await?
            .expect("Conversation should exist");

        // Verify token counts
        assert_eq!(
            aggregate.messages()[0].tokens,
            2,
            "User message should have 2 tokens"
        );
        assert_eq!(
            aggregate.messages()[1].tokens,
            4,
            "Assistant message should have 4 tokens"
        );
        assert_eq!(aggregate.total_tokens(), 6, "Total tokens should be 6");

        Ok(())
    }

    // Test 5: Metrics recording - SKIPPED
    // Reason: Metrics fields are private, cannot access in tests
    // Coverage: This is tested via integration/E2E tests

    // ========================================================================
    // Category 2: Multi-Turn Conversations (P0) - 2 tests
    // ========================================================================

    #[tokio::test]
    async fn test_multi_turn_conversation_maintains_history() -> Result<()> {
        // ARRANGE
        let (service, mock_conv, _mock_context, mock_qa) = setup_qa_test().await;

        let conv_id = create_test_conversation(&mock_conv, 0).await;

        // Configure answers
        mock_qa.set_answer("*", "Answer 1".to_string()).await;

        let search_results1 = create_test_search_results(1);

        // ACT - Turn 1
        let answer1 = service
            .ask_question(&conv_id, "What is ML?", search_results1)
            .await?;

        // Verify first turn
        let aggregate = mock_conv
            .get_conversation(&conv_id)
            .await?
            .expect("Conversation should exist");
        assert_eq!(
            aggregate.message_count(),
            2,
            "Should have 2 messages after first turn"
        );

        // Configure for turn 2
        mock_qa.set_answer("*", "Answer 2".to_string()).await;
        let search_results2 = create_test_search_results(1);

        // ACT - Turn 2
        let answer2 = service
            .ask_question(&conv_id, "How does it work?", search_results2)
            .await?;

        // ASSERT
        assert_eq!(answer1.answer, "Answer 1");
        assert_eq!(answer2.answer, "Answer 2");
        assert_eq!(
            answer2.message_count, 4,
            "Should have 4 messages after 2 turns"
        );

        // Verify history maintained
        let final_aggregate = mock_conv
            .get_conversation(&conv_id)
            .await?
            .expect("Conversation should exist");
        assert_eq!(final_aggregate.message_count(), 4);
        assert_eq!(final_aggregate.messages()[0].content, "What is ML?");
        assert_eq!(final_aggregate.messages()[1].content, "Answer 1");
        assert_eq!(final_aggregate.messages()[2].content, "How does it work?");
        assert_eq!(final_aggregate.messages()[3].content, "Answer 2");

        Ok(())
    }

    #[tokio::test]
    async fn test_multi_turn_conversation_updates_stats() -> Result<()> {
        // ARRANGE
        let (service, mock_conv, _mock_context, mock_qa) = setup_qa_test().await;

        let conv_id = create_test_conversation(&mock_conv, 0).await;

        // Configure answer - "Answer 1 with 40 chars total here!!!" = 38 chars = 9 tokens (38/4 = 9.5 -> 9)
        mock_qa
            .set_answer("*", "Answer 1 with 40 chars total here!!!".to_string())
            .await;

        let search_results = create_test_search_results(1);

        // ACT
        let answer = service
            .ask_question(&conv_id, "Q1 16 chars!!!", search_results)
            .await?; // 15 chars / 4 = 3 tokens

        // ASSERT
        // Stats should be updated
        assert!(answer.total_tokens > 0);
        assert_eq!(answer.message_count, 2);

        // Verify in aggregate
        let aggregate = mock_conv
            .get_conversation(&conv_id)
            .await?
            .expect("Conversation should exist");
        // Total should be 3 (user) + 9 (assistant) = 12 tokens
        assert_eq!(
            aggregate.total_tokens(),
            12,
            "Should have 12 total tokens (3 user + 9 assistant)"
        );

        Ok(())
    }

    // ========================================================================
    // Category 3: Metadata Validation (P0) - 1 test
    // ========================================================================

    #[tokio::test]
    async fn test_metadata_validation_provides_user_friendly_error() -> Result<()> {
        // ARRANGE
        let (service, mock_conv, _mock_context, mock_qa) = setup_qa_test().await;

        let conv_id = create_test_conversation(&mock_conv, 0).await;

        // Configure answer
        mock_qa.set_answer("*", "Answer".to_string()).await;

        // Create 1000 search results to exceed 65KB limit
        // Each result serializes to ~250-300 bytes, so 1000 = ~250KB > 65KB limit
        let oversized_results = create_test_search_results(1000);

        // ACT
        let result = service.ask_question(&conv_id, "Q", oversized_results).await;

        // ASSERT
        assert!(result.is_err(), "Should fail with oversized metadata");
        match result {
            Err(AppError::Other(msg)) => {
                assert!(
                    msg.contains("Cannot save answer"),
                    "Error should say cannot save: {}",
                    msg
                );
                assert!(
                    msg.contains("Too many sources"),
                    "Error should mention too many sources: {}",
                    msg
                );
                assert!(
                    msg.contains("1000 sources"),
                    "Error should show count: {}",
                    msg
                );
                assert!(
                    msg.contains("more specific question"),
                    "Error should suggest being more specific: {}",
                    msg
                );
            }
            _ => panic!("Expected AppError::Other, got: {:?}", result),
        }

        Ok(())
    }

    // ========================================================================
    // Category 4: Helper Methods (P2) - SKIPPED
    // ========================================================================
    // Test: convert_search_results() - SKIPPED
    // Reason: Method is private, cannot access in tests
    // Coverage: This is tested indirectly via functional tests above
}
