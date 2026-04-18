//! Favorites feature dependency injection.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::FavoritesRepositoryPort;
use crate::features::favorites::repository::FavoritesRepository;
use crate::features::favorites::use_cases::{
    AddFavoriteUseCase, IsFavoriteUseCase, ListFavoritesUseCase, RemoveFavoriteUseCase,
};

#[derive(Clone)]
pub struct FavoritesDi {
    pub favorites_repo: Arc<dyn FavoritesRepositoryPort>,
    pub add_favorite_use_case: Arc<AddFavoriteUseCase>,
    pub remove_favorite_use_case: Arc<RemoveFavoriteUseCase>,
    pub list_favorites_use_case: Arc<ListFavoritesUseCase>,
    pub is_favorite_use_case: Arc<IsFavoriteUseCase>,
}

pub fn build(db_pool: SqlitePool) -> FavoritesDi {
    let favorites_repo =
        Arc::new(FavoritesRepository::new(db_pool)) as Arc<dyn FavoritesRepositoryPort>;

    FavoritesDi {
        add_favorite_use_case: Arc::new(AddFavoriteUseCase::new(favorites_repo.clone())),
        remove_favorite_use_case: Arc::new(RemoveFavoriteUseCase::new(favorites_repo.clone())),
        list_favorites_use_case: Arc::new(ListFavoritesUseCase::new(favorites_repo.clone())),
        is_favorite_use_case: Arc::new(IsFavoriteUseCase::new(favorites_repo.clone())),
        favorites_repo,
    }
}
