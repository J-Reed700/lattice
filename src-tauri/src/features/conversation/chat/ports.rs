//! Dependencies the chat workflows are allowed to use. Composition lives in the feature DI adapter.
//!
//! Every dependency is a trait: the turn never sees a database pool, a concrete
//! web client or a concrete search use case, so a stage can be driven by fakes.
pub(crate) use super::dto::ExplorerFocusDto;
pub(crate) use super::ExplorerTurn;
use crate::application::contracts::search::{CorpusDocument, CorpusPassage};
use crate::application::ports::conversation_memory::{
    ConversationMemoryPort, ConversationMemoryReadPort,
};
use crate::application::ports::llm_port::OptionalLlmLoader;
use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::features::conversation::workspace_dto::ConversationWebSourceSnapshotDto;
pub(crate) use crate::features::function_calling::domain::{
    FunctionCall, FunctionResult, ToolDefinition,
};
pub(crate) use crate::features::function_calling::dto::FetchUrlContentOutput;
pub(crate) use crate::features::settings::dto::SettingsDto;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;

#[async_trait]
pub trait ChatModels: Send + Sync {
    async fn get_or_load_llm(&self) -> Result<Arc<dyn LLMPort>>;
    async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>>;
    /// Loads the utility model, or `None` when none is configured. Owned, so
    /// work that outlives the turn can load it later.
    fn utility_llm_loader(&self) -> OptionalLlmLoader;
    async fn get_or_load_embedding(&self) -> Result<Arc<dyn EmbeddingPort>>;
}

/// What a turn reads and writes about its own conversation beyond the
/// conversation service: the retrieval scope, archived and cited web pages,
/// the memory ledger and its recall index.
#[async_trait]
pub trait ChatRecords: ConversationMemoryPort + ConversationMemoryReadPort {
    /// The conversation's space and the documents a turn may read in it.
    async fn retrieval_document_scope(
        &self,
        conversation_id: &str,
    ) -> Result<Option<(String, HashSet<String>)>>;
    async fn retrieval_catalog(&self, allowed_ids: &HashSet<String>)
        -> Result<Vec<CorpusDocument>>;
    /// The opening passages of each selected document, within `allowed`.
    async fn retrieval_openings(
        &self,
        selected_ids: &[String],
        allowed_ids: &HashSet<String>,
        chunks_per_document: usize,
    ) -> Result<Vec<CorpusPassage>>;
    /// The passages of the named sections, within `allowed`.
    async fn retrieval_section_passages(
        &self,
        identifiers: &[String],
        allowed: &HashSet<String>,
    ) -> Result<Vec<CorpusPassage>>;
    /// Section and page of each chunk, keyed by chunk id.
    async fn retrieval_locations(
        &self,
        chunk_ids: &[String],
    ) -> Result<std::collections::HashMap<String, (Option<String>, Option<u32>)>>;
    /// Passages beside `chunk_id` in its section, within `allowed`.
    async fn retrieval_neighbors(
        &self,
        chunk_id: &str,
        allowed: &HashSet<String>,
    ) -> Result<Vec<CorpusPassage>>;
    async fn web_source_snapshot(
        &self,
        conversation_id: &str,
        url: &str,
    ) -> Result<Option<ConversationWebSourceSnapshotDto>>;
    async fn store_web_source_snapshot(
        &self,
        conversation_id: &str,
        url: &str,
        title: Option<String>,
        content: String,
        truncated: bool,
    ) -> Result<()>;
    /// Keep a page an answer cited as the conversation's own context.
    async fn add_cited_web_source(
        &self,
        conversation_id: &str,
        url: String,
        title: Option<String>,
        excerpt: Option<String>,
        score: Option<f32>,
    ) -> Result<()>;
    async fn persist_memory_vector(&self, vector: MemoryVector<'_>) -> Result<u64>;
    /// The same records, recalling by meaning as well as by words.
    fn with_recall_embedding(
        &self,
        embedding: Option<Arc<dyn EmbeddingPort>>,
    ) -> Arc<dyn ChatRecords>;
}

/// One message's embedding, as the memory index stores it.
pub struct MemoryVector<'a> {
    pub conversation_id: &'a str,
    pub vector_id: &'a str,
    pub message_id: &'a str,
    pub role: &'a str,
    pub content: &'a str,
    pub embedding: Vec<u8>,
    pub dimension: i64,
    pub embedding_model: &'a str,
    pub created_at: &'a str,
}

#[async_trait]
pub trait ChatStorage: Send + Sync {
    fn conversation_service(
        &self,
    ) -> Arc<dyn crate::features::conversation::ConversationServiceTrait>;
    fn conversation_history(&self) -> Arc<dyn crate::application::ports::ConversationHistoryPort>;
    fn conversation_context(
        &self,
    ) -> Arc<dyn crate::application::ports::conversation_context::ConversationContextPort>;
    fn document_scope(
        &self,
    ) -> Arc<dyn crate::application::ports::document_scope::DocumentScopePort>;
    fn document_repository(&self) -> Arc<dyn crate::application::ports::DocumentRepository>;
    fn chunk_repository(&self) -> Arc<dyn crate::application::ports::ChunkRepositoryPort>;
    fn chat_records(&self) -> Arc<dyn ChatRecords>;
    /// The folder an Explorer conversation reads beside the chat, with the
    /// open file the request names checked against it; `None` for every
    /// other conversation.
    async fn explorer_turn(
        &self,
        conversation_id: &str,
        focus: Option<&ExplorerFocusDto>,
    ) -> Option<ExplorerTurn>;
}

/// The tools a model may call: what is registered, and running one.
#[async_trait]
pub trait ChatTools: Send + Sync {
    fn list_tools(&self) -> Vec<ToolDefinition>;
    async fn execute(&self, call: FunctionCall) -> Result<FunctionResult>;
}

/// Reads a web page live, under the network policy every read obeys.
#[async_trait]
pub trait PageReader: Send + Sync {
    async fn read_page(&self, url: &str) -> Result<FetchUrlContentOutput>;
}

#[async_trait]
pub trait ChatRetrieval: Send + Sync {
    fn tools(&self) -> Arc<dyn ChatTools>;
    fn library_search(&self) -> Arc<dyn crate::features::search::trait_def::LibrarySearchTrait>;
    fn page_reader(&self) -> Arc<dyn PageReader>;
    async fn summary_search(
        &self,
    ) -> Option<Arc<dyn crate::features::summaries::search::SummarySearchPort>>;
}

#[async_trait]
pub trait ChatPolicy: Send + Sync {
    async fn settings(&self) -> Result<SettingsDto>;
    async fn validate_message(&self, message: &str) -> Result<String>;
    async fn create_conversation(
        &self,
        request: crate::features::conversation::dto::CreateConversationRequestDto,
    ) -> Result<crate::features::conversation::dto::CreateConversationResponseDto>;
    /// `cancel` is the turn's stop button: the user is waiting on this
    /// compaction, so stopping the turn stops it too.
    async fn compact_for_turn(
        &self,
        id: &str,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<()>;
    fn consolidate_after_turn(&self, id: String);
}

/// Where work that outlives a request runs: a deep research turn is a job.
pub trait ChatJobs: Send + Sync {
    fn jobs(&self) -> Arc<crate::shared::runtime::jobs::JobRuntime>;
}

pub trait ChatRuntime: ChatModels + ChatStorage + ChatRetrieval + ChatPolicy + ChatJobs {
    fn share(&self) -> Arc<dyn ChatRuntime>;
}
