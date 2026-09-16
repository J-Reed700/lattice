//! Favorites Repository Port

use crate::application::contracts::favorites::FavoriteRecord;
use crate::shared::result::Result;
use async_trait::async_trait;

#[async_trait]
pub trait FavoritesRepositoryPort: Send + Sync {
    async fn add_favorite(&self, document_id: &str) -> Result<FavoriteRecord>;

    async fn remove_favorite(&self, document_id: &str) -> Result<()>;

    async fn list_favorites(&self) -> Result<Vec<FavoriteRecord>>;

    async fn is_favorite(&self, document_id: &str) -> Result<bool>;
}
