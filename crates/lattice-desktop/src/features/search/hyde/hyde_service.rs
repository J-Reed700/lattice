//! HyDE Service Orchestrator
//!
//! Coordinates query classification and HyDE generation.

use crate::features::search::hyde::hyde_generator::HyDEGenerator;
use crate::features::search::hyde::query_classifier::QueryClassifier;
use lattice_core::application::ports::query_interpreter::QueryInterpreter;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::qa::hyde::{HyDEInterpretation, QueryType};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::select_informative_terms;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info};

/// Orchestrates HyDE query interpretation pipeline.
pub struct HyDEService {
    classifier: QueryClassifier,
    generator: HyDEGenerator,
}

impl HyDEService {
    /// Create a new HyDE service with LLM backend.
    pub fn new(llm: Arc<dyn LLMPort>) -> Self {
        Self {
            classifier: QueryClassifier::new(),
            generator: HyDEGenerator::new(llm),
        }
    }

    /// Stop every model call this service makes when `cancel` fires: a stop
    /// press takes a rewrite out of the backend's queue instead of leaving it
    /// to hold the slot the next turn needs.
    pub fn with_cancellation(mut self, cancel: tokio_util::sync::CancellationToken) -> Self {
        self.generator = self.generator.with_cancellation(cancel);
        self
    }

    /// Interpret a user query with HyDE enrichment.
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

    /// Further searches on the subject of `root` for deep research, each from
    /// a different angle. See [`HyDEGenerator::generate_research_followups`].
    pub async fn generate_research_followups_with_context(
        &self,
        root: &str,
        query: &str,
        conversation_context: Option<&str>,
        count: usize,
    ) -> Result<Vec<String>> {
        self.generator
            .generate_research_followups(root, query, conversation_context, count)
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
    pub async fn interpret_query_with_type(
        &self,
        query: impl Into<String>,
        query_type: lattice_core::domain::qa::hyde::QueryType,
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

#[async_trait::async_trait]
impl QueryInterpreter for HyDEService {
    async fn interpret_with_context(
        &self,
        query: &str,
        conversation_context: Option<&str>,
    ) -> Result<HyDEInterpretation> {
        self.interpret_query_with_context(query, conversation_context)
            .await
    }

    async fn classify_with_context(
        &self,
        query: &str,
        conversation_context: Option<&str>,
    ) -> Result<QueryType> {
        self.classify_query_with_context(query, conversation_context)
            .await
    }

    async fn web_search_query_with_context(
        &self,
        query: &str,
        conversation_context: Option<&str>,
    ) -> Result<String> {
        self.generate_web_search_query_with_context(query, conversation_context)
            .await
    }

    async fn research_followups_with_context(
        &self,
        root: &str,
        query: &str,
        conversation_context: Option<&str>,
        count: usize,
    ) -> Result<Vec<String>> {
        self.generate_research_followups_with_context(root, query, conversation_context, count)
            .await
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
mod tests;
