//! The narrow runtime a chat test needs, and nothing behind it: no database,
//! no model and no container unless the test hands one in. A dependency the
//! test did not provide fails the test if it is reached.
use super::ports::*;
use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::features::conversation::mocks::MockConversationService;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::function_calling::domain::{FunctionCall, FunctionResult, ToolDefinition};
use crate::shared::error::{AppError, Result};
use crate::shared::runtime::jobs::JobRuntime;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const UNUSED: &str = "not provided to the test";

#[derive(Clone)]
pub(super) struct FakeRuntime {
    pub(super) llm: Option<Arc<dyn LLMPort>>,
    pub(super) tools: Option<Arc<dyn ChatTools>>,
    pub(super) jobs: Option<Arc<JobRuntime>>,
    /// One service for the whole test, so what a turn saves can be read back.
    pub(super) conversations: Arc<MockConversationService>,
}

impl Default for FakeRuntime {
    fn default() -> Self {
        Self {
            llm: None,
            tools: None,
            jobs: None,
            conversations: Arc::new(MockConversationService::new()),
        }
    }
}

#[async_trait::async_trait]
impl ChatModels for FakeRuntime {
    async fn get_or_load_llm(&self) -> Result<Arc<dyn LLMPort>> {
        Ok(Arc::clone(self.llm.as_ref().expect(UNUSED)))
    }
    async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>> {
        unreachable!("{UNUSED}")
    }
    fn utility_llm_loader(&self) -> crate::application::ports::llm_port::OptionalLlmLoader {
        Arc::new(|| Box::pin(async { Ok(None) }))
    }
    async fn get_or_load_embedding(&self) -> Result<Arc<dyn EmbeddingPort>> {
        Err(AppError::ServiceNotAvailable("no embedding model".into()))
    }
}

#[async_trait::async_trait]
impl ChatStorage for FakeRuntime {
    fn conversation_service(
        &self,
    ) -> Arc<dyn crate::features::conversation::ConversationServiceTrait> {
        self.conversations.clone()
    }
    fn conversation_history(&self) -> Arc<dyn crate::application::ports::ConversationHistoryPort> {
        unreachable!("{UNUSED}")
    }
    fn conversation_context(
        &self,
    ) -> Arc<dyn crate::application::ports::conversation_context::ConversationContextPort> {
        unreachable!("{UNUSED}")
    }
    fn document_scope(
        &self,
    ) -> Arc<dyn crate::application::ports::document_scope::DocumentScopePort> {
        unreachable!("{UNUSED}")
    }
    fn document_repository(&self) -> Arc<dyn crate::application::ports::DocumentRepository> {
        unreachable!("{UNUSED}")
    }
    fn chunk_repository(&self) -> Arc<dyn crate::application::ports::ChunkRepositoryPort> {
        unreachable!("{UNUSED}")
    }
    fn chat_records(&self) -> Arc<dyn ChatRecords> {
        // Never queried unless the model calls a history tool.
        Arc::new(ConversationRepository::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .unwrap(),
        ))
    }
    async fn explorer_turn(
        &self,
        _conversation_id: &str,
        _focus: Option<&crate::features::explorer::dto::ExplorerFocusDto>,
    ) -> Option<crate::features::explorer::prompt::ExplorerTurn> {
        None
    }
}

struct NoTools;

#[async_trait::async_trait]
impl ChatTools for NoTools {
    fn list_tools(&self) -> Vec<ToolDefinition> {
        Vec::new()
    }
    async fn execute(&self, _call: FunctionCall) -> Result<FunctionResult> {
        unreachable!("{UNUSED}")
    }
}

#[async_trait::async_trait]
impl ChatRetrieval for FakeRuntime {
    fn tools(&self) -> Arc<dyn ChatTools> {
        self.tools.clone().unwrap_or_else(|| Arc::new(NoTools))
    }
    fn library_search(&self) -> Arc<dyn crate::features::search::trait_def::LibrarySearchTrait> {
        unreachable!("{UNUSED}")
    }
    fn page_reader(&self) -> Arc<dyn PageReader> {
        unreachable!("{UNUSED}")
    }
    async fn summary_search(
        &self,
    ) -> Option<Arc<dyn crate::features::summaries::search::SummarySearchPort>> {
        None
    }
}

#[async_trait::async_trait]
impl ChatPolicy for FakeRuntime {
    /// The defaults, without the grounding check: it runs after the answer
    /// and would race what a test reads back.
    async fn settings(&self) -> Result<crate::features::settings::dto::SettingsDto> {
        let mut settings = crate::features::settings::dto::SettingsDto::default();
        settings.llm.verification.enabled = false;
        Ok(settings)
    }
    async fn validate_message(&self, message: &str) -> Result<String> {
        Ok(message.to_string())
    }
    async fn create_conversation(
        &self,
        _request: crate::features::conversation::dto::CreateConversationRequestDto,
    ) -> Result<crate::features::conversation::dto::CreateConversationResponseDto> {
        unreachable!("{UNUSED}")
    }
    async fn compact_for_turn(
        &self,
        _id: &str,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        unreachable!("{UNUSED}")
    }
    fn consolidate_after_turn(&self, _id: String) {}
}

impl ChatJobs for FakeRuntime {
    fn jobs(&self) -> Arc<JobRuntime> {
        Arc::clone(self.jobs.as_ref().expect(UNUSED))
    }
}

impl ChatRuntime for FakeRuntime {
    fn share(&self) -> Arc<dyn ChatRuntime> {
        Arc::new(self.clone())
    }
}

/// A model that answers from a script, in order, and keeps every request.
pub(super) struct Scripted {
    replies: Mutex<VecDeque<CompletionResponse>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl Scripted {
    pub(super) fn new(replies: impl IntoIterator<Item = CompletionResponse>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into_iter().collect()),
            requests: Mutex::default(),
        })
    }

    pub(super) fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LLMPort for Scripted {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.requests.lock().unwrap().push(request.clone());
        let reply = self.replies.lock().unwrap().pop_front();
        match reply {
            Some(reply) => Ok(reply),
            // Out of script: wait to be stopped.
            None => std::future::pending().await,
        }
    }
    fn model_name(&self) -> &str {
        "scripted"
    }
    fn max_context_tokens(&self) -> usize {
        32_768
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}
