//! Stage tests against a fake runtime: no database, no model, no container.
use super::assemble::{PlannedRequest, RenderedPrompt};
use super::prepare::PreparedTurn;
use super::*;
use crate::application::ports::llm_port::{CompletionInput, CompletionRequest, CompletionResponse};
use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::features::conversation::chat::ports::*;
use crate::features::conversation::mocks::MockConversationService;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::function_calling::domain::{FunctionCall, FunctionResult, ToolDefinition};
use std::sync::Mutex;

/// The narrow runtime a stage needs, and nothing behind it: every dependency a
/// stage under test should not reach fails the test if it does.
struct FakeRuntime;

const UNUSED: &str = "not reached by the stage under test";

#[async_trait::async_trait]
impl ChatModels for FakeRuntime {
    async fn get_or_load_llm(&self) -> Result<Arc<dyn LLMPort>> {
        unreachable!("{UNUSED}")
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
        Arc::new(MockConversationService::new())
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
        Arc::new(NoTools)
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
    async fn settings(&self) -> Result<crate::features::settings::dto::SettingsDto> {
        Ok(Default::default())
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

impl ChatRuntime for FakeRuntime {
    fn share(&self) -> Arc<dyn ChatRuntime> {
        Arc::new(FakeRuntime)
    }
}

/// A model that never answers, and says when it has been asked.
struct Unanswering {
    asked: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl LLMPort for Unanswering {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        self.asked.notify_one();
        std::future::pending().await
    }
    fn model_name(&self) -> &str {
        "unanswering"
    }
    fn max_context_tokens(&self) -> usize {
        8192
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn prompt() -> RenderedPrompt {
    RenderedPrompt {
        enhanced_message: "question".into(),
        sources: Vec::new(),
        retrieval_trace: None,
        short_circuit_response: None,
        pages_read: Default::default(),
        can_open_pages: false,
        has_grounded_context: false,
        attachment_names: Vec::new(),
        attachment_ids: Vec::new(),
    }
}

fn request(turn: &PreparedTurn) -> PlannedRequest {
    PlannedRequest {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: "question".into(),
        }],
        tools: Vec::new(),
        max_output_tokens: turn.budget.output_tokens,
        input_budget: turn.budget.input_budget,
        memory_usage: None,
    }
}

fn recording_sink() -> (ChatEventSink, Arc<Mutex<Vec<ChatStreamEventDto>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let sink: ChatEventSink = Arc::new(move |event| {
        captured.lock().unwrap().push(event);
        Ok(())
    });
    (sink, events)
}

#[tokio::test]
async fn stopping_the_turn_ends_the_generate_stage_while_the_model_is_still_generating() {
    let llm = Arc::new(Unanswering {
        asked: tokio::sync::Notify::new(),
    });
    let turn = PreparedTurn::for_test(
        "stage-cancel-conversation",
        "stage-cancel-turn",
        llm.clone(),
        Arc::new(MockConversationService::new()),
    )
    .unwrap();
    let (sink, events) = recording_sink();
    let mut metrics = ConversationFlowTimingMetrics::default();
    let flags = SearchFlags::from_preferences(None);
    let planned = request(&turn);

    let stop = async {
        llm.asked.notified().await;
        assert!(cancel_generation_for_conversation(
            "stage-cancel-conversation",
            Some("stage-cancel-turn")
        ));
    };
    let generate = generate::generate_answer(
        &FakeRuntime,
        &turn,
        flags,
        prompt(),
        planned,
        &sink,
        &mut metrics,
    );
    let (result, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(generate, stop)
    })
    .await
    .expect("a stop press ends the stage, not the time budget");

    let error = result.err().expect("a cancelled turn has no answer");
    assert!(error.to_string().contains("cancelled"), "{error}");
    // The bubble is never left generating: the stream still closes.
    assert!(
        events.lock().unwrap().iter().any(|event| event.done),
        "no terminal event"
    );
}

#[tokio::test]
async fn a_router_answer_skips_the_model_entirely() {
    let llm = Arc::new(Unanswering {
        asked: tokio::sync::Notify::new(),
    });
    let turn = PreparedTurn::for_test(
        "stage-canned-conversation",
        "stage-canned-turn",
        llm,
        Arc::new(MockConversationService::new()),
    )
    .unwrap();
    let (sink, _) = recording_sink();
    let mut metrics = ConversationFlowTimingMetrics::default();
    let planned = request(&turn);
    let mut canned = prompt();
    canned.short_circuit_response = Some("Hello!".into());

    let generated = generate::generate_answer(
        &FakeRuntime,
        &turn,
        SearchFlags::from_preferences(None),
        canned,
        planned,
        &sink,
        &mut metrics,
    )
    .await
    .unwrap();

    assert_eq!(generated.response, "Hello!");
    assert!(metrics.generation_subtimings.is_some());
}
