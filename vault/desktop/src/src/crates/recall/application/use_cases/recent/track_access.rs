//! Track Document Access Use Case

use crate::application::dtos::{TrackAccessRequestDto, TrackAccessResponseDto};
use crate::application::ports::RecentDocumentsRepositoryPort;
use crate::shared::result::Result;
use std::sync::Arc;

pub struct TrackAccessUseCase {
    recent_repo: Arc<dyn RecentDocumentsRepositoryPort>,
}

impl TrackAccessUseCase {
    pub fn new(recent_repo: Arc<dyn RecentDocumentsRepositoryPort>) -> Self {
        Self { recent_repo }
    }

    pub async fn execute(&self, request: TrackAccessRequestDto) -> Result<TrackAccessResponseDto> {
        self.recent_repo.track_access(&request.document_id).await?;

        Ok(TrackAccessResponseDto {
            status: "success".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::dtos::RecentDocumentDto;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockRecentDocumentsRepository {
        accesses: Mutex<Vec<String>>,
    }

    impl MockRecentDocumentsRepository {
        fn new() -> Self {
            Self {
                accesses: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl RecentDocumentsRepositoryPort for MockRecentDocumentsRepository {
        async fn track_access(&self, document_id: &str) -> Result<()> {
            self.accesses.lock().unwrap().push(document_id.to_string());
            Ok(())
        }

        async fn get_recent_documents(&self, _limit: usize) -> Result<Vec<RecentDocumentDto>> {
            unreachable!()
        }

        async fn clear_recent_history(&self, _before_date: Option<&str>) -> Result<usize> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn test_track_access_success() {
        let mock_repo = Arc::new(MockRecentDocumentsRepository::new());
        let use_case = TrackAccessUseCase::new(mock_repo.clone());

        let request = TrackAccessRequestDto {
            document_id: "doc-123".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, "success");
        assert_eq!(mock_repo.accesses.lock().unwrap().len(), 1);
        assert_eq!(mock_repo.accesses.lock().unwrap()[0], "doc-123");
    }
}
