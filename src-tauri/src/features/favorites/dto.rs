//! Favorites DTOs

use serde::{Deserialize, Serialize};

pub use crate::application::contracts::favorites::FavoriteRecord as FavoriteDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddFavoriteRequestDto {
    pub document_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddFavoriteResponseDto {
    pub favorite: FavoriteDto,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveFavoriteRequestDto {
    pub document_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveFavoriteResponseDto {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFavoritesResponseDto {
    pub favorites: Vec<FavoriteDto>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsFavoriteRequestDto {
    pub document_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsFavoriteResponseDto {
    pub is_favorite: bool,
}
