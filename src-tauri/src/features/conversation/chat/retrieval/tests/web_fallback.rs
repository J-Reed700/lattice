//! A web search the intent step guessed at is a fallback: the vault is
//! searched first, and the web only when the vault came back empty or too
//! weak to answer from. Run through the retrieval pipeline itself on the fake
//! runtime, over a library that finds a fixed set of passages for every query
//! and a web that records each search.
use super::*;
use crate::features::conversation::chat::intent::TurnIntent;
use crate::features::conversation::chat::ports::{ChatRecords, ChatTools};
use crate::features::conversation::chat::test_runtime::{FakeRuntime, TestVault};
use crate::features::conversation::chat::turn_record::{TurnRecorder, TurnStepDto, TurnStepKind};
use crate::features::conversation::chat::ToolPreferences;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::ConversationServiceTrait;
use crate::features::function_calling::domain::{FunctionCall, FunctionResult, ToolDefinition};
use lattice_core::application::contracts::search::SearchRequestDto;
use lattice_core::application::contracts::search::{
    QueryBranches, RankedBranch, RerankOptions, Reranked,
};
use lattice_core::application::ports::library_search::LibrarySearchTrait;
use lattice_core::application::ports::LLMPort;
use std::sync::Mutex;

const CONVERSATION: &str = "fallback-chat";
const QUESTION: &str = "Which HTTP failures may the client retry?";

/// The records retrieval reads: one handbook in the General space.
async fn handbook_records() -> Arc<dyn ChatRecords> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE conversations (id TEXT, space_id TEXT);
         CREATE TABLE documents (id TEXT, file_name TEXT, file_path TEXT, source_context TEXT,
             owner_conversation_id TEXT);
         CREATE TABLE text_chunks (id TEXT, document_id TEXT, content TEXT, chunk_index INTEGER,
             section TEXT, page_number INTEGER, from_ocr BOOLEAN NOT NULL DEFAULT 0);
         CREATE TABLE document_space_memberships (document_id TEXT, space_id TEXT);
         INSERT INTO conversations VALUES ('fallback-chat', 'space_general');
         INSERT INTO documents VALUES ('handbook', 'Handbook.md', '/vault/Handbook.md', NULL, NULL);
         INSERT INTO text_chunks (id, document_id, content, chunk_index, section, page_number)
             VALUES ('handbook-0', 'handbook', 'Engineering handbook', 0, NULL, NULL);",
    )
    .execute(&pool)
    .await
    .unwrap();
    Arc::new(ConversationRepository::new(pool))
}

fn passage(index: usize, content: &str) -> SearchResultDto {
    SearchResultDto {
        id: format!("handbook-{index}"),
        title: "Handbook.md".to_string(),
        content: content.to_string(),
        score: 0.03,
        path: Some("/vault/Handbook.md".to_string()),
        document_id: Some("handbook".to_string()),
        position: Some(index),
        vector_score: None,
        bm25_score: None,
        vector_rank: None,
        bm25_rank: None,
        metadata: HashMap::new(),
    }
}

/// Passages that carry every term of the question: enough to answer from.
fn answering_passages() -> Vec<SearchResultDto> {
    (1..=3)
        .map(|index| {
            passage(
                index,
                "Which HTTP failures may the client retry? The client may retry HTTP 429 and 503 failures.",
            )
        })
        .collect()
}

/// Passages from the right file that never touch the question.
fn unrelated_passages() -> Vec<SearchResultDto> {
    (1..=3)
        .map(|index| passage(index, "Parking spaces are assigned by the facilities team."))
        .collect()
}

/// Finds the same passages for every query, and never reranks them.
struct FixedLibrary {
    passages: Vec<SearchResultDto>,
}

#[async_trait::async_trait]
impl LibrarySearchTrait for FixedLibrary {
    async fn execute_scoped(
        &self,
        _request: SearchRequestDto,
        _space_id: Option<&str>,
        _allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<SearchResponseDto> {
        Ok(SearchResponseDto {
            total: self.passages.len(),
            results: self.passages.clone(),
            query_time_ms: 0,
        })
    }
    async fn search_branches(
        &self,
        _query: &str,
        _space_id: Option<&str>,
        _allowed_document_ids: Option<&HashSet<String>>,
        _limit: usize,
        _vector_weight: f32,
        _bm25_weight: f32,
    ) -> QueryBranches {
        QueryBranches {
            branches: Vec::new(),
            answered: 2,
        }
    }
    fn fuse(&self, _branches: Vec<RankedBranch>, limit: usize) -> Vec<SearchResultDto> {
        self.passages.iter().take(limit).cloned().collect()
    }
    async fn rerank(&self, response: SearchResponseDto, _options: &RerankOptions) -> Reranked {
        Reranked {
            response,
            applied: false,
        }
    }
}

/// Records every web search and finds nothing, so no page is ever read.
#[derive(Default)]
struct RecordingWeb {
    queries: Mutex<Vec<String>>,
}

impl RecordingWeb {
    fn queries(&self) -> Vec<String> {
        self.queries.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ChatTools for RecordingWeb {
    fn list_tools(&self) -> Vec<ToolDefinition> {
        Vec::new()
    }
    async fn execute(&self, call: FunctionCall) -> Result<FunctionResult> {
        assert_eq!(call.name, "web_search", "the pipeline called {}", call.name);
        let query = call.arguments["query"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.queries.lock().unwrap().push(query.clone());
        Ok(FunctionResult::success(serde_json::json!({
            "results": [],
            "query": query,
            "result_count": 0,
        })))
    }
}

/// The utility model's every answer: the query interpretation and the web
/// query rewrite both read it.
struct Rewriter;

#[async_trait::async_trait]
impl LLMPort for Rewriter {
    async fn complete(
        &self,
        _request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        Ok(
            lattice_core::application::ports::llm_port::CompletionResponse::from_text(
                "client retry HTTP failures",
            ),
        )
    }
    fn model_name(&self) -> &str {
        "rewriter"
    }
    fn max_context_tokens(&self) -> usize {
        32_768
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The flags of an unflagged turn the intent step read as needing the web.
fn intent_wants_the_web() -> SearchFlags {
    super::super::super::apply_turn_intent(
        SearchFlags::from_preferences(None),
        &TurnIntent {
            needs_knowledge_base: false,
            needs_web: true,
            is_followup: false,
            confidence: 0.9,
        },
    )
}

struct Ran {
    outcome: RetrievalPipelineOutcome,
    steps: Vec<TurnStepDto>,
    web_queries: Vec<String>,
}

impl Ran {
    fn position(&self, kind: TurnStepKind) -> Option<usize> {
        self.steps.iter().position(|step| step.kind == kind)
    }

    fn web_step(&self) -> Option<&TurnStepDto> {
        self.steps
            .iter()
            .find(|step| step.kind == TurnStepKind::WebSearch)
    }
}

async fn run(passages: Vec<SearchResultDto>, flags: SearchFlags, turn: &str) -> Ran {
    let web = Arc::new(RecordingWeb::default());
    let runtime = FakeRuntime {
        tools: Some(web.clone()),
        vault: Some(TestVault {
            records: handbook_records().await,
            search: Arc::new(FixedLibrary { passages }),
        }),
        ..FakeRuntime::default()
    };
    let conversations: Arc<dyn ConversationServiceTrait> = runtime.conversations.clone();
    let llm: Arc<dyn LLMPort> = Arc::new(Rewriter);
    let recorder = TurnRecorder::silent(CONVERSATION, turn);
    let decision = RouterDecisionOutcome {
        action: RouterAction::NewSearch,
        recent_doc_meta: None,
        clarify_message: None,
        router_may_search: true,
    };
    let outcome = run_retrieval_pipeline(
        &runtime,
        ChatTurn {
            conv_service: &conversations,
            conversation_id: CONVERSATION,
            request_id: turn,
            message: QUESTION,
            llm: &llm,
            highlight_terms: &[],
            focus: &crate::features::conversation::chat::focus::FocusScope::default(),
            recorder: &recorder,
        },
        &SettingsDto::default(),
        &decision,
        flags,
        &[],
        EvidenceBudget::new(20_000),
        None,
    )
    .await;
    Ran {
        outcome,
        steps: recorder.steps(),
        web_queries: web.queries(),
    }
}

/// The reported defect: a handbook question went out as a public search and
/// the pages joined the handbook's answer. When the vault answers, the
/// question never leaves the machine.
#[tokio::test]
async fn an_intent_web_search_is_never_sent_when_the_vault_answers() {
    let ran = run(
        answering_passages(),
        intent_wants_the_web(),
        "fallback-vault-answers",
    )
    .await;

    assert!(ran.outcome.kb_attempted);
    assert_eq!(
        ran.outcome
            .sufficiency
            .as_ref()
            .map(|verdict| verdict.sufficient),
        Some(true)
    );
    assert!(!ran.outcome.sources.is_empty(), "the vault's passages stay");
    assert!(ran.web_queries.is_empty(), "searched {:?}", ran.web_queries);
    assert!(ran.web_step().is_none());
    assert!(ran.outcome.web_context.is_empty());
}

#[tokio::test]
async fn an_intent_web_search_runs_after_a_vault_that_fell_short() {
    let ran = run(
        unrelated_passages(),
        intent_wants_the_web(),
        "fallback-vault-insufficient",
    )
    .await;

    assert_eq!(
        ran.outcome
            .sufficiency
            .as_ref()
            .map(|verdict| verdict.sufficient),
        Some(false)
    );
    assert_eq!(ran.web_queries.len(), 1);
    let searched_vault = ran
        .position(TurnStepKind::SearchDocuments)
        .expect("the vault was searched");
    let judged = ran
        .position(TurnStepKind::Sufficiency)
        .expect("the vault was judged");
    let searched_web = ran
        .position(TurnStepKind::WebSearch)
        .expect("the web was searched");
    assert!(searched_vault < judged && judged < searched_web);
    assert_eq!(
        ran.web_step().map(|step| step.label.as_str()),
        Some(WebSearchReason::VaultInsufficient.step_label())
    );
}

#[tokio::test]
async fn an_intent_web_search_runs_after_an_empty_vault() {
    let ran = run(Vec::new(), intent_wants_the_web(), "fallback-vault-empty").await;

    assert!(ran.outcome.kb_attempted);
    assert!(ran.outcome.search_response.results.is_empty());
    assert_eq!(ran.web_queries.len(), 1);
    let searched_vault = ran
        .position(TurnStepKind::SearchDocuments)
        .expect("the vault was searched");
    let searched_web = ran
        .position(TurnStepKind::WebSearch)
        .expect("the web was searched");
    assert!(searched_vault < searched_web);
    assert_eq!(
        ran.web_step().map(|step| step.label.as_str()),
        Some(WebSearchReason::VaultEmpty.step_label())
    );
}

/// The user's toggle is unchanged: the web is searched whatever the vault
/// holds, beside it, and the record says the user asked.
#[tokio::test]
async fn a_web_search_the_user_asked_for_runs_whatever_the_vault_holds() {
    let toggled = SearchFlags::from_preferences(Some(&ToolPreferences {
        web_search: true,
        ..ToolPreferences::default()
    }));

    let ran = run(answering_passages(), toggled, "fallback-user-asked").await;

    assert!(ran.outcome.kb_attempted);
    assert!(!ran.outcome.sources.is_empty());
    assert_eq!(ran.web_queries.len(), 1);
    assert_eq!(
        ran.web_step().map(|step| step.label.as_str()),
        Some(WebSearchReason::Requested.step_label())
    );
}

/// No intent reopens a closed book: neither the vault nor the web is asked.
#[tokio::test]
async fn a_closed_book_turn_stays_closed_whatever_the_intent_says() {
    let closed = SearchFlags {
        closed_book: true,
        ..SearchFlags::from_preferences(None)
    };
    let flags = super::super::super::apply_turn_intent(
        closed,
        &TurnIntent {
            needs_knowledge_base: true,
            needs_web: true,
            is_followup: false,
            confidence: 1.0,
        },
    );

    let ran = run(Vec::new(), flags, "fallback-closed-book").await;

    assert!(!ran.outcome.kb_attempted);
    assert!(ran.web_queries.is_empty());
    assert!(ran.steps.is_empty());
}
