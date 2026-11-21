//! Unit tests for unified turn triage, zero-LLM fast paths, and custom router fallback.
use super::classify;
use super::prepare::PreparedTurn;
use super::tests::Folder;
use super::*;
use crate::features::conversation::chat::retrieval::{
    run_retrieval_pipeline_with_scope, RecentDocumentMetadata, SpaceDocumentScope,
};
use crate::features::conversation::chat::router::RouterAction;
use crate::features::conversation::chat::scope::ScopeSnapshot;
use crate::features::conversation::chat::test_runtime::{FakeEmbedding, FakeRuntime, Scripted};
use crate::features::conversation::ConversationServiceTrait;
use async_trait::async_trait;
use lattice_core::application::ports::document_repository_port::DocumentRepositoryPort;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::ports::repository_port::RepositoryPort;
use lattice_core::application::ports::DocumentRepository;
use lattice_core::application::services::context_assembler::EvidenceBudget;
use lattice_core::domain::conversation::DocumentReference;
use lattice_core::domain::entities::document::{
    Category, DocumentStatus, Language, StoredDocument,
};
use lattice_core::domain::entities::Document;
use lattice_core::domain::value_objects::checksum::Checksum;
use lattice_core::shared::types::{DocumentId, ValidatedFilePath};
use std::collections::HashSet;

fn recent_scope(title: &str) -> (ScopeSnapshot, Vec<DocumentReference>) {
    let refs = vec![DocumentReference {
        document_id: "doc-1".to_string(),
        chunk_id: None,
        relevance_score: Some(0.95),
        added_at: chrono::Utc::now(),
    }];
    let scope = ScopeSnapshot {
        space: Some(SpaceDocumentScope {
            space_id: "space_general".to_string(),
            document_ids: HashSet::from(["doc-1".to_string()]),
        }),
        resolved: true,
        focus: FocusScope::default(),
        recent_document: Some(RecentDocumentMetadata {
            title: title.to_string(),
        }),
        load_ms: 1,
    };
    (scope, refs)
}

fn turn_with_recent_doc(
    runtime: &FakeRuntime,
    chat_llm: Arc<Scripted>,
    message: &str,
) -> PreparedTurn {
    let mut turn = PreparedTurn::for_test(
        "triage-conv",
        "triage-turn",
        chat_llm,
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    let (scope, refs) = recent_scope("Roadmap.md");
    turn.message = message.to_string();
    turn.scope = scope;
    turn.document_context = refs;
    turn.settings.llm.router.enabled = true;
    turn
}

#[tokio::test]
async fn turn2_with_recent_doc_runs_single_unified_call_and_zero_router_loads() {
    let cases = [
        (
            r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"standaloneQuestion":"More on Roadmap.md","documentAction":"use_last_document","clarifyQuestion":"","rationale":"follows recent doc","confidence":0.92}"#,
            RouterAction::UseLastDocument,
            false,
            Some("use_last_document"),
            None,
        ),
        (
            r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":false,"standaloneQuestion":"Q3 budget numbers","documentAction":"new_search","clarifyQuestion":"","rationale":"different topic","confidence":0.88}"#,
            RouterAction::NewSearch,
            false,
            Some("new_search"),
            None,
        ),
        (
            r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":false,"standaloneQuestion":"","documentAction":"clarify","clarifyQuestion":"Do you mean Roadmap.md or another document?","rationale":"ambiguous","confidence":0.55}"#,
            RouterAction::Clarify,
            false,
            Some("clarify"),
            Some("Do you mean Roadmap.md or another document?"),
        ),
        (
            r#"{"needsKnowledgeBase":false,"needsWeb":false,"isFollowup":false,"standaloneQuestion":"","documentAction":"none","clarifyQuestion":"","rationale":"general CS","confidence":0.95}"#,
            RouterAction::NewSearch,
            true,
            None,
            None,
        ),
    ];

    for (reply, expected_action, expected_skip_kb, expected_record_action, expected_clarify) in
        cases
    {
        let utility_llm = Scripted::new([CompletionResponse::from_text(reply)]);
        let chat_llm = Scripted::new([]);
        let router_llm = Scripted::new([]);
        let runtime = FakeRuntime {
            utility_llm: Some(utility_llm.clone()),
            router_llm: Some(router_llm.clone()),
            ..FakeRuntime::default()
        };
        let turn = turn_with_recent_doc(&runtime, chat_llm.clone(), "Tell me more");
        let mut metrics = ConversationFlowTimingMetrics::default();

        let (inferred, route) = classify::triage_turn(&runtime, &turn, None, &mut metrics)
            .await
            .unwrap();

        assert_eq!(utility_llm.requests().len(), 1, "{reply}");
        assert_eq!(chat_llm.requests().len(), 0, "{reply}");
        assert_eq!(router_llm.requests().len(), 0, "{reply}");
        assert_eq!(runtime.loader_counts.utility_loader_calls(), 1, "{reply}");
        assert_eq!(runtime.loader_counts.router_llm_calls(), 0, "{reply}");
        assert_eq!(route.decision.action, expected_action, "{reply}");
        assert_eq!(inferred.flags.skip_auto_kb, expected_skip_kb, "{reply}");
        assert_eq!(
            route.record.as_ref().map(|r| r.action.as_str()),
            expected_record_action,
            "{reply}"
        );
        assert_eq!(
            route.decision.clarify_message.as_deref(),
            expected_clarify,
            "{reply}"
        );
    }
}

#[tokio::test]
async fn empty_vault_in_strict_sources_mode_runs_zero_llm_calls() {
    let utility_llm = Scripted::new([]);
    let chat_llm = Scripted::new([]);
    let router_llm = Scripted::new([]);
    let runtime = FakeRuntime {
        utility_llm: Some(utility_llm.clone()),
        router_llm: Some(router_llm.clone()),
        ..FakeRuntime::default()
    };
    let mut turn = PreparedTurn::for_test(
        "empty-vault-conv",
        "empty-vault-turn",
        chat_llm.clone(),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    turn.message = "What is in my notes?".to_string();
    turn.requested_flags = SearchFlags::from_preferences(Some(&ToolPreferences {
        strict_sources: true,
        ..Default::default()
    }));
    turn.settings.llm.router.enabled = true;
    turn.scope = ScopeSnapshot {
        space: Some(SpaceDocumentScope {
            space_id: "space_general".to_string(),
            document_ids: HashSet::new(),
        }),
        resolved: true,
        focus: FocusScope::default(),
        recent_document: None,
        load_ms: 1,
    };

    let mut metrics = ConversationFlowTimingMetrics::default();
    let (inferred, route) = classify::triage_turn(&runtime, &turn, None, &mut metrics)
        .await
        .unwrap();

    assert_eq!(utility_llm.requests().len(), 0);
    assert_eq!(chat_llm.requests().len(), 0);
    assert_eq!(router_llm.requests().len(), 0);
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 0);
    assert_eq!(runtime.loader_counts.router_llm_calls(), 0);
    assert!(inferred.flags.strict_sources);
    assert_eq!(route.decision.action, RouterAction::NewSearch);
    assert!(route.record.is_none());
}

#[tokio::test]
async fn force_followup_mode_with_recent_doc_resolves_use_last_document_with_zero_llm_calls() {
    let utility_llm = Scripted::new([]);
    let chat_llm = Scripted::new([]);
    let router_llm = Scripted::new([]);
    let runtime = FakeRuntime {
        utility_llm: Some(utility_llm.clone()),
        router_llm: Some(router_llm.clone()),
        ..FakeRuntime::default()
    };
    let prefs = ToolPreferences {
        followup_mode: true,
        ..Default::default()
    };
    let mut turn = turn_with_recent_doc(&runtime, chat_llm.clone(), "Go on");
    turn.requested_flags = SearchFlags::from_preferences(Some(&prefs));

    let mut metrics = ConversationFlowTimingMetrics::default();
    let (_inferred, route) = classify::triage_turn(&runtime, &turn, Some(&prefs), &mut metrics)
        .await
        .unwrap();

    assert_eq!(utility_llm.requests().len(), 0);
    assert_eq!(chat_llm.requests().len(), 0);
    assert_eq!(router_llm.requests().len(), 0);
    assert_eq!(runtime.loader_counts.utility_loader_calls(), 0);
    assert_eq!(runtime.loader_counts.router_llm_calls(), 0);
    assert_eq!(route.decision.action, RouterAction::UseLastDocument);
    assert_eq!(
        route
            .decision
            .recent_doc_meta
            .as_ref()
            .map(|m| m.title.as_str()),
        Some("Roadmap.md")
    );
    assert!(route.record.is_none());
}

#[tokio::test]
async fn custom_router_prompt_template_still_delegates_to_router_llm_when_not_skipped() {
    let utility_llm = Scripted::new([CompletionResponse::from_text(
        r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"standaloneQuestion":"More on Roadmap","confidence":0.9}"#,
    )]);
    let chat_llm = Scripted::new([]);
    let router_llm = Scripted::new([CompletionResponse::from_text(
        r#"{"action":"use_last_document","confidence":0.95,"rationale":"custom template"}"#,
    )]);
    let runtime = FakeRuntime {
        utility_llm: Some(utility_llm.clone()),
        router_llm: Some(router_llm.clone()),
        ..FakeRuntime::default()
    };
    let mut turn = turn_with_recent_doc(&runtime, chat_llm.clone(), "Expand section 2");
    turn.settings.llm.router.prompt_template =
        "Custom router: {query} / {recent_document_title}".to_string();

    let mut metrics = ConversationFlowTimingMetrics::default();
    let (inferred, route) = classify::triage_turn(&runtime, &turn, None, &mut metrics)
        .await
        .unwrap();

    assert_eq!(utility_llm.requests().len(), 1);
    assert_eq!(router_llm.requests().len(), 1);
    assert_eq!(chat_llm.requests().len(), 0);
    assert_eq!(runtime.loader_counts.router_llm_calls(), 1);
    assert!(inferred.flags.force_kb_search);
    assert_eq!(route.decision.action, RouterAction::UseLastDocument);
    assert_eq!(
        route.record.as_ref().map(|r| r.action.as_str()),
        Some("use_last_document")
    );
}

struct SingleDocRepo(Document);

#[async_trait]
impl RepositoryPort<Document> for SingleDocRepo {
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

#[async_trait]
impl DocumentRepositoryPort for SingleDocRepo {
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
    async fn find_all_paginated(
        &self,
        _page: lattice_core::shared::persistence::Page,
    ) -> Result<Vec<Document>> {
        Ok(vec![self.0.clone()])
    }
}

impl DocumentRepository for SingleDocRepo {}

fn roadmap_doc() -> Document {
    let now = chrono::Utc::now();
    Document::with_id(StoredDocument {
        id: DocumentId::from_string("doc-1".to_string()).unwrap(),
        file_path: ValidatedFilePath::new(std::path::PathBuf::from("/vault/Roadmap.md")).unwrap(),
        file_name: "Roadmap.md".to_string(),
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
        content: "Q4 milestones and timeline for Roadmap.".to_string(),
    })
}

#[tokio::test]
async fn triage_handoff_to_retrieval_preserves_use_last_document_and_clarify_when_kb_true() {
    // 1. needsKnowledgeBase: true + documentAction: "use_last_document" must reopen the document
    //    without triggering KB retrieval or loading embeddings.
    let use_last_reply = r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"standaloneQuestion":"More on Roadmap.md","documentAction":"use_last_document","clarifyQuestion":"","rationale":"continues doc","confidence":0.92}"#;
    let runtime = FakeRuntime {
        utility_llm: Some(Scripted::new([CompletionResponse::from_text(
            use_last_reply,
        )])),
        documents: Some(Arc::new(SingleDocRepo(roadmap_doc()))),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };
    let conv = runtime
        .conversations
        .create_conversation("handoff".into(), None)
        .await
        .unwrap();
    let mut turn = PreparedTurn::for_test(
        &conv.id.to_string(),
        "handoff-use-last",
        Scripted::new([]),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    let (scope, refs) = recent_scope("Roadmap.md");
    turn.message = "Tell me more".to_string();
    turn.scope = scope;
    turn.document_context = refs;
    turn.settings.llm.router.enabled = true;

    let mut metrics = ConversationFlowTimingMetrics::default();
    let (inferred, route) = classify::triage_turn(&runtime, &turn, None, &mut metrics)
        .await
        .unwrap();
    assert!(!inferred.flags.force_kb_search);
    assert_eq!(route.decision.action, RouterAction::UseLastDocument);

    let outcome = run_retrieval_pipeline_with_scope(
        &runtime,
        turn.chat_turn(),
        &turn.settings,
        &route.decision,
        inferred.flags,
        &turn.document_context,
        EvidenceBudget::new(4000),
        None,
        Some(&turn.scope),
    )
    .await;
    assert!(outcome.followup_context.is_some());
    assert!(!outcome.kb_attempted);
    assert_eq!(runtime.loader_counts.embedding_calls(), 0);

    // 2. needsKnowledgeBase: true + documentAction: "clarify" must short-circuit with the clarify
    //    question without triggering KB retrieval or loading embeddings.
    let clarify_reply = r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":false,"standaloneQuestion":"","documentAction":"clarify","clarifyQuestion":"Do you mean Roadmap.md?","rationale":"ambiguous","confidence":0.55}"#;
    let clarify_runtime = FakeRuntime {
        utility_llm: Some(Scripted::new([CompletionResponse::from_text(
            clarify_reply,
        )])),
        documents: Some(Arc::new(SingleDocRepo(roadmap_doc()))),
        embedding: Some(Arc::new(FakeEmbedding)),
        ..FakeRuntime::default()
    };
    let clarify_turn = turn_with_recent_doc(&clarify_runtime, Scripted::new([]), "And the date?");
    let (clarify_inferred, clarify_route) =
        classify::triage_turn(&clarify_runtime, &clarify_turn, None, &mut metrics)
            .await
            .unwrap();
    assert!(!clarify_inferred.flags.force_kb_search);
    assert_eq!(clarify_route.decision.action, RouterAction::Clarify);

    let clarify_outcome = run_retrieval_pipeline_with_scope(
        &clarify_runtime,
        clarify_turn.chat_turn(),
        &clarify_turn.settings,
        &clarify_route.decision,
        clarify_inferred.flags,
        &clarify_turn.document_context,
        EvidenceBudget::new(4000),
        None,
        Some(&clarify_turn.scope),
    )
    .await;
    assert_eq!(
        clarify_outcome.short_circuit_response.as_deref(),
        Some("Do you mean Roadmap.md?")
    );
    assert!(!clarify_outcome.kb_attempted);
    assert_eq!(clarify_runtime.loader_counts.embedding_calls(), 0);
}

#[tokio::test]
async fn malformed_triage_output_falls_back_to_new_search_never_clarify() {
    let utility_llm = Scripted::new([CompletionResponse::from_text("not valid json")]);
    let chat_llm = Scripted::new([]);
    let router_llm = Scripted::new([]);
    let runtime = FakeRuntime {
        utility_llm: Some(utility_llm.clone()),
        router_llm: Some(router_llm.clone()),
        ..FakeRuntime::default()
    };
    let turn = turn_with_recent_doc(&runtime, chat_llm, "Tell me more");
    let mut metrics = ConversationFlowTimingMetrics::default();

    let (_inferred, route) = classify::triage_turn(&runtime, &turn, None, &mut metrics)
        .await
        .unwrap();

    assert_eq!(route.decision.action, RouterAction::NewSearch);
    assert!(route.decision.clarify_message.is_none());
    assert!(route.record.is_none());
    assert_eq!(utility_llm.requests().len(), 1);
    assert_eq!(router_llm.requests().len(), 0);
}

/// A turn the user marked as a follow-up goes back to the recent document only
/// when the router is on. With it off, the turn searches afresh like any other.
#[tokio::test]
async fn a_followup_with_the_router_off_searches_afresh() {
    let utility_llm = Scripted::new([]);
    let router_llm = Scripted::new([]);
    let runtime = FakeRuntime {
        utility_llm: Some(utility_llm.clone()),
        router_llm: Some(router_llm.clone()),
        ..FakeRuntime::default()
    };
    let prefs = ToolPreferences {
        followup_mode: true,
        ..Default::default()
    };
    let mut turn = turn_with_recent_doc(&runtime, Scripted::new([]), "Go on");
    turn.requested_flags = SearchFlags::from_preferences(Some(&prefs));
    turn.settings.llm.router.enabled = false;

    let mut metrics = ConversationFlowTimingMetrics::default();
    let (_inferred, route) = classify::triage_turn(&runtime, &turn, Some(&prefs), &mut metrics)
        .await
        .unwrap();

    assert_eq!(route.decision.action, RouterAction::NewSearch);
    assert!(route.record.is_none());
    assert_eq!(utility_llm.requests().len(), 0);
    assert_eq!(router_llm.requests().len(), 0);
    assert_eq!(runtime.loader_counts.router_llm_calls(), 0);
}

/// An Explorer turn reads the library only when the user points it there.
/// Marking the turn as a follow-up does not, so the library document the
/// conversation last read stays closed.
#[tokio::test]
async fn an_explorer_followup_leaves_the_library_document_closed() {
    let utility_llm = Scripted::new([]);
    let router_llm = Scripted::new([]);
    let runtime = FakeRuntime {
        utility_llm: Some(utility_llm.clone()),
        router_llm: Some(router_llm.clone()),
        ..FakeRuntime::default()
    };
    let prefs = ToolPreferences {
        followup_mode: true,
        ..Default::default()
    };
    let mut turn = turn_with_recent_doc(&runtime, Scripted::new([]), "Go on");
    turn.requested_flags = SearchFlags::from_preferences(Some(&prefs));
    turn.source = Some(Arc::new(Folder));

    let mut metrics = ConversationFlowTimingMetrics::default();
    let (_inferred, route) = classify::triage_turn(&runtime, &turn, Some(&prefs), &mut metrics)
        .await
        .unwrap();

    assert_eq!(route.decision.action, RouterAction::NewSearch);
    assert!(route.decision.recent_doc_meta.is_none());
    assert!(!route.decision.router_may_search);
    assert!(route.record.is_none());
    assert_eq!(utility_llm.requests().len(), 0);
    assert_eq!(router_llm.requests().len(), 0);
    assert_eq!(runtime.loader_counts.router_llm_calls(), 0);
}

/// A greeting needs no model to say what it needs: it skips the vault search,
/// except in strict mode, where every answer comes from the sources.
#[tokio::test]
async fn a_greeting_skips_the_vault_search_except_in_strict_mode() {
    for (strict_sources, skips_vault) in [(false, true), (true, false)] {
        let utility_llm = Scripted::new([]);
        let router_llm = Scripted::new([]);
        let runtime = FakeRuntime {
            utility_llm: Some(utility_llm.clone()),
            router_llm: Some(router_llm.clone()),
            ..FakeRuntime::default()
        };
        let prefs = ToolPreferences {
            strict_sources,
            ..Default::default()
        };
        let mut turn = PreparedTurn::for_test(
            "greeting-conv",
            "greeting-turn",
            Scripted::new([]),
            runtime.conversations.clone(),
            runtime.turn_cancellation.clone(),
        )
        .unwrap();
        turn.message = "hello!".to_string();
        turn.requested_flags = SearchFlags::from_preferences(Some(&prefs));

        let mut metrics = ConversationFlowTimingMetrics::default();
        let (inferred, route) = classify::triage_turn(&runtime, &turn, Some(&prefs), &mut metrics)
            .await
            .unwrap();

        assert_eq!(
            inferred.flags.skip_auto_kb, skips_vault,
            "strict: {strict_sources}"
        );
        assert_eq!(
            route.decision.action,
            RouterAction::NewSearch,
            "strict: {strict_sources}"
        );
        assert_eq!(utility_llm.requests().len(), 0, "strict: {strict_sources}");
        assert_eq!(
            runtime.loader_counts.utility_loader_calls(),
            0,
            "strict: {strict_sources}"
        );
        assert_eq!(
            runtime.loader_counts.router_llm_calls(),
            0,
            "strict: {strict_sources}"
        );
    }
}

/// A turn that skips the vault never asks the router, custom template or not.
#[tokio::test]
async fn custom_router_prompt_template_skips_the_router_when_the_turn_skips_the_vault() {
    let utility_llm = Scripted::new([CompletionResponse::from_text(
        r#"{"needsKnowledgeBase":false,"needsWeb":false,"isFollowup":false,"confidence":0.95}"#,
    )]);
    let router_llm = Scripted::new([]);
    let runtime = FakeRuntime {
        utility_llm: Some(utility_llm.clone()),
        router_llm: Some(router_llm.clone()),
        ..FakeRuntime::default()
    };
    let mut turn = turn_with_recent_doc(&runtime, Scripted::new([]), "What is a hash map?");
    turn.settings.llm.router.prompt_template =
        "Custom router: {query} / {recent_document_title}".to_string();

    let mut metrics = ConversationFlowTimingMetrics::default();
    let (inferred, route) = classify::triage_turn(&runtime, &turn, None, &mut metrics)
        .await
        .unwrap();

    assert!(inferred.flags.skip_auto_kb);
    assert_eq!(route.decision.action, RouterAction::NewSearch);
    assert!(route.decision.recent_doc_meta.is_none());
    assert!(route.record.is_none());
    assert_eq!(utility_llm.requests().len(), 1);
    assert_eq!(router_llm.requests().len(), 0);
    assert_eq!(runtime.loader_counts.router_llm_calls(), 0);
}
