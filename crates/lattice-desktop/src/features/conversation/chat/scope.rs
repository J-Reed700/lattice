//! Per-turn scope snapshot: resolves the conversation's space scope, explicit
//! document focus, confined `document_context`, and recent document metadata
//! with a single `retrieval_document_scope` read.

use std::collections::HashSet;
use std::time::Instant;

use lattice_core::domain::conversation::DocumentReference;

use super::focus::FocusScope;
use super::ports::ChatRuntime;
use super::retrieval::{
    keep_references_in_scope, load_recent_document_metadata, load_space_document_scope,
    RecentDocumentMetadata, SpaceDocumentScope,
};

/// Resolved scope state carried through prepare, triage, and retrieval so the
/// turn never queries `retrieval_document_scope` more than once.
#[derive(Debug, Clone, Default)]
pub(in crate::features::conversation::chat) struct ScopeSnapshot {
    pub(in crate::features::conversation::chat) space: Option<SpaceDocumentScope>,
    pub(in crate::features::conversation::chat) resolved: bool,
    pub(in crate::features::conversation::chat) focus: FocusScope,
    pub(in crate::features::conversation::chat) recent_document: Option<RecentDocumentMetadata>,
    pub(in crate::features::conversation::chat) load_ms: u64,
}

impl ScopeSnapshot {
    /// Resolves space scope, focus intersection, confined `document_context`,
    /// and recent-document metadata in one pass.
    ///
    /// A `closed_book` turn performs zero database queries and clears all
    /// `document_context`.
    pub(in crate::features::conversation::chat) async fn resolve(
        container: &dyn ChatRuntime,
        conversation_id: &str,
        requested_focus_ids: Option<&Vec<String>>,
        raw_document_context: Vec<DocumentReference>,
        closed_book: bool,
    ) -> (Self, Vec<DocumentReference>) {
        if closed_book {
            return (Self::default(), Vec::new());
        }

        let start = Instant::now();
        let raw_space = load_space_document_scope(container, conversation_id).await;
        let focus = match raw_space {
            Some(ref space) => FocusScope::against_scope(
                requested_focus_ids,
                &space.document_ids,
                false,
                conversation_id,
            ),
            None => FocusScope::against_scope(
                requested_focus_ids,
                &HashSet::new(),
                false,
                conversation_id,
            ),
        };

        let (space, confined_refs) = match raw_space {
            Some(mut space) => {
                focus.confine(&mut space.document_ids);
                let confined_refs =
                    keep_references_in_scope(raw_document_context, &space.document_ids);
                (Some(space), confined_refs)
            }
            None => (None, Vec::new()),
        };

        let recent_document = if confined_refs.is_empty() {
            None
        } else {
            load_recent_document_metadata(container, &confined_refs).await
        };

        (
            Self {
                space,
                resolved: true,
                focus,
                recent_document,
                load_ms: super::elapsed_ms(start),
            },
            confined_refs,
        )
    }

    /// True when the space scope resolved cleanly, focus did not fail closed,
    /// and zero documents are allowed in scope.
    pub(in crate::features::conversation::chat) fn is_known_empty_vault(&self) -> bool {
        self.resolved
            && !self.focus.blocks_everything()
            && self
                .space
                .as_ref()
                .is_some_and(|space| space.document_ids.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::conversation::chat::ports::{ChatRecords, MemoryVector};
    use crate::features::conversation::chat::test_runtime::FakeRuntime;
    use crate::features::conversation::workspace_dto::ConversationWebSourceSnapshotDto;
    use async_trait::async_trait;
    use lattice_core::application::contracts::search::{CorpusDocument, CorpusPassage};
    use lattice_core::application::ports::conversation_memory::{
        CommittedMemorySnapshot, ConversationMemoryPort, ConversationMemoryReadPort,
        MemoryCommitCandidate, MemoryCommitError, MemoryCommitPreconditions, RecallCandidates,
        ResolvedSpan, SourcePage, SourceReadLimits, SourceSpanRef,
    };
    use lattice_core::application::ports::document_repository_port::DocumentRepositoryPort;
    use lattice_core::application::ports::repository_port::RepositoryPort;
    use lattice_core::application::ports::{DocumentRepository, EmbeddingPort};
    use lattice_core::domain::conversation::memory::{
        ConversationMemoryState, MemoryItem, MemorySnapshot, SourceMessage, SuppressedSpan,
    };
    use lattice_core::domain::conversation::ChatModelChoice;
    use lattice_core::domain::entities::document::{
        Category, DocumentStatus, Language, StoredDocument,
    };
    use lattice_core::domain::entities::Document;
    use lattice_core::domain::value_objects::checksum::Checksum;
    use lattice_core::shared::error::Result;
    use lattice_core::shared::persistence::Page;
    use lattice_core::shared::types::{DocumentId, ValidatedFilePath};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountingRecords {
        scope: Option<(String, HashSet<String>)>,
        scope_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ConversationMemoryPort for CountingRecords {
        async fn load_snapshot(&self, _conversation_id: &str) -> Result<MemorySnapshot> {
            unreachable!("unused")
        }
        async fn page_source_messages(
            &self,
            _conversation_id: &str,
            _after_sequence: i64,
            _through_sequence: i64,
            _limits: SourceReadLimits,
        ) -> Result<SourcePage> {
            unreachable!("unused")
        }
        async fn page_recent_source_messages(
            &self,
            _conversation_id: &str,
            _after_sequence: i64,
            _through_sequence: i64,
            _limits: SourceReadLimits,
        ) -> Result<SourcePage> {
            unreachable!("unused")
        }
        async fn read_source_spans(
            &self,
            _conversation_id: &str,
            _spans: &[SourceSpanRef],
            _limits: SourceReadLimits,
        ) -> Result<Vec<ResolvedSpan>> {
            unreachable!("unused")
        }
        async fn suppressed_spans(
            &self,
            _conversation_id: &str,
            _message_ids: &[String],
        ) -> Result<Vec<SuppressedSpan>> {
            unreachable!("unused")
        }
        async fn forgotten_labels(&self, _conversation_id: &str) -> Result<Vec<String>> {
            unreachable!("unused")
        }
        async fn commit_memory(
            &self,
            _preconditions: &MemoryCommitPreconditions,
            _candidate: &MemoryCommitCandidate,
        ) -> std::result::Result<CommittedMemorySnapshot, MemoryCommitError> {
            unreachable!("unused")
        }
        async fn record_memory_error(&self, _conversation_id: &str, _code: &str) -> Result<()> {
            unreachable!("unused")
        }
        async fn mark_rebuild_required(&self, _conversation_id: &str, _code: &str) -> Result<()> {
            unreachable!("unused")
        }
        async fn page_inactive_items(
            &self,
            _conversation_id: &str,
            _offset: i64,
            _limit: i64,
        ) -> Result<Vec<MemoryItem>> {
            unreachable!("unused")
        }
        async fn load_state(&self, _conversation_id: &str) -> Result<ConversationMemoryState> {
            unreachable!("unused")
        }
    }

    #[async_trait]
    impl ConversationMemoryReadPort for CountingRecords {
        async fn search_source_messages(
            &self,
            _conversation_id: &str,
            _query: &str,
            _exact_terms: &[String],
            _limit: usize,
        ) -> Result<RecallCandidates> {
            unreachable!("unused")
        }
        async fn read_adjacent_turns(
            &self,
            _conversation_id: &str,
            _sequence: i64,
            _before: usize,
            _after: usize,
            _limits: SourceReadLimits,
        ) -> Result<Vec<SourceMessage>> {
            unreachable!("unused")
        }
        async fn read_messages(
            &self,
            _conversation_id: &str,
            _message_ids: &[String],
            _limits: SourceReadLimits,
        ) -> Result<Vec<SourceMessage>> {
            unreachable!("unused")
        }
        async fn read_sequence_range(
            &self,
            _conversation_id: &str,
            _from_sequence: i64,
            _to_sequence: i64,
            _limits: SourceReadLimits,
        ) -> Result<SourcePage> {
            unreachable!("unused")
        }
    }

    #[async_trait]
    impl ChatRecords for CountingRecords {
        async fn model_choice(&self, _conversation_id: &str) -> Result<ChatModelChoice> {
            unreachable!("unused")
        }
        async fn set_model_choice(
            &self,
            _conversation_id: &str,
            _choice: &ChatModelChoice,
        ) -> Result<()> {
            unreachable!("unused")
        }
        async fn retrieval_document_scope(
            &self,
            _conversation_id: &str,
        ) -> Result<Option<(String, HashSet<String>)>> {
            self.scope_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.scope.clone())
        }
        async fn retrieval_catalog(
            &self,
            _allowed_ids: &HashSet<String>,
        ) -> Result<Vec<CorpusDocument>> {
            unreachable!("unused")
        }
        async fn retrieval_mean_chunk_chars(
            &self,
            _allowed_ids: &HashSet<String>,
        ) -> Result<Option<usize>> {
            unreachable!("unused")
        }
        async fn retrieval_openings(
            &self,
            _selected_ids: &[String],
            _allowed_ids: &HashSet<String>,
            _chunks_per_document: usize,
        ) -> Result<Vec<CorpusPassage>> {
            unreachable!("unused")
        }
        async fn retrieval_section_passages(
            &self,
            _identifiers: &[String],
            _allowed: &HashSet<String>,
        ) -> Result<Vec<CorpusPassage>> {
            unreachable!("unused")
        }
        async fn retrieval_locations(
            &self,
            _chunk_ids: &[String],
        ) -> Result<HashMap<String, (Option<String>, Option<u32>, bool)>> {
            unreachable!("unused")
        }
        async fn retrieval_neighbors(
            &self,
            _chunk_id: &str,
            _allowed: &HashSet<String>,
        ) -> Result<Vec<CorpusPassage>> {
            unreachable!("unused")
        }
        async fn web_source_snapshot(
            &self,
            _conversation_id: &str,
            _url: &str,
        ) -> Result<Option<ConversationWebSourceSnapshotDto>> {
            unreachable!("unused")
        }
        async fn store_web_source_snapshot(
            &self,
            _conversation_id: &str,
            _url: &str,
            _title: Option<String>,
            _content: String,
            _truncated: bool,
        ) -> Result<()> {
            unreachable!("unused")
        }
        async fn add_cited_web_source(
            &self,
            _conversation_id: &str,
            _url: String,
            _title: Option<String>,
            _excerpt: Option<String>,
            _score: Option<f32>,
        ) -> Result<()> {
            unreachable!("unused")
        }
        async fn persist_memory_vector(&self, _vector: MemoryVector<'_>) -> Result<u64> {
            unreachable!("unused")
        }
        fn with_recall_embedding(
            &self,
            _embedding: Option<Arc<dyn EmbeddingPort>>,
        ) -> Arc<dyn ChatRecords> {
            unreachable!("unused")
        }
    }

    struct MapDocRepo(HashMap<String, Document>);

    #[async_trait]
    impl RepositoryPort<Document> for MapDocRepo {
        async fn find_by_id(&self, id: &str) -> Result<Option<Document>> {
            Ok(self.0.get(id).cloned())
        }
        async fn find_all(&self) -> Result<Vec<Document>> {
            Ok(self.0.values().cloned().collect())
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
            Ok(self.0.len())
        }
        async fn exists(&self, id: &str) -> Result<bool> {
            Ok(self.0.contains_key(id))
        }
    }

    #[async_trait]
    impl DocumentRepositoryPort for MapDocRepo {
        async fn find_file_path_by_id(&self, document_id: &str) -> Result<String> {
            Ok(self
                .0
                .get(document_id)
                .map(|doc| doc.file_path().to_string_lossy().into_owned())
                .unwrap_or_default())
        }
        async fn document_exists(&self, document_id: &str) -> Result<bool> {
            Ok(self.0.contains_key(document_id))
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
            Ok(self.0.len() as i64)
        }
        async fn count_chunks(&self) -> Result<i64> {
            Ok(0)
        }
        async fn find_all_paginated(&self, _page: Page) -> Result<Vec<Document>> {
            Ok(self.0.values().cloned().collect())
        }
    }

    impl DocumentRepository for MapDocRepo {}

    fn make_doc(id: &str, title: &str) -> Document {
        let now = chrono::Utc::now();
        Document::with_id(StoredDocument {
            id: DocumentId::from_string(id.to_string()).unwrap(),
            file_path: ValidatedFilePath::new(std::path::PathBuf::from(format!("/vault/{title}")))
                .unwrap(),
            file_name: title.to_string(),
            file_type: Some("md".to_string()),
            mime_type: "text/markdown".to_string(),
            size_bytes: 64,
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
            word_count: 10,
            content: "body".to_string(),
        })
    }

    fn doc_ref(document_id: &str, score: f32, seconds: i64) -> DocumentReference {
        DocumentReference {
            document_id: document_id.to_string(),
            chunk_id: None,
            relevance_score: Some(score),
            added_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH
                + chrono::Duration::seconds(seconds),
        }
    }

    #[tokio::test]
    async fn closed_book_makes_zero_scope_queries_and_clears_document_context() {
        let scope_calls = Arc::new(AtomicUsize::new(0));
        let runtime = FakeRuntime {
            records: Some(Arc::new(CountingRecords {
                scope: Some((
                    "space_general".to_string(),
                    HashSet::from(["doc-1".to_string()]),
                )),
                scope_calls: Arc::clone(&scope_calls),
            })),
            documents: Some(Arc::new(MapDocRepo(HashMap::from([(
                "doc-1".to_string(),
                make_doc("doc-1", "Guide.md"),
            )])))),
            ..FakeRuntime::default()
        };
        let focus_ids = vec!["doc-1".to_string()];
        let refs = vec![doc_ref("doc-1", 0.9, 100)];

        let (snapshot, confined) =
            ScopeSnapshot::resolve(&runtime, "conv-closed", Some(&focus_ids), refs, true).await;

        assert_eq!(scope_calls.load(Ordering::SeqCst), 0);
        assert!(!snapshot.resolved);
        assert!(snapshot.space.is_none());
        assert!(snapshot.recent_document.is_none());
        assert!(!snapshot.focus.is_requested());
        assert!(confined.is_empty());
        assert!(!snapshot.is_known_empty_vault());
    }

    #[tokio::test]
    async fn resolve_queries_scope_once_intersects_focus_and_drops_out_of_scope_recent_doc() {
        let scope_calls = Arc::new(AtomicUsize::new(0));
        let runtime = FakeRuntime {
            records: Some(Arc::new(CountingRecords {
                scope: Some((
                    "space_1".to_string(),
                    HashSet::from(["doc-allowed".to_string(), "doc-space-only".to_string()]),
                )),
                scope_calls: Arc::clone(&scope_calls),
            })),
            documents: Some(Arc::new(MapDocRepo(HashMap::from([
                (
                    "doc-allowed".to_string(),
                    make_doc("doc-allowed", "Allowed.md"),
                ),
                (
                    "doc-space-only".to_string(),
                    make_doc("doc-space-only", "SpaceOnly.md"),
                ),
                (
                    "doc-foreign".to_string(),
                    make_doc("doc-foreign", "Foreign.md"),
                ),
            ])))),
            ..FakeRuntime::default()
        };

        // Case 1: focus narrows space to `doc-allowed`; `doc-space-only` and `doc-foreign` are
        // dropped from both `space.document_ids` and `document_context`, leaving `Allowed.md`.
        let focus_ids = vec!["doc-allowed".to_string(), "doc-foreign".to_string()];
        let refs = vec![
            doc_ref("doc-allowed", 0.5, 100),
            doc_ref("doc-space-only", 0.95, 200),
            doc_ref("doc-foreign", 0.99, 200),
        ];
        let (snapshot, confined) =
            ScopeSnapshot::resolve(&runtime, "conv-1", Some(&focus_ids), refs, false).await;

        assert_eq!(scope_calls.load(Ordering::SeqCst), 1);
        assert!(snapshot.resolved);
        assert!(snapshot.focus.is_requested());
        assert_eq!(snapshot.focus.reported_count(), Some(1));
        assert_eq!(
            snapshot.space.as_ref().map(|s| &s.document_ids),
            Some(&HashSet::from(["doc-allowed".to_string()]))
        );
        assert_eq!(confined.len(), 1);
        assert_eq!(confined[0].document_id, "doc-allowed");
        assert_eq!(
            snapshot.recent_document.as_ref().map(|m| m.title.as_str()),
            Some("Allowed.md")
        );
        assert!(!snapshot.is_known_empty_vault());

        // Case 2: if the only referenced document is outside the allowed scope, `recent_document`
        // is dropped to `None`.
        let refs_outside = vec![doc_ref("doc-foreign", 0.99, 200)];
        let (snapshot_outside, confined_outside) =
            ScopeSnapshot::resolve(&runtime, "conv-1", None, refs_outside, false).await;
        assert_eq!(scope_calls.load(Ordering::SeqCst), 2);
        assert!(confined_outside.is_empty());
        assert!(snapshot_outside.recent_document.is_none());
    }

    #[tokio::test]
    async fn resolve_detects_known_empty_vault_only_when_not_blocked_by_focus() {
        let scope_calls = Arc::new(AtomicUsize::new(0));
        let runtime = FakeRuntime {
            records: Some(Arc::new(CountingRecords {
                scope: Some(("space_general".to_string(), HashSet::new())),
                scope_calls: Arc::clone(&scope_calls),
            })),
            ..FakeRuntime::default()
        };

        let (empty_snapshot, _) =
            ScopeSnapshot::resolve(&runtime, "conv-empty", None, Vec::new(), false).await;
        assert!(empty_snapshot.is_known_empty_vault());

        let missing_focus = vec!["doc-missing".to_string()];
        let (blocked_snapshot, _) = ScopeSnapshot::resolve(
            &runtime,
            "conv-empty",
            Some(&missing_focus),
            Vec::new(),
            false,
        )
        .await;
        assert!(blocked_snapshot.focus.blocks_everything());
        assert!(!blocked_snapshot.is_known_empty_vault());
    }
}
