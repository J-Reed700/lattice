//! # Favorite Mapper
//!
//! Converts between favorite domain models and DTOs.

use crate::features::favorites::dto::FavoriteDto;

pub struct FavoriteMapper;

impl FavoriteMapper {
    pub fn create_dto(
        id: String,
        document_id: String,
        document_name: String,
        document_path: String,
        file_type: Option<String>,
        added_at: String,
    ) -> FavoriteDto {
        FavoriteDto {
            id,
            document_id,
            document_name,
            document_path,
            file_type,
            added_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_dto() {
        let dto = FavoriteMapper::create_dto(
            "fav-1".to_string(),
            "doc-123".to_string(),
            "test.txt".to_string(),
            "/path/to/test.txt".to_string(),
            Some("text/plain".to_string()),
            "2024-01-01T00:00:00Z".to_string(),
        );

        assert_eq!(dto.id, "fav-1");
        assert_eq!(dto.document_id, "doc-123");
        assert_eq!(dto.document_name, "test.txt");
        assert_eq!(dto.document_path, "/path/to/test.txt");
        assert_eq!(dto.file_type, Some("text/plain".to_string()));
        assert_eq!(dto.added_at, "2024-01-01T00:00:00Z");
    }
}
