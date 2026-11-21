use super::*;
use crate::features::conversation::chat::ports::{ChatRecords, ChatTools};
use crate::features::conversation::chat::test_runtime::{
    FakeEmbedding, FakeRuntime, Scripted, TestVault,
};
use crate::features::conversation::chat::turn_record::TurnRecorder;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::ConversationServiceTrait;
use lattice_core::application::contracts::search::{
    QueryBranches, RankedBranch, RerankOptions, Reranked, SearchRequestDto,
};
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult, ToolDefinition};
use lattice_core::application::ports::hybrid_search::HybridSearchTrait;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::persistence::Database;
use std::sync::Mutex;

const CONVERSATION: &str = "lazy-loading-chat";

async fn records_with_documents(with_doc: bool) -> Arc<dyn ChatRecords> {
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
         INSERT INTO conversations VALUES ('lazy-loading-chat', 'space_general');",
    )
    .execute(&pool)
    .await
    .unwrap();
    if with_doc {
        sqlx::raw_sql(
            "INSERT INTO documents VALUES ('doc-1', 'Guide.md', '/vault/Guide.md', NULL, NULL);
             INSERT INTO text_chunks (id, document_id, content, chunk_index, section, page_number)
                 VALUES ('doc-1-0', 'doc-1', 'Lattice architecture guide', 0, NULL, NULL);",
        )
        .execute(&pool)
        .await
        .unwrap();
    }
    Arc::new(ConversationRepository::new(Database::new(pool)))
}

struct FixedSearch {
    passages: Vec<SearchResultDto>,
}

#[async_trait::async_trait]
impl HybridSearchTrait for FixedSearch {
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

#[derive(Default)]
struct RecordingExternalTools {
    calls: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl ChatTools for RecordingExternalTools {
    fn list_tools(&self) -> Vec<ToolDefinition> {
        Vec::new()
    }
    async fn execute(&self, call: FunctionCall) -> Result<FunctionResult> {
        self.calls.lock().unwrap().push(call.name.clone());
        match call.name.as_str() {
            "web_search" => Ok(FunctionResult::success(serde_json::json!({
                "results": [],
                "query": call.arguments["query"],
                "result_count": 0,
            }))),
            "wiki_search" => Ok(FunctionResult::success(serde_json::json!({
                "results": [],
                "query": call.arguments["query"],
                "result_count": 0,
            }))),
            other => panic!("unexpected tool call: {other}"),
        }
    }
}

fn sample_passage() -> SearchResultDto {
    SearchResultDto {
        id: "doc-1-0".to_string(),
        title: "Guide.md".to_string(),
        content: "Lattice architecture guide".to_string(),
        score: 0.5,
        path: Some("/vault/Guide.md".to_string()),
        document_id: Some("doc-1".to_string()),
        position: Some(0),
        vector_score: None,
        bm25_score: None,
        vector_rank: None,
        bm25_rank: None,
        metadata: HashMap::new(),
    }
}

async fn invoke_pipeline(
    runtime: &FakeRuntime,
    decision: &RouterDecisionOutcome,
    flags: SearchFlags,
    request_id: &str,
) -> RetrievalPipelineOutcome {
    let conversations: Arc<dyn ConversationServiceTrait> = runtime.conversations.clone();
    let llm: Arc<dyn LLMPort> = Scripted::new([
        CompletionResponse::from_text("rewritten query"),
        CompletionResponse::from_text("rewritten query"),
    ]);
    let recorder = TurnRecorder::silent(CONVERSATION, request_id);
    run_retrieval_pipeline(
        runtime,
        ChatTurn {
            conv_service: &conversations,
            conversation_id: CONVERSATION,
            request_id,
            message: "What is the architecture?",
            standalone_question: None,
            llm: &llm,
            highlight_terms: &[],
            focus: &crate::features::conversation::chat::focus::FocusScope::default(),
            recorder: &recorder,
            names: &Arc::new(
                lattice_core::application::services::model_contract::kept::TurnNames::unstored(),
            ),
        },
        &SettingsDto::default(),
        decision,
        flags,
        &[],
        EvidenceBudget::new(20_000),
        None,
    )
    .await
}

fn new_search_decision() -> RouterDecisionOutcome {
    RouterDecisionOutcome {
        action: RouterAction::NewSearch,
        recent_doc_meta: None,
        clarify_message: None,
        router_may_search: true,
    }
}

#[tokio::test]
async fn skip_auto_kb_without_external_loads_neither_embedding_nor_utility() {
    let runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: vec![sample_passage()],
            }),
        }),
        utility_llm: Some(Scripted::new([])),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };

    let outcome = invoke_pipeline(
        &runtime,
        &new_search_decision(),
        SearchFlags {
            skip_auto_kb: true,
            ..SearchFlags::default()
        },
        "req-skip-auto-kb",
    )
    .await;

    assert!(!outcome.kb_attempted);
    assert!(outcome.kb_unavailable_reason.is_none());
    assert_eq!(runtime.loader_counts.embedding_calls(), 0);
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 0);
}

#[tokio::test]
async fn clarify_short_circuit_loads_neither_embedding_nor_utility() {
    let runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: vec![sample_passage()],
            }),
        }),
        utility_llm: Some(Scripted::new([])),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };
    let decision = RouterDecisionOutcome {
        action: RouterAction::Clarify,
        recent_doc_meta: Some(RecentDocumentMetadata {
            title: "Guide.md".to_string(),
        }),
        clarify_message: Some("Did you mean Guide.md?".to_string()),
        router_may_search: true,
    };

    let outcome = invoke_pipeline(
        &runtime,
        &decision,
        SearchFlags {
            force_wiki_search: true,
            ..SearchFlags::default()
        },
        "req-clarify",
    )
    .await;

    assert_eq!(
        outcome.short_circuit_response.as_deref(),
        Some("Did you mean Guide.md?")
    );
    assert!(!outcome.kb_attempted);
    assert_eq!(runtime.loader_counts.embedding_calls(), 0);
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 0);
}

#[tokio::test]
async fn pure_external_search_loads_utility_once_and_never_loads_embedding() {
    let tools = Arc::new(RecordingExternalTools::default());
    let utility = Scripted::new(
        std::iter::repeat_with(|| CompletionResponse::from_text("architecture overview")).take(8),
    );
    let runtime = FakeRuntime {
        tools: Some(tools.clone()),
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: Vec::new(),
            }),
        }),
        utility_llm: Some(utility),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };

    let outcome = invoke_pipeline(
        &runtime,
        &new_search_decision(),
        SearchFlags {
            skip_auto_kb: true,
            force_wiki_search: true,
            ..SearchFlags::default()
        },
        "req-pure-external",
    )
    .await;

    assert!(!outcome.kb_attempted);
    assert!(outcome.kb_unavailable_reason.is_none());
    assert_eq!(runtime.loader_counts.embedding_calls(), 0);
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 1);
    assert_eq!(tools.calls.lock().unwrap().as_slice(), ["wiki_search"]);
}

#[tokio::test]
async fn empty_vault_never_wakes_embedding_or_utility_and_reports_empty_vault() {
    let runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(false).await,
            search: Arc::new(FixedSearch {
                passages: Vec::new(),
            }),
        }),
        utility_llm: Some(Scripted::new([])),
        ..FakeRuntime::default()
    };

    let outcome = invoke_pipeline(
        &runtime,
        &new_search_decision(),
        SearchFlags {
            force_kb_search: true,
            ..SearchFlags::default()
        },
        "req-empty-vault",
    )
    .await;

    assert!(outcome.kb_attempted);
    assert_eq!(outcome.searched_documents, 0);
    assert_eq!(
        outcome.kb_unavailable_reason.as_deref(),
        Some("the vault has no indexed documents yet")
    );
    assert_ne!(
        outcome.kb_unavailable_reason.as_deref(),
        Some("the embedding model is not ready")
    );
    assert_eq!(runtime.loader_counts.embedding_calls(), 0);
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 0);
}

#[tokio::test]
async fn nonempty_space_with_unavailable_embedding_reports_reason_only_when_no_sources() {
    let empty_results_runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: Vec::new(),
            }),
        }),
        ..FakeRuntime::default()
    };

    let empty_outcome = invoke_pipeline(
        &empty_results_runtime,
        &new_search_decision(),
        SearchFlags {
            force_kb_search: true,
            ..SearchFlags::default()
        },
        "req-no-embed-no-hits",
    )
    .await;

    assert!(empty_outcome.kb_attempted);
    assert_eq!(empty_results_runtime.loader_counts.embedding_calls(), 1);
    assert_eq!(
        empty_outcome.kb_unavailable_reason.as_deref(),
        Some("the embedding model is not ready")
    );

    // When keyword search still yields sources, kb_unavailable_reason remains None.
    let keyword_hits_runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: vec![sample_passage()],
            }),
        }),
        ..FakeRuntime::default()
    };

    let keyword_outcome = invoke_pipeline(
        &keyword_hits_runtime,
        &new_search_decision(),
        SearchFlags {
            force_kb_search: true,
            ..SearchFlags::default()
        },
        "req-no-embed-with-hits",
    )
    .await;

    assert!(keyword_outcome.kb_attempted);
    assert_eq!(keyword_hits_runtime.loader_counts.embedding_calls(), 1);
    assert!(!keyword_outcome.sources.is_empty());
    assert!(keyword_outcome.kb_unavailable_reason.is_none());
}

#[tokio::test]
async fn kb_alongside_external_runs_both_phases_and_calls_both_loaders() {
    let tools = Arc::new(RecordingExternalTools::default());
    let utility = Scripted::new(
        std::iter::repeat_with(|| CompletionResponse::from_text("architecture query")).take(16),
    );
    let runtime = FakeRuntime {
        tools: Some(tools.clone()),
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: vec![sample_passage()],
            }),
        }),
        utility_llm: Some(utility),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };

    let outcome = invoke_pipeline(
        &runtime,
        &new_search_decision(),
        SearchFlags {
            force_kb_search: true,
            force_web_search: true,
            ..SearchFlags::default()
        },
        "req-kb-alongside-external",
    )
    .await;

    assert!(outcome.kb_attempted);
    assert!(!outcome.sources.is_empty());
    assert_eq!(tools.calls.lock().unwrap().as_slice(), ["web_search"]);
    assert_eq!(runtime.loader_counts.embedding_calls(), 1);
    assert!(runtime.loader_counts.utility_loader_calls() >= 1);
}

#[tokio::test]
async fn kb_retrieval_skips_embedding_load_when_already_cancelled() {
    let runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: vec![sample_passage()],
            }),
        }),
        ..FakeRuntime::default()
    };
    let conversations: Arc<dyn ConversationServiceTrait> = runtime.conversations.clone();
    let llm: Arc<dyn LLMPort> = Scripted::new([]);
    let recorder = TurnRecorder::silent(CONVERSATION, "req-cancel-embed");
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();

    let kb_outcome = super::super::kb_retrieval::run_kb_retrieval(
        &runtime,
        ChatTurn {
            conv_service: &conversations,
            conversation_id: CONVERSATION,
            request_id: "req-cancel-embed",
            message: "What is the architecture?",
            standalone_question: None,
            llm: &llm,
            highlight_terms: &[],
            focus: &crate::features::conversation::chat::focus::FocusScope::default(),
            recorder: &recorder,
            names: &Arc::new(
                lattice_core::application::services::model_contract::kept::TurnNames::unstored(),
            ),
        },
        SearchFlags {
            force_kb_search: true,
            ..SearchFlags::default()
        },
        10_000,
        4.0,
        0.5,
        &RetrievalTuningSettingsDto::default(),
        &cancel,
    )
    .await;

    assert_eq!(runtime.loader_counts.embedding_calls(), 0);
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 0);
    assert!(kb_outcome.sources.is_empty());
    assert!(kb_outcome.kb_unavailable_reason.is_none());
    assert_eq!(kb_outcome.searched_documents, 0);
    assert!(kb_outcome.sufficiency.is_none());
}

#[tokio::test]
async fn web_fallback_skips_external_utility_load_and_interpretation_when_kb_is_sufficient() {
    let tools = Arc::new(RecordingExternalTools::default());
    let answering_passages: Vec<SearchResultDto> = (0..3)
        .map(|index| SearchResultDto {
            id: format!("doc-1-{index}"),
            content: "What is the architecture? Lattice uses a layered Rust architecture."
                .to_string(),
            position: Some(index),
            ..sample_passage()
        })
        .collect();
    let runtime = FakeRuntime {
        tools: Some(tools.clone()),
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: answering_passages,
            }),
        }),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };
    let conversations: Arc<dyn ConversationServiceTrait> = runtime.conversations.clone();
    let scripted = Scripted::new([
        CompletionResponse::from_text("unexpected call 1"),
        CompletionResponse::from_text("unexpected call 2"),
    ]);
    let llm: Arc<dyn LLMPort> = scripted.clone();
    let recorder = TurnRecorder::silent(CONVERSATION, "req-web-fallback-kb-sufficient");

    let outcome = run_retrieval_pipeline(
        &runtime,
        ChatTurn {
            conv_service: &conversations,
            conversation_id: CONVERSATION,
            request_id: "req-web-fallback-kb-sufficient",
            message: "What is the architecture?",
            standalone_question: None,
            llm: &llm,
            highlight_terms: &[],
            focus: &crate::features::conversation::chat::focus::FocusScope::default(),
            recorder: &recorder,
            names: &Arc::new(
                lattice_core::application::services::model_contract::kept::TurnNames::unstored(),
            ),
        },
        &SettingsDto::default(),
        &new_search_decision(),
        SearchFlags {
            web_fallback: true,
            ..SearchFlags::default()
        },
        &[],
        EvidenceBudget::new(20_000),
        None,
    )
    .await;

    assert!(outcome.kb_attempted);
    assert!(
        outcome.sufficiency.as_ref().is_some_and(|v| v.sufficient),
        "expected KB passages to be sufficient: {:?}",
        outcome.sufficiency
    );
    assert!(!outcome.sources.is_empty());
    assert!(tools.calls.lock().unwrap().is_empty());
    assert!(
        scripted.requests().is_empty(),
        "external phase must not invoke chat LLM when KB already answered"
    );
    assert_eq!(runtime.loader_counts.embedding_calls(), 1);
    // KB corpus planner checks utility_llm_loader() once, gets None, and falls back locally;
    // the external phase must NOT call utility_llm_loader() a second time.
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 1);
    assert_eq!(outcome.sub_timings.external_hyde_interpretation_ms, 0);
}
