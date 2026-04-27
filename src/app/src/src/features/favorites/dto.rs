//! Favorites DTOs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteDto {
    pub id: String,
    pub document_id: String,
    pub document_name: String,
    pub document_path: String,
    pub file_type: Option<String>,
    pub added_at: String,
}

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
