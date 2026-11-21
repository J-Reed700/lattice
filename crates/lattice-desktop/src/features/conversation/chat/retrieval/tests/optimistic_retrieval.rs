use super::lazy_loading::{
    invoke_pipeline, new_search_decision, records_with_documents, sample_passage, FixedSearch,
    CONVERSATION,
};
use super::*;
use crate::features::conversation::chat::ports::ChatRecords;
use crate::features::conversation::chat::test_runtime::{
    FakeEmbedding, FakeRuntime, Scripted, TestVault,
};
use crate::features::conversation::chat::turn_record::TurnRecorder;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::ConversationServiceTrait;
use lattice_core::application::contracts::search::{
    QueryBranches, RankedBranch, RerankOptions, Reranked, SearchRequestDto,
};
use lattice_core::application::ports::document_repository_port::DocumentRepositoryPort;
use lattice_core::application::ports::hybrid_search::HybridSearchTrait;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::ports::repository_port::RepositoryPort;
use lattice_core::application::ports::{DocumentRepository, LLMPort};
use lattice_core::domain::entities::Document;
use lattice_core::domain::value_objects::checksum::Checksum;
use lattice_core::shared::persistence::{Database, Page};

fn sufficient_passages() -> Vec<SearchResultDto> {
    (0..3)
        .map(|index| SearchResultDto {
            id: format!("doc-1-{index}"),
            content: "What is the architecture? Lattice uses a layered Rust architecture."
                .to_string(),
            position: Some(index),
            ..sample_passage()
        })
        .collect()
}

async fn records_without_conversation_row() -> Arc<dyn ChatRecords> {
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
         INSERT INTO documents VALUES ('doc-1', 'Guide.md', '/vault/Guide.md', NULL, NULL);
         INSERT INTO text_chunks (id, document_id, content, chunk_index, section, page_number)
             VALUES ('doc-1-0', 'doc-1', 'Lattice architecture guide', 0, NULL, NULL);",
    )
    .execute(&pool)
    .await
    .unwrap();
    Arc::new(ConversationRepository::new(Database::new(pool)))
}

struct EscalatingSearch {
    pass1: Vec<SearchResultDto>,
    pass2: Vec<SearchResultDto>,
}

#[async_trait::async_trait]
impl HybridSearchTrait for EscalatingSearch {
    async fn execute_scoped(
        &self,
        _request: SearchRequestDto,
        _space_id: Option<&str>,
        _allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<SearchResponseDto> {
        Ok(SearchResponseDto {
            total: 0,
            results: Vec::new(),
            query_time_ms: 0,
        })
    }
    async fn search_branches(
        &self,
        query: &str,
        _space_id: Option<&str>,
        _allowed_document_ids: Option<&HashSet<String>>,
        _limit: usize,
        _vector_weight: f32,
        _bm25_weight: f32,
    ) -> QueryBranches {
        let results = if query.contains("lattice layers") {
            self.pass2.clone()
        } else {
            self.pass1.clone()
        };
        QueryBranches {
            branches: vec![RankedBranch {
                kind: lattice_core::application::contracts::search::BranchKind::Lexical,
                weight: 1.0,
                results,
            }],
            answered: 2,
        }
    }
    fn fuse(&self, branches: Vec<RankedBranch>, limit: usize) -> Vec<SearchResultDto> {
        let mut seen = HashSet::new();
        branches
            .into_iter()
            .rev()
            .flat_map(|branch| branch.results)
            .filter(|result| seen.insert(result.id.clone()))
            .take(limit)
            .collect()
    }
    async fn rerank(&self, response: SearchResponseDto, _options: &RerankOptions) -> Reranked {
        Reranked {
            response,
            applied: false,
        }
    }
}

struct OneDocRepo(Document);

#[async_trait::async_trait]
impl RepositoryPort<Document> for OneDocRepo {
    async fn find_by_id(&self, id: &str) -> Result<Option<Document>> {
        Ok((self.0.id().to_string() == id).then(|| self.0.clone()))
    }
    async fn find_all(&self) -> Result<Vec<Document>> {
        Ok(vec![self.0.clone()])
    }
    async fn save(&self, _entity: &Document) -> Result<()> {
        Ok(())
    }
    async fn save_batch(&self, _entities: &[Document]) -> Result<()> {
        Ok(())
    }
    async fn delete(&self, _id: &str) -> Result<()> {
        Ok(())
    }
    async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
        Ok(())
    }
    async fn count(&self) -> Result<usize> {
        Ok(1)
    }
    async fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.0.id().to_string() == id)
    }
}

#[async_trait::async_trait]
impl DocumentRepositoryPort for OneDocRepo {
    async fn find_file_path_by_id(&self, _document_id: &str) -> Result<String> {
        Ok(self.0.file_path().to_string_lossy().into_owned())
    }
    async fn document_exists(&self, document_id: &str) -> Result<bool> {
        Ok(self.0.id().to_string() == document_id)
    }
    async fn find_id_by_path(&self, _file_path: &str) -> Result<Option<String>> {
        Ok(None)
    }
    async fn delete(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }
    async fn delete_and_release_blob(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }
    async fn find_by_checksum(&self, _checksum: &Checksum) -> Result<Option<Document>> {
        Ok(None)
    }
    async fn count_documents(&self) -> Result<i64> {
        Ok(1)
    }
    async fn count_chunks(&self) -> Result<i64> {
        Ok(0)
    }
    async fn find_all_paginated(&self, _page: Page) -> Result<Vec<Document>> {
        Ok(vec![self.0.clone()])
    }
}

impl DocumentRepository for OneDocRepo {}

fn sample_document(id: &str, title: &str) -> Document {
    use lattice_core::domain::entities::document::{
        Category, DocumentStatus, Language, StoredDocument,
    };
    use lattice_core::shared::types::{DocumentId, ValidatedFilePath};

    let now = chrono::Utc::now();
    Document::with_id(StoredDocument {
        id: DocumentId::from_string(id.to_string()).unwrap(),
        file_path: ValidatedFilePath::new(std::path::PathBuf::from(format!("/vault/{title}")))
            .unwrap(),
        file_name: title.to_string(),
        file_type: Some("md".to_string()),
        mime_type: "text/markdown".to_string(),
        size_bytes: 128,
        modified_at: now,
        indexed_at: now,
        checksum: Checksum::new("a".repeat(64)).unwrap(),
        status: DocumentStatus::Indexed,
        error_message: None,
        language: Language::English,
        category: Category::Notes,
        quality_score: 1.0,
        access_count: 0,
        last_accessed_at: None,
        word_count: 20,
        content: "Lattice uses a layered Rust architecture.".to_string(),
    })
}

#[tokio::test]
async fn optimistic_pass_1_sufficient_never_loads_or_calls_utility_llm() {
    let utility = Scripted::new([]);
    let runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: sufficient_passages(),
            }),
        }),
        utility_llm: Some(utility.clone()),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };

    let outcome = invoke_pipeline(
        &runtime,
        &new_search_decision(),
        SearchFlags {
            force_kb_search: true,
            ..SearchFlags::default()
        },
        "req-optimistic-sufficient",
    )
    .await;

    assert!(outcome.kb_attempted);
    assert!(outcome.sufficiency.as_ref().is_some_and(|v| v.sufficient));
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 0);
    assert!(utility.requests().is_empty());
    assert!(outcome.sub_timings.kb_planner_skipped);
    assert_eq!(outcome.sub_timings.kb_corrective_retries, 0);
}

#[tokio::test]
async fn insufficient_pass_1_escalates_to_utility_llm_once_and_fuses_results() {
    let utility = Scripted::new([CompletionResponse::from_text(
        r#"{"queries":[{"query":"lattice layers architecture"}],"opening_document_ids":[],"start_at_beginning":"false"}"#,
    )]);
    let unrelated = (10..13)
        .map(|index| SearchResultDto {
            id: format!("doc-1-{index}"),
            content: "Parking spaces are assigned by the facilities team.".to_string(),
            position: Some(index),
            ..sample_passage()
        })
        .collect();
    let runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(EscalatingSearch {
                pass1: unrelated,
                pass2: sufficient_passages(),
            }),
        }),
        utility_llm: Some(utility.clone()),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };

    let outcome = invoke_pipeline(
        &runtime,
        &new_search_decision(),
        SearchFlags {
            force_kb_search: true,
            ..SearchFlags::default()
        },
        "req-escalate-on-demand",
    )
    .await;

    assert!(outcome.kb_attempted);
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 1);
    assert_eq!(utility.requests().len(), 1);
    assert_eq!(outcome.sub_timings.kb_corrective_retries, 1);
    assert!(!outcome.sub_timings.kb_planner_skipped);
    assert!(outcome.sufficiency.as_ref().is_some_and(|v| v.sufficient));
    assert!(outcome
        .sources
        .iter()
        .any(|source| source.content.contains("layered Rust architecture")));
}

async fn run_turn(
    runtime: &FakeRuntime,
    request_id: &str,
    message: &str,
    standalone_question: Option<&str>,
    decision: &RouterDecisionOutcome,
    flags: SearchFlags,
    doc_context: &[lattice_core::domain::conversation::DocumentReference],
    scope: Option<&crate::features::conversation::chat::scope::ScopeSnapshot>,
) -> RetrievalPipelineOutcome {
    let conversations: Arc<dyn ConversationServiceTrait> = runtime.conversations.clone();
    let llm: Arc<dyn LLMPort> = Scripted::new([]);
    let recorder = TurnRecorder::silent(CONVERSATION, request_id);
    run_retrieval_pipeline_with_scope(
        runtime,
        ChatTurn {
            conv_service: &conversations,
            conversation_id: CONVERSATION,
            request_id,
            message,
            standalone_question,
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
        doc_context,
        EvidenceBudget::new(20_000),
        None,
        scope,
    )
    .await
}

#[tokio::test]
async fn whole_document_intent_plans_opening_documents_upfront() {
    let utility = Scripted::new([CompletionResponse::from_text(
        r#"{"queries":[{"query":"Guide overview"}],"opening_document_ids":["d1"],"start_at_beginning":"false"}"#,
    )]);
    let overview_passages = (1..=3)
        .map(|index| SearchResultDto {
            id: format!("doc-1-{index}"),
            content: "Guide overview: Lattice architecture guide.".to_string(),
            position: Some(index),
            ..sample_passage()
        })
        .collect();
    let runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: overview_passages,
            }),
        }),
        utility_llm: Some(utility.clone()),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };

    let outcome = run_turn(
        &runtime,
        "req-whole-doc-intent",
        "Where do I start with these documents?",
        None,
        &new_search_decision(),
        SearchFlags {
            force_kb_search: true,
            ..SearchFlags::default()
        },
        &[],
        None,
    )
    .await;

    assert!(outcome.kb_attempted);
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 1);
    assert_eq!(utility.requests().len(), 1);
    assert!(!outcome.sub_timings.kb_planner_skipped);
    assert_eq!(outcome.sub_timings.kb_corrective_retries, 0);
    assert_eq!(
        outcome
            .search_response
            .results
            .first()
            .and_then(|result| result.metadata.get("retrievalMethod")),
        Some(&serde_json::json!("document_opening"))
    );
    assert_eq!(
        outcome
            .search_response
            .results
            .first()
            .and_then(|result| result.document_id.as_deref()),
        Some("doc-1")
    );
}

#[tokio::test]
async fn whole_document_intent_in_standalone_question_plans_upfront() {
    let utility = Scripted::new([CompletionResponse::from_text(
        r#"{"queries":[{"query":"Guide overview"}],"opening_document_ids":["d1"],"start_at_beginning":"false"}"#,
    )]);
    let overview_passages = (1..=3)
        .map(|index| SearchResultDto {
            id: format!("doc-1-{index}"),
            content: "Guide overview: Lattice architecture guide.".to_string(),
            position: Some(index),
            ..sample_passage()
        })
        .collect();
    let runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: overview_passages,
            }),
        }),
        utility_llm: Some(utility.clone()),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };

    let outcome = run_turn(
        &runtime,
        "req-standalone-whole-doc",
        "what about those?",
        Some("Where do I start with these documents?"),
        &new_search_decision(),
        SearchFlags {
            force_kb_search: true,
            ..SearchFlags::default()
        },
        &[],
        None,
    )
    .await;

    assert!(outcome.kb_attempted);
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 1);
    assert_eq!(utility.requests().len(), 1);
    assert!(!outcome.sub_timings.kb_planner_skipped);
}

#[tokio::test]
async fn use_last_document_skips_kb_when_reopened_and_falls_back_when_missing() {
    let doc = sample_document("doc-1", "Guide.md");
    let doc_ref = lattice_core::domain::conversation::DocumentReference {
        document_id: "doc-1".to_string(),
        chunk_id: None,
        relevance_score: Some(0.9),
        added_at: chrono::Utc::now(),
    };
    let decision = RouterDecisionOutcome {
        action: RouterAction::UseLastDocument,
        recent_doc_meta: Some(RecentDocumentMetadata {
            title: "Guide.md".to_string(),
        }),
        clarify_message: None,
        router_may_search: true,
    };

    let reopened_runtime = FakeRuntime {
        documents: Some(Arc::new(OneDocRepo(doc))),
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: sufficient_passages(),
            }),
        }),
        utility_llm: Some(Scripted::new([])),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };
    let reopened_outcome = run_turn(
        &reopened_runtime,
        "req-use-last-doc",
        "Tell me more about the layers",
        None,
        &decision,
        SearchFlags::default(),
        std::slice::from_ref(&doc_ref),
        None,
    )
    .await;

    assert!(reopened_outcome.followup_context.is_some());
    assert!(!reopened_outcome.kb_attempted);
    assert_eq!(reopened_runtime.loader_counts.embedding_calls(), 0);
    assert_eq!(reopened_runtime.loader_counts.utility_loader_calls(), 0);

    // When the referenced document cannot be reopened, fall back to searching the vault.
    let missing_runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: sufficient_passages(),
            }),
        }),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };
    let missing_outcome = run_turn(
        &missing_runtime,
        "req-use-last-doc-missing",
        "What is the architecture?",
        None,
        &decision,
        SearchFlags::default(),
        &[doc_ref],
        None,
    )
    .await;

    assert!(missing_outcome.followup_context.is_none());
    assert!(missing_outcome.kb_attempted);
    assert_eq!(missing_runtime.loader_counts.embedding_calls(), 1);
    assert!(!missing_outcome.sources.is_empty());
}

#[tokio::test]
async fn resolved_scope_snapshot_is_reused_without_querying_document_scope() {
    // Records has no row for `CONVERSATION`, so calling `retrieval_document_scope`
    // would return `None` and fail closed.
    let runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_without_conversation_row().await,
            search: Arc::new(FixedSearch {
                passages: sufficient_passages(),
            }),
        }),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };
    let snapshot = crate::features::conversation::chat::scope::ScopeSnapshot {
        space: Some(SpaceDocumentScope {
            space_id: DEFAULT_SPACE_ID.to_string(),
            document_ids: HashSet::from(["doc-1".to_string()]),
        }),
        resolved: true,
        focus: crate::features::conversation::chat::focus::FocusScope::default(),
        recent_document: None,
        load_ms: 17,
    };

    let outcome = run_turn(
        &runtime,
        "req-scope-snapshot",
        "What is the architecture?",
        None,
        &new_search_decision(),
        SearchFlags {
            force_kb_search: true,
            ..SearchFlags::default()
        },
        &[],
        Some(&snapshot),
    )
    .await;

    assert!(outcome.kb_attempted);
    assert_eq!(outcome.searched_documents, 1);
    assert_eq!(outcome.sub_timings.kb_scope_load_ms, 17);
    assert!(!outcome.sources.is_empty());
    assert_eq!(runtime.loader_counts.embedding_calls(), 1);

    // Conversely, a resolved snapshot with `space: None` fails closed without querying Records
    // even when `conversations` does contain the conversation row.
    let valid_records_runtime = FakeRuntime {
        vault: Some(TestVault {
            records: records_with_documents(true).await,
            search: Arc::new(FixedSearch {
                passages: sufficient_passages(),
            }),
        }),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };
    let convs: Arc<dyn ConversationServiceTrait> = valid_records_runtime.conversations.clone();
    let llm: Arc<dyn LLMPort> = Scripted::new([]);
    let valid_recorder = TurnRecorder::silent(CONVERSATION, "req-scope-snapshot-none");
    let cancel = tokio_util::sync::CancellationToken::new();
    let none_snapshot = crate::features::conversation::chat::scope::ScopeSnapshot {
        space: None,
        resolved: true,
        focus: crate::features::conversation::chat::focus::FocusScope::default(),
        recent_document: None,
        load_ms: 23,
    };

    let kb_outcome = super::super::kb_retrieval::run_kb_retrieval(
        &valid_records_runtime,
        ChatTurn {
            conv_service: &convs,
            conversation_id: CONVERSATION,
            request_id: "req-scope-snapshot-none",
            message: "What is the architecture?",
            standalone_question: None,
            llm: &llm,
            highlight_terms: &[],
            focus: &crate::features::conversation::chat::focus::FocusScope::default(),
            recorder: &valid_recorder,
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
        Some(&none_snapshot),
    )
    .await;

    assert_eq!(
        kb_outcome.kb_unavailable_reason.as_deref(),
        Some("the conversation scope could not be resolved")
    );
    assert_eq!(kb_outcome.timings.kb_scope_load_ms, 23);
    assert_eq!(kb_outcome.searched_documents, 0);
    assert_eq!(valid_records_runtime.loader_counts.embedding_calls(), 0);
}
