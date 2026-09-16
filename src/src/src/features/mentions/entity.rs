//! # Mention Entity
//!
//! Mention entity within the domain layer.
//!
//! This is a pure domain entity with NO infrastructure concerns:
//! - No database IDs (handled by repositories)
//! - Just the core business data and logic
//! - Rich behavior for mention management

use crate::shared::domain_types::{ChunkId, DocumentId, MentionId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Mention type categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum MentionType {
    /// Person mention (e.g., @john)
    Person,
    /// Organization mention (e.g., @company)
    Organization,
    /// Location mention (e.g., @place)
    Location,
    /// General/uncategorized mention
    #[default]
    General,
}

impl std::fmt::Display for MentionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Person => write!(f, "person"),
            Self::Organization => write!(f, "organization"),
            Self::Location => write!(f, "location"),
            Self::General => write!(f, "general"),
        }
    }
}

impl std::str::FromStr for MentionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "person" => Ok(Self::Person),
            "organization" => Ok(Self::Organization),
            "location" => Ok(Self::Location),
            "general" => Ok(Self::General),
            _ => Err(format!("Invalid mention type: {}", s)),
        }
    }
}

/// Mention entity representing a reference to an entity in text.
///
/// Mentions capture references to people, organizations, locations, or other
/// entities mentioned within document chunks.
///
/// ## Pure Domain Model
///
/// This entity contains only business logic. Infrastructure concerns like
/// database persistence and search indexes are handled by the infrastructure layer.
///
/// ## Invariants
///
/// - Mention text cannot be empty
/// - Must be associated with a document and chunk
/// - Position must be within chunk boundaries
///
/// ## Example
///
/// ```rust,no_run
/// use lattice::domain::entities::mention::{Mention, MentionType};
/// use lattice::shared::domain_types::{DocumentId, ChunkId};
///
/// let doc_id = DocumentId::new();
/// let chunk_id = ChunkId::new();
/// let mention = Mention::new(
///     doc_id,
///     chunk_id,
///     "John Doe".to_string(),
///     MentionType::Person,
///     100,
///     108,
/// );
///
/// assert_eq!(mention.text(), "John Doe");
/// assert_eq!(mention.mention_type(), MentionType::Person);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mention {
    id: MentionId,
    document_id: DocumentId,
    chunk_id: ChunkId,
    text: String,
    mention_type: MentionType,
    position_start: usize,
    position_end: usize,
    created_at: DateTime<Utc>,
}

impl Mention {
    /// Create new mention.
    ///
    /// # Arguments
    ///
    /// * `document_id` - ID of the document containing the mention
    /// * `chunk_id` - ID of the chunk containing the mention
    /// * `text` - The mentioned text (e.g., "John Doe")
    /// * `mention_type` - Type of mention (Person, Organization, etc.)
    /// * `position_start` - Start position in the chunk (character offset)
    /// * `position_end` - End position in the chunk (character offset)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::entities::mention::{Mention, MentionType};
    /// use lattice::shared::domain_types::{DocumentId, ChunkId};
    ///
    /// let mention = Mention::new(
    ///     DocumentId::new(),
    ///     ChunkId::new(),
    ///     "@company".to_string(),
    ///     MentionType::Organization,
    ///     50,
    ///     58,
    /// );
    /// assert_eq!(mention.length(), 8);
    /// ```
    pub fn new(
        document_id: DocumentId,
        chunk_id: ChunkId,
        text: String,
        mention_type: MentionType,
        position_start: usize,
        position_end: usize,
    ) -> Self {
        Self {
            id: MentionId::new(),
            document_id,
            chunk_id,
            text,
            mention_type,
            position_start,
            position_end,
            created_at: Utc::now(),
        }
    }

    /// Create mention with existing ID (for reconstruction from storage).
    ///
    /// This is typically used by repositories when loading mentions from storage.
    #[allow(clippy::too_many_arguments)]
    pub fn with_id(
        id: MentionId,
        document_id: DocumentId,
        chunk_id: ChunkId,
        text: String,
        mention_type: MentionType,
        position_start: usize,
        position_end: usize,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            document_id,
            chunk_id,
            text,
            mention_type,
            position_start,
            position_end,
            created_at,
        }
    }

    /// Get mention ID.
    pub fn id(&self) -> &MentionId {
        &self.id
    }

    /// Get document ID.
    pub fn document_id(&self) -> &DocumentId {
        &self.document_id
    }

    /// Get chunk ID.
    pub fn chunk_id(&self) -> &ChunkId {
        &self.chunk_id
    }

    /// Get mention text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get mention type.
    pub fn mention_type(&self) -> MentionType {
        self.mention_type
    }

    /// Get start position in chunk.
    pub fn position_start(&self) -> usize {
        self.position_start
    }

    /// Get end position in chunk.
    pub fn position_end(&self) -> usize {
        self.position_end
    }

    /// Get creation timestamp.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// Get mention length in characters.
    pub fn length(&self) -> usize {
        self.position_end.saturating_sub(self.position_start)
    }

    /// Check if mention is a person.
    pub fn is_person(&self) -> bool {
        self.mention_type == MentionType::Person
    }

    /// Check if mention is an organization.
    pub fn is_organization(&self) -> bool {
        self.mention_type == MentionType::Organization
    }

    /// Check if mention is a location.
    pub fn is_location(&self) -> bool {
        self.mention_type == MentionType::Location
    }

    /// Update mention type.
    pub fn update_type(&mut self, mention_type: MentionType) {
        self.mention_type = mention_type;
    }

    /// Check if mention text matches query (case-insensitive).
    pub fn matches_query(&self, query: &str) -> bool {
        self.text.to_lowercase().contains(&query.to_lowercase())
    }

    /// Get normalized mention text (lowercase, trimmed).
    pub fn normalized_text(&self) -> String {
        self.text.trim().to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mention_creation() {
        let doc_id = DocumentId::new();
        let chunk_id = ChunkId::new();
        let mention = Mention::new(
            doc_id.clone(),
            chunk_id.clone(),
            "John Doe".to_string(),
            MentionType::Person,
            100,
            108,
        );

        assert_eq!(mention.document_id(), &doc_id);
        assert_eq!(mention.chunk_id(), &chunk_id);
        assert_eq!(mention.text(), "John Doe");
        assert_eq!(mention.mention_type(), MentionType::Person);
        assert_eq!(mention.position_start(), 100);
        assert_eq!(mention.position_end(), 108);
        assert!(!mention.id().as_str().is_empty());
    }

    #[test]
    fn test_mention_with_id() {
        let id = MentionId::new();
        let doc_id = DocumentId::new();
        let chunk_id = ChunkId::new();
        let now = Utc::now();

        let mention = Mention::with_id(
            id.clone(),
            doc_id.clone(),
            chunk_id.clone(),
            "Company Inc".to_string(),
            MentionType::Organization,
            50,
            61,
            now,
        );

        assert_eq!(mention.id(), &id);
        assert_eq!(mention.text(), "Company Inc");
        assert_eq!(mention.created_at(), &now);
    }

    #[test]
    fn test_mention_length() {
        let mention = Mention::new(
            DocumentId::new(),
            ChunkId::new(),
            "Test".to_string(),
            MentionType::General,
            10,
            14,
        );

        assert_eq!(mention.length(), 4);
    }

    #[test]
    fn test_mention_type_checks() {
        let person = Mention::new(
            DocumentId::new(),
            ChunkId::new(),
            "Alice".to_string(),
            MentionType::Person,
            0,
            5,
        );
        assert!(person.is_person());
        assert!(!person.is_organization());
        assert!(!person.is_location());

        let org = Mention::new(
            DocumentId::new(),
            ChunkId::new(),
            "ACME Corp".to_string(),
            MentionType::Organization,
            0,
            9,
        );
        assert!(!org.is_person());
        assert!(org.is_organization());
        assert!(!org.is_location());

        let location = Mention::new(
            DocumentId::new(),
            ChunkId::new(),
            "New York".to_string(),
            MentionType::Location,
            0,
            8,
        );
        assert!(!location.is_person());
        assert!(!location.is_organization());
        assert!(location.is_location());
    }

    #[test]
    fn test_mention_update_type() {
        let mut mention = Mention::new(
            DocumentId::new(),
            ChunkId::new(),
            "Ambiguous".to_string(),
            MentionType::General,
            0,
            9,
        );

        assert_eq!(mention.mention_type(), MentionType::General);

        mention.update_type(MentionType::Person);
        assert_eq!(mention.mention_type(), MentionType::Person);
    }

    #[test]
    fn test_mention_matches_query() {
        let mention = Mention::new(
            DocumentId::new(),
            ChunkId::new(),
            "John Doe".to_string(),
            MentionType::Person,
            0,
            8,
        );

        assert!(mention.matches_query("john"));
        assert!(mention.matches_query("DOE"));
        assert!(mention.matches_query("hn do"));
        assert!(!mention.matches_query("alice"));
    }

    #[test]
    fn test_mention_normalized_text() {
        let mention = Mention::new(
            DocumentId::new(),
            ChunkId::new(),
            "  John Doe  ".to_string(),
            MentionType::Person,
            0,
            12,
        );

        assert_eq!(mention.normalized_text(), "john doe");
    }

    #[test]
    fn test_mention_type_string_conversion() {
        assert_eq!(MentionType::Person.to_string(), "person");
        assert_eq!(MentionType::Organization.to_string(), "organization");
        assert_eq!(MentionType::Location.to_string(), "location");
        assert_eq!(MentionType::General.to_string(), "general");

        assert_eq!(
            "person".parse::<MentionType>().unwrap(),
            MentionType::Person
        );
        assert_eq!(
            "organization".parse::<MentionType>().unwrap(),
            MentionType::Organization
        );
        assert_eq!(
            "location".parse::<MentionType>().unwrap(),
            MentionType::Location
        );
        assert_eq!(
            "general".parse::<MentionType>().unwrap(),
            MentionType::General
        );
    }

    #[test]
    fn test_mention_serialization() {
        let mention = Mention::new(
            DocumentId::new(),
            ChunkId::new(),
            "Test Mention".to_string(),
            MentionType::Person,
            100,
            112,
        );

        // Serialize
        let json = serde_json::to_string(&mention).unwrap();

        // Deserialize
        let deserialized: Mention = serde_json::from_str(&json).unwrap();

        assert_eq!(mention, deserialized);
        assert_eq!(deserialized.text(), "Test Mention");
        assert_eq!(deserialized.mention_type(), MentionType::Person);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Strategy: Arbitrary MentionType
    fn arbitrary_mention_type() -> impl Strategy<Value = MentionType> {
        prop_oneof![
            Just(MentionType::Person),
            Just(MentionType::Organization),
            Just(MentionType::Location),
            Just(MentionType::General),
        ]
    }

    // Strategy: Valid mention text
    fn valid_mention_text() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-zA-Z0-9 \\-_.'@]{1,100}").unwrap()
    }

    // Strategy: Valid position range
    fn valid_position_range() -> impl Strategy<Value = (usize, usize)> {
        (0usize..1000usize)
            .prop_flat_map(|start| (start + 1..start + 100).prop_map(move |end| (start, end)))
    }

    // Strategy: Arbitrary Mention
    fn arbitrary_mention() -> impl Strategy<Value = Mention> {
        (
            arbitrary_mention_type(),
            valid_mention_text(),
            valid_position_range(),
        )
            .prop_map(|(mention_type, text, (start, end))| {
                Mention::new(
                    DocumentId::new(),
                    ChunkId::new(),
                    text,
                    mention_type,
                    start,
                    end,
                )
            })
    }

    proptest! {
        #[test]
        fn prop_mention_has_unique_id(_dummy in 0..10u32) {
            let mention1 = Mention::new(
                DocumentId::new(),
                ChunkId::new(),
                "test1".to_string(),
                MentionType::Person,
                0,
                5,
            );
            let mention2 = Mention::new(
                DocumentId::new(),
                ChunkId::new(),
                "test2".to_string(),
                MentionType::Person,
                0,
                5,
            );

            prop_assert_ne!(mention1.id(), mention2.id());
        }

        #[test]
        fn prop_mention_id_stable_across_clones(mention in arbitrary_mention()) {
            let cloned = mention.clone();
            prop_assert_eq!(mention.id(), cloned.id());
        }

        #[test]
        fn prop_mention_preserves_document_and_chunk_ids(mention in arbitrary_mention()) {
            let doc_id = mention.document_id().clone();
            let chunk_id = mention.chunk_id().clone();
            let cloned = mention.clone();

            prop_assert_eq!(cloned.document_id(), &doc_id);
            prop_assert_eq!(cloned.chunk_id(), &chunk_id);
        }

        #[test]
        fn prop_mention_creation_time_reasonable(_dummy in 0..10u32) {
            let before = Utc::now();
            let mention = Mention::new(
                DocumentId::new(),
                ChunkId::new(),
                "test".to_string(),
                MentionType::Person,
                0,
                4,
            );
            let after = Utc::now();

            prop_assert!(mention.created_at() >= &before);
            prop_assert!(mention.created_at() <= &after);
        }

        #[test]
        fn prop_is_person_true_only_for_person_type(mention_type in arbitrary_mention_type()) {
            let mention = Mention::new(
                DocumentId::new(),
                ChunkId::new(),
                "test".to_string(),
                mention_type,
                0,
                4,
            );

            if mention_type == MentionType::Person {
                prop_assert!(mention.is_person());
            } else {
                prop_assert!(!mention.is_person());
            }
        }

        #[test]
        fn prop_is_organization_true_only_for_org_type(mention_type in arbitrary_mention_type()) {
            let mention = Mention::new(
                DocumentId::new(),
                ChunkId::new(),
                "test".to_string(),
                mention_type,
                0,
                4,
            );

            if mention_type == MentionType::Organization {
                prop_assert!(mention.is_organization());
            } else {
                prop_assert!(!mention.is_organization());
            }
        }

        #[test]
        fn prop_is_location_true_only_for_location_type(mention_type in arbitrary_mention_type()) {
            let mention = Mention::new(
                DocumentId::new(),
                ChunkId::new(),
                "test".to_string(),
                mention_type,
                0,
                4,
            );

            if mention_type == MentionType::Location {
                prop_assert!(mention.is_location());
            } else {
                prop_assert!(!mention.is_location());
            }
        }

        #[test]
        fn prop_update_type_changes_type(
            mut mention in arbitrary_mention(),
            new_type in arbitrary_mention_type()
        ) {
            mention.update_type(new_type);
            prop_assert_eq!(mention.mention_type(), new_type);
        }

        #[test]
        fn prop_mention_text_non_empty(mention in arbitrary_mention()) {
            prop_assert!(!mention.text().is_empty());
        }

        #[test]
        fn prop_mention_length_matches_positions(
            text in valid_mention_text(),
            start in 0usize..500usize
        ) {
            let end = start + text.len();
            let mention = Mention::new(
                DocumentId::new(),
                ChunkId::new(),
                text,
                MentionType::General,
                start,
                end,
            );

            let expected_length = end - start;
            prop_assert_eq!(mention.length(), expected_length);
        }

        #[test]
        fn prop_matches_query_case_insensitive(text in valid_mention_text()) {
            let mention = Mention::new(
                DocumentId::new(),
                ChunkId::new(),
                text.clone(),
                MentionType::General,
                0,
                text.len(),
            );

            // Query with different cases
            let lower = text.to_lowercase();
            let upper = text.to_uppercase();

            prop_assert!(mention.matches_query(&text));
            prop_assert!(mention.matches_query(&lower));
            prop_assert!(mention.matches_query(&upper));
        }

        #[test]
        fn prop_mention_serialization_roundtrip(mention in arbitrary_mention()) {
            let json = serde_json::to_string(&mention).unwrap();
            let deserialized: Mention = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(mention.id(), deserialized.id());
            prop_assert_eq!(mention.text(), deserialized.text());
            prop_assert_eq!(mention.mention_type(), deserialized.mention_type());
            prop_assert_eq!(mention.position_start(), deserialized.position_start());
            prop_assert_eq!(mention.position_end(), deserialized.position_end());
        }

        #[test]
        fn prop_mention_clone_creates_equal(mention in arbitrary_mention()) {
            let cloned = mention.clone();

            prop_assert_eq!(mention.id(), cloned.id());
            prop_assert_eq!(mention.document_id(), cloned.document_id());
            prop_assert_eq!(mention.chunk_id(), cloned.chunk_id());
            prop_assert_eq!(mention.text(), cloned.text());
            prop_assert_eq!(mention.mention_type(), cloned.mention_type());
            prop_assert_eq!(mention.position_start(), cloned.position_start());
            prop_assert_eq!(mention.position_end(), cloned.position_end());
        }
    }
}
