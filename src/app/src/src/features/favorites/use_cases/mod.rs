//! Favorites feature — use cases.

pub mod add;
pub mod is_favorite;
pub mod list;
pub mod remove;

pub use add::AddFavoriteUseCase;
pub use is_favorite::IsFavoriteUseCase;
pub use list::ListFavoritesUseCase;
pub use remove::RemoveFavoriteUseCase;
