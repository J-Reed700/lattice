//! Smoke tests for Favorites plugin DTOs

use vault::features::favorites::commands::{
    FavoriteDocument, FavoriteOperation, FavoriteResponse,
};

#[test]
fn test_favorite_document_creation() {
    let fav = FavoriteDocument {
        id: "fav-123".to_string(),
        document_id: "doc-456".to_string(),
        document_name: "Test Document".to_string(),
        document_path: "/path/to/doc.md".to_string(),
        file_type: Some("markdown".to_string()),
        added_at: "2024-01-01T00:00:00Z".to_string(),
    };

    assert_eq!(fav.id, "fav-123");
    assert_eq!(fav.document_id, "doc-456");
    assert_eq!(fav.document_name, "Test Document");
}

#[test]
fn test_favorite_operation_add() {
    let op = FavoriteOperation::Add {
        document_id: "doc-123".to_string(),
    };

    if let FavoriteOperation::Add { document_id } = op {
        assert_eq!(document_id, "doc-123");
    } else {
        panic!("Expected Add variant");
    }
}

#[test]
fn test_favorite_operation_remove() {
    let op = FavoriteOperation::Remove {
        document_id: "doc-456".to_string(),
    };

    if let FavoriteOperation::Remove { document_id } = op {
        assert_eq!(document_id, "doc-456");
    } else {
        panic!("Expected Remove variant");
    }
}

#[test]
fn test_favorite_operation_get_all() {
    let op = FavoriteOperation::GetAll;
    matches!(op, FavoriteOperation::GetAll);
}

#[test]
fn test_favorite_operation_check() {
    let op = FavoriteOperation::Check {
        document_id: "doc-789".to_string(),
    };

    if let FavoriteOperation::Check { document_id } = op {
        assert_eq!(document_id, "doc-789");
    } else {
        panic!("Expected Check variant");
    }
}

#[test]
fn test_favorite_response_modified() {
    let response = FavoriteResponse::Modified;
    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("modified"));
}

#[test]
fn test_favorite_response_status() {
    let response = FavoriteResponse::Status(true);
    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("status"));
}
