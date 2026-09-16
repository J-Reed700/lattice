//! Extract and Resolve Links Use Case (Composite)
//!
//! Combines wikilink parsing and resolution into a single operation.
//! This is a **composite use case** that orchestrates two other use cases.
//!
//! # Architecture
//! - **Composition Pattern**: Delegates to `ParseWikilinksUseCase` and `ResolveWikilinkUseCase`
//! - **Single Responsibility**: Coordinates parsing + resolution workflow
//! - **Async Orchestration**: Handles async execution of sub-use-cases
//!
//! # Workflow
//! 1. Parse wikilinks from document content
//! 2. Get all available documents from repository
//! 3. For each parsed link, resolve to document ID
//! 4. Return combined results with resolution metadata
//!
//! # Example
//! ```rust,no_run
//! let use_case = ExtractAndResolveLinksUseCase::new(
//!     parse_use_case,
//!     resolve_use_case,
//!     document_repository,
//! );
//! let request = ExtractAndResolveRequestDto {
//!     document_id: "doc-123".into(),
//!     content: "See [[todo]] and [[projects/ml|ML Project]]".into(),
//! };
//! let result = use_case.execute(request).await?;
//! assert_eq!(result.links.len(), 2);
//! ```

use crate::application::ports::RepositoryPort;
use crate::domain::entities::Document;
use crate::features::extraction::dto::{
    DocumentRefDto, ExtractAndResolveRequestDto, ExtractAndResolveResponseDto,
    ParseWikilinksRequestDto, ParseWikilinksResponseDto, ResolveWikilinkRequestDto,
    ResolveWikilinkResponseDto, ResolvedLinkDto,
};
use crate::features::extraction::use_cases::{ParseWikilinksUseCase, ResolveWikilinkUseCase};
use crate::shared::error::AppError;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait ParseWikilinksPort: Send + Sync {
    async fn execute(
        &self,
        request: ParseWikilinksRequestDto,
    ) -> Result<ParseWikilinksResponseDto, AppError>;
}

#[async_trait]
impl ParseWikilinksPort for ParseWikilinksUseCase {
    async fn execute(
        &self,
        request: ParseWikilinksRequestDto,
    ) -> Result<ParseWikilinksResponseDto, AppError> {
        self.execute(request).await
    }
}

#[async_trait]
pub trait ResolveWikilinkPort: Send + Sync {
    async fn execute(
        &self,
        request: ResolveWikilinkRequestDto,
    ) -> Result<ResolveWikilinkResponseDto, AppError>;
}

#[async_trait]
impl ResolveWikilinkPort for ResolveWikilinkUseCase {
    async fn execute(
        &self,
        request: ResolveWikilinkRequestDto,
    ) -> Result<ResolveWikilinkResponseDto, AppError> {
        self.execute(request).await
    }
}

pub struct ExtractAndResolveLinksUseCase {
    parse_wikilinks_use_case: Arc<dyn ParseWikilinksPort>,
    resolve_wikilink_use_case: Arc<dyn ResolveWikilinkPort>,
    document_repository: Arc<dyn RepositoryPort<Document>>,
}

impl ExtractAndResolveLinksUseCase {
    pub fn new(
        parse_wikilinks_use_case: Arc<dyn ParseWikilinksPort>,
        resolve_wikilink_use_case: Arc<dyn ResolveWikilinkPort>,
        document_repository: Arc<dyn RepositoryPort<Document>>,
    ) -> Self {
        Self {
            parse_wikilinks_use_case,
            resolve_wikilink_use_case,
            document_repository,
        }
    }

    pub async fn execute(
        &self,
        request: ExtractAndResolveRequestDto,
    ) -> Result<ExtractAndResolveResponseDto, AppError> {
        tracing::debug!(
            document_id = %request.document_id,
            content_length = request.content.len(),
            "Extracting and resolving wikilinks"
        );

        let parse_result = self
            .parse_wikilinks_use_case
            .execute(ParseWikilinksRequestDto {
                text: request.content.clone(),
                source_path: None,
            })
            .await?;

        tracing::debug!(
            parsed_links = parse_result.links.len(),
            "Parsed wikilinks, now resolving..."
        );

        let all_documents = self.document_repository.find_all().await?;

        let document_refs: Vec<DocumentRefDto> = all_documents
            .into_iter()
            .map(|agg| {
                let doc = &agg;
                DocumentRefDto {
                    document_id: doc.id().to_string(),
                    file_path: doc.file_path().display().to_string(),
                    title: None,
                }
            })
            .collect();

        tracing::debug!(
            available_documents = document_refs.len(),
            "Fetched available documents for resolution"
        );

        let mut resolved_links = Vec::new();

        for link in parse_result.links {
            let resolve_result = self
                .resolve_wikilink_use_case
                .execute(ResolveWikilinkRequestDto {
                    link_target: link.target.clone(),
                    source_document_id: Some(request.document_id.clone()),
                    available_documents: document_refs.clone(),
                })
                .await?;

            resolved_links.push(ResolvedLinkDto {
                link,
                resolved_document_id: resolve_result.resolved_document_id,
                confidence: resolve_result.confidence,
            });
        }

        tracing::info!(
            total_links = resolved_links.len(),
            resolved_count = resolved_links
                .iter()
                .filter(|l| l.resolved_document_id.is_some())
                .count(),
            "Successfully extracted and resolved wikilinks"
        );

        Ok(ExtractAndResolveResponseDto {
            links: resolved_links,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::extraction::dto::{
        ParseWikilinksResponseDto, ResolveWikilinkResponseDto, WikiLinkDto,
    };
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockParseWikilinksUseCase {
        response: Mutex<Vec<WikiLinkDto>>,
    }

    impl MockParseWikilinksUseCase {
        fn new() -> Self {
            Self {
                response: Mutex::new(vec![]),
            }
        }

        fn set_response(&self, links: Vec<WikiLinkDto>) {
            *self.response.lock().unwrap() = links;
        }
    }

    #[async_trait]
    impl ParseWikilinksPort for MockParseWikilinksUseCase {
        async fn execute(
            &self,
            _request: ParseWikilinksRequestDto,
        ) -> Result<ParseWikilinksResponseDto, AppError> {
            Ok(ParseWikilinksResponseDto {
                links: self.response.lock().unwrap().clone(),
            })
        }
    }

    struct MockResolveWikilinkUseCase {
        responses: Mutex<std::collections::HashMap<String, (Option<String>, f32)>>,
    }

    impl MockResolveWikilinkUseCase {
        fn new() -> Self {
            Self {
                responses: Mutex::new(std::collections::HashMap::new()),
            }
        }

        fn set_response(&self, target: &str, doc_id: Option<String>, confidence: f32) {
            self.responses
                .lock()
                .unwrap()
                .insert(target.to_string(), (doc_id, confidence));
        }
    }

    #[async_trait]
    impl ResolveWikilinkPort for MockResolveWikilinkUseCase {
        async fn execute(
            &self,
            request: ResolveWikilinkRequestDto,
        ) -> Result<ResolveWikilinkResponseDto, AppError> {
            let (resolved_document_id, confidence) = self
                .responses
                .lock()
                .unwrap()
                .get(&request.link_target)
                .cloned()
                .unwrap_or((None, 0.0));

            Ok(ResolveWikilinkResponseDto {
                resolved_document_id,
                resolved_path: None,
                confidence,
            })
        }
    }

    struct MockDocumentRepository {
        documents: Mutex<Vec<Document>>,
    }

    impl MockDocumentRepository {
        fn new() -> Self {
            Self {
                documents: Mutex::new(vec![]),
            }
        }

        fn set_documents(&self, docs: Vec<Document>) {
            *self.documents.lock().unwrap() = docs;
        }
    }

    #[async_trait]
    impl RepositoryPort<Document> for MockDocumentRepository {
        async fn find_by_id(&self, _id: &str) -> Result<Option<Document>, AppError> {
            unimplemented!()
        }

        async fn find_by_filter(
            &self,
            _filter: &dyn crate::application::ports::Filter,
        ) -> Result<Vec<Document>, AppError> {
            unimplemented!()
        }

        async fn find_all(&self) -> Result<Vec<Document>, AppError> {
            Ok(self.documents.lock().unwrap().clone())
        }

        async fn save(&self, _entity: &Document) -> Result<(), AppError> {
            unimplemented!()
        }

        async fn save_batch(&self, _entities: &[Document]) -> Result<(), AppError> {
            unimplemented!()
        }

        async fn delete(&self, _id: &str) -> Result<(), AppError> {
            unimplemented!()
        }

        async fn delete_batch(&self, _ids: &[&str]) -> Result<(), AppError> {
            unimplemented!()
        }

        async fn count(&self) -> Result<usize, AppError> {
            Ok(self.documents.lock().unwrap().len())
        }

        async fn exists(&self, _id: &str) -> Result<bool, AppError> {
            unimplemented!()
        }
    }

    fn create_test_document_aggregate(_id: &str, path: &str, _title: Option<&str>) -> Document {
        use crate::domain::value_objects::{Checksum, ChunkingStrategy, FileMetadata};
        use crate::shared::domain_types::ValidatedFilePath;
        use chrono::Utc;
        use std::path::PathBuf;

        let validated_path = ValidatedFilePath::new(PathBuf::from(path)).unwrap_or_else(|_| {
            ValidatedFilePath::new(std::env::temp_dir().join("test.md")).unwrap()
        });

        let metadata = FileMetadata::new(
            "test.md".to_string(),
            "text/markdown".to_string(),
            100,
            Utc::now(),
        )
        .unwrap();

        let checksum = Checksum::new("a".repeat(64)).unwrap();

        Document::from_file(
            validated_path,
            metadata,
            checksum,
            "test content".to_string(),
            ChunkingStrategy::FixedSize { size: 100 },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_extract_and_resolve_multiple_links() {
        let mock_parse = Arc::new(MockParseWikilinksUseCase::new());
        mock_parse.set_response(vec![
            WikiLinkDto {
                target: "note1".into(),
                display_text: None,
                header: None,
                line_number: 1,
            },
            WikiLinkDto {
                target: "note2".into(),
                display_text: None,
                header: None,
                line_number: 5,
            },
        ]);

        let mock_resolve = Arc::new(MockResolveWikilinkUseCase::new());
        mock_resolve.set_response("note1", Some("doc-id-1".into()), 1.0);
        mock_resolve.set_response("note2", Some("doc-id-2".into()), 0.8);

        let mock_repo = Arc::new(MockDocumentRepository::new());
        mock_repo.set_documents(vec![
            create_test_document_aggregate("doc-id-1", "/lattice/note1.md", Some("Note 1")),
            create_test_document_aggregate("doc-id-2", "/lattice/note2.md", Some("Note 2")),
        ]);

        let use_case = ExtractAndResolveLinksUseCase::new(
            mock_parse,
            mock_resolve,
            mock_repo as Arc<dyn RepositoryPort<Document>>,
        );

        let result = use_case
            .execute(ExtractAndResolveRequestDto {
                document_id: "doc-123".into(),
                content: "Some content with [[note1]] and [[note2]]".into(),
            })
            .await
            .unwrap();

        assert_eq!(result.links.len(), 2);
        assert_eq!(
            result.links[0].resolved_document_id,
            Some("doc-id-1".into())
        );
        assert_eq!(result.links[0].confidence, 1.0);
        assert_eq!(
            result.links[1].resolved_document_id,
            Some("doc-id-2".into())
        );
        assert_eq!(result.links[1].confidence, 0.8);
    }

    #[tokio::test]
    async fn test_extract_and_resolve_no_links() {
        let mock_parse = Arc::new(MockParseWikilinksUseCase::new());
        mock_parse.set_response(vec![]);

        let mock_resolve = Arc::new(MockResolveWikilinkUseCase::new());

        let mock_repo = Arc::new(MockDocumentRepository::new());
        mock_repo.set_documents(vec![]);

        let use_case = ExtractAndResolveLinksUseCase::new(
            mock_parse,
            mock_resolve,
            mock_repo as Arc<dyn RepositoryPort<Document>>,
        );

        let result = use_case
            .execute(ExtractAndResolveRequestDto {
                document_id: "doc-123".into(),
                content: "No links here".into(),
            })
            .await
            .unwrap();

        assert_eq!(result.links.len(), 0);
    }

    #[tokio::test]
    async fn test_extract_and_resolve_some_unresolvable() {
        let mock_parse = Arc::new(MockParseWikilinksUseCase::new());
        mock_parse.set_response(vec![
            WikiLinkDto {
                target: "exists".into(),
                display_text: None,
                header: None,
                line_number: 1,
            },
            WikiLinkDto {
                target: "missing".into(),
                display_text: None,
                header: None,
                line_number: 2,
            },
        ]);

        let mock_resolve = Arc::new(MockResolveWikilinkUseCase::new());
        mock_resolve.set_response("exists", Some("doc-1".into()), 1.0);
        mock_resolve.set_response("missing", None, 0.0);

        let mock_repo = Arc::new(MockDocumentRepository::new());
        mock_repo.set_documents(vec![create_test_document_aggregate(
            "doc-1",
            "/lattice/exists.md",
            Some("Exists"),
        )]);

        let use_case = ExtractAndResolveLinksUseCase::new(
            mock_parse,
            mock_resolve,
            mock_repo as Arc<dyn RepositoryPort<Document>>,
        );

        let result = use_case
            .execute(ExtractAndResolveRequestDto {
                document_id: "doc-123".into(),
                content: "[[exists]] and [[missing]]".into(),
            })
            .await
            .unwrap();

        assert_eq!(result.links.len(), 2);
        assert_eq!(result.links[0].resolved_document_id, Some("doc-1".into()));
        assert_eq!(result.links[1].resolved_document_id, None);
        assert_eq!(result.links[1].confidence, 0.0);
    }

    #[tokio::test]
    async fn test_extract_and_resolve_many_links_performance() {
        let mock_parse = Arc::new(MockParseWikilinksUseCase::new());
        let links: Vec<WikiLinkDto> = (0..15)
            .map(|i| WikiLinkDto {
                target: format!("note{}", i),
                display_text: None,
                header: None,
                line_number: i + 1,
            })
            .collect();
        mock_parse.set_response(links);

        let mock_resolve = Arc::new(MockResolveWikilinkUseCase::new());
        for i in 0..15 {
            mock_resolve.set_response(&format!("note{}", i), Some(format!("doc-{}", i)), 0.8);
        }

        let mock_repo = Arc::new(MockDocumentRepository::new());
        let docs: Vec<Document> = (0..15)
            .map(|i| {
                create_test_document_aggregate(
                    &format!("doc-{}", i),
                    &format!("/lattice/note{}.md", i),
                    Some(&format!("Note {}", i)),
                )
            })
            .collect();
        mock_repo.set_documents(docs);

        let use_case = ExtractAndResolveLinksUseCase::new(
            mock_parse,
            mock_resolve,
            mock_repo as Arc<dyn RepositoryPort<Document>>,
        );

        let result = use_case
            .execute(ExtractAndResolveRequestDto {
                document_id: "doc-main".into(),
                content: "Content with many links".into(),
            })
            .await
            .unwrap();

        assert_eq!(result.links.len(), 15);
        assert!(result
            .links
            .iter()
            .all(|l| l.resolved_document_id.is_some()));
    }
}
