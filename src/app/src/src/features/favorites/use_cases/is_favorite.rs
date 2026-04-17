//! Is Favorite Use Case

use crate::features::favorites::dto::{IsFavoriteRequestDto, IsFavoriteResponseDto};
use crate::application::ports::FavoritesRepositoryPort;
use crate::shared::result::Result;
use std::sync::Arc;

pub struct IsFavoriteUseCase {
    favorites_repo: Arc<dyn FavoritesRepositoryPort>,
}

impl IsFavoriteUseCase {
    pub fn new(favorites_repo: Arc<dyn FavoritesRepositoryPort>) -> Self {
        Self { favorites_repo }
    }

    pub async fn execute(&self, request: IsFavoriteRequestDto) -> Result<IsFavoriteResponseDto> {
        let is_favorite = self
            .favorites_repo
            .is_favorite(&request.document_id)
            .await?;

        Ok(IsFavoriteResponseDto { is_favorite })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::favorites::dto::FavoriteDto;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockFavoritesRepository {
        favorites: Mutex<Vec<FavoriteDto>>,
    }

    impl MockFavoritesRepository {
        fn new_with_favorite() -> Self {
            Self {
                favorites: Mutex::new(vec![FavoriteDto {
                    id: "fav-1".to_string(),
                    document_id: "doc-123".to_string(),
                    document_name: "test.txt".to_string(),
                    document_path: "/path/to/test.txt".to_string(),
                    file_type: Some("text/plain".to_string()),
                    added_at: "2024-01-01T00:00:00Z".to_string(),
                }]),
            }
        }
    }

    #[async_trait]
    impl FavoritesRepositoryPort for MockFavoritesRepository {
        async fn add_favorite(&self, _document_id: &str) -> Result<FavoriteDto> {
            unreachable!()
        }

        async fn remove_favorite(&self, _document_id: &str) -> Result<()> {
            unreachable!()
        }

        async fn list_favorites(&self) -> Result<Vec<FavoriteDto>> {
            unreachable!()
        }

        async fn is_favorite(&self, document_id: &str) -> Result<bool> {
            Ok(self
                .favorites
                .lock()
                .unwrap()
                .iter()
                .any(|f| f.document_id == document_id))
        }
    }

    #[tokio::test]
    async fn test_is_favorite_true() {
        let mock_repo = Arc::new(MockFavoritesRepository::new_with_favorite());
        let use_case = IsFavoriteUseCase::new(mock_repo);

        let request = IsFavoriteRequestDto {
            document_id: "doc-123".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();
        assert!(response.is_favorite);
    }

    #[tokio::test]
    async fn test_is_favorite_false() {
        let mock_repo = Arc::new(MockFavoritesRepository::new_with_favorite());
        let use_case = IsFavoriteUseCase::new(mock_repo);

        let request = IsFavoriteRequestDto {
            document_id: "doc-456".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();
        assert!(!response.is_favorite);
    }
}
