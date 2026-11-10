//! Favorites feature dependency injection.
//!
//! Commands and tool execution share one repository implementation.

use std::sync::Arc;

use crate::features::favorites::repository::FavoritesRepository;
use crate::interfaces::di::Container;
use lattice_core::application::ports::FavoritesRepositoryPort;
use lattice_core::shared::persistence::Database;

#[derive(Clone)]
pub struct FavoritesDi {
    pub favorites_repo: Arc<dyn FavoritesRepositoryPort>,
}

pub fn build(database: Database) -> FavoritesDi {
    let favorites_repo =
        Arc::new(FavoritesRepository::new(database)) as Arc<dyn FavoritesRepositoryPort>;
    FavoritesDi { favorites_repo }
}

/// Favorites' registrar surface on `Container`.
impl Container {
    pub fn favorites_repository(&self) -> Arc<dyn FavoritesRepositoryPort> {
        Arc::clone(self.library.favorites_repo())
    }
}
