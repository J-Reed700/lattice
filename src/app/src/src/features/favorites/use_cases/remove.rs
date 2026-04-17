//! Remove Favorite Use Case

use crate::features::favorites::dto::{RemoveFavoriteRequestDto, RemoveFavoriteResponseDto};
use crate::application::ports::FavoritesRepositoryPort;
use crate::shared::result::Result;
use std::sync::Arc;

pub struct RemoveFavoriteUseCase {
    favorites_repo: Arc<dyn FavoritesRepositoryPort>,
}

impl RemoveFavoriteUseCase {
    pub fn new(favorites_repo: Arc<dyn FavoritesRepositoryPort>) -> Self {
        Self { favorites_repo }
    }

    pub async fn execute(
        &self,
        request: RemoveFavoriteRequestDto,
    ) -> Result<RemoveFavoriteResponseDto> {
        self.favorites_repo
            .remove_favorite(&request.document_id)
            .await?;

        Ok(RemoveFavoriteResponseDto {
            status: "success".to_string(),
        })
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
        fn new() -> Self {
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
        async fn add_favorite(&self, document_id: &str) -> Result<FavoriteDto> {
            let favorite = FavoriteDto {
                id: "fav-1".to_string(),
                document_id: document_id.to_string(),
                document_name: "test.txt".to_string(),
                document_path: "/path/to/test.txt".to_string(),
                file_type: Some("text/plain".to_string()),
                added_at: "2024-01-01T00:00:00Z".to_string(),
            };

            self.favorites.lock().unwrap().push(favorite.clone());
            Ok(favorite)
        }

        async fn remove_favorite(&self, document_id: &str) -> Result<()> {
            self.favorites
                .lock()
                .unwrap()
                .retain(|f| f.document_id != document_id);
            Ok(())
        }

        async fn list_favorites(&self) -> Result<Vec<FavoriteDto>> {
            Ok(self.favorites.lock().unwrap().clone())
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
    async fn test_remove_favorite_success() {
        let mock_repo = Arc::new(MockFavoritesRepository::new());
        let use_case = RemoveFavoriteUseCase::new(mock_repo.clone());

        let request = RemoveFavoriteRequestDto {
            document_id: "doc-123".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, "success");
        assert_eq!(mock_repo.favorites.lock().unwrap().len(), 0);
    }
}
