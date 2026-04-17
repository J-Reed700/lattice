//! List Favorites Use Case

use crate::features::favorites::dto::ListFavoritesResponseDto;
use crate::application::ports::FavoritesRepositoryPort;
use crate::shared::result::Result;
use std::sync::Arc;

pub struct ListFavoritesUseCase {
    favorites_repo: Arc<dyn FavoritesRepositoryPort>,
}

impl ListFavoritesUseCase {
    pub fn new(favorites_repo: Arc<dyn FavoritesRepositoryPort>) -> Self {
        Self { favorites_repo }
    }

    pub async fn execute(&self) -> Result<ListFavoritesResponseDto> {
        let favorites = self.favorites_repo.list_favorites().await?;
        let total = favorites.len();

        Ok(ListFavoritesResponseDto { favorites, total })
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
        fn new_with_favorites() -> Self {
            Self {
                favorites: Mutex::new(vec![
                    FavoriteDto {
                        id: "fav-1".to_string(),
                        document_id: "doc-1".to_string(),
                        document_name: "test1.txt".to_string(),
                        document_path: "/path/to/test1.txt".to_string(),
                        file_type: Some("text/plain".to_string()),
                        added_at: "2024-01-01T00:00:00Z".to_string(),
                    },
                    FavoriteDto {
                        id: "fav-2".to_string(),
                        document_id: "doc-2".to_string(),
                        document_name: "test2.pdf".to_string(),
                        document_path: "/path/to/test2.pdf".to_string(),
                        file_type: Some("application/pdf".to_string()),
                        added_at: "2024-01-02T00:00:00Z".to_string(),
                    },
                ]),
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
            Ok(self.favorites.lock().unwrap().clone())
        }

        async fn is_favorite(&self, _document_id: &str) -> Result<bool> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn test_list_favorites_success() {
        let mock_repo = Arc::new(MockFavoritesRepository::new_with_favorites());
        let use_case = ListFavoritesUseCase::new(mock_repo);

        let response = use_case.execute().await.unwrap();

        assert_eq!(response.total, 2);
        assert_eq!(response.favorites.len(), 2);
        assert_eq!(response.favorites[0].document_id, "doc-1");
        assert_eq!(response.favorites[1].document_id, "doc-2");
    }
}
