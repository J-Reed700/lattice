//! Tests for HyDE rewriting, follow-up classification and web-query cleanup.

use super::*;
use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use crate::domain::qa::hyde::SearchStrategy;
use async_trait::async_trait;

/// A small model numbers its lines, quotes some, repeats the main search
/// and wanders into prose; only the usable, distinct queries survive.
#[test]
fn research_followups_keep_only_distinct_search_bar_queries() {
    let raw = "1. when to plant leafy greens in spring\n\
                   - \"common pests on lettuce and kale\"\n\
                   \n\
                   Next steps growing greens for home garden\n\
                   greens\n\
                   Here are some searches you could run to learn more about growing greens at home this year\n\
                   • succession sowing salad greens\n\
                   when to plant leafy greens in spring\n\
                   harvesting greens cut and come again";
    let followups = parse_research_followups(raw, "next steps growing greens for home garden", 3);
    assert_eq!(
        followups,
        vec![
            "when to plant leafy greens in spring",
            "common pests on lettuce and kale",
            "succession sowing salad greens",
        ]
    );
}

/// Shared mock LLM for testing HyDE generation.
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
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse::from_text(
            self.respond(request.user_text()).await?,
        ))
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
impl MockLLM {
    async fn respond(&self, _prompt: &str) -> Result<String> {
        Ok(self.response.clone())
    }
}

/// Typed-completion mock that records every request and refuses the
/// legacy path, so a rewrite that skips `complete` fails loudly.
struct TypedLLM {
    response: String,
    finish_reason: String,
    requests: std::sync::Mutex<Vec<CompletionRequest>>,
}

impl TypedLLM {
    fn new(response: impl Into<String>, finish_reason: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            finish_reason: finish_reason.into(),
            requests: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl LLMPort for TypedLLM {
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<crate::application::ports::llm_port::CompletionResponse> {
        self.requests.lock().unwrap().push(request.clone());
        Ok(crate::application::ports::llm_port::CompletionResponse {
            text: self.response.clone(),
            finish_reason: self.finish_reason.clone(),
            ..Default::default()
        })
    }

    fn model_name(&self) -> &str {
        "typed-llm"
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

/// A stop press must reach a rewrite still queued behind other work.
#[tokio::test]
async fn every_rewrite_carries_the_turns_cancellation() {
    let llm = Arc::new(TypedLLM::new("Rust ownership and borrowing.", "stop"));
    let turn = tokio_util::sync::CancellationToken::new();
    let generator = HyDEGenerator::new(llm.clone()).with_cancellation(turn.clone());
    generator
        .generate_web_search_query("How does Rust ownership work?", None)
        .await
        .unwrap();
    let requests = llm.requests();
    assert_eq!(requests.len(), 1);
    let carried = requests[0].cancel.clone().expect("the turn's token");
    assert!(!carried.is_cancelled());
    turn.cancel();
    assert!(carried.is_cancelled());

    let untied = Arc::new(TypedLLM::new("Rust ownership and borrowing.", "stop"));
    HyDEGenerator::new(untied.clone())
        .generate_web_search_query("How does Rust ownership work?", None)
        .await
        .unwrap();
    assert!(untied.requests()[0].cancel.is_none());
}

#[tokio::test]
async fn test_typed_rewrites_keep_model_default_reasoning() {
    let llm = Arc::new(TypedLLM::new(
        "Rust ownership, borrowing, and lifetimes in systems programming.",
        "stop",
    ));
    let generator = HyDEGenerator::new(llm.clone());

    generator
        .classify_followup_with_context("what about those?", "User: Rust ownership")
        .await
        .unwrap();
    generator
        .generate_web_search_query("How does Rust ownership work?", None)
        .await
        .unwrap();
    generator
        .generate(QueryType::Question, "How does Rust ownership work?")
        .await
        .unwrap();
    generator
        .generate(QueryType::Command, "find notes on Rust ownership")
        .await
        .unwrap();
    let followup = generator
        .generate_with_context(
            QueryType::Followup,
            "and borrowing?",
            Some("User: How does Rust ownership work?"),
        )
        .await
        .unwrap();
    assert_eq!(followup.search_strategy, SearchStrategy::Hybrid);

    let requests = llm.requests();
    assert_eq!(requests.len(), 5);
    for request in &requests {
        assert!(request.reasoning_effort.is_none());
        assert_eq!(request.time_budget, Some(REWRITE_TIME_BUDGET));
        assert!(request.tools.is_empty());
        assert!(request.json_schema.is_none());
        assert!(matches!(
            request.input.as_slice(),
            [CompletionInput::Message { role, .. }] if role == "user"
        ));
    }
    let CompletionInput::Message { content, .. } = &requests[2].input[0] else {
        unreachable!("asserted above");
    };
    assert!(content.contains("Question: How does Rust ownership work?"));
}

#[tokio::test]
async fn test_typed_rewrite_truncated_at_length_is_an_error() {
    let llm = Arc::new(TypedLLM::new("Rust ownership and", "length"));
    let generator = HyDEGenerator::new(llm);

    let result = generator
        .generate(QueryType::Question, "How does Rust ownership work?")
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_greeting_fast_path() {
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
async fn test_question_generates_hyde() {
    let mock_llm = Arc::new(MockLLM::new(
        "Domain-Driven Design is a software development approach that focuses on modeling complex business domains.",
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
async fn test_command_generates_hyde_hybrid() {
    let mock_llm = Arc::new(MockLLM::new(
        "Documents about file system operations, search algorithms, and document indexing.",
    ));
    let generator = HyDEGenerator::new(mock_llm);

    let result = generator
        .generate(QueryType::Command, "search for documents")
        .await
        .unwrap();

    assert_eq!(result.query_type, QueryType::Command);
    assert_eq!(result.original_query, "search for documents");
    assert!(result.hyde_text.is_some());
    assert_eq!(result.search_strategy, SearchStrategy::Hybrid);
}

#[tokio::test]
async fn test_followup_generates_hyde_hybrid() {
    let mock_llm = Arc::new(MockLLM::new(
        "The benefits include better code organization and maintainability.",
    ));
    let generator = HyDEGenerator::new(mock_llm);

    let result = generator
        .generate(QueryType::Followup, "what are the benefits?")
        .await
        .unwrap();

    assert_eq!(result.query_type, QueryType::Followup);
    assert_eq!(result.original_query, "what are the benefits?");
    assert!(result.hyde_text.is_some());
    assert_eq!(result.search_strategy, SearchStrategy::Hybrid);
}

#[tokio::test]
async fn test_empty_query_returns_error() {
    let mock_llm = Arc::new(MockLLM::new("response"));
    let generator = HyDEGenerator::new(mock_llm);

    let result = generator.generate(QueryType::Question, "").await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty"));
}

#[tokio::test]
async fn test_empty_llm_response_falls_back_to_raw() {
    let mock_llm = Arc::new(MockLLM::new(""));
    let generator = HyDEGenerator::new(mock_llm);

    let result = generator
        .generate(QueryType::Question, "What is Rust?")
        .await
        .unwrap();

    assert_eq!(result.query_type, QueryType::Question);
    assert!(result.hyde_text.is_none());
    assert_eq!(result.search_strategy, SearchStrategy::RawOnly);
}

#[tokio::test]
async fn test_whitespace_trimmed() {
    let mock_llm = Arc::new(MockLLM::new("  Trimmed response  "));
    let generator = HyDEGenerator::new(mock_llm);

    let result = generator
        .generate(QueryType::Question, "test query")
        .await
        .unwrap();

    let hyde = result.hyde_text.unwrap();
    assert_eq!(hyde, "Trimmed response");
    assert!(!hyde.starts_with(' '));
    assert!(!hyde.ends_with(' '));
}

#[tokio::test]
async fn test_prompt_templates_contain_query() {
    assert!(QUESTION_HYDE_TEMPLATE.contains("{query}"));
    assert!(COMMAND_HYDE_TEMPLATE.contains("{query}"));
    assert!(WEB_SEARCH_QUERY_TEMPLATE.contains("{query}"));
    assert!(WEB_SEARCH_QUERY_WITH_CONTEXT_TEMPLATE.contains("{query}"));
    assert!(WEB_SEARCH_QUERY_WITH_CONTEXT_TEMPLATE.contains("{context}"));
}

#[tokio::test]
async fn test_generate_web_search_query_normalizes_prefix() {
    let mock_llm = Arc::new(MockLLM::new(
        "Search query: \"SAVE America Act House passed Senate Trump endorsement\"",
    ));
    let generator = HyDEGenerator::new(mock_llm);

    let query = generator
        .generate_web_search_query("What is the SAVE America Act that passed the House?", None)
        .await
        .unwrap();

    assert_eq!(
        query,
        "SAVE America Act House passed Senate Trump endorsement"
    );
}

/// The turn in the log: asked for crops beyond the ones they already grow,
/// the model answered with the reader's list copied back out. The old cap
/// cut that to its first twenty-four words and searched for it, ending on
/// a dangling "continuous" — and a search engine given a paragraph matches
/// on one common word, so the sources came back including the dictionary
/// definition of "indoor".
#[tokio::test]
async fn test_generate_web_search_query_rejects_a_retelling() {
    let mock_llm = Arc::new(MockLLM::new(
        "indoor vegetable garden list supplement existing crops spinach chard kale sage \
             dill basil tomatoes thyme oregano golden beets beets bell peppers limited space \
             continuous harvest",
    ));
    let generator = HyDEGenerator::new(mock_llm);

    let error = generator
        .generate_web_search_query(
            "I'm keeping my herbs and greens. I need things beyond this",
            Some("User is planning an indoor garden of spinach, chard, kale and basil"),
        )
        .await
        .expect_err("a retelling is not a search query");

    assert!(
        error.to_string().contains("25 words"),
        "the failure should say how long the rewrite was: {error}"
    );
}

/// Twelve words is the net, not the target. A rewrite that reaches it is
/// still something an engine can work with, so it goes out unaltered —
/// nothing is trimmed to fit any more.
#[tokio::test]
async fn test_generate_web_search_query_keeps_a_query_at_the_limit() {
    let at_the_limit = "SAVE America Act House vote Senate schedule Trump endorsement \
                            2026 amendments filibuster";
    let mock_llm = Arc::new(MockLLM::new(at_the_limit));
    let generator = HyDEGenerator::new(mock_llm);

    let query = generator
        .generate_web_search_query("What is happening with the SAVE America Act?", None)
        .await
        .unwrap();

    assert_eq!(query.split_whitespace().count(), 12);
    assert_eq!(query, at_the_limit);
}

/// The bound belongs in the prompt too, not only in the guard. The guard
/// can only throw a rewrite away, and a turn that loses its rewrite falls
/// back to a query assembled out of keywords.
#[tokio::test]
async fn test_web_search_prompts_bound_the_query_and_forbid_lists() {
    for template in [
        WEB_SEARCH_QUERY_TEMPLATE,
        WEB_SEARCH_QUERY_WITH_CONTEXT_TEMPLATE,
    ] {
        assert!(template.contains("Use 3 to 8 words"));
        assert!(template.contains("Do not copy out a list"));
        assert!(template.contains("Example search query:"));
    }
}

#[tokio::test]
async fn test_generate_web_search_query_falls_back_to_input_when_empty() {
    let mock_llm = Arc::new(MockLLM::new("   "));
    let generator = HyDEGenerator::new(mock_llm);

    let raw = "What is the SAVE America Act?";
    let query = generator
        .generate_web_search_query(raw, Some("User asked about SAVE Act"))
        .await
        .unwrap();

    assert_eq!(query, raw);
}

#[tokio::test]
async fn test_greeting_no_llm_call() {
    struct PanicLLM;

    #[async_trait]
    impl LLMPort for PanicLLM {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse::from_text(
                self.respond(request.user_text()).await?,
            ))
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
    impl PanicLLM {
        async fn respond(&self, _prompt: &str) -> Result<String> {
            unreachable!("PanicLLM is test-only mock - LLM should not be called for greetings")
        }
    }

    let panic_llm = Arc::new(PanicLLM);
    let generator = HyDEGenerator::new(panic_llm);

    let result = generator
        .generate(QueryType::Greeting, "Hi!")
        .await
        .unwrap();
    assert_eq!(result.query_type, QueryType::Greeting);
}
