//! Dependencies the chat workflows are allowed to use. Composition lives in the feature DI adapter.
//!
//! Every dependency is a trait: the turn never sees a database pool, a concrete
//! web client or a concrete search use case, so a stage can be driven by fakes.
pub(crate) use super::dto::ExplorerFocusDto;
pub(crate) use super::ExplorerTurn;
use crate::application::contracts::search::{CorpusDocument, CorpusPassage};
pub(crate) use crate::application::contracts::web::FetchUrlContentOutput;
use crate::application::ports::conversation_memory::{
    ConversationMemoryPort, ConversationMemoryReadPort,
};
use crate::application::ports::llm_port::OptionalLlmLoader;
use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::features::conversation::workspace_dto::ConversationWebSourceSnapshotDto;
pub(crate) use crate::features::function_calling::domain::{
    FunctionCall, FunctionResult, ToolDefinition,
};
pub(crate) use crate::features::settings::dto::SettingsDto;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;

#[async_trait]
pub trait ChatModels: Send + Sync {
    async fn get_or_load_llm(&self) -> Result<Arc<dyn LLMPort>>;
    async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>>;
    /// The utility model, or `None` when none is configured. Loaders exist so
    /// work that outlives the turn can load it later; this is the eager form
    /// for callers that need the model now.
    async fn get_or_load_utility_llm(&self) -> Result<Option<Arc<dyn LLMPort>>>;
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
    /// Section, page and OCR flag of each chunk, keyed by chunk id.
    async fn retrieval_locations(
        &self,
        chunk_ids: &[String],
    ) -> Result<std::collections::HashMap<String, (Option<String>, Option<u32>, bool)>>;
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
    /// The memory ledger: what each conversation has already established.
    fn conversation_memory(
        &self,
    ) -> Arc<dyn crate::application::ports::conversation_memory::ConversationMemoryPort>;
    /// Ask for a frozen copy of a cited page (the Page view): saves its row and
    /// queues its capture job, nothing more. Failures are logged, never
    /// returned. Test runtimes keep this no-op.
    async fn ensure_page_snapshot(&self, conversation_id: &str, url: &str) {
        let _ = (conversation_id, url);
    }
    /// The per-conversation single-flight slots that keep one conversation
    /// from compacting twice at once. One per process, by construction.
    fn compaction_slots(&self) -> Arc<crate::features::conversation::memory::CompactionSlots>;
    /// The conversations a continue-in-new-chat handoff is summarizing.
    fn handoffs_in_flight(&self) -> Arc<crate::features::conversation::handoff::HandoffsInFlight>;
    /// Tangents, forks, truncation and the regenerate lift: the writes a
    /// conversation can branch with.
    fn conversation_branches(&self) -> Arc<dyn ConversationBranches>;
    /// The corpus-derived starter workflow: the space's document scope and the
    /// fingerprint cache with its aggregate reads.
    fn chat_starter_cache(&self) -> Arc<dyn ChatStarterCache>;
    /// The folder an Explorer conversation reads beside the chat, with the
    /// open file the request names checked against it; `None` for every
    /// other conversation.
    async fn explorer_turn(
        &self,
        conversation_id: &str,
        focus: Option<&ExplorerFocusDto>,
    ) -> Option<ExplorerTurn>;
}

/// The writes a conversation can branch with: side threads (tangents), forks,
/// truncation, and the regenerate lift.
#[async_trait]
pub trait ConversationBranches: Send + Sync {
    /// Open a side thread off this conversation.
    async fn create_tangent(
        &self,
        request: crate::features::conversation::tangent_dto::CreateTangentRequestDto,
    ) -> Result<crate::features::conversation::tangent_dto::ConversationTangentDto>;
    /// The side threads of this conversation.
    async fn list_tangents(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<crate::features::conversation::tangent_dto::ConversationTangentDto>>;
    /// Promote a side thread to a full conversation.
    async fn promote_tangent(&self, conversation_id: &str) -> Result<()>;
    /// A fresh conversation in the same space whose only message stands in
    /// for the summarized old thread.
    async fn create_continuation(
        &self,
        conversation_id: &str,
        new_id: &str,
        new_title: &str,
        summary: &str,
        metadata: &str,
    ) -> Result<()>;
    /// A conversation by id, or `None` when it is gone.
    async fn find_by_id(
        &self,
        id: &str,
    ) -> Result<Option<crate::domain::conversation::Conversation>>;
    /// The conversation as the client sees it.
    async fn project_conversation(
        &self,
        conversation: &crate::domain::conversation::Conversation,
    ) -> Result<crate::features::conversation::dto::ConversationDto>;
    /// Drop everything after `message_id` (or inclusive of it).
    async fn truncate_after(
        &self,
        conversation_id: &str,
        message_id: &str,
        inclusive: bool,
    ) -> Result<u64>;
    /// Every message in the thread, in order.
    async fn get_messages(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<crate::domain::conversation::ConversationMessage>>;
    /// A branch of the conversation, up to an optional message.
    async fn fork(
        &self,
        conversation_id: &str,
        up_to_message_id: Option<&str>,
        new_id: &str,
        new_title: &str,
    ) -> Result<(String, u32)>;
    /// Lift the last user turn off the thread so a regenerate can answer it
    /// again.
    async fn take_last_user_turn(
        &self,
        conversation_id: &str,
    ) -> Result<Option<(String, i64, Option<String>)>>;
    /// A message with an explicit status, for the failed-question restore.
    async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: crate::domain::conversation::MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&str>,
        status: &str,
    ) -> Result<crate::domain::conversation::ConversationMessage>;
}

/// The reads and writes the corpus-derived chat-starter workflow needs: the
/// space's document scope and the fingerprint cache with its aggregate reads.
#[async_trait]
pub trait ChatStarterCache: Send + Sync {
    /// The documents this space can see — the only ones questions may be about.
    async fn space_document_scope(&self, space_id: &str) -> Result<HashSet<String>>;
    /// The cached `(starters_json, created_at)` for a fingerprint, if any.
    async fn get(&self, fingerprint: &str) -> Result<Option<(String, String)>>;
    /// Upsert one fingerprint's starters.
    async fn put(&self, fingerprint: &str, starters_json: &str, created_at: &str) -> Result<()>;
    /// Keep only the newest `keep` rows.
    async fn prune(&self, keep: i64) -> Result<()>;
    /// `(file_type, count)` over `allowed_ids`, most common first.
    async fn type_mix(&self, allowed_ids: &HashSet<String>) -> Result<Vec<(String, i64)>>;
    /// The newest document titles in `allowed_ids`.
    async fn recent_titles(
        &self,
        allowed_ids: &HashSet<String>,
        limit: usize,
    ) -> Result<Vec<String>>;
    /// Newest `indexed_at` in `allowed_ids`, or an empty string when there is none.
    async fn latest_indexed_at(&self, allowed_ids: &HashSet<String>) -> Result<String>;
}

/// A journal page exactly as stored, before its archived citations are
/// recovered. The synthesis chooses pages on `title` and `content` alone; the
/// row goes back to [`SynthesisEntries::hydrate_note`] unchanged, so a page it
/// passes over is never parsed or written.
#[derive(Debug, Clone)]
pub struct StoredJournalPage {
    pub revision: i64,
    pub id: String,
    pub title: String,
    pub journal_id: Option<String>,
    pub content: String,
    pub linked_document_ids: String,
    pub linked_conversation_ids: String,
    pub highlights_json: String,
    pub sticky_notes_json: String,
    pub conversation_snapshots_json: String,
    pub sources_json: String,
    pub created_at: String,
    pub updated_at: String,
}

/// The entries a "week" journal synthesis reads beyond conversations: the
/// saved passages and journal pages it combines, as the app stores them.
#[async_trait]
pub trait SynthesisEntries: Send + Sync {
    /// Passages saved since `cutoff`, newest first.
    async fn references_created_since(
        &self,
        cutoff: &str,
        limit: i64,
    ) -> Result<Vec<crate::features::references::repository::PassageReferenceRecord>>;
    /// Journal pages updated between `start_date` and `end_date`, as stored.
    async fn notes_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<StoredJournalPage>>;
    /// One page the synthesis chose, with its archived citations recovered.
    /// Recovery persists what it finds, so this may write the page back.
    async fn hydrate_note(
        &self,
        page: StoredJournalPage,
    ) -> Result<crate::features::daily_notes::dto::WorkspaceNoteDto>;
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
    /// How much prompt the corpus planner can take in time, learned across
    /// every turn this runtime runs — and never shared with a test's runtime.
    fn planner_health(&self) -> Arc<crate::features::conversation::chat::retrieval::PlannerHealth>;
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
    /// The rate limit a conversation read (list/get) spends before it runs.
    async fn check_read_rate_limit(&self, name: &str) -> Result<()>;
    /// The rate limit a user-initiated delete spends.
    async fn check_delete_rate_limit(&self, name: &str) -> Result<()>;
    /// Remove a conversation and everything it owns — its in-flight turn, its
    /// attachments, its messages — without spending the user delete's rate
    /// limit.
    async fn delete_conversation_unmetered(&self, conversation_id: &str) -> Result<()>;
}

/// Where work that outlives a request runs: a deep research turn is a job.
pub trait ChatJobs: Send + Sync {
    fn jobs(&self) -> Arc<crate::shared::runtime::jobs::JobRuntime>;
    /// The process-wide map of commands waiting on their research turns, so
    /// the job that runs a turn answers the command that sent it — and a test
    /// process never answers the app's commands.
    fn research_waiters(&self) -> Arc<crate::features::conversation::chat::ResearchWaiters>;
    /// The app's background scope: work a turn leaves running after it
    /// answers — memory indexing, consolidation, the grounding check — runs
    /// in it, so shutdown stops and joins it.
    fn background(&self) -> Arc<crate::shared::runtime::background::BackgroundTasks>;
}

/// The stop button: the process-wide registry of in-flight turns, so a stop
/// press reaches the same map the turn it targets registered with — and a test
/// process never shares armed cancellations with the app.
pub trait ChatCancellation: Send + Sync {
    fn turn_cancellation(
        &self,
    ) -> Arc<crate::features::conversation::chat::cancellation::TurnCancellationRegistry>;
}

pub trait ChatRuntime:
    ChatModels
    + ChatStorage
    + ChatRetrieval
    + ChatPolicy
    + ChatJobs
    + SynthesisEntries
    + ChatCancellation
{
    fn share(&self) -> Arc<dyn ChatRuntime>;
}
