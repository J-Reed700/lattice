//! HyDE Generator Service
//!
//! Generates hypothetical document embeddings (HyDE) for query expansion.
//! Uses LLM to rewrite questions into retrieval-oriented intent expansions
//! that semantically match what relevant documents would contain.
//!
//! ## Purpose
//!
//! - For greetings: Fast-path with no LLM call
//! - For questions: Generate hypothetical document via LLM
//! - For commands: Generate expanded command interpretation
//!
//! ## Architecture
//!
//! ```text
//! QueryType + Query Text
//!         ↓
//!   HyDEGenerator
//!         ↓
//!   ┌─────┴─────┐
//!   │           │
//! Greeting   Question/Command
//!   │           │
//! Fast-path   LLM Call
//!   │           │
//!   └─────┬─────┘
//!         ↓
//! HyDEInterpretation
//! ```

use crate::application::ports::LLMPort;
use crate::domain::qa::hyde::{HyDEInterpretation, QueryType};
use crate::shared::error::{AppError, Result};
use crate::shared::text_utils::safe_truncate;
use lazy_regex::regex;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

// ============================================================================
// Prompt Templates
// ============================================================================

/// Prompt template for question expansion via HyDE.
///
/// Instructs the LLM to expand intent and retrieval vocabulary without
/// providing an answer.
const QUESTION_HYDE_TEMPLATE: &str = r#"You are an expert assistant helping with document retrieval.

Your task: Rewrite and expand the user question into retrieval-oriented text that captures intent and likely terminology found in relevant documents.

Guidelines:
- Write 2-4 short sentences (max 90 words)
- Preserve key entities, places, dates, and constraints from the question
- Add closely related terms, aliases, and corrected spellings when helpful
- Focus on what relevant documents would discuss, not the final answer
- Do NOT answer the question directly
- Do NOT assert facts that are not explicitly present in the query
- Do NOT use first-person narration (no "I", "we")
- Do NOT explain what you're doing; output only the expansion text

Question: {query}

Expansion:"#;

/// Prompt template for follow-up query expansion with conversation context.
///
/// Instructs the LLM to resolve references in the follow-up against recent
/// conversation turns, then emit retrieval-oriented expansion text.
const FOLLOWUP_HYDE_TEMPLATE: &str = r#"You are an expert assistant helping with document retrieval.

Your task: Infer what the follow-up question is referring to using recent conversation context, then rewrite it into retrieval-oriented text.

Guidelines:
- Resolve pronouns and references ("those", "it", "they", "that") using context
- Preserve key entities, compounds, nutrients, and constraints already discussed
- Write 2-4 short sentences (max 90 words)
- Focus on terms likely to appear in relevant documents
- Do NOT answer the question directly
- Do NOT invent facts not present in the follow-up or context
- Output only the expansion text

Recent conversation context:
{context}

Follow-up question:
{query}

Expansion:"#;

/// Prompt template for command interpretation.
///
/// Expands a command into a description of what documents would be relevant.
const COMMAND_HYDE_TEMPLATE: &str = r#"You are an expert assistant helping with document retrieval.

Your task: Describe what kind of documents would be relevant for this command. Write 2-3 sentences describing the target documents.

Guidelines:
- Focus on document characteristics, not actions
- Use terminology that would appear in relevant documents
- Be specific and concrete
- Do NOT explain what you're doing, just write the description

Command: {query}

Description:"#;

/// Prompt template for contextual follow-up classification.
///
/// The model must output only `1` (context-dependent follow-up) or `0`
/// (standalone query).
const FOLLOWUP_CLASSIFIER_TEMPLATE: &str = r#"You are classifying whether a user query depends on previous conversation context.

Return exactly one character:
- 1 = context-dependent follow-up
- 0 = standalone query

Context:
{context}

Query:
{query}

Answer:"#;

/// Prompt template for web search query generation.
///
/// Produces one query string suitable for a web search bar.
const WEB_SEARCH_QUERY_TEMPLATE: &str = r#"You are rewriting user input into a web search query.

Task:
- Return exactly ONE search query line.
- Keep key entities, exact names, and important dates.
- Remove filler like "go into detail" or "please explain".
- Prefer specific terms over generic wording.
- Do not answer the question.
- Output only the query text.

User input:
{query}

Search query:"#;

/// Prompt template for contextual web search query generation.
///
/// Uses recent context to resolve short follow-ups and generic commands.
const WEB_SEARCH_QUERY_WITH_CONTEXT_TEMPLATE: &str = r#"You are rewriting user input into a web search query.

Task:
- Return exactly ONE search query line for a search engine.
- If the latest user input is short or generic (for example "search web"),
  infer the topic from recent conversation context.
- Keep key entities, exact names, and important dates.
- Remove filler and conversational wording.
- Do not answer the question.
- Output only the query text.

Recent conversation context:
{context}

Latest user input:
{query}

Search query:"#;

// ============================================================================
// HyDE Generator
// ============================================================================

/// Service for generating HyDE interpretations.
///
/// Coordinates LLM calls to expand queries into hypothetical documents
/// for semantic search enhancement.
///
/// # Design Decision: No Tool Intent Classification
///
/// Tool intent classification (vault_only, web_only, etc.) was previously
/// performed as a second LLM call per query, but the result was never consumed
/// by the RAG pipeline. This added 500-2000ms of wasted latency per query.
/// The classification has been removed; if routing is needed in the future,
/// it should be wired end-to-end before being re-enabled.
pub struct HyDEGenerator {
    llm: Arc<dyn LLMPort>,
}

fn compact_hyde_text(raw: &str) -> String {
    const MAX_WORDS: usize = 90;
    let words: Vec<&str> = raw.split_whitespace().collect();
    if words.len() <= MAX_WORDS {
        return raw.trim().to_string();
    }
    words
        .iter()
        .take(MAX_WORDS)
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokenize_guard_terms(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            tokens.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn term_information_score(term: &str) -> f32 {
    if term.is_empty() {
        return 0.0;
    }

    let len = term.len() as f32;
    let alpha_count = term.chars().filter(|c| c.is_ascii_alphabetic()).count() as f32;
    if alpha_count <= 0.0 {
        return 0.0;
    }

    let unique_ratio = (term.chars().collect::<HashSet<char>>().len() as f32 / len).min(1.0);
    let alpha_ratio = (alpha_count / len).min(1.0);
    let length_boost = (len + 1.0).ln();

    alpha_ratio * (0.62 + 0.38 * unique_ratio) * length_boost
}

fn is_informative_term(term: &str) -> bool {
    term.len() >= 4 && term_information_score(term) >= 0.9
}

fn build_retrieval_fallback(query: &str) -> String {
    let question = query.trim().trim_end_matches(&['?', '.', '!'][..]).trim();
    let mut seen = HashSet::new();
    let keywords: Vec<String> = tokenize_guard_terms(question)
        .into_iter()
        .filter(|token| is_informative_term(token))
        .filter(|token| seen.insert(token.clone()))
        .take(8)
        .collect();

    if keywords.is_empty() {
        return question.to_string();
    }

    format!(
        "{question}. Retrieval focus terms: {}.",
        keywords.join(", ")
    )
}

fn has_placeholder_artifacts(text: &str) -> bool {
    regex!(
        r"(?ix)
        \[
            \s*
            (the\s+)?(subject|topic|entity|item|thing)
            \s*
        \]
        |
        <\s*(subject|topic|entity|item|thing)\s*>
        "
    )
    .is_match(text)
}

fn looks_like_direct_answer(query: &str, hyde_text: &str) -> bool {
    let hyde_terms: Vec<String> = tokenize_guard_terms(hyde_text)
        .into_iter()
        .filter(|token| is_informative_term(token))
        .collect();
    if hyde_terms.is_empty() {
        return false;
    }

    let query_terms: HashSet<String> = tokenize_guard_terms(query)
        .into_iter()
        .filter(|token| is_informative_term(token))
        .collect();
    let shared_count = hyde_terms
        .iter()
        .filter(|term| query_terms.contains(*term))
        .count();
    let novelty_count = hyde_terms
        .iter()
        .filter(|term| !query_terms.contains(*term))
        .count();
    let shared_ratio = shared_count as f32 / hyde_terms.len() as f32;
    let sentence_count = hyde_text
        .split(&['.', '!', '?'][..])
        .filter(|segment| !segment.trim().is_empty())
        .count();
    let hyde_word_count = hyde_text.split_whitespace().count();
    let is_question = query.trim_end().ends_with('?');

    is_question
        && hyde_word_count <= 28
        && sentence_count <= 2
        && novelty_count <= 2
        && shared_ratio >= 0.55
}

fn normalize_web_search_query(raw: &str) -> String {
    const MAX_WORDS: usize = 24;

    let mut line = raw
        .lines()
        .find(|candidate| !candidate.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_string();

    for prefix in ["search query:", "query:"] {
        if line.to_ascii_lowercase().starts_with(prefix) {
            line = line[prefix.len()..].trim().to_string();
            break;
        }
    }

    if line.starts_with('"') && line.ends_with('"') && line.len() >= 2 {
        line = line[1..line.len() - 1].trim().to_string();
    }
    if line.starts_with('\'') && line.ends_with('\'') && line.len() >= 2 {
        line = line[1..line.len() - 1].trim().to_string();
    }

    let compact = line
        .split_whitespace()
        .take(MAX_WORDS)
        .collect::<Vec<_>>()
        .join(" ");

    compact.trim_matches(&['.', ';'][..]).trim().to_string()
}

impl HyDEGenerator {
    /// Create a new HyDE generator with an LLM backend.
    pub fn new(llm: Arc<dyn LLMPort>) -> Self {
        Self { llm }
    }

    /// Classify whether a query is context-dependent follow-up.
    pub async fn classify_followup_with_context(
        &self,
        query: &str,
        conversation_context: &str,
    ) -> Result<bool> {
        if query.trim().is_empty() || conversation_context.trim().is_empty() {
            return Ok(false);
        }

        let prompt = FOLLOWUP_CLASSIFIER_TEMPLATE
            .replace("{query}", query)
            .replace("{context}", conversation_context);
        let output = self.llm.generate(&prompt, &[], None).await.map_err(|e| {
            error!(
                error = %e,
                query = %query,
                "Follow-up classifier LLM call failed"
            );
            AppError::Other(format!("Follow-up classification failed: {}", e))
        })?;

        let decision = output
            .chars()
            .find(|ch| *ch == '0' || *ch == '1')
            .unwrap_or('0');
        Ok(decision == '1')
    }

    /// Generate a search-bar-ready web query from user input and optional context.
    pub async fn generate_web_search_query(
        &self,
        query: &str,
        conversation_context: Option<&str>,
    ) -> Result<String> {
        if query.trim().is_empty() {
            return Err(AppError::InvalidInput("Query cannot be empty".to_string()));
        }

        let prompt =
            if let Some(context) = conversation_context.filter(|ctx| !ctx.trim().is_empty()) {
                WEB_SEARCH_QUERY_WITH_CONTEXT_TEMPLATE
                    .replace("{context}", context)
                    .replace("{query}", query)
            } else {
                WEB_SEARCH_QUERY_TEMPLATE.replace("{query}", query)
            };

        debug!(
            query_len = query.len(),
            context_len = conversation_context.map(|ctx| ctx.len()).unwrap_or(0),
            prompt_len = prompt.len(),
            prompt_preview = %safe_truncate(&prompt, 240),
            "HyDE web query prompt prepared"
        );

        let generated = self.llm.generate(&prompt, &[], None).await.map_err(|e| {
            error!(
                error = %e,
                query = %query,
                "HyDE web query generation LLM call failed"
            );
            AppError::Other(format!("HyDE web query generation failed: {}", e))
        })?;

        let normalized = normalize_web_search_query(generated.trim());
        if normalized.is_empty() {
            return Ok(query.trim().to_string());
        }

        info!(
            query_len = normalized.len(),
            query_preview = %safe_truncate(&normalized, 180),
            "Generated web-search query via HyDE"
        );
        Ok(normalized)
    }

    /// Generate HyDE interpretation for a query.
    ///
    /// Handles different query types:
    /// - **Greeting**: Fast-path, returns immediately without LLM call
    /// - **Question**: Calls LLM to generate hypothetical answer document
    /// - **Command**: Calls LLM to expand command interpretation
    /// - **Followup**: Currently uses question template (future: context-aware)
    pub async fn generate(
        &self,
        query_type: QueryType,
        query: impl Into<String>,
    ) -> Result<HyDEInterpretation> {
        self.generate_with_context(query_type, query, None).await
    }

    /// Generate HyDE interpretation for a query with optional conversation context.
    pub async fn generate_with_context(
        &self,
        query_type: QueryType,
        query: impl Into<String>,
        conversation_context: Option<&str>,
    ) -> Result<HyDEInterpretation> {
        let query = query.into();

        if query.trim().is_empty() {
            return Err(AppError::InvalidInput("Query cannot be empty".to_string()));
        }

        match query_type {
            QueryType::Greeting => self.generate_greeting(query).await,
            QueryType::Question => self.generate_question(query).await,
            QueryType::Command => self.generate_command(query).await,
            QueryType::Followup => self.generate_followup(query, conversation_context).await,
        }
    }

    /// Fast-path for greetings - no LLM call needed.
    async fn generate_greeting(&self, query: String) -> Result<HyDEInterpretation> {
        debug!("HyDE fast-path for greeting: {}", query);
        Ok(HyDEInterpretation::for_greeting(query))
    }

    /// Generate retrieval-focused expansion text for a question.
    async fn generate_question(&self, query: String) -> Result<HyDEInterpretation> {
        info!("Generating HyDE for question: {}", query);

        let prompt = QUESTION_HYDE_TEMPLATE.replace("{query}", &query);
        debug!(
            query_len = query.len(),
            prompt_len = prompt.len(),
            prompt_preview = %safe_truncate(&prompt, 240),
            "HyDE question prompt prepared"
        );

        let hyde_text = self.llm.generate(&prompt, &[], None).await.map_err(|e| {
            error!(
                error = %e,
                query = %query,
                "HyDE question generation LLM call failed"
            );
            AppError::Other(format!("HyDE generation failed: {}", e))
        })?;

        let mut hyde_text = compact_hyde_text(hyde_text.trim());
        if looks_like_direct_answer(&query, &hyde_text) {
            warn!(
                query = %query,
                hyde_preview = %safe_truncate(&hyde_text, 180),
                "HyDE question output resembled a direct answer; replacing with retrieval expansion fallback"
            );
            hyde_text = build_retrieval_fallback(&query);
        }
        if has_placeholder_artifacts(&hyde_text) {
            warn!(
                query = %query,
                hyde_preview = %safe_truncate(&hyde_text, 180),
                "HyDE question output contained unresolved placeholders; replacing with retrieval expansion fallback"
            );
            hyde_text = build_retrieval_fallback(&query);
        }

        if hyde_text.is_empty() {
            info!("LLM returned empty HyDE text, falling back to raw query");
            return Ok(HyDEInterpretation::raw_only(query, QueryType::Question));
        }

        info!(
            "Generated HyDE ({} chars) for question: {}",
            hyde_text.len(),
            &query
        );
        debug!(
            hyde_word_count = hyde_text.split_whitespace().count(),
            hyde_preview = %safe_truncate(&hyde_text, 240),
            "HyDE question output"
        );
        if hyde_text.trim().eq_ignore_ascii_case(query.trim()) {
            warn!(
                query = %query,
                "HyDE question output is identical to original query"
            );
        }

        Ok(HyDEInterpretation::for_question(query, hyde_text))
    }

    /// Generate command interpretation.
    async fn generate_command(&self, query: String) -> Result<HyDEInterpretation> {
        info!("Generating HyDE for command: {}", query);

        let prompt = COMMAND_HYDE_TEMPLATE.replace("{query}", &query);
        debug!(
            query_len = query.len(),
            prompt_len = prompt.len(),
            prompt_preview = %safe_truncate(&prompt, 240),
            "HyDE command prompt prepared"
        );

        let hyde_text = self.llm.generate(&prompt, &[], None).await.map_err(|e| {
            error!(
                error = %e,
                query = %query,
                "HyDE command generation LLM call failed"
            );
            AppError::Other(format!("HyDE generation failed: {}", e))
        })?;

        let mut hyde_text = compact_hyde_text(hyde_text.trim());
        if looks_like_direct_answer(&query, &hyde_text) {
            warn!(
                query = %query,
                hyde_preview = %safe_truncate(&hyde_text, 180),
                "HyDE followup output resembled a direct answer; replacing with retrieval expansion fallback"
            );
            hyde_text = build_retrieval_fallback(&query);
        }
        if has_placeholder_artifacts(&hyde_text) {
            warn!(
                query = %query,
                hyde_preview = %safe_truncate(&hyde_text, 180),
                "HyDE command output contained unresolved placeholders; replacing with retrieval expansion fallback"
            );
            hyde_text = build_retrieval_fallback(&query);
        }

        if hyde_text.is_empty() {
            info!("LLM returned empty HyDE text for command, falling back to raw query");
            return Ok(HyDEInterpretation::raw_only(query, QueryType::Command));
        }

        info!(
            "Generated HyDE ({} chars) for command: {}",
            hyde_text.len(),
            &query
        );
        debug!(
            hyde_word_count = hyde_text.split_whitespace().count(),
            hyde_preview = %safe_truncate(&hyde_text, 240),
            "HyDE command output"
        );
        if hyde_text.trim().eq_ignore_ascii_case(query.trim()) {
            warn!(
                query = %query,
                "HyDE command output is identical to original query"
            );
        }

        Ok(HyDEInterpretation::hybrid(
            query,
            hyde_text,
            QueryType::Command,
        ))
    }

    /// Generate interpretation for follow-up question.
    async fn generate_followup(
        &self,
        query: String,
        conversation_context: Option<&str>,
    ) -> Result<HyDEInterpretation> {
        info!("Generating HyDE for followup: {}", query);

        let prompt =
            if let Some(context) = conversation_context.filter(|ctx| !ctx.trim().is_empty()) {
                FOLLOWUP_HYDE_TEMPLATE
                    .replace("{context}", context)
                    .replace("{query}", &query)
            } else {
                QUESTION_HYDE_TEMPLATE.replace("{query}", &query)
            };
        debug!(
            query_len = query.len(),
            context_len = conversation_context.map(|ctx| ctx.len()).unwrap_or(0),
            prompt_len = prompt.len(),
            prompt_preview = %safe_truncate(&prompt, 240),
            "HyDE followup prompt prepared"
        );

        let hyde_text = self.llm.generate(&prompt, &[], None).await.map_err(|e| {
            error!(
                error = %e,
                query = %query,
                "HyDE followup generation LLM call failed"
            );
            AppError::Other(format!("HyDE generation failed: {}", e))
        })?;

        let mut hyde_text = compact_hyde_text(hyde_text.trim());
        if has_placeholder_artifacts(&hyde_text) {
            warn!(
                query = %query,
                hyde_preview = %safe_truncate(&hyde_text, 180),
                "HyDE followup output contained unresolved placeholders; replacing with retrieval expansion fallback"
            );
            hyde_text = build_retrieval_fallback(&query);
        }

        if hyde_text.is_empty() {
            info!("LLM returned empty HyDE text for followup, falling back to raw query");
            return Ok(HyDEInterpretation::raw_only(query, QueryType::Followup));
        }

        info!(
            "Generated HyDE ({} chars) for followup: {}",
            hyde_text.len(),
            &query
        );
        debug!(
            hyde_word_count = hyde_text.split_whitespace().count(),
            hyde_preview = %safe_truncate(&hyde_text, 240),
            "HyDE followup output"
        );
        if hyde_text.trim().eq_ignore_ascii_case(query.trim()) {
            warn!(
                query = %query,
                "HyDE followup output is identical to original query"
            );
        }

        Ok(HyDEInterpretation::hybrid(
            query,
            hyde_text,
            QueryType::Followup,
        ))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::qa::hyde::SearchStrategy;
    use async_trait::async_trait;
    use futures::stream::{self, Stream};
    use std::pin::Pin;

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
            async fn generate(
                &self,
                _prompt: &str,
                _context: &[String],
                _images: Option<Vec<String>>,
            ) -> Result<String> {
                unreachable!("PanicLLM is test-only mock - LLM should not be called for greetings")
            }

            async fn generate_streaming(
                &self,
                _prompt: &str,
                _context: &[String],
                _images: Option<Vec<String>>,
            ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
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
        let generator = HyDEGenerator::new(panic_llm);

        let result = generator
            .generate(QueryType::Greeting, "Hi!")
            .await
            .unwrap();
        assert_eq!(result.query_type, QueryType::Greeting);
    }
}
