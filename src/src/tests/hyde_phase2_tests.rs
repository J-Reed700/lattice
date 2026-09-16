#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Integration tests for HyDE Phase 2: Generator and Service
//!
//! Tests the complete HyDE pipeline:
//! 1. Query classification
//! 2. HyDE generation via LLM
//! 3. Service orchestration
use async_trait::async_trait;
use futures::stream::{self, Stream};
use lattice::application::ports::LLMPort;
use lattice::domain::qa::hyde::{HyDEInterpretation, QueryType, SearchStrategy};
use lattice::infrastructure::services::hyde::{HyDEGenerator, HyDEService, QueryClassifier};
use lattice::shared::error::Result;
use std::sync::Arc;

// ============================================================================
// Mock LLM for Testing
// ============================================================================

struct MockLLM {
    response: String,
}

impl MockLLM {
    fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
        }
    }
}

#[async_trait]
impl LLMPort for MockLLM {
    async fn generate(
        &self,
        _prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<String> {
        Ok(self.response.clone())
    }

    async fn generate_streaming(
        &self,
        _prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
        let stream = stream::once(async { Ok(self.response.clone()) });
        Ok(Box::new(Box::pin(stream)))
    }

    fn model_name(&self) -> &str {
        "mock-llm"
    }

    fn max_context_tokens(&self) -> usize {
        4096
    }

    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

// ============================================================================
// Phase 1 Tests: Query Classifier
// ============================================================================

#[test]
fn test_classifier_greeting_detection() {
    let classifier = QueryClassifier::new();

    assert_eq!(classifier.classify("hi"), QueryType::Greeting);
    assert_eq!(classifier.classify("hello"), QueryType::Greeting);
    assert_eq!(classifier.classify("good morning"), QueryType::Greeting);
    assert_eq!(classifier.classify("HeLLo"), QueryType::Greeting);
}

#[test]
fn test_classifier_question_detection() {
    let classifier = QueryClassifier::new();

    assert_eq!(classifier.classify("what is rust?"), QueryType::Question);
    assert_eq!(
        classifier.classify("how does it work?"),
        QueryType::Question
    );
    assert_eq!(classifier.classify("explain DDD"), QueryType::Question);
}

#[test]
fn test_classifier_command_detection() {
    let classifier = QueryClassifier::new();

    assert_eq!(classifier.classify("search files"), QueryType::Command);
    assert_eq!(classifier.classify("find documents"), QueryType::Command);
    assert_eq!(classifier.classify("list all tags"), QueryType::Command);
}

// ============================================================================
// Phase 2 Tests: HyDE Generator
// ============================================================================

#[tokio::test]
async fn test_generator_greeting_fast_path() {
    let mock_llm = Arc::new(MockLLM::new("This should not be called"));
    let generator = HyDEGenerator::new(mock_llm);

    let result = generator
        .generate(QueryType::Greeting, "Hello!")
        .await
        .unwrap();

    assert_eq!(result.query_type, QueryType::Greeting);
    assert_eq!(result.original_query, "Hello!");
    assert!(result.hyde_text.is_none());
    assert_eq!(result.search_strategy, SearchStrategy::RawOnly);
}

#[tokio::test]
async fn test_generator_question_generates_hyde() {
    let mock_llm = Arc::new(MockLLM::new(
        "Domain-Driven Design is a software development approach focusing on complex business domains.",
    ));
    let generator = HyDEGenerator::new(mock_llm);

    let result = generator
        .generate(QueryType::Question, "What is DDD?")
        .await
        .unwrap();

    assert_eq!(result.query_type, QueryType::Question);
    assert_eq!(result.original_query, "What is DDD?");
    assert!(result.hyde_text.is_some());
    assert!(result.hyde_text.unwrap().contains("Domain-Driven Design"));
    assert_eq!(result.search_strategy, SearchStrategy::HyDEOnly);
}

#[tokio::test]
async fn test_generator_command_hybrid_strategy() {
    let mock_llm = Arc::new(MockLLM::new(
        "Documents about search algorithms and indexing.",
    ));
    let generator = HyDEGenerator::new(mock_llm);

    let result = generator
        .generate(QueryType::Command, "search documents")
        .await
        .unwrap();

    assert_eq!(result.query_type, QueryType::Command);
    assert!(result.hyde_text.is_some());
    assert_eq!(result.search_strategy, SearchStrategy::Hybrid);
}

#[tokio::test]
async fn test_generator_empty_llm_response_fallback() {
    let mock_llm = Arc::new(MockLLM::new(""));
    let generator = HyDEGenerator::new(mock_llm);

    let result = generator
        .generate(QueryType::Question, "What is Rust?")
        .await
        .unwrap();

    // Should fall back to raw query
    assert_eq!(result.query_type, QueryType::Question);
    assert!(result.hyde_text.is_none());
    assert_eq!(result.search_strategy, SearchStrategy::RawOnly);
}

// ============================================================================
// Phase 2 Tests: HyDE Service (Full Integration)
// ============================================================================

#[tokio::test]
async fn test_service_end_to_end_greeting() {
    let mock_llm = Arc::new(MockLLM::new("Should not be called"));
    let service = HyDEService::new(mock_llm);

    let interpretation = service.interpret_query("Hello!").await.unwrap();

    assert_eq!(interpretation.query_type, QueryType::Greeting);
    assert_eq!(interpretation.original_query, "Hello!");
    assert!(interpretation.hyde_text.is_none());
    assert_eq!(interpretation.search_strategy, SearchStrategy::RawOnly);
}

#[tokio::test]
async fn test_service_end_to_end_question() {
    let mock_llm = Arc::new(MockLLM::new(
        "Rust is a systems programming language that runs blazingly fast.",
    ));
    let service = HyDEService::new(mock_llm);

    let interpretation = service.interpret_query("What is Rust?").await.unwrap();

    assert_eq!(interpretation.query_type, QueryType::Question);
    assert_eq!(interpretation.original_query, "What is Rust?");
    assert!(interpretation.hyde_text.is_some());

    let hyde = interpretation.hyde_text.unwrap();
    assert!(hyde.contains("Rust"));
    assert!(hyde.contains("programming language"));

    assert_eq!(interpretation.search_strategy, SearchStrategy::HyDEOnly);
}

#[tokio::test]
async fn test_service_end_to_end_command() {
    let mock_llm = Arc::new(MockLLM::new(
        "Documents containing search functionality and file operations.",
    ));
    let service = HyDEService::new(mock_llm);

    let interpretation = service
        .interpret_query("find all markdown files")
        .await
        .unwrap();

    assert_eq!(interpretation.query_type, QueryType::Command);
    assert!(interpretation.hyde_text.is_some());
    assert_eq!(interpretation.search_strategy, SearchStrategy::Hybrid);
}

#[tokio::test]
async fn test_service_multiple_queries_sequentially() {
    let mock_llm = Arc::new(MockLLM::new("Generic response"));
    let service = HyDEService::new(mock_llm);

    // Test multiple queries
    let greeting = service.interpret_query("Hi").await.unwrap();
    assert_eq!(greeting.query_type, QueryType::Greeting);

    let question = service.interpret_query("What is DDD?").await.unwrap();
    assert_eq!(question.query_type, QueryType::Question);

    let command = service.interpret_query("search files").await.unwrap();
    assert_eq!(command.query_type, QueryType::Command);
}

#[tokio::test]
async fn test_service_empty_query_error() {
    let mock_llm = Arc::new(MockLLM::new("response"));
    let service = HyDEService::new(mock_llm);

    let result = service.interpret_query("").await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty"));
}

#[tokio::test]
async fn test_service_case_insensitive_classification() {
    let mock_llm = Arc::new(MockLLM::new("response"));
    let service = HyDEService::new(mock_llm);

    let lower = service.interpret_query("hello").await.unwrap();
    assert_eq!(lower.query_type, QueryType::Greeting);

    let upper = service.interpret_query("HELLO").await.unwrap();
    assert_eq!(upper.query_type, QueryType::Greeting);

    let mixed = service.interpret_query("HeLLo").await.unwrap();
    assert_eq!(mixed.query_type, QueryType::Greeting);
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
async fn test_greeting_fast_path_performance() {
    // Panic if LLM is called (should use fast path)
    struct PanicLLM;

    #[async_trait]
    impl LLMPort for PanicLLM {
        async fn generate(
            &self,
            _prompt: &str,
            _context: &[String],
            _images: Option<Vec<String>>,
        ) -> Result<String> {
            panic!("LLM should not be called for greetings!");
        }

        async fn generate_streaming(
            &self,
            _prompt: &str,
            _context: &[String],
            _images: Option<Vec<String>>,
        ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
            panic!("LLM should not be called for greetings!");
        }

        fn model_name(&self) -> &str {
            "panic-llm"
        }

        fn max_context_tokens(&self) -> usize {
            4096
        }

        fn count_tokens(&self, text: &str) -> usize {
            text.len()
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    let panic_llm = Arc::new(PanicLLM);
    let service = HyDEService::new(panic_llm);

    // This should succeed without calling the LLM
    let result = service.interpret_query("Hello!").await.unwrap();
    assert_eq!(result.query_type, QueryType::Greeting);
}

// ============================================================================
// Domain Model Tests
// ============================================================================

#[test]
fn test_interpretation_search_text_extraction() {
    // Test HyDEOnly strategy
    let hyde_only = HyDEInterpretation::for_question("original", "hyde text");
    assert_eq!(hyde_only.search_text(), vec!["hyde text"]);

    // Test RawOnly strategy
    let raw_only = HyDEInterpretation::for_greeting("greeting");
    assert_eq!(raw_only.search_text(), vec!["greeting"]);

    // Test Hybrid strategy
    let hybrid = HyDEInterpretation::hybrid("original", "hyde", QueryType::Command);
    assert_eq!(hybrid.search_text(), vec!["original", "hyde"]);
}

#[test]
fn test_interpretation_builder_methods() {
    let greeting = HyDEInterpretation::for_greeting("Hi");
    assert_eq!(greeting.query_type, QueryType::Greeting);
    assert!(greeting.hyde_text.is_none());

    let question = HyDEInterpretation::for_question("What?", "Answer");
    assert_eq!(question.query_type, QueryType::Question);
    assert_eq!(question.hyde_text, Some("Answer".to_string()));

    let raw = HyDEInterpretation::raw_only("query", QueryType::Command);
    assert!(raw.hyde_text.is_none());
    assert_eq!(raw.search_strategy, SearchStrategy::RawOnly);
}
