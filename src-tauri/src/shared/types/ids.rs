use super::DomainTypeError;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// Strongly-typed document identifier
///
/// Prevents accidental mixing of document IDs with other entity IDs.
///
/// # Examples
///
/// ```rust
/// use lattice::shared::types::DocumentId;
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
