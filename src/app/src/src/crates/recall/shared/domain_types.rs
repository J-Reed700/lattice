//! # Domain Types Module
//!
//! Provides type-safe wrappers for domain entities using the newtype pattern.
//!
//! ## Purpose
//!
//! This module defines strongly-typed IDs and domain primitives that prevent
//! common errors such as mixing up IDs of different entity types.
//!
//! ## Examples
//!
//! ```rust
//! use vault_desktop::domain_types::*;
//!
//! // Type-safe IDs prevent mistakes
//! let doc_id = DocumentId::new();
//! let tag_id = TagId::new();
//!
//! // This won't compile (different types):
//! // add_tag_to_document(tag_id, doc_id);  // ❌ Compile error!
//!
//! // Correct usage:
//! add_tag_to_document(&doc_id, &tag_id);  // ✅
//! ```

use derive_more::{Display, From};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Error type for domain type validation
#[derive(Debug, thiserror::Error)]
pub enum DomainTypeError {
    #[error("Invalid ID format: {0}")]
    InvalidId(String),

    #[error("Empty value not allowed")]
    EmptyValue,

    #[error("Value too long: {0} (max: {1})")]
    ValueTooLong(usize, usize),
}

// ============================================================================
// Document ID
// ============================================================================

/// Strongly-typed document identifier
///
/// Prevents accidental mixing of document IDs with other entity IDs.
///
/// # Examples
///
/// ```rust
/// use vault_desktop::domain_types::DocumentId;
///
/// let id = DocumentId::new();
/// let id_str = id.to_string();
/// let parsed = DocumentId::from_string(id_str)?;
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(transparent)]
pub struct DocumentId(String);

impl DocumentId {
    /// Create a new random document ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from an existing string
    ///
    /// # Errors
    ///
    /// Returns `DomainTypeError::InvalidId` if the string is empty
    pub fn from_string(s: String) -> Result<Self, DomainTypeError> {
        if s.is_empty() {
            return Err(DomainTypeError::EmptyValue);
        }
        Ok(Self(s))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for DocumentId {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for DocumentId {
    type Err = DomainTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_string(s.to_string())
    }
}

// Manual From<String> implementation
impl From<String> for DocumentId {
    fn from(s: String) -> Self {
        DocumentId(s)
    }
}

// Manual From<&str> implementation (convenient)
impl From<&str> for DocumentId {
    fn from(s: &str) -> Self {
        DocumentId(s.to_string())
    }
}

// Manual Into<String> implementation
impl From<DocumentId> for String {
    fn from(id: DocumentId) -> Self {
        id.0
    }
}

// ============================================================================
// Tag ID
// ============================================================================

/// Strongly-typed tag identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(transparent)]
pub struct TagId(String);

impl TagId {
    /// Create a new random tag ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from an existing string
    pub fn from_string(s: String) -> Result<Self, DomainTypeError> {
        if s.is_empty() {
            return Err(DomainTypeError::EmptyValue);
        }
        Uuid::parse_str(&s)
            .map_err(|_| DomainTypeError::InvalidId(format!("Invalid UUID format: {}", s)))?;
        Ok(Self(s))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TagId {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for TagId {
    type Err = DomainTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_string(s.to_string())
    }
}

impl From<String> for TagId {
    fn from(s: String) -> Self {
        TagId(s)
    }
}

impl From<&str> for TagId {
    fn from(s: &str) -> Self {
        TagId(s.to_string())
    }
}

impl From<TagId> for String {
    fn from(id: TagId) -> Self {
        id.0
    }
}

// ============================================================================
// Chunk ID
// ============================================================================

/// Strongly-typed chunk identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(transparent)]
pub struct ChunkId(String);

impl ChunkId {
    /// Create a new random chunk ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from an existing string
    pub fn from_string(s: String) -> Result<Self, DomainTypeError> {
        if s.is_empty() {
            return Err(DomainTypeError::EmptyValue);
        }
        Ok(Self(s))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ChunkId {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for ChunkId {
    type Err = DomainTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_string(s.to_string())
    }
}

impl From<String> for ChunkId {
    fn from(s: String) -> Self {
        ChunkId(s)
    }
}

impl From<&str> for ChunkId {
    fn from(s: &str) -> Self {
        ChunkId(s.to_string())
    }
}

impl From<ChunkId> for String {
    fn from(id: ChunkId) -> Self {
        id.0
    }
}

// ============================================================================
// Mention ID
// ============================================================================

/// Strongly-typed mention identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(transparent)]
pub struct MentionId(String);

impl MentionId {
    /// Create a new random mention ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from an existing string
    pub fn from_string(s: String) -> Result<Self, DomainTypeError> {
        if s.is_empty() {
            return Err(DomainTypeError::EmptyValue);
        }
        Ok(Self(s))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for MentionId {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for MentionId {
    type Err = DomainTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_string(s.to_string())
    }
}

impl From<String> for MentionId {
    fn from(s: String) -> Self {
        MentionId(s)
    }
}

impl From<&str> for MentionId {
    fn from(s: &str) -> Self {
        MentionId(s.to_string())
    }
}

impl From<MentionId> for String {
    fn from(id: MentionId) -> Self {
        id.0
    }
}

// ============================================================================
// Conversation ID
// ============================================================================

/// Strongly-typed conversation identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(transparent)]
pub struct ConversationId(String);

impl ConversationId {
    /// Create a new random conversation ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from an existing string
    pub fn from_string(s: String) -> Result<Self, DomainTypeError> {
        if s.is_empty() {
            return Err(DomainTypeError::EmptyValue);
        }
        Ok(Self(s))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for ConversationId {
    type Err = DomainTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_string(s.to_string())
    }
}

impl From<String> for ConversationId {
    fn from(s: String) -> Self {
        ConversationId(s)
    }
}

impl From<&str> for ConversationId {
    fn from(s: &str) -> Self {
        ConversationId(s.to_string())
    }
}

impl From<ConversationId> for String {
    fn from(id: ConversationId) -> Self {
        id.0
    }
}

// ============================================================================
// Tag Name (validated)
// ============================================================================

const MAX_TAG_NAME_LENGTH: usize = 50;

/// Validated tag name with length constraints
///
/// Tag names must be:
/// - Non-empty
/// - Maximum 50 characters
/// - Contain valid characters
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(transparent)]
pub struct TagName(String);

impl TagName {
    /// Create a new validated tag name
    ///
    /// # Errors
    ///
    /// Returns an error if the name is empty or too long
    pub fn new(name: String) -> Result<Self, DomainTypeError> {
        if name.is_empty() {
            return Err(DomainTypeError::EmptyValue);
        }

        if name.len() > MAX_TAG_NAME_LENGTH {
            return Err(DomainTypeError::ValueTooLong(
                name.len(),
                MAX_TAG_NAME_LENGTH,
            ));
        }

        Ok(Self(name))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TagName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for TagName {
    type Err = DomainTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s.to_string())
    }
}

// ============================================================================
// File Path (validated)
// ============================================================================

use std::path::{Path, PathBuf};

/// Validated file path
///
/// Provides a type-safe wrapper around PathBuf with validation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ValidatedFilePath(PathBuf);

impl ValidatedFilePath {
    /// Create a new validated file path
    ///
    /// # Errors
    ///
    /// Returns an error if the path is empty or invalid
    pub fn new(path: PathBuf) -> Result<Self, DomainTypeError> {
        if path.as_os_str().is_empty() {
            return Err(DomainTypeError::EmptyValue);
        }

        // Check for path traversal attacks (CWE-22)
        // Only reject ".." as a path component, not in file/directory names
        for component in path.components() {
            if let std::path::Component::ParentDir = component {
                return Err(DomainTypeError::InvalidId(
                    "Path contains parent directory reference '..' which is not allowed"
                        .to_string(),
                ));
            }
        }

        Ok(Self(path))
    }

    /// Get the inner PathBuf
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Get the path as a string slice
    pub fn as_str(&self) -> &str {
        self.0.to_str().unwrap_or("")
    }

    /// Convert to PathBuf
    pub fn into_inner(self) -> PathBuf {
        self.0
    }
}

impl fmt::Display for ValidatedFilePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl From<ValidatedFilePath> for PathBuf {
    fn from(vfp: ValidatedFilePath) -> Self {
        vfp.0
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_id_creation() {
        let id1 = DocumentId::new();
        let id2 = DocumentId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_document_id_from_string() {
        let id_str = "test-id-123".to_string();
        let id = DocumentId::from_string(id_str.clone()).unwrap();
        assert_eq!(id.as_str(), "test-id-123");
    }

    #[test]
    fn test_document_id_empty_string_error() {
        let result = DocumentId::from_string("".to_string());
        assert!(result.is_err());
        assert!(matches!(result, Err(DomainTypeError::EmptyValue)));
    }

    #[test]
    fn test_tag_name_validation() {
        assert!(TagName::new("rust".to_string()).is_ok());
        assert!(TagName::new("".to_string()).is_err());

        let long_name = "a".repeat(51);
        assert!(TagName::new(long_name).is_err());
    }

    #[test]
    fn test_tag_name_max_length() {
        let max_name = "a".repeat(50);
        assert!(TagName::new(max_name).is_ok());
    }

    #[test]
    fn test_ids_are_different_types() {
        let doc_id = DocumentId::new();
        let tag_id = TagId::new();

        // This test ensures IDs are different types
        // (won't compile if they're the same type)
        fn takes_doc_id(_: &DocumentId) {}
        fn takes_tag_id(_: &TagId) {}

        takes_doc_id(&doc_id);
        takes_tag_id(&tag_id);
    }

    #[test]
    fn test_validated_file_path() {
        let path = PathBuf::from("/path/to/file.txt");
        let validated = ValidatedFilePath::new(path.clone()).unwrap();
        assert_eq!(validated.as_path(), path.as_path());
    }

    #[test]
    fn test_validated_file_path_empty_error() {
        let path = PathBuf::from("");
        let result = ValidatedFilePath::new(path);
        assert!(result.is_err());
    }

    // ========================================================================
    // Property-Based Tests
    // ========================================================================

    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        // ====================================================================
        // Test Strategies
        // ====================================================================

        /// Generate valid non-empty strings
        fn non_empty_string() -> impl Strategy<Value = String> {
            prop::string::string_regex("[a-zA-Z0-9_-]{1,100}").expect("Valid regex")
        }

        /// Generate valid tag names (within length limits)
        fn valid_tag_name() -> impl Strategy<Value = String> {
            prop::string::string_regex("[a-zA-Z0-9_-]{1,50}").expect("Valid regex")
        }

        /// Generate too-long tag names
        fn too_long_tag_name() -> impl Strategy<Value = String> {
            prop::string::string_regex("[a-zA-Z]{51,100}").expect("Valid regex")
        }

        /// Generate valid file paths
        fn valid_file_path() -> impl Strategy<Value = String> {
            // Generate paths that don't contain ".." or other path traversal patterns
            prop::collection::vec("[a-zA-Z0-9_-]{1,20}", 1..5)
                .prop_map(|components| format!("/{}", components.join("/")))
        }

        /// Generate valid UUIDs for testing
        fn valid_uuid() -> impl Strategy<Value = String> {
            Just(()).prop_map(|_| Uuid::new_v4().to_string())
        }

        // ====================================================================
        // DocumentId Properties
        // ====================================================================

        proptest! {
            /// Property: DocumentId roundtrip through string conversion
            ///
            /// from_string(id.to_string()) should equal the original ID
            #[test]
            fn prop_document_id_roundtrip(
                s in non_empty_string()
            ) {
                let id = DocumentId::from_string(s.clone()).unwrap();
                let roundtrip = DocumentId::from_string(id.to_string()).unwrap();

                prop_assert_eq!(&id, &roundtrip, "DocumentId roundtrip failed");
                prop_assert_eq!(id.as_str(), s, "Inner string should match");
            }

            /// Property: Generated IDs are unique
            #[test]
            fn prop_document_ids_unique(
                _seed in 0..100u32
            ) {
                let id1 = DocumentId::new();
                let id2 = DocumentId::new();

                prop_assert_ne!(id1, id2, "Generated IDs should be unique");
            }

            /// Property: Empty strings are rejected
            #[test]
            fn prop_document_id_rejects_empty(
                _dummy in 0..10u32
            ) {
                let result = DocumentId::from_string("".to_string());
                prop_assert!(result.is_err(), "Empty string should be rejected");
                prop_assert!(
                    matches!(result, Err(DomainTypeError::EmptyValue)),
                    "Should return EmptyValue error"
                );
            }

            /// Property: as_str returns the inner string
            #[test]
            fn prop_document_id_as_str(
                s in non_empty_string()
            ) {
                let id = DocumentId::from_string(s.clone()).unwrap();
                prop_assert_eq!(id.as_str(), s.as_str());
            }

            /// Property: Display trait shows the ID
            #[test]
            fn prop_document_id_display(
                s in non_empty_string()
            ) {
                let id = DocumentId::from_string(s.clone()).unwrap();
                let displayed = id.to_string();
                prop_assert_eq!(displayed, s);
            }

            /// Property: FromStr implementation works
            #[test]
            fn prop_document_id_from_str(
                s in non_empty_string()
            ) {
                let id: DocumentId = s.parse().unwrap();
                prop_assert_eq!(id.as_str(), s);
            }
        }

        // ====================================================================
        // TagId Properties
        // ====================================================================

        proptest! {
            /// Property: TagId roundtrip through string conversion
            #[test]
            fn prop_tag_id_roundtrip(
                s in valid_uuid()
            ) {
                let id = TagId::from_string(s.clone()).unwrap();
                let roundtrip = TagId::from_string(id.to_string()).unwrap();

                prop_assert_eq!(id, roundtrip);
            }

            /// Property: Generated TagIDs are unique
            #[test]
            fn prop_tag_ids_unique(
                _seed in 0..100u32
            ) {
                let id1 = TagId::new();
                let id2 = TagId::new();

                prop_assert_ne!(id1, id2);
            }

            /// Property: TagId rejects empty strings
            #[test]
            fn prop_tag_id_rejects_empty(
                _dummy in 0..10u32
            ) {
                let result = TagId::from_string("".to_string());
                prop_assert!(result.is_err());
            }
        }

        // ====================================================================
        // ChunkId Properties
        // ====================================================================

        proptest! {
            /// Property: ChunkId roundtrip through string conversion
            #[test]
            fn prop_chunk_id_roundtrip(
                s in non_empty_string()
            ) {
                let id = ChunkId::from_string(s.clone()).unwrap();
                let roundtrip = ChunkId::from_string(id.to_string()).unwrap();

                prop_assert_eq!(id, roundtrip);
            }

            /// Property: Generated ChunkIDs are unique
            #[test]
            fn prop_chunk_ids_unique(
                _seed in 0..100u32
            ) {
                let id1 = ChunkId::new();
                let id2 = ChunkId::new();

                prop_assert_ne!(id1, id2);
            }
        }

        // ====================================================================
        // MentionId Properties
        // ====================================================================

        proptest! {
            /// Property: MentionId roundtrip through string conversion
            #[test]
            fn prop_mention_id_roundtrip(
                s in non_empty_string()
            ) {
                let id = MentionId::from_string(s.clone()).unwrap();
                let roundtrip = MentionId::from_string(id.to_string()).unwrap();

                prop_assert_eq!(id, roundtrip);
            }

            /// Property: Generated MentionIDs are unique
            #[test]
            fn prop_mention_ids_unique(
                _seed in 0..100u32
            ) {
                let id1 = MentionId::new();
                let id2 = MentionId::new();

                prop_assert_ne!(id1, id2);
            }
        }

        // ====================================================================
        // TagName Properties
        // ====================================================================

        proptest! {
            /// Property: Valid tag names are accepted
            #[test]
            fn prop_tag_name_accepts_valid(
                name in valid_tag_name()
            ) {
                let result = TagName::new(name.clone());
                prop_assert!(result.is_ok(), "Valid tag name should be accepted");

                let tag_name = result.unwrap();
                prop_assert_eq!(tag_name.as_str(), name);
            }

            /// Property: Empty tag names are rejected
            #[test]
            fn prop_tag_name_rejects_empty(
                _dummy in 0..10u32
            ) {
                let result = TagName::new("".to_string());
                prop_assert!(result.is_err());
                prop_assert!(matches!(result, Err(DomainTypeError::EmptyValue)));
            }

            /// Property: Tag names exceeding max length are rejected
            #[test]
            fn prop_tag_name_rejects_too_long(
                name in too_long_tag_name()
            ) {
                let result = TagName::new(name.clone());
                prop_assert!(result.is_err(), "Tag name longer than 50 should be rejected");

                if let Err(e) = result {
                    prop_assert!(
                        matches!(e, DomainTypeError::ValueTooLong(_, _)),
                        "Should return ValueTooLong error"
                    );
                }
            }

            /// Property: Tag name at exactly max length is accepted
            #[test]
            fn prop_tag_name_max_length_accepted(
                _seed in 0..100u32
            ) {
                let max_name = "a".repeat(MAX_TAG_NAME_LENGTH);
                let result = TagName::new(max_name.clone());
                prop_assert!(result.is_ok(), "Tag name at max length should be accepted");

                let tag_name = result.unwrap();
                prop_assert_eq!(tag_name.as_str().len(), MAX_TAG_NAME_LENGTH);
            }

            /// Property: Tag name one character over max is rejected
            #[test]
            fn prop_tag_name_over_max_rejected(
                _seed in 0..100u32
            ) {
                let over_max = "a".repeat(MAX_TAG_NAME_LENGTH + 1);
                let result = TagName::new(over_max);
                prop_assert!(result.is_err());
            }

            /// Property: TagName Display shows the name
            #[test]
            fn prop_tag_name_display(
                name in valid_tag_name()
            ) {
                let tag_name = TagName::new(name.clone()).unwrap();
                prop_assert_eq!(tag_name.to_string(), name);
            }

            /// Property: TagName FromStr works
            #[test]
            fn prop_tag_name_from_str(
                name in valid_tag_name()
            ) {
                let tag_name: TagName = name.parse().unwrap();
                prop_assert_eq!(tag_name.as_str(), name);
            }
        }

        // ====================================================================
        // ValidatedFilePath Properties
        // ====================================================================

        proptest! {
            /// Property: Valid file paths are accepted
            #[test]
            fn prop_file_path_accepts_valid(
                path_str in valid_file_path()
            ) {
                let path = PathBuf::from(path_str);
                let result = ValidatedFilePath::new(path.clone());
                prop_assert!(result.is_ok());

                let validated = result.unwrap();
                prop_assert_eq!(validated.as_path(), path.as_path());
            }

            /// Property: Empty paths are rejected
            #[test]
            fn prop_file_path_rejects_empty(
                _dummy in 0..10u32
            ) {
                let result = ValidatedFilePath::new(PathBuf::from(""));
                prop_assert!(result.is_err());
                prop_assert!(matches!(result, Err(DomainTypeError::EmptyValue)));
            }

            /// Property: into_inner returns the original path
            #[test]
            fn prop_file_path_into_inner(
                path_str in valid_file_path()
            ) {
                let path = PathBuf::from(path_str);
                let validated = ValidatedFilePath::new(path.clone()).unwrap();
                let inner = validated.into_inner();
                prop_assert_eq!(inner, path);
            }

            /// Property: Display shows the path
            #[test]
            fn prop_file_path_display(
                path_str in valid_file_path()
            ) {
                let path = PathBuf::from(&path_str);
                let validated = ValidatedFilePath::new(path).unwrap();
                let displayed = validated.to_string();
                prop_assert_eq!(displayed, path_str);
            }

            /// Property: Conversion to PathBuf works
            #[test]
            fn prop_file_path_to_pathbuf(
                path_str in valid_file_path()
            ) {
                let path = PathBuf::from(&path_str);
                let validated = ValidatedFilePath::new(path.clone()).unwrap();
                let converted: PathBuf = validated.into();
                prop_assert_eq!(converted, path);
            }
        }

        // ====================================================================
        // Type Safety Properties
        // ====================================================================

        proptest! {
            /// Property: Different ID types are not interchangeable
            ///
            /// This test verifies that the type system prevents mixing different ID types
            #[test]
            fn prop_ids_are_distinct_types(
                _seed in 0..100u32
            ) {
                let doc_id = DocumentId::new();
                let tag_id = TagId::new();
                let chunk_id = ChunkId::new();
                let mention_id = MentionId::new();

                // These should all be different types
                // (This test mainly documents the type safety; actual mixing would fail to compile)

                fn takes_doc(_: &DocumentId) {}
                fn takes_tag(_: &TagId) {}
                fn takes_chunk(_: &ChunkId) {}
                fn takes_mention(_: &MentionId) {}

                takes_doc(&doc_id);
                takes_tag(&tag_id);
                takes_chunk(&chunk_id);
                takes_mention(&mention_id);

                // Verify they're actually different as strings
                prop_assert_ne!(doc_id.as_str(), tag_id.as_str());
                prop_assert_ne!(doc_id.as_str(), chunk_id.as_str());
                prop_assert_ne!(tag_id.as_str(), chunk_id.as_str());
            }
        }

        // ====================================================================
        // Hash and Equality Properties
        // ====================================================================

        proptest! {
            /// Property: IDs with same string are equal
            #[test]
            fn prop_ids_equal_same_string(
                s in non_empty_string()
            ) {
                let id1 = DocumentId::from_string(s.clone()).unwrap();
                let id2 = DocumentId::from_string(s).unwrap();
                prop_assert_eq!(id1, id2);
            }

            /// Property: IDs with different strings are not equal
            #[test]
            fn prop_ids_not_equal_different_strings(
                s1 in non_empty_string(),
                s2 in non_empty_string(),
            ) {
                if s1 != s2 {
                    let id1 = DocumentId::from_string(s1).unwrap();
                    let id2 = DocumentId::from_string(s2).unwrap();
                    prop_assert_ne!(id1, id2);
                }
            }

            /// Property: Hash consistency - equal IDs have equal hashes
            #[test]
            fn prop_hash_consistency(
                s in non_empty_string()
            ) {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let id1 = DocumentId::from_string(s.clone()).unwrap();
                let id2 = DocumentId::from_string(s).unwrap();

                let mut hasher1 = DefaultHasher::new();
                let mut hasher2 = DefaultHasher::new();

                id1.hash(&mut hasher1);
                id2.hash(&mut hasher2);

                prop_assert_eq!(
                    hasher1.finish(),
                    hasher2.finish(),
                    "Equal IDs should have equal hashes"
                );
            }
        }

        // ====================================================================
        // Serialization Properties
        // ====================================================================

        proptest! {
            /// Property: DocumentId serialization roundtrip
            #[test]
            fn prop_document_id_serde_roundtrip(
                s in non_empty_string()
            ) {
                let id = DocumentId::from_string(s).unwrap();
                let json = serde_json::to_string(&id).unwrap();
                let deserialized: DocumentId = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(id, deserialized);
            }

            /// Property: TagName serialization roundtrip
            #[test]
            fn prop_tag_name_serde_roundtrip(
                name in valid_tag_name()
            ) {
                let tag_name = TagName::new(name).unwrap();
                let json = serde_json::to_string(&tag_name).unwrap();
                let deserialized: TagName = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(tag_name, deserialized);
            }

            /// Property: ValidatedFilePath serialization roundtrip
            #[test]
            fn prop_file_path_serde_roundtrip(
                path_str in valid_file_path()
            ) {
                let validated = ValidatedFilePath::new(PathBuf::from(path_str)).unwrap();
                let json = serde_json::to_string(&validated).unwrap();
                let deserialized: ValidatedFilePath = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(validated, deserialized);
            }
        }

        // ====================================================================
        // Invariant Properties
        // ====================================================================

        proptest! {
            /// Property: Once created, IDs are immutable
            ///
            /// The string value cannot be changed after creation
            #[test]
            fn prop_ids_immutable(
                s in non_empty_string()
            ) {
                let id = DocumentId::from_string(s.clone()).unwrap();
                prop_assert_eq!(id.as_str(), s);
                // as_str returns &str, not &mut str, so mutation is prevented
            }

            /// Property: Default IDs are valid
            #[test]
            fn prop_default_ids_valid(
                _seed in 0..100u32
            ) {
                let doc_id = DocumentId::default();
                let tag_id = TagId::default();
                let chunk_id = ChunkId::default();
                let mention_id = MentionId::default();

                prop_assert!(!doc_id.as_str().is_empty());
                prop_assert!(!tag_id.as_str().is_empty());
                prop_assert!(!chunk_id.as_str().is_empty());
                prop_assert!(!mention_id.as_str().is_empty());
            }
        }
    }
}
