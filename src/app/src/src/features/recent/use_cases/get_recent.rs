//! Get Recent Documents Use Case

use crate::features::recent::dto::{GetRecentDocumentsRequestDto, GetRecentDocumentsResponseDto};
use crate::application::ports::RecentDocumentsRepositoryPort;
use crate::shared::result::Result;
use std::sync::Arc;

pub struct GetRecentDocumentsUseCase {
    recent_repo: Arc<dyn RecentDocumentsRepositoryPort>,
}

impl GetRecentDocumentsUseCase {
    pub fn new(recent_repo: Arc<dyn RecentDocumentsRepositoryPort>) -> Self {
        Self { recent_repo }
    }

    pub async fn execute(
        &self,
        request: GetRecentDocumentsRequestDto,
    ) -> Result<GetRecentDocumentsResponseDto> {
        let documents = self.recent_repo.get_recent_documents(request.limit).await?;
        let total = documents.len();

        Ok(GetRecentDocumentsResponseDto { documents, total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::recent::dto::RecentDocumentDto;
    use async_trait::async_trait;

    struct MockRecentDocumentsRepository {
        documents: Vec<RecentDocumentDto>,
    }

    impl MockRecentDocumentsRepository {
        fn new_with_documents() -> Self {
            Self {
                documents: vec![
                    RecentDocumentDto {
                        id: "recent-1".to_string(),
                        document_id: "doc-1".to_string(),
                        document_name: "recent1.txt".to_string(),
                        document_path: "/path/to/recent1.txt".to_string(),
                        file_type: Some("text/plain".to_string()),
                        last_accessed_at: "2024-01-02T00:00:00Z".to_string(),
                        access_count: 5,
                    },
                    RecentDocumentDto {
                        id: "recent-2".to_string(),
                        document_id: "doc-2".to_string(),
                        document_name: "recent2.pdf".to_string(),
                        document_path: "/path/to/recent2.pdf".to_string(),
                        file_type: Some("application/pdf".to_string()),
                        last_accessed_at: "2024-01-01T00:00:00Z".to_string(),
                        access_count: 3,
                    },
                ],
            }
        }
    }

    #[async_trait]
    impl RecentDocumentsRepositoryPort for MockRecentDocumentsRepository {
        async fn track_access(&self, _document_id: &str) -> Result<()> {
            unreachable!()
        }

        async fn get_recent_documents(&self, limit: usize) -> Result<Vec<RecentDocumentDto>> {
            Ok(self.documents.iter().take(limit).cloned().collect())
        }

        async fn clear_recent_history(&self, _before_date: Option<&str>) -> Result<usize> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn test_get_recent_documents_success() {
        let mock_repo = Arc::new(MockRecentDocumentsRepository::new_with_documents());
        let use_case = GetRecentDocumentsUseCase::new(mock_repo);

        let request = GetRecentDocumentsRequestDto { limit: 20 };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.total, 2);
        assert_eq!(response.documents.len(), 2);
        assert_eq!(response.documents[0].document_id, "doc-1");
        assert_eq!(response.documents[0].access_count, 5);
    }

    #[tokio::test]
    async fn test_get_recent_documents_with_limit() {
        let mock_repo = Arc::new(MockRecentDocumentsRepository::new_with_documents());
        let use_case = GetRecentDocumentsUseCase::new(mock_repo);

        let request = GetRecentDocumentsRequestDto { limit: 1 };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.total, 1);
        assert_eq!(response.documents.len(), 1);
        assert_eq!(response.documents[0].document_id, "doc-1");
    }
}
