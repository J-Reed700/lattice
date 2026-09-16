//! Favorites feature dependency injection.
//!
//! Commands and tool execution share one repository implementation.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::FavoritesRepositoryPort;
use crate::features::favorites::repository::FavoritesRepository;
use crate::interfaces::di::Container;

#[derive(Clone)]
pub struct FavoritesDi {
    pub favorites_repo: Arc<dyn FavoritesRepositoryPort>,
}

pub fn build(db_pool: SqlitePool) -> FavoritesDi {
    let favorites_repo =
        Arc::new(FavoritesRepository::new(db_pool)) as Arc<dyn FavoritesRepositoryPort>;
    FavoritesDi { favorites_repo }
}

/// Favorites' registrar surface on `Container`.
impl Container {
    pub fn favorites_repository(&self) -> Arc<dyn FavoritesRepositoryPort> {
        Arc::clone(self.library.favorites_repo())
    }
}
