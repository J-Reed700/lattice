//! # Tag Entity
//!
//! Tag entity within the domain layer.
//!
//! This is a pure domain entity with NO infrastructure concerns:
//! - No database IDs (handled by repositories)
//! - Just the core business data and logic
//! - Rich behavior for tag management

use crate::shared::domain_types::{TagId, TagName};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Tag entity for categorizing and organizing documents.
///
/// Tags provide a flexible way to categorize documents with user-defined labels.
///
/// ## Pure Domain Model
///
/// This entity contains only business logic. Infrastructure concerns like
/// database persistence and search indexes are handled by the infrastructure layer.
///
/// ## Invariants
///
/// - Tag name must be non-empty and <= 50 characters
/// - Tag name is unique (enforced by repository)
/// - Color must be valid hex format (#RRGGBB)
/// - Created timestamp is immutable
///
/// ## Example
///
/// ```rust,no_run
/// use vault_desktop::domain::entities::tag::Tag;
/// use vault_desktop::domain_types::TagName;
///
/// let name = TagName::new("rust".to_string()).unwrap();
/// let tag = Tag::new(name, "#ff5733".to_string());
///
/// assert_eq!(tag.name().as_str(), "rust");
/// assert_eq!(tag.color(), "#ff5733");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    id: TagId,
    name: TagName,
    color: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Tag {
    /// Create new tag.
    ///
    /// # Arguments
    ///
    /// * `name` - Validated tag name (non-empty, <= 50 characters)
    /// * `color` - Hex color code (#RRGGBB format)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::entities::tag::Tag;
    /// use vault_desktop::domain_types::TagName;
    ///
    /// let name = TagName::new("machine-learning".to_string()).unwrap();
    /// let tag = Tag::new(name, "#6366f1".to_string());
    /// assert!(!tag.id().as_str().is_empty());
    /// assert_eq!(tag.color(), "#6366f1");
    /// ```
    pub fn new(name: TagName, color: String) -> Self {
        let now = Utc::now();
        Self {
            id: TagId::new(),
            name,
            color,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create tag with existing ID (for reconstruction from storage).
    ///
    /// This is typically used by repositories when loading tags from storage.
    pub fn with_id(
        id: TagId,
        name: TagName,
        color: String,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            name,
            color,
            created_at,
            updated_at,
        }
    }

    /// Get tag ID.
    pub fn id(&self) -> &TagId {
        &self.id
    }

    /// Get tag name.
    pub fn name(&self) -> &TagName {
        &self.name
    }

    /// Get tag color.
    pub fn color(&self) -> &str {
        &self.color
    }

    /// Get creation timestamp.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// Get last update timestamp.
    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }

    /// Update tag name.
    ///
    /// Validates the new name before updating.
    pub fn rename(&mut self, new_name: TagName) {
        self.name = new_name;
        self.updated_at = Utc::now();
    }

    /// Update tag color.
    pub fn update_color(&mut self, new_color: String) {
        self.color = new_color;
        self.updated_at = Utc::now();
    }

    /// Create new tag with different name (builder-style).
    ///
    /// Returns a new tag instance with updated name and timestamp.
    pub fn with_name(mut self, new_name: TagName) -> Self {
        self.name = new_name;
        self.updated_at = Utc::now();
        self
    }

    /// Create new tag with different color (builder-style).
    ///
    /// Returns a new tag instance with updated color and timestamp.
    pub fn with_color(mut self, new_color: String) -> Self {
        self.color = new_color;
        self.updated_at = Utc::now();
        self
    }

    /// Check if tag name matches query (case-insensitive).
    pub fn matches_query(&self, query: &str) -> bool {
        self.name
            .as_str()
            .to_lowercase()
            .contains(&query.to_lowercase())
    }

    /// Check if tag name starts with prefix (case-insensitive).
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.name
            .as_str()
            .to_lowercase()
            .starts_with(&prefix.to_lowercase())
    }

    /// Get display name (same as tag name).
    pub fn display_name(&self) -> &str {
        self.name.as_str()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_creation() {
        let name = TagName::new("rust".to_string()).unwrap();
        let tag = Tag::new(name.clone(), "#ff5733".to_string());

        assert_eq!(tag.name(), &name);
        assert_eq!(tag.color(), "#ff5733");
        assert!(!tag.id().as_str().is_empty());
        assert!(tag.created_at() <= &Utc::now());
        assert_eq!(tag.created_at(), tag.updated_at());
    }

    #[test]
    fn test_tag_with_id() {
        let id = TagId::new();
        let name = TagName::new("python".to_string()).unwrap();
        let now = Utc::now();

        let tag = Tag::with_id(id.clone(), name.clone(), "#6366f1".to_string(), now, now);

        assert_eq!(tag.id(), &id);
        assert_eq!(tag.name(), &name);
        assert_eq!(tag.color(), "#6366f1");
        assert_eq!(tag.created_at(), &now);
        assert_eq!(tag.updated_at(), &now);
    }

    #[test]
    fn test_tag_rename() {
        let name = TagName::new("old-name".to_string()).unwrap();
        let mut tag = Tag::new(name, "#ff5733".to_string());
        let old_updated_at = *tag.updated_at();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let new_name = TagName::new("new-name".to_string()).unwrap();
        tag.rename(new_name.clone());

        assert_eq!(tag.name(), &new_name);
        assert!(tag.updated_at() > &old_updated_at);
    }

    #[test]
    fn test_tag_update_color() {
        let name = TagName::new("rust".to_string()).unwrap();
        let mut tag = Tag::new(name, "#ff5733".to_string());
        let old_updated_at = *tag.updated_at();

        std::thread::sleep(std::time::Duration::from_millis(10));

        tag.update_color("#00ff00".to_string());

        assert_eq!(tag.color(), "#00ff00");
        assert!(tag.updated_at() > &old_updated_at);
    }

    #[test]
    fn test_tag_matches_query() {
        let name = TagName::new("Machine-Learning".to_string()).unwrap();
        let tag = Tag::new(name, "#ff5733".to_string());

        assert!(tag.matches_query("machine"));
        assert!(tag.matches_query("LEARNING"));
        assert!(tag.matches_query("learn"));
        assert!(!tag.matches_query("python"));
    }

    #[test]
    fn test_tag_starts_with() {
        let name = TagName::new("rust-programming".to_string()).unwrap();
        let tag = Tag::new(name, "#ff5733".to_string());

        assert!(tag.starts_with("rust"));
        assert!(tag.starts_with("RUST"));
        assert!(tag.starts_with("rust-"));
        assert!(!tag.starts_with("programming"));
    }

    #[test]
    fn test_tag_display_name() {
        let name = TagName::new("rust".to_string()).unwrap();
        let tag = Tag::new(name.clone(), "#ff5733".to_string());

        assert_eq!(tag.display_name(), name.as_str());
    }

    #[test]
    fn test_tag_equality() {
        let id = TagId::new();
        let name = TagName::new("rust".to_string()).unwrap();
        let now = Utc::now();

        let tag1 = Tag::with_id(id.clone(), name.clone(), "#ff5733".to_string(), now, now);
        let tag2 = Tag::with_id(id.clone(), name.clone(), "#ff5733".to_string(), now, now);

        assert_eq!(tag1, tag2);
    }

    #[test]
    fn test_tag_serialization() {
        let name = TagName::new("rust".to_string()).unwrap();
        let tag = Tag::new(name, "#ff5733".to_string());

        // Serialize
        let json = serde_json::to_string(&tag).unwrap();

        // Deserialize
        let deserialized: Tag = serde_json::from_str(&json).unwrap();

        assert_eq!(tag, deserialized);
        assert_eq!(deserialized.name().as_str(), "rust");
        assert_eq!(deserialized.color(), "#ff5733");
    }

    #[test]
    fn test_tag_serialization_camelcase() {
        let name = TagName::new("rust".to_string()).unwrap();
        let tag = Tag::new(name, "#ff5733".to_string());

        let json = serde_json::to_string(&tag).unwrap();

        assert!(json.contains("\"createdAt\":"));
        assert!(json.contains("\"updatedAt\":"));
    }
}
