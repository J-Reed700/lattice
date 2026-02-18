//! Favorites Use Cases

pub mod add_favorite;
pub mod is_favorite;
pub mod list_favorites;
pub mod remove_favorite;

pub use add_favorite::AddFavoriteUseCase;
pub use is_favorite::IsFavoriteUseCase;
pub use list_favorites::ListFavoritesUseCase;
pub use remove_favorite::RemoveFavoriteUseCase;
