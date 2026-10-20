//! The narrow runtime a chat test needs, and nothing behind it: no database,
//! no model and no container unless the test hands one in. A dependency the
//! test did not provide fails the test if it is reached.
use super::ports::*;
use crate::features::conversation::mocks::MockConversationService;
use crate::features::conversation::repository::ConversationRepository;
use lattice_core::application::contracts::images::{
    ChatImageDto, FindImageInput, FindImageOutput, FoundImage,
};
use lattice_core::application::contracts::settings::LLMSettingsDto;
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult, ToolDefinition};
use lattice_core::application::ports::llm_port::{
    CompletionRequest, CompletionResponse, ModelRequirements, PinnedModel, ResolvedModel,
};
use lattice_core::application::ports::{ChatImagesPort, ImageTurnScope};
use lattice_core::application::ports::{EmbeddingPort, LLMPort};
use lattice_core::application::services::chat_model_resolution::{
    resolve, resolve_learning, DefaultChatModel,
};
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::JobRuntime;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const UNUSED: &str = "not provided to the test";

/// A library a turn can search: the records retrieval reads, and the search
/// over them. Document and chunk lookups for citations answer from empty
/// in-memory stores, so a passage is cited under its search title.
#[derive(Clone)]
pub(super) struct TestVault {
    pub(super) records: Arc<dyn ChatRecords>,
    pub(super) search: Arc<dyn lattice_core::application::ports::hybrid_search::HybridSearchTrait>,
}

/// The model `FakeRuntime::llm` stands for: the Settings default.
pub(super) fn default_model() -> ModelRef {
    ModelRef::Local {
        model_id: "settings-default".into(),
    }
}

#[derive(Clone)]
pub(in crate::features::conversation) struct FakeRuntime {
    /// The Settings default chat model, named [`default_model`].
    pub(super) llm: Option<Arc<dyn LLMPort>>,
    /// The other models a chat, a request or the Learning role may name.
    pub(super) models: Vec<(ModelRef, Arc<dyn LLMPort>)>,
    /// The Learning model set in Settings, if any.
    pub(super) learning_model: Option<ModelRef>,
    pub(super) tools: Option<Arc<dyn ChatTools>>,
    pub(super) jobs: Option<Arc<JobRuntime>>,
    /// The library retrieval searches. Without one, a turn that reaches the
    /// vault finds no scope and searches nothing.
    pub(super) vault: Option<TestVault>,
    /// One service for the whole test, so what a turn saves can be read back.
    pub(super) conversations: Arc<MockConversationService>,
    /// One registry per test, so a stop press lands on the map the turn
    /// registered with — and never on another test's.
    pub(super) turn_cancellation:
        Arc<crate::features::conversation::chat::cancellation::TurnCancellationRegistry>,
    /// One map per test, so a research job answers the command of its own
    /// test — and never another test's.
    pub(super) research_waiters: Arc<crate::features::conversation::chat::ResearchWaiters>,
    /// One background scope per test, so work a turn leaves running stays in
    /// its own test.
    pub(super) background: Arc<lattice_core::shared::runtime::background::BackgroundTasks>,
    /// One planner health per test, so a planner timeout in one test never
    /// shrinks another's prompt.
    pub(super) planner_health: Arc<crate::features::conversation::chat::retrieval::PlannerHealth>,
    /// The `answer_thinking` setting the turn reads.
    pub(super) answer_thinking: bool,
    /// What a cited page the turn never read is captured with.
    pub(super) page_reader: Option<Arc<dyn PageReader>>,
    /// The conversation records, for a test that reads back what was archived
    /// without a library to search.
    pub(super) records: Option<Arc<dyn ChatRecords>>,
    /// One picture store per test, so what a turn stored, attached and
    /// released can be read back.
    pub(in crate::features::conversation) images: Arc<FakeChatImages>,
}

impl Default for FakeRuntime {
    fn default() -> Self {
        Self {
            llm: None,
            models: Vec::new(),
            learning_model: None,
            tools: None,
            jobs: None,
            vault: None,
            conversations: Arc::new(MockConversationService::new()),
            turn_cancellation: Arc::new(
                crate::features::conversation::chat::cancellation::TurnCancellationRegistry::new(),
            ),
            research_waiters: Arc::new(crate::features::conversation::chat::ResearchWaiters::new()),
            background: Arc::default(),
            planner_health: Arc::default(),
            answer_thinking: false,
            page_reader: None,
            records: None,
            images: Arc::default(),
        }
    }
}

/// The page every test picture comes from.
pub(super) const TEST_PICTURE_PAGE: &str =
    "https://commons.wikimedia.org/wiki/File:Apollo_11_Launch.jpg";

/// A stored picture as the port reports it, under `r#ref`.
pub(super) fn test_picture(r#ref: &str) -> ChatImageDto {
    ChatImageDto {
        id: format!("{:0>32}", r#ref.trim_start_matches("img-")),
        r#ref: r#ref.to_string(),
        file_name: format!("{:0>32}.jpg", r#ref.trim_start_matches("img-")),
        mime: "image/jpeg".to_string(),
        width: 1280,
        height: 1600,
        byte_size: 2048,
        caption: "Saturn V lifting off".to_string(),
        source_url: Some(TEST_PICTURE_PAGE.to_string()),
        source_title: Some("File:Apollo 11 Launch.jpg".to_string()),
        origin: lattice_core::application::contracts::images::ImageOrigin::WikimediaCommons,
        license: Some("Public domain".to_string()),
        attribution: None,
        original_name: None,
    }
}

/// A picture the reader attached, `width`×`height`, staged as `att-<n>`.
pub(in crate::features::conversation) fn test_attachment(
    n: usize,
    width: u32,
    height: u32,
) -> ChatImageDto {
    ChatImageDto {
        id: format!("{n:a>32}"),
        r#ref: format!("att-{n}"),
        file_name: format!("{n:a>32}.png"),
        mime: "image/png".to_string(),
        width,
        height,
        byte_size: 4096,
        caption: format!("shot-{n}.png"),
        source_url: None,
        source_title: None,
        origin: lattice_core::application::contracts::images::ImageOrigin::Attachment,
        license: None,
        attribution: None,
        original_name: Some(format!("shot-{n}.png")),
    }
}

/// A picture store that stores what the test gave it and keeps every call,
/// so a test can see what the turn asked for, attached and released.
#[derive(Default)]
pub(in crate::features::conversation) struct FakeChatImages {
    /// What the next `find_image` call stores, refs as given.
    pub(super) canned: Mutex<Vec<ChatImageDto>>,
    /// Pictures the composer staged, by id: what `rows_for_ids` and
    /// `prepared` read, and what `attach_ids_to_message` claims.
    pub(super) staged: Mutex<Vec<ChatImageDto>>,
    /// Stored and not yet released, by request.
    stored: Mutex<Vec<(String, ChatImageDto)>>,
    /// Each call in order: `find <query>`, `attach <request> <message> <refs>`,
    /// `release_request <id>`, `release_message <id>`, `release_conversation <id>`,
    /// `remove_unclaimed_files`, `prepared <id> <max tokens or ->`,
    /// `claim <ids> <message> <request>`, `release_images <ids>`,
    /// `restage <id>`, `store_attachment <name>`.
    pub(super) calls: Mutex<Vec<String>>,
}

impl FakeChatImages {
    pub(super) fn with(images: Vec<ChatImageDto>) -> Arc<Self> {
        Arc::new(Self {
            canned: Mutex::new(images),
            ..Self::default()
        })
    }

    /// A store holding `pictures` as staged attachments.
    pub(in crate::features::conversation) fn staged(pictures: Vec<ChatImageDto>) -> Arc<Self> {
        Arc::new(Self {
            staged: Mutex::new(pictures),
            ..Self::default()
        })
    }

    pub(in crate::features::conversation) fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
}

#[async_trait::async_trait]
impl ChatImagesPort for FakeChatImages {
    async fn find_and_store(
        &self,
        scope: &ImageTurnScope,
        input: FindImageInput,
        _deadline: Option<std::time::Instant>,
    ) -> Result<(FindImageOutput, Vec<ChatImageDto>)> {
        self.record(format!("find {}", input.query));
        let stored: Vec<ChatImageDto> = std::mem::take(&mut *self.canned.lock().unwrap());
        self.stored.lock().unwrap().extend(
            stored
                .iter()
                .map(|image| (scope.request_id.clone(), image.clone())),
        );
        let images = stored
            .iter()
            .map(|image| FoundImage {
                r#ref: image.r#ref.clone(),
                caption: image.caption.clone(),
                source_title: image.source_title.clone(),
                source_url: image.source_url.clone().unwrap_or_default(),
                license: image.license.clone(),
                width: image.width,
                height: image.height,
                markdown: format!("![{}]({})", image.caption, image.r#ref),
            })
            .collect();
        Ok((FindImageOutput { images, note: None }, stored))
    }
    async fn attach_to_message(
        &self,
        request_id: &str,
        message_id: &str,
        referenced_refs: &[String],
    ) -> Result<Vec<ChatImageDto>> {
        self.record(format!(
            "attach {request_id} {message_id} {}",
            referenced_refs.join(",")
        ));
        let mut stored = self.stored.lock().unwrap();
        let attached = stored
            .iter()
            .filter(|(request, image)| {
                request == request_id && referenced_refs.contains(&image.r#ref)
            })
            .map(|(_, image)| image.clone())
            .collect();
        stored.retain(|(request, _)| request != request_id);
        Ok(attached)
    }
    async fn release_request(&self, request_id: &str) -> Result<()> {
        self.record(format!("release_request {request_id}"));
        self.stored
            .lock()
            .unwrap()
            .retain(|(request, _)| request != request_id);
        Ok(())
    }
    async fn release_message(&self, message_id: &str) -> Result<()> {
        self.record(format!("release_message {message_id}"));
        Ok(())
    }
    async fn release_conversation(&self, conversation_id: &str) -> Result<()> {
        self.record(format!("release_conversation {conversation_id}"));
        Ok(())
    }
    async fn list_for_message(&self, _message_id: &str) -> Result<Vec<ChatImageDto>> {
        Ok(Vec::new())
    }
    async fn copy_for_messages(
        &self,
        _conversation_id: &str,
        copies: &[lattice_core::application::ports::CopiedMessage],
    ) -> Result<Vec<(String, Vec<ChatImageDto>)>> {
        Ok(copies
            .iter()
            .map(|copy| {
                self.record(format!("copy {}", copy.message_id));
                (copy.message_id.clone(), Vec::new())
            })
            .collect())
    }
    async fn remove_unclaimed_files(&self) -> Result<u64> {
        self.record("remove_unclaimed_files".to_string());
        Ok(0)
    }
    async fn store_attachment(
        &self,
        scope: &ImageTurnScope,
        file_name: &str,
        _bytes: Vec<u8>,
    ) -> Result<ChatImageDto> {
        self.record(format!("store_attachment {file_name}"));
        let mut staged = self.staged.lock().unwrap();
        let n = staged.len() + 1;
        let mut picture = test_attachment(n, 640, 480);
        picture.original_name = Some(file_name.to_string());
        picture.caption = format!("{} {}", file_name, scope.request_id);
        staged.push(picture.clone());
        Ok(picture)
    }
    async fn prepared(
        &self,
        image_id: &str,
        geometry: lattice_core::domain::models::vision::VisionGeometry,
        max_tokens: Option<u32>,
    ) -> Result<Option<lattice_core::application::contracts::images::PreparedImageDto>> {
        self.record(format!(
            "prepared {image_id} {}",
            max_tokens.map_or_else(|| "-".to_string(), |max| max.to_string())
        ));
        let staged = self.staged.lock().unwrap();
        let Some(picture) = staged.iter().find(|picture| picture.id == image_id) else {
            return Ok(None);
        };
        let fitted = match max_tokens {
            Some(max) => geometry.fit_within(picture.width, picture.height, max),
            None => Some(geometry.plan(picture.width, picture.height)),
        }
        .ok_or_else(|| AppError::InvalidInput("no fit".to_string()))?;
        Ok(Some(
            lattice_core::application::contracts::images::PreparedImageDto {
                mime: "image/jpeg".to_string(),
                data_base64: "QUJD".to_string(),
                width: fitted.width,
                height: fitted.height,
                tokens: fitted.tokens,
            },
        ))
    }
    async fn rows_for_ids(
        &self,
        _conversation_id: &str,
        image_ids: &[String],
    ) -> Result<Vec<ChatImageDto>> {
        let staged = self.staged.lock().unwrap();
        Ok(image_ids
            .iter()
            .filter_map(|id| staged.iter().find(|picture| &picture.id == id).cloned())
            .collect())
    }
    async fn attach_ids_to_message(
        &self,
        _conversation_id: &str,
        image_ids: &[String],
        message_id: &str,
        request_id: &str,
    ) -> Result<Vec<ChatImageDto>> {
        self.record(format!(
            "claim {} {message_id} {request_id}",
            image_ids.join(",")
        ));
        let staged = self.staged.lock().unwrap();
        Ok(image_ids
            .iter()
            .filter_map(|id| staged.iter().find(|picture| &picture.id == id).cloned())
            .collect())
    }
    async fn release_images(&self, _conversation_id: &str, image_ids: &[String]) -> Result<()> {
        self.record(format!("release_images {}", image_ids.join(",")));
        self.staged
            .lock()
            .unwrap()
            .retain(|picture| !image_ids.contains(&picture.id));
        Ok(())
    }
    async fn restage(&self, _conversation_id: &str, image_id: &str) -> Result<ChatImageDto> {
        self.record(format!("restage {image_id}"));
        let mut staged = self.staged.lock().unwrap();
        let source = staged
            .iter()
            .find(|picture| picture.id == image_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(image_id.to_string()))?;
        let copy = ChatImageDto {
            id: format!("{:b>32}", staged.len() + 1),
            ..source
        };
        staged.push(copy.clone());
        Ok(copy)
    }
}

impl FakeRuntime {
    /// The loaded model a reference names, or the refusal a missing one gets.
    fn loaded(&self, model: &ModelRef) -> Result<Arc<dyn LLMPort>> {
        if *model == default_model() {
            if let Some(llm) = &self.llm {
                return Ok(Arc::clone(llm));
            }
        }
        self.models
            .iter()
            .find(|(named, _)| named == model)
            .map(|(_, llm)| Arc::clone(llm))
            .ok_or_else(|| AppError::AiModelsNotInstalled(format!("{model} is not downloaded")))
    }

    fn pinned(&self, resolved: ResolvedModel) -> Result<PinnedModel> {
        Ok(PinnedModel {
            llm: self.loaded(&resolved.model)?,
            resolved,
            instance_id: 0,
            degraded: None,
        })
    }
}

/// Resolved the way the app resolves: the request override, else the chat's
/// choice as its conversation keeps it, else the Settings default.
#[async_trait::async_trait]
impl ChatModels for FakeRuntime {
    async fn chat_llm_for(
        &self,
        conversation_id: &str,
        request_override: Option<&ModelRef>,
        _requirements: ModelRequirements,
    ) -> Result<PinnedModel> {
        let choice = self.conversations.model_choice(conversation_id)?;
        let default =
            DefaultChatModel(self.llm.as_ref().map(|_| default_model()).ok_or_else(|| {
                AppError::AiModelsNotInstalled(
                    "No chat model is selected in Settings → Models".into(),
                )
            }));
        self.pinned(resolve(request_override, &choice, &default)?)
    }
    async fn learning_llm(&self, _requirements: ModelRequirements) -> Result<PinnedModel> {
        let settings = LLMSettingsDto {
            learning_model: self.learning_model.clone(),
            ..LLMSettingsDto::default()
        };
        self.pinned(resolve_learning(&settings)?)
    }
    async fn default_chat_llm(&self) -> Result<Arc<dyn LLMPort>> {
        self.loaded(&default_model())
    }
    async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>> {
        unreachable!("{UNUSED}")
    }
    async fn get_or_load_utility_llm(&self) -> Result<Option<Arc<dyn LLMPort>>> {
        Ok(None)
    }
    fn utility_llm_loader(&self) -> lattice_core::application::ports::llm_port::OptionalLlmLoader {
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
    fn conversation_history(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::ConversationHistoryPort> {
        unreachable!("{UNUSED}")
    }
    fn conversation_context(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::conversation_context::ConversationContextPort>
    {
        unreachable!("{UNUSED}")
    }
    fn document_scope(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::document_scope::DocumentScopePort> {
        unreachable!("{UNUSED}")
    }
    fn document_repository(&self) -> Arc<dyn lattice_core::application::ports::DocumentRepository> {
        assert!(self.vault.is_some(), "{UNUSED}");
        Arc::new(lattice_core::domain::repositories::mocks::DddMockDocumentRepository::new())
    }
    fn chunk_repository(&self) -> Arc<dyn lattice_core::application::ports::ChunkRepositoryPort> {
        assert!(self.vault.is_some(), "{UNUSED}");
        Arc::new(lattice_core::domain::repositories::mocks::MockChunkRepository::new())
    }
    fn chat_records(&self) -> Arc<dyn ChatRecords> {
        if let Some(vault) = &self.vault {
            return Arc::clone(&vault.records);
        }
        if let Some(records) = &self.records {
            return Arc::clone(records);
        }
        // Never queried unless the model calls a history tool.
        Arc::new(ConversationRepository::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .unwrap(),
        ))
    }
    fn chat_starter_cache(&self) -> Arc<dyn ChatStarterCache> {
        unreachable!("{UNUSED}")
    }
    fn conversation_memory(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::conversation_memory::ConversationMemoryPort>
    {
        unreachable!("{UNUSED}")
    }
    fn chat_images(&self) -> Arc<dyn ChatImagesPort> {
        self.images.clone()
    }
    fn compaction_slots(&self) -> Arc<crate::features::conversation::memory::CompactionSlots> {
        unreachable!("{UNUSED}")
    }
    fn handoffs_in_flight(&self) -> Arc<crate::features::conversation::handoff::HandoffsInFlight> {
        unreachable!("{UNUSED}")
    }
    fn conversation_branches(&self) -> Arc<dyn ConversationBranches> {
        unreachable!("{UNUSED}")
    }
    async fn explorer_turn(
        &self,
        _conversation_id: &str,
        _focus: Option<&lattice_core::application::contracts::explorer::ExplorerFocusDto>,
    ) -> Option<ExplorerTurn> {
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
    fn library_search(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::hybrid_search::HybridSearchTrait> {
        Arc::clone(&self.vault.as_ref().expect(UNUSED).search)
    }
    fn page_reader(&self) -> Arc<dyn PageReader> {
        Arc::clone(self.page_reader.as_ref().expect(UNUSED))
    }
    async fn summary_search(
        &self,
    ) -> Option<Arc<dyn lattice_core::application::ports::summary_search::SummarySearchPort>> {
        None
    }
    fn planner_health(&self) -> Arc<crate::features::conversation::chat::retrieval::PlannerHealth> {
        Arc::clone(&self.planner_health)
    }

    fn query_interpreter(
        &self,
        llm: Arc<dyn LLMPort>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Arc<dyn lattice_core::application::ports::query_interpreter::QueryInterpreter> {
        Arc::new(crate::features::search::hyde::HyDEService::new(llm).with_cancellation(cancel))
    }
}

#[async_trait::async_trait]
impl ChatPolicy for FakeRuntime {
    /// The defaults, without the grounding check: it runs after the answer
    /// and would race what a test reads back.
    async fn settings(&self) -> Result<SettingsDto> {
        let mut settings = SettingsDto::default();
        settings.llm.verification.enabled = false;
        settings.llm.answer_thinking = self.answer_thinking;
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
        _llm: Arc<dyn LLMPort>,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        unreachable!("{UNUSED}")
    }
    fn consolidate_after_turn(
        &self,
        _id: String,
        _summary_refold: Option<lattice_core::domain::conversation::memory::SummaryRefold>,
    ) {
    }
    async fn delete_conversation(&self, _conversation_id: &str) -> Result<()> {
        unreachable!("{UNUSED}")
    }
}

impl ChatJobs for FakeRuntime {
    fn jobs(&self) -> Arc<JobRuntime> {
        Arc::clone(self.jobs.as_ref().expect(UNUSED))
    }
    fn research_waiters(&self) -> Arc<crate::features::conversation::chat::ResearchWaiters> {
        Arc::clone(&self.research_waiters)
    }
    fn background(&self) -> Arc<lattice_core::shared::runtime::background::BackgroundTasks> {
        Arc::clone(&self.background)
    }
}

impl ChatCancellation for FakeRuntime {
    fn turn_cancellation(
        &self,
    ) -> Arc<crate::features::conversation::chat::cancellation::TurnCancellationRegistry> {
        Arc::clone(&self.turn_cancellation)
    }
}

#[async_trait::async_trait]
impl SynthesisEntries for FakeRuntime {
    async fn references_created_since(
        &self,
        _cutoff: &str,
    ) -> Result<Vec<lattice_core::application::contracts::references::PassageReferenceRecord>> {
        unreachable!("{UNUSED}")
    }
    async fn notes_in_range(
        &self,
        _start_date: &str,
        _end_date: &str,
    ) -> Result<Vec<StoredJournalPage>> {
        unreachable!("{UNUSED}")
    }
    async fn hydrate_note(
        &self,
        _page: StoredJournalPage,
    ) -> Result<lattice_core::application::contracts::notes::WorkspaceNoteDto> {
        unreachable!("{UNUSED}")
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
