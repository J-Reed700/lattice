use super::*;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::domain::qa::hyde::{QueryType, SearchStrategy};

/// Mock LLM for testing
struct MockLLM {
    response: String,
    calls: std::sync::atomic::AtomicUsize,
}

impl MockLLM {
    fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
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
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.response.clone())
    }
}

/// A web-only turn needs the turn's type and nothing else. Working that out
/// must not cost a model call — the expansion it used to generate alongside
/// took the utility model over five seconds and nothing read it.
#[tokio::test]
async fn classifying_a_first_question_never_calls_the_model() {
    let mock_llm = Arc::new(MockLLM::new("Should not be called"));
    let service = HyDEService::new(mock_llm.clone());

    let query_type = service
        .classify_query_with_context("I need an in depth recap of seasons 1 and 2 of Silo", None)
        .await
        .unwrap();

    assert_eq!(query_type, QueryType::Question);
    assert_eq!(mock_llm.calls(), 0);
}

/// The same question through the full interpretation does pay for the
/// expansion, which is what makes the split worth having.
#[tokio::test]
async fn interpreting_the_same_question_does_call_the_model() {
    let mock_llm = Arc::new(MockLLM::new("An expansion about Silo."));
    let service = HyDEService::new(mock_llm.clone());

    let interpretation = service
        .interpret_query("I need an in depth recap of seasons 1 and 2 of Silo")
        .await
        .unwrap();

    assert!(interpretation.hyde_text.is_some());
    assert!(mock_llm.calls() >= 1);
}

#[tokio::test]
async fn classifying_an_empty_query_is_an_error() {
    let service = HyDEService::new(Arc::new(MockLLM::new("unused")));
    assert!(service
        .classify_query_with_context("   ", None)
        .await
        .is_err());
}

#[tokio::test]
async fn test_interpret_greeting() {
    let mock_llm = Arc::new(MockLLM::new("Should not be called"));
    let service = HyDEService::new(mock_llm);

    let interpretation = service.interpret_query("Hello!").await.unwrap();

    assert_eq!(interpretation.query_type, QueryType::Greeting);
    assert_eq!(interpretation.original_query, "Hello!");
    assert!(interpretation.hyde_text.is_none());
    assert_eq!(interpretation.search_strategy, SearchStrategy::RawOnly);
}

#[tokio::test]
async fn test_interpret_question() {
    let mock_llm = Arc::new(MockLLM::new(
        "Domain-Driven Design is a software development approach.",
    ));
    let service = HyDEService::new(mock_llm);

    let interpretation = service
        .interpret_query("What is Domain-Driven Design?")
        .await
        .unwrap();

    assert_eq!(interpretation.query_type, QueryType::Question);
    assert_eq!(
        interpretation.original_query,
        "What is Domain-Driven Design?"
    );
    assert!(interpretation.hyde_text.is_some());
    assert_eq!(interpretation.search_strategy, SearchStrategy::HyDEOnly);
}

#[tokio::test]
async fn test_interpret_command() {
    let mock_llm = Arc::new(MockLLM::new(
        "Documents about search functionality and algorithms.",
    ));
    let service = HyDEService::new(mock_llm);

    let interpretation = service
        .interpret_query("search for documents about Rust")
        .await
        .unwrap();

    assert_eq!(interpretation.query_type, QueryType::Command);
    assert!(interpretation.hyde_text.is_some());
    assert_eq!(interpretation.search_strategy, SearchStrategy::Hybrid);
}

#[tokio::test]
async fn test_empty_query_returns_error() {
    let mock_llm = Arc::new(MockLLM::new("response"));
    let service = HyDEService::new(mock_llm);

    let result = service.interpret_query("").await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty"));
}

#[tokio::test]
async fn test_whitespace_only_query_returns_error() {
    let mock_llm = Arc::new(MockLLM::new("response"));
    let service = HyDEService::new(mock_llm);

    let result = service.interpret_query("   ").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_interpret_with_explicit_type() {
    let mock_llm = Arc::new(MockLLM::new("Rust is a systems programming language."));
    let service = HyDEService::new(mock_llm);

    // Force treating "Rust" as a question instead of letting it classify naturally
    let interpretation = service
        .interpret_query_with_type("Rust", QueryType::Question)
        .await
        .unwrap();

    assert_eq!(interpretation.query_type, QueryType::Question);
    assert!(interpretation.hyde_text.is_some());
}

#[tokio::test]
async fn test_generate_web_search_query_with_context() {
    let mock_llm = Arc::new(MockLLM::new(
        "Search query: \"SAVE America Act House Senate bill text\"",
    ));
    let service = HyDEService::new(mock_llm);

    let query = service
        .generate_web_search_query_with_context(
            "search web",
            Some("User: What is the SAVE America Act that passed the House?"),
        )
        .await
        .unwrap();

    assert_eq!(query, "SAVE America Act House Senate bill text");
}

#[tokio::test]
async fn test_contextual_followup_is_upgraded_when_context_is_present() {
    let mock_llm = Arc::new(MockLLM::new("1"));
    let service = HyDEService::new(mock_llm);
    let context = "User: We discussed anthocyanins and flavonoids in this plant.\nAssistant: Those nutrients are linked to antioxidant and anti-inflammatory effects.";

    let interpretation = service
        .interpret_query_with_context("What health benefits do those offer?", Some(context))
        .await
        .unwrap();

    assert_eq!(interpretation.query_type, QueryType::Followup);
    assert_eq!(interpretation.search_strategy, SearchStrategy::Hybrid);
    assert!(interpretation.hyde_text.is_some());
}

#[tokio::test]
async fn test_discourse_marked_question_is_upgraded_to_followup_with_context() {
    let mock_llm = Arc::new(MockLLM::new(
        "Health benefits of glucosinolates and isothiocyanates in arugula.",
    ));
    let service = HyDEService::new(mock_llm);
    let context = "User: is the bitterness in arugula from nitric oxide?\nAssistant: Bitterness comes from glucosinolates and isothiocyanates in arugula.";

    let interpretation = service
        .interpret_query_with_context("What are the health benefits though?", Some(context))
        .await
        .unwrap();

    assert_eq!(interpretation.query_type, QueryType::Followup);
}

#[tokio::test]
async fn test_additive_followup_question_is_upgraded_with_context() {
    let mock_llm = Arc::new(MockLLM::new(
        "Additional health effects of arugula glucosinolates and isothiocyanates.",
    ));
    let service = HyDEService::new(mock_llm);
    let context = "User: Are there any health benefits to those compounds?\nAssistant: Yes, glucosinolates in arugula support antioxidant and anti-inflammatory pathways.";

    let interpretation = service
        .interpret_query_with_context(
            "what other health benefits does arugula provide?",
            Some(context),
        )
        .await
        .unwrap();

    assert_eq!(interpretation.query_type, QueryType::Followup);
}

#[tokio::test]
async fn test_referential_query_without_context_stays_question() {
    let mock_llm = Arc::new(MockLLM::new("general retrieval expansion"));
    let service = HyDEService::new(mock_llm);

    let interpretation = service
        .interpret_query_with_context("What health benefits do those offer?", None)
        .await
        .unwrap();

    assert_eq!(interpretation.query_type, QueryType::Question);
}

#[tokio::test]
async fn test_multiple_queries_sequentially() {
    let mock_llm = Arc::new(MockLLM::new("Test response"));
    let service = HyDEService::new(mock_llm);

    let greeting = service.interpret_query("Hi").await.unwrap();
    assert_eq!(greeting.query_type, QueryType::Greeting);

    let question = service.interpret_query("What is Rust?").await.unwrap();
    assert_eq!(question.query_type, QueryType::Question);

    let command = service.interpret_query("search files").await.unwrap();
    assert_eq!(command.query_type, QueryType::Command);
}

#[tokio::test]
async fn test_case_insensitive_classification() {
    let mock_llm = Arc::new(MockLLM::new("Response"));
    let service = HyDEService::new(mock_llm);

    let lower = service.interpret_query("hello").await.unwrap();
    assert_eq!(lower.query_type, QueryType::Greeting);

    let upper = service.interpret_query("HELLO").await.unwrap();
    assert_eq!(upper.query_type, QueryType::Greeting);

    let mixed = service.interpret_query("HeLLo").await.unwrap();
    assert_eq!(mixed.query_type, QueryType::Greeting);
}

#[tokio::test]
async fn test_greeting_variations() {
    let mock_llm = Arc::new(MockLLM::new("Should not be called"));
    let service = HyDEService::new(mock_llm);

    let greetings = vec![
        "hi",
        "hello",
        "hey",
        "good morning",
        "good afternoon",
        "good evening",
    ];

    for greeting in greetings {
        let result = service.interpret_query(greeting).await.unwrap();
        assert_eq!(
            result.query_type,
            QueryType::Greeting,
            "Failed for: {}",
            greeting
        );
        assert!(
            result.hyde_text.is_none(),
            "Greeting should not have HyDE text: {}",
            greeting
        );
    }
}

#[tokio::test]
async fn test_question_patterns() {
    let mock_llm = Arc::new(MockLLM::new("Test answer"));
    let service = HyDEService::new(mock_llm);

    let questions = vec![
        "what is rust",
        "how does it work",
        "why is it important",
        "when was it created",
        "where can I learn",
        "who created it",
        "explain rust",
        "tell me about rust",
    ];

    for question in questions {
        let result = service.interpret_query(question).await.unwrap();
        assert_eq!(
            result.query_type,
            QueryType::Question,
            "Failed for: {}",
            question
        );
    }
}

#[tokio::test]
async fn test_command_patterns() {
    let mock_llm = Arc::new(MockLLM::new("Command description"));
    let service = HyDEService::new(mock_llm);

    let commands = vec![
        "search files",
        "find documents",
        "show results",
        "list all",
        "index directory",
        "delete old files",
    ];

    for command in commands {
        let result = service.interpret_query(command).await.unwrap();
        assert_eq!(
            result.query_type,
            QueryType::Command,
            "Failed for: {}",
            command
        );
    }
}

#[tokio::test]
async fn test_integration_greeting_no_llm_call() {
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
            // P0-6 FIX: Use unreachable! instead of panic! for test invariants
            unreachable!("PanicLLM is test-only mock - LLM should not be called for greetings")
        }
    }

    let panic_llm = Arc::new(PanicLLM);
    let service = HyDEService::new(panic_llm);

    // This should succeed without calling the LLM
    let result = service.interpret_query("Hello!").await.unwrap();
    assert_eq!(result.query_type, QueryType::Greeting);
}
