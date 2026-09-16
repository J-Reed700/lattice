//! # Tag DTOs
//!
//! Data Transfer Objects for tag management operations.
//!
//! These DTOs handle requests and responses for creating, updating, and assigning tags.

use serde::{Deserialize, Serialize};

/// Tag representation.
///
/// Simple DTO for tag data across application boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct TagDto {
    /// Tag identifier
    pub id: String,

    /// Tag name
    pub name: String,

    /// Optional color for UI display (hex color code)
    pub color: Option<String>,

    /// Optional description
    pub description: Option<String>,
}

impl TagDto {
    /// Get tag ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get tag name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get tag color
    pub fn color(&self) -> Option<&str> {
        self.color.as_deref()
    }

    /// Get tag description
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}

/// Request to create a new tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTagRequestDto {
    /// Tag name
    pub name: String,

    /// Optional color (hex color code)
    pub color: Option<String>,

    /// Optional description
    pub description: Option<String>,
}

/// Response from creating a tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTagResponseDto {
    /// The created tag
    pub tag: TagDto,

    /// Status message
    pub status: String,
}

/// Request to update an existing tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTagRequestDto {
    /// Tag ID to update
    pub id: String,

    /// Optional new name
    pub name: Option<String>,

    /// Optional new color
    pub color: Option<String>,

    /// Optional new description
    pub description: Option<String>,
}

/// Request to assign a tag to a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignTagRequestDto {
    /// Document ID to tag
    pub document_id: String,

    /// Tag ID to assign
    pub tag_id: String,
}

/// Response from assigning a tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignTagResponseDto {
    /// Status message
    pub status: String,

    /// The assigned tag
    pub tag: TagDto,
}

/// Request to remove a tag from a document.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct RemoveTagRequestDto {
    /// Document ID
    pub document_id: String,

    /// Tag ID to remove
    pub tag_id: String,
}

/// Response listing tags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTagsResponseDto {
    /// List of tags
    pub tags: Vec<TagDto>,

    /// Total count
    pub total: usize,
}

/// Tag with document count.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct TagWithCountDto {
    /// Tag identifier
    pub id: String,

    /// Tag name
    pub name: String,

    /// Optional color for UI display (hex color code)
    pub color: Option<String>,

    /// Creation timestamp
    pub created_at: String,

    /// Number of documents with this tag
    pub document_count: i64,
}

impl TagWithCountDto {
    /// Get tag ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get tag name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get document count
    pub fn count(&self) -> i64 {
        self.document_count
    }
}

/// Request to generate tags for a document using LLM.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct GenerateTagsRequestDto {
    /// Document ID
    pub document_id: String,

    /// Maximum number of tags to generate (default: 5)
    #[serde(default = "default_max_tags")]
    pub max_tags: usize,
}

fn default_max_tags() -> usize {
    5
}

/// Response from generating tags.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct GenerateTagsResponseDto {
    /// Generated tag names
    pub tags: Vec<String>,

    /// Status message
    pub status: String,
}

/// Request to apply multiple tags to a document.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ApplyTagsRequestDto {
    /// Document ID
    pub document_id: String,

    /// Tag names to apply (creates tags if they don't exist)
    pub tag_names: Vec<String>,
}

/// Response from applying tags.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ApplyTagsResponseDto {
    /// All tags now associated with the document
    pub tags: Vec<TagDto>,

    /// Status message
    pub status: String,
}

/// Request to search documents by tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchByTagRequestDto {
    /// Tag name to search for
    pub tag_name: String,
}

/// Response from searching by tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchByTagResponseDto {
    /// Document IDs with this tag
    pub document_ids: Vec<String>,

    /// Total count
    pub total: usize,
}

/// Request to auto-tag all untagged documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTagRequestDto {
    /// Maximum number of documents to process (default: 100)
    #[serde(default = "default_max_documents")]
    pub max_documents: usize,
}

fn default_max_documents() -> usize {
    100
}

/// Response from auto-tagging operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTagResponseDto {
    /// Number of documents successfully tagged
    pub tagged_count: usize,

    /// Status message
    pub status: String,
}

/// Request to delete a tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTagRequestDto {
    /// Tag ID to delete
    pub tag_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_dto_serialization() {
        let tag = TagDto {
            id: "tag-123".to_string(),
            name: "Important".to_string(),
            color: Some("#FF0000".to_string()),
            description: Some("Important documents".to_string()),
        };

        let json = serde_json::to_string(&tag).unwrap();
        let deserialized: TagDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "tag-123");
        assert_eq!(deserialized.name, "Important");
    }

    #[test]
    fn test_create_tag_request_dto_serialization() {
        let request = CreateTagRequestDto {
            name: "Work".to_string(),
            color: Some("#0000FF".to_string()),
            description: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CreateTagRequestDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "Work");
        assert_eq!(deserialized.color, Some("#0000FF".to_string()));
    }

    #[test]
    fn test_assign_tag_request_dto_serialization() {
        let request = AssignTagRequestDto {
            document_id: "doc-456".to_string(),
            tag_id: "tag-789".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: AssignTagRequestDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.document_id, "doc-456");
        assert_eq!(deserialized.tag_id, "tag-789");
    }

    #[test]
    fn test_list_tags_response_dto_serialization() {
        let response = ListTagsResponseDto {
            tags: vec![
                TagDto {
                    id: "tag-1".to_string(),
                    name: "Personal".to_string(),
                    color: None,
                    description: None,
                },
                TagDto {
                    id: "tag-2".to_string(),
                    name: "Work".to_string(),
                    color: Some("#FF0000".to_string()),
                    description: Some("Work related".to_string()),
                },
            ],
            total: 2,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ListTagsResponseDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total, 2);
        assert_eq!(deserialized.tags.len(), 2);
    }
}
