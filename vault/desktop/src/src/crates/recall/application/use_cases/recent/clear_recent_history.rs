//! Clear Recent History Use Case

use crate::application::dtos::{ClearRecentHistoryRequestDto, ClearRecentHistoryResponseDto};
use crate::application::ports::RecentDocumentsRepositoryPort;
use crate::shared::result::Result;
use std::sync::Arc;

pub struct ClearRecentHistoryUseCase {
    recent_repo: Arc<dyn RecentDocumentsRepositoryPort>,
}

impl ClearRecentHistoryUseCase {
    pub fn new(recent_repo: Arc<dyn RecentDocumentsRepositoryPort>) -> Self {
        Self { recent_repo }
    }

    pub async fn execute(
        &self,
        request: ClearRecentHistoryRequestDto,
    ) -> Result<ClearRecentHistoryResponseDto> {
        let cleared_count = self
            .recent_repo
            .clear_recent_history(request.before_date.as_deref())
            .await?;

        Ok(ClearRecentHistoryResponseDto {
            cleared_count,
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
        count: Mutex<usize>,
    }

    impl MockRecentDocumentsRepository {
        fn new(count: usize) -> Self {
            Self {
                count: Mutex::new(count),
            }
        }
    }

    #[async_trait]
    impl RecentDocumentsRepositoryPort for MockRecentDocumentsRepository {
        async fn track_access(&self, _document_id: &str) -> Result<()> {
            unreachable!()
        }

        async fn get_recent_documents(&self, _limit: usize) -> Result<Vec<RecentDocumentDto>> {
            unreachable!()
        }

        async fn clear_recent_history(&self, _before_date: Option<&str>) -> Result<usize> {
            let mut count = self.count.lock().unwrap();
            let cleared = *count;
            *count = 0;
            Ok(cleared)
        }
    }

    #[tokio::test]
    async fn test_clear_recent_history_success() {
        let mock_repo = Arc::new(MockRecentDocumentsRepository::new(5));
        let use_case = ClearRecentHistoryUseCase::new(mock_repo);

        let request = ClearRecentHistoryRequestDto { before_date: None };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, "success");
        assert_eq!(response.cleared_count, 5);
    }

    #[tokio::test]
    async fn test_clear_recent_history_with_date() {
        let mock_repo = Arc::new(MockRecentDocumentsRepository::new(3));
        let use_case = ClearRecentHistoryUseCase::new(mock_repo);

        let request = ClearRecentHistoryRequestDto {
            before_date: Some("2024-01-01T00:00:00Z".to_string()),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, "success");
        assert_eq!(response.cleared_count, 3);
    }
}
