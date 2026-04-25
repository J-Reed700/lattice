//! Favorites feature dependency injection.
//!
//! Tauri commands in `commands.rs` use raw sqlx via internal helpers,
//! not the repository port. The port + concrete impl exist only because
//! `function_calling/executor.rs` consumes `Arc<dyn FavoritesRepositoryPort>`
//! at construction.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::FavoritesRepositoryPort;
use crate::features::favorites::repository::FavoritesRepository;

#[derive(Clone)]
pub struct FavoritesDi {
    pub favorites_repo: Arc<dyn FavoritesRepositoryPort>,
}

pub fn build(db_pool: SqlitePool) -> FavoritesDi {
    let favorites_repo =
        Arc::new(FavoritesRepository::new(db_pool)) as Arc<dyn FavoritesRepositoryPort>;
    FavoritesDi { favorites_repo }
}
