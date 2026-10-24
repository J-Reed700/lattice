//! HyDE (Hypothetical Document Embeddings) domain models for question answering.

use serde::{Deserialize, Serialize};

/// Type of user query for appropriate handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryType {
    Greeting,
    Question,
    Followup,
    Command,
}

impl std::fmt::Display for QueryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryType::Greeting => write!(f, "Greeting"),
            QueryType::Question => write!(f, "Question"),
            QueryType::Followup => write!(f, "Followup"),
            QueryType::Command => write!(f, "Command"),
        }
    }
}

/// Strategy for document search based on query type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchStrategy {
    HyDEOnly,
    RawOnly,
    Hybrid,
}

impl std::fmt::Display for SearchStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchStrategy::HyDEOnly => write!(f, "HyDE Only"),
            SearchStrategy::RawOnly => write!(f, "Raw Only"),
            SearchStrategy::Hybrid => write!(f, "Hybrid"),
        }
    }
}

/// Intended retrieval scope for answering a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ToolIntent {
    None,
    VaultOnly,
    WebOnly,
    #[default]
    VaultThenWeb,
}

impl ToolIntent {
    pub fn default_for_query_type(query_type: QueryType) -> Self {
        match query_type {
            QueryType::Greeting => ToolIntent::None,
            QueryType::Command => ToolIntent::VaultOnly,
            QueryType::Question | QueryType::Followup => ToolIntent::VaultThenWeb,
        }
    }
}

impl std::fmt::Display for ToolIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolIntent::None => write!(f, "None"),
            ToolIntent::VaultOnly => write!(f, "VaultOnly"),
            ToolIntent::WebOnly => write!(f, "WebOnly"),
            ToolIntent::VaultThenWeb => write!(f, "VaultThenWeb"),
        }
    }
}

/// Interpreted query with HyDE enrichment and search strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyDEInterpretation {
    pub query_type: QueryType,
    pub original_query: String,
    pub hyde_text: Option<String>,
    pub search_strategy: SearchStrategy,
    #[serde(default)]
    pub tool_intent: ToolIntent,
}

impl HyDEInterpretation {
    /// Create interpretation for a greeting (no search needed).
    pub fn for_greeting(query: impl Into<String>) -> Self {
        Self {
            query_type: QueryType::Greeting,
            original_query: query.into(),
            hyde_text: None,
            search_strategy: SearchStrategy::RawOnly,
            tool_intent: ToolIntent::None,
        }
    }

    /// Create interpretation for a question with HyDE text.
    pub fn for_question(query: impl Into<String>, hyde_text: impl Into<String>) -> Self {
        Self {
            query_type: QueryType::Question,
            original_query: query.into(),
            hyde_text: Some(hyde_text.into()),
            search_strategy: SearchStrategy::HyDEOnly,
            tool_intent: ToolIntent::default_for_query_type(QueryType::Question),
        }
    }

    /// Create interpretation using only raw query (no HyDE).
    pub fn raw_only(query: impl Into<String>, query_type: QueryType) -> Self {
        Self {
            query_type,
            original_query: query.into(),
            hyde_text: None,
            search_strategy: SearchStrategy::RawOnly,
            tool_intent: ToolIntent::default_for_query_type(query_type),
        }
    }

    /// Create interpretation with hybrid search strategy.
    pub fn hybrid(
        query: impl Into<String>,
        hyde_text: impl Into<String>,
        query_type: QueryType,
    ) -> Self {
        Self {
            query_type,
            original_query: query.into(),
            hyde_text: Some(hyde_text.into()),
            search_strategy: SearchStrategy::Hybrid,
            tool_intent: ToolIntent::default_for_query_type(query_type),
        }
    }

    /// Get the text(s) to use for searching based on strategy.
    pub fn search_text(&self) -> Vec<&str> {
        match self.search_strategy {
            SearchStrategy::HyDEOnly => self
                .hyde_text
                .as_ref()
                .map(|s| vec![s.as_str()])
                .unwrap_or_default(),
            SearchStrategy::RawOnly => vec![self.original_query.as_str()],
            SearchStrategy::Hybrid => {
                let mut texts = vec![self.original_query.as_str()];
                if let Some(hyde) = &self.hyde_text {
                    texts.push(hyde.as_str());
                }
                texts
            }
        }
    }
}

/// Context enriched with retrieved documents for answering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedContext {
    pub interpretation: HyDEInterpretation,
    pub documents: Vec<DocumentChunk>,
    pub total_searched: usize,
}

impl EnrichedContext {
    pub fn new(interpretation: HyDEInterpretation) -> Self {
        Self {
            interpretation,
            documents: Vec::new(),
            total_searched: 0,
        }
    }

    pub fn with_document(mut self, document: DocumentChunk) -> Self {
        self.documents.push(document);
        self
    }

    pub fn with_documents(mut self, documents: Vec<DocumentChunk>) -> Self {
        self.documents.extend(documents);
        self
    }

    pub fn with_total_searched(mut self, total: usize) -> Self {
        self.total_searched = total;
        self
    }

    pub fn has_documents(&self) -> bool {
        !self.documents.is_empty()
    }

    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// Formats retrieved documents into a string for inclusion in an LLM prompt.
    pub fn format_for_prompt(&self) -> String {
        if self.documents.is_empty() {
            return String::new();
        }

        let mut context = String::from("Retrieved documents:\n\n");
        for (i, doc) in self.documents.iter().enumerate() {
            context.push_str(&format!(
                "[{}] File: {}\nContent: {}\n\n",
                i + 1,
                doc.file_path,
                doc.content
            ));
        }
        context
    }
}

/// Retrieved document chunk with relevance score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub content: String,
    pub file_path: String,
    pub similarity_score: f32,
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub metadata: Option<ChunkMetadata>,
}

impl DocumentChunk {
    pub fn new(
        content: impl Into<String>,
        file_path: impl Into<String>,
        similarity_score: f32,
    ) -> Self {
        Self {
            content: content.into(),
            file_path: file_path.into(),
            similarity_score,
            chunk_index: 0,
            total_chunks: 1,
            metadata: None,
        }
    }

    pub fn with_position(mut self, index: usize, total: usize) -> Self {
        self.chunk_index = index;
        self.total_chunks = total;
        self
    }

    pub fn with_metadata(mut self, metadata: ChunkMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Metadata about a document chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub date: Option<String>,
    pub author: Option<String>,
}

impl ChunkMetadata {
    pub fn empty() -> Self {
        Self {
            title: None,
            tags: Vec::new(),
            date: None,
            author: None,
        }
    }

    pub fn with_title(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            tags: Vec::new(),
            date: None,
            author: None,
        }
    }
}

/// Response from the chat/Q&A system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub answer: String,
    pub sources: Vec<Source>,
    pub metadata: ResponseMetadata,
}

impl ChatResponse {
    pub fn simple(answer: impl Into<String>) -> Self {
        Self {
            answer: answer.into(),
            sources: Vec::new(),
            metadata: ResponseMetadata::default(),
        }
    }

    pub fn with_sources(mut self, sources: Vec<Source>) -> Self {
        self.sources = sources;
        self
    }

    pub fn with_metadata(mut self, metadata: ResponseMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Source document reference that contributed to an answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub file_path: String,
    pub relevance_score: f32,
    pub excerpt: Option<String>,
    pub chunk_index: usize,
}

impl Source {
    pub fn new(file_path: impl Into<String>, relevance_score: f32, chunk_index: usize) -> Self {
        Self {
            file_path: file_path.into(),
            relevance_score,
            excerpt: None,
            chunk_index,
        }
    }

    pub fn with_excerpt(mut self, excerpt: impl Into<String>) -> Self {
        self.excerpt = Some(excerpt.into());
        self
    }
}

/// Metadata about response generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMetadata {
    pub query_type: QueryType,
    pub search_strategy: SearchStrategy,
    pub documents_retrieved: usize,
    pub used_hyde: bool,
    pub processing_time_ms: Option<u64>,
}

impl Default for ResponseMetadata {
    fn default() -> Self {
        Self {
            query_type: QueryType::Question,
            search_strategy: SearchStrategy::HyDEOnly,
            documents_retrieved: 0,
            used_hyde: false,
            processing_time_ms: None,
        }
    }
}

impl ResponseMetadata {
    pub fn from_context(interpretation: &HyDEInterpretation, context: &EnrichedContext) -> Self {
        Self {
            query_type: interpretation.query_type,
            search_strategy: interpretation.search_strategy,
            documents_retrieved: context.document_count(),
            used_hyde: interpretation.hyde_text.is_some(),
            processing_time_ms: None,
        }
    }

    pub fn with_processing_time(mut self, time_ms: u64) -> Self {
        self.processing_time_ms = Some(time_ms);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_type_display() {
        assert_eq!(QueryType::Greeting.to_string(), "Greeting");
        assert_eq!(QueryType::Question.to_string(), "Question");
        assert_eq!(QueryType::Followup.to_string(), "Followup");
        assert_eq!(QueryType::Command.to_string(), "Command");
    }

    #[test]
    fn test_search_strategy_display() {
        assert_eq!(SearchStrategy::HyDEOnly.to_string(), "HyDE Only");
        assert_eq!(SearchStrategy::RawOnly.to_string(), "Raw Only");
        assert_eq!(SearchStrategy::Hybrid.to_string(), "Hybrid");
    }

    #[test]
    fn test_interpretation_for_greeting() {
        let greeting = HyDEInterpretation::for_greeting("Hello!");
        assert_eq!(greeting.query_type, QueryType::Greeting);
        assert_eq!(greeting.original_query, "Hello!");
        assert!(greeting.hyde_text.is_none());
        assert_eq!(greeting.search_strategy, SearchStrategy::RawOnly);
    }

    #[test]
    fn test_interpretation_for_question() {
        let question = HyDEInterpretation::for_question("What is Rust?", "Rust is...");
        assert_eq!(question.query_type, QueryType::Question);
        assert_eq!(question.original_query, "What is Rust?");
        assert_eq!(question.hyde_text, Some("Rust is...".to_string()));
        assert_eq!(question.search_strategy, SearchStrategy::HyDEOnly);
    }

    #[test]
    fn test_interpretation_search_text_hyde_only() {
        let interpretation = HyDEInterpretation {
            query_type: QueryType::Question,
            original_query: "original".to_string(),
            hyde_text: Some("hyde".to_string()),
            search_strategy: SearchStrategy::HyDEOnly,
            tool_intent: ToolIntent::default_for_query_type(QueryType::Question),
        };
        let search_texts = interpretation.search_text();
        assert_eq!(search_texts, vec!["hyde"]);
    }

    #[test]
    fn test_interpretation_search_text_raw_only() {
        let interpretation = HyDEInterpretation {
            query_type: QueryType::Greeting,
            original_query: "Hello".to_string(),
            hyde_text: None,
            search_strategy: SearchStrategy::RawOnly,
            tool_intent: ToolIntent::None,
        };
        let search_texts = interpretation.search_text();
        assert_eq!(search_texts, vec!["Hello"]);
    }

    #[test]
    fn test_interpretation_search_text_hybrid() {
        let interpretation = HyDEInterpretation {
            query_type: QueryType::Question,
            original_query: "original".to_string(),
            hyde_text: Some("hyde".to_string()),
            search_strategy: SearchStrategy::Hybrid,
            tool_intent: ToolIntent::default_for_query_type(QueryType::Question),
        };
        let search_texts = interpretation.search_text();
        assert_eq!(search_texts, vec!["original", "hyde"]);
    }

    #[test]
    fn test_enriched_context_builder() {
        let interpretation = HyDEInterpretation::for_greeting("Hi");
        let doc = DocumentChunk::new("content", "/path/doc.txt", 0.95);

        let context = EnrichedContext::new(interpretation)
            .with_document(doc)
            .with_total_searched(100);

        assert_eq!(context.documents.len(), 1);
        assert_eq!(context.total_searched, 100);
        assert!(context.has_documents());
    }

    #[test]
    fn test_enriched_context_format_for_prompt() {
        let interpretation = HyDEInterpretation::for_greeting("Hi");
        let doc = DocumentChunk::new("Test content", "/test.txt", 0.9);
        let context = EnrichedContext::new(interpretation).with_document(doc);

        let formatted = context.format_for_prompt();
        assert!(formatted.contains("Retrieved documents:"));
        assert!(formatted.contains("/test.txt"));
        assert!(formatted.contains("Test content"));
    }

    #[test]
    fn test_enriched_context_empty_format() {
        let interpretation = HyDEInterpretation::for_greeting("Hi");
        let context = EnrichedContext::new(interpretation);

        let formatted = context.format_for_prompt();
        assert!(formatted.is_empty());
    }

    #[test]
    fn test_document_chunk_builder() {
        let chunk = DocumentChunk::new("content", "/path.txt", 0.85)
            .with_position(2, 5)
            .with_metadata(ChunkMetadata::with_title("Test Doc"));

        assert_eq!(chunk.content, "content");
        assert_eq!(chunk.similarity_score, 0.85);
        assert_eq!(chunk.chunk_index, 2);
        assert_eq!(chunk.total_chunks, 5);
        assert!(chunk.metadata.is_some());
    }

    #[test]
    fn test_chat_response_builder() {
        let response = ChatResponse::simple("Answer here")
            .with_sources(vec![Source::new("/doc.txt", 0.9, 0)])
            .with_metadata(ResponseMetadata::default());

        assert_eq!(response.answer, "Answer here");
        assert_eq!(response.sources.len(), 1);
    }

    #[test]
    fn test_source_with_excerpt() {
        let source = Source::new("/doc.txt", 0.95, 0).with_excerpt("This is an excerpt");

        assert_eq!(source.file_path, "/doc.txt");
        assert_eq!(source.relevance_score, 0.95);
        assert_eq!(source.excerpt, Some("This is an excerpt".to_string()));
    }

    #[test]
    fn test_response_metadata_from_context() {
        let interpretation = HyDEInterpretation::for_question("What?", "Answer...");
        let doc = DocumentChunk::new("content", "/doc.txt", 0.9);
        let context = EnrichedContext::new(interpretation.clone()).with_document(doc);

        let metadata = ResponseMetadata::from_context(&interpretation, &context);

        assert_eq!(metadata.query_type, QueryType::Question);
        assert_eq!(metadata.search_strategy, SearchStrategy::HyDEOnly);
        assert_eq!(metadata.documents_retrieved, 1);
        assert!(metadata.used_hyde);
    }

    #[test]
    fn test_chunk_metadata_builder() {
        let metadata = ChunkMetadata::with_title("My Document");
        assert_eq!(metadata.title, Some("My Document".to_string()));
        assert!(metadata.tags.is_empty());

        let empty = ChunkMetadata::empty();
        assert!(empty.title.is_none());
    }
}
