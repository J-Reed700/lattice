//! # Recent Document Mapper
//!
//! Converts between recent document domain models and DTOs.

use crate::features::recent::dto::RecentDocumentDto;

pub struct RecentDocumentMapper;

impl RecentDocumentMapper {
    pub fn create_dto(
        id: String,
        document_id: String,
        document_name: String,
        document_path: String,
        file_type: Option<String>,
        last_accessed_at: String,
        access_count: i64,
    ) -> RecentDocumentDto {
        RecentDocumentDto {
            id,
            document_id,
            document_name,
            document_path,
            file_type,
            last_accessed_at,
            access_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_dto() {
        let dto = RecentDocumentMapper::create_dto(
            "recent-1".to_string(),
            "doc-123".to_string(),
            "test.txt".to_string(),
            "/path/to/test.txt".to_string(),
            Some("text/plain".to_string()),
            "2024-01-01T00:00:00Z".to_string(),
            5,
        );

        assert_eq!(dto.id, "recent-1");
        assert_eq!(dto.document_id, "doc-123");
        assert_eq!(dto.document_name, "test.txt");
        assert_eq!(dto.document_path, "/path/to/test.txt");
        assert_eq!(dto.file_type, Some("text/plain".to_string()));
        assert_eq!(dto.last_accessed_at, "2024-01-01T00:00:00Z");
        assert_eq!(dto.access_count, 5);
    }
}
