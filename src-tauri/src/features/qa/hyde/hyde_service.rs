//! HyDE Service Orchestrator
//!
//! Main entry point for HyDE (Hypothetical Document Embeddings) system.
//! Coordinates query classification and HyDE generation.
//!
//! ## Architecture
//!
//! ```text
//! User Query
//!     ↓
//! HyDEService::interpret_query()
//!     ↓
//! QueryClassifier
//!     ↓
//! QueryType
//!     ↓
//! HyDEGenerator
//!     ↓
//! HyDEInterpretation
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use lattice::features::qa::hyde::HyDEService;
//! use std::sync::Arc;
//!
//! async fn example(llm: Arc<dyn LLMPort>) {
//!     let service = HyDEService::new(llm);
//!
//!     let interpretation = service.interpret_query("What is Rust?").await?;
//!
//!     println!("Query type: {}", interpretation.query_type);
//!     println!("Search strategy: {}", interpretation.search_strategy);
//!     if let Some(hyde) = interpretation.hyde_text {
//!         println!("HyDE text: {}", hyde);
//!     }
//! }
//! ```

use crate::application::ports::LLMPort;
use crate::domain::qa::hyde::{HyDEInterpretation, QueryType};
use crate::features::qa::hyde::hyde_generator::HyDEGenerator;
use crate::features::qa::hyde::query_classifier::QueryClassifier;
use crate::features::search::engine::query_expansion::dictionaries::select_informative_terms;
use crate::shared::error::{AppError, Result};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info};

/// Orchestrates HyDE query interpretation pipeline.
///
/// This service is the main entry point for HyDE processing. It:
/// 1. Classifies the query type (greeting, question, command, followup)
/// 2. Generates appropriate HyDE expansion via LLM (if needed)
/// 3. Returns interpretation with search strategy
///
/// # Example
///
/// ```rust,no_run
/// use lattice::features::qa::hyde::HyDEService;
/// use lattice::domain::qa::hyde::QueryType;
/// use std::sync::Arc;
///
/// async fn example(llm: Arc<dyn LLMPort>) -> Result<()> {
///     let service = HyDEService::new(llm);
///
///     // Greeting - fast path, no LLM call
///     let greeting = service.interpret_query("Hello!").await?;
///     assert_eq!(greeting.query_type, QueryType::Greeting);
///
///     // Question - LLM generates hypothetical answer
///     let question = service.interpret_query("What is DDD?").await?;
///     assert_eq!(question.query_type, QueryType::Question);
///     assert!(question.hyde_text.is_some());
///
///     Ok(())
/// }
/// ```
pub struct HyDEService {
    classifier: QueryClassifier,
    generator: HyDEGenerator,
}

impl HyDEService {
    /// Create a new HyDE service with LLM backend.
    ///
    /// # Arguments
    ///
    /// * `llm` - LLM port implementation for HyDE generation
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::features::qa::hyde::HyDEService;
    /// use std::sync::Arc;
    ///
    /// let service = HyDEService::new(llm_port);
    /// ```
    pub fn new(llm: Arc<dyn LLMPort>) -> Self {
        Self {
            classifier: QueryClassifier::new(),
            generator: HyDEGenerator::new(llm),
        }
    }

    /// Interpret a user query with HyDE enrichment.
    ///
    /// This is the main entry point for the HyDE pipeline:
    /// 1. **Classify** the query type using pattern matching
    /// 2. **Generate** HyDE expansion via LLM (if appropriate)
    /// 3. **Return** interpretation with search strategy
    ///
    /// # Arguments
    ///
    /// * `query` - The user's query string
    ///
    /// # Returns
    ///
    /// `HyDEInterpretation` containing:
    /// - Original query
    /// - Classified query type
    /// - Optional HyDE-generated text
    /// - Recommended search strategy
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if query is empty or too long
    /// - `AppError::Network` if LLM call fails (for questions/commands)
    /// - `AppError::Other` for LLM generation errors
    ///
    /// # Performance
    ///
    /// - **Greetings**: ~1ms (fast-path, no LLM)
    /// - **Questions**: ~500-2000ms (LLM call)
    /// - **Commands**: ~500-2000ms (LLM call)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let interpretation = service.interpret_query("What is Rust?").await?;
    ///
    /// match interpretation.query_type {
    ///     QueryType::Greeting => println!("No search needed"),
    ///     QueryType::Question => {
    ///         let hyde = interpretation.hyde_text.unwrap();
    ///         println!("Search using HyDE: {}", hyde);
    ///     }
    ///     _ => {}
    /// }
    /// ```
    pub async fn interpret_query(&self, query: impl Into<String>) -> Result<HyDEInterpretation> {
        self.interpret_query_with_context(query, None).await
    }

    /// Generate a search-engine-ready query for web tools.
    ///
    /// This uses a dedicated prompt that is different from KB-oriented HyDE
    /// expansion, so web lookup gets a concise search-bar query.
    pub async fn generate_web_search_query_with_context(
        &self,
        query: impl Into<String>,
        conversation_context: Option<&str>,
    ) -> Result<String> {
        let query = query.into();
        if query.trim().is_empty() {
            return Err(AppError::InvalidInput("Query cannot be empty".to_string()));
        }
        self.generator
            .generate_web_search_query(&query, conversation_context)
            .await
    }

    /// Decide what kind of turn this is, without writing anything for it.
    ///
    /// When a query appears referential (e.g., "what do those do?") and recent
    /// context is available, this upgrades classification to `Followup`.
    ///
    /// Classification is rule-based and close to free; the expansion
    /// [`Self::interpret_query_with_context`] goes on to generate is a full
    /// model call. A caller that only needs the type — a turn searching the web,
    /// where nothing reads the expansion — should stop here.
    pub async fn classify_query_with_context(
        &self,
        query: &str,
        conversation_context: Option<&str>,
    ) -> Result<QueryType> {
        if query.trim().is_empty() {
            return Err(AppError::InvalidInput("Query cannot be empty".to_string()));
        }

        debug!("Classifying query: {}", query);
        let mut query_type = self.classifier.classify(query);
        if let Some(context) = conversation_context {
            if matches!(query_type, QueryType::Question) {
                let continuity_score = contextual_followup_score(query, context);
                let classifier_followup = self
                    .generator
                    .classify_followup_with_context(query, context)
                    .await
                    .unwrap_or(false);
                if continuity_score >= 0.30 || classifier_followup {
                    query_type = QueryType::Followup;
                    info!(
                        continuity_score = continuity_score,
                        classifier_followup = classifier_followup,
                        "Query upgraded to Followup via contextual scoring + model classifier"
                    );
                }
            }
        }
        info!("Query classified as: {}", query_type);
        Ok(query_type)
    }

    /// Interpret a user query with optional conversation context: classify it,
    /// then generate the retrieval-oriented expansion for that type.
    pub async fn interpret_query_with_context(
        &self,
        query: impl Into<String>,
        conversation_context: Option<&str>,
    ) -> Result<HyDEInterpretation> {
        let query = query.into();
        let query_type = self
            .classify_query_with_context(&query, conversation_context)
            .await?;

        let interpretation = self
            .generator
            .generate_with_context(query_type, query, conversation_context)
            .await?;

        info!(
            "Generated interpretation - Type: {}, Strategy: {}, HyDE: {}",
            interpretation.query_type,
            interpretation.search_strategy,
            if interpretation.hyde_text.is_some() {
                "present"
            } else {
                "none"
            }
        );

        Ok(interpretation)
    }

    /// Interpret a query with explicit query type (for testing/debugging).
    ///
    /// Skips classification and directly generates HyDE with the provided type.
    ///
    /// # Arguments
    ///
    /// * `query` - The user's query string
    /// * `query_type` - Explicitly specified query type
    ///
    /// # Returns
    ///
    /// `HyDEInterpretation` with the specified query type.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::qa::hyde::QueryType;
    ///
    /// // Force treating a query as a question
    /// let interpretation = service.interpret_query_with_type(
    ///     "machine learning",
    ///     QueryType::Question
    /// ).await?;
    /// ```
    pub async fn interpret_query_with_type(
        &self,
        query: impl Into<String>,
        query_type: crate::domain::qa::hyde::QueryType,
    ) -> Result<HyDEInterpretation> {
        let query = query.into();

        if query.trim().is_empty() {
            return Err(AppError::InvalidInput("Query cannot be empty".to_string()));
        }

        debug!(
            "Generating interpretation with explicit type: {}",
            query_type
        );
        self.generator.generate(query_type, query).await
    }
}

fn contextual_followup_score(query: &str, conversation_context: &str) -> f32 {
    if query.trim().is_empty() || conversation_context.trim().is_empty() {
        return 0.0;
    }

    let query_tokens = tokenize_terms(query);
    if query_tokens.is_empty() {
        return 0.0;
    }

    let query_salient = salient_terms(&query_tokens, 10);
    if query_salient.is_empty() {
        return 0.0;
    }
    let query_token_set: HashSet<String> = query_tokens
        .iter()
        .filter(|token| token.len() >= 3)
        .cloned()
        .collect();

    let context_tail = take_last_chars(conversation_context, 12000);
    let context_tail_terms: HashSet<String> = salient_terms(&tokenize_terms(&context_tail), 64)
        .into_iter()
        .collect();
    let context_full_terms: HashSet<String> =
        salient_terms(&tokenize_terms(conversation_context), 128)
            .into_iter()
            .collect();
    let context_tail_tokens: HashSet<String> = tokenize_terms(&context_tail)
        .into_iter()
        .filter(|token| token.len() >= 3)
        .collect();

    let overlap_tail = query_salient
        .iter()
        .filter(|term| context_tail_terms.contains(*term))
        .count() as f32
        / query_salient.len() as f32;
    let overlap_full = query_salient
        .iter()
        .filter(|term| context_full_terms.contains(*term))
        .count() as f32
        / query_salient.len() as f32;
    let token_overlap_tail = if query_token_set.is_empty() {
        0.0
    } else {
        query_token_set
            .iter()
            .filter(|term| context_tail_tokens.contains(*term))
            .count() as f32
            / query_token_set.len() as f32
    };
    let brevity_bonus = match query_tokens.len() {
        0..=8 => 0.16,
        9..=14 => 0.10,
        15..=22 => 0.04,
        _ => 0.0,
    };
    let salience_compactness =
        1.0 - ((query_salient.len() as f32) / (query_tokens.len() as f32)).min(1.0);
    let discourse_bonus = if query_tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "though" | "also" | "instead" | "otherwise" | "then"
        )
    }) {
        0.30
    } else {
        0.0
    };

    (overlap_tail * 0.45)
        + (overlap_full * 0.20)
        + (token_overlap_tail * 0.25)
        + (brevity_bonus * 0.07)
        + (salience_compactness * 0.03)
        + discourse_bonus
}

fn tokenize_terms(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn salient_terms(tokens: &[String], max_terms: usize) -> Vec<String> {
    select_informative_terms(tokens.to_vec(), max_terms)
}

fn take_last_chars(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let start_char = total.saturating_sub(max_chars);
    let start_byte = text
        .char_indices()
        .nth(start_char)
        .map(|(idx, _)| idx)
        .unwrap_or(0);
    text[start_byte..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::qa::hyde::{QueryType, SearchStrategy};
    use async_trait::async_trait;
    use futures::stream::{self, Stream};

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
        async fn generate(
            &self,
            _prompt: &str,
            _context: &[String],
            _images: Option<Vec<String>>,
        ) -> Result<String> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

    /// A web-only turn needs the turn's type and nothing else. Working that out
    /// must not cost a model call — the expansion it used to generate alongside
    /// took the utility model over five seconds and nothing read it.
    #[tokio::test]
    async fn classifying_a_first_question_never_calls_the_model() {
        let mock_llm = Arc::new(MockLLM::new("Should not be called"));
        let service = HyDEService::new(mock_llm.clone());

        let query_type = service
            .classify_query_with_context(
                "I need an in depth recap of seasons 1 and 2 of Silo",
                None,
            )
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
            async fn generate(
                &self,
                _prompt: &str,
                _context: &[String],
                _images: Option<Vec<String>>,
            ) -> Result<String> {
                // P0-6 FIX: Use unreachable! instead of panic! for test invariants
                unreachable!("PanicLLM is test-only mock - LLM should not be called for greetings")
            }

            async fn generate_streaming(
                &self,
                _prompt: &str,
                _context: &[String],
                _images: Option<Vec<String>>,
            ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
                // P0-6 FIX: Use unreachable! instead of panic! for test invariants
                unreachable!("PanicLLM is test-only mock - LLM should not be called for greetings")
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
}
