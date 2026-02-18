//! Audit event types and definitions.
//!
//! This module defines the core data structures for audit logging,
//! including event types, results, and metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Error type for AuditEventBuilder.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BuilderError {
    /// A required field was not set.
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

/// Represents the action being audited.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    /// File indexed into the system
    FileIndexed,

    /// File updated in the system
    FileUpdated,

    /// File deleted from the system
    FileDeleted,

    /// Search query performed
    SearchPerformed,

    /// Question answered via RAG
    QuestionAnswered,

    /// Credential accessed from secure storage
    CredentialAccessed,

    /// Credential stored in secure storage
    CredentialStored,

    /// Credential deleted from secure storage
    CredentialDeleted,

    /// Application configuration changed
    ConfigChanged,

    /// Database backup created
    BackupCreated,

    /// Database restored from backup
    BackupRestored,

    /// File watcher started
    WatcherStarted,

    /// File watcher stopped
    WatcherStopped,

    /// Embedding model loaded
    ModelLoaded,

    /// LLM model downloaded
    ModelDownloaded,

    /// LLM model deleted
    ModelDeleted,

    /// Active chat model set
    ModelSetActive,

    /// Custom model added
    AddCustomModel,

    /// Custom model validated
    ValidateCustomModel,

    /// Custom model deleted
    DeleteCustomModel,

    /// Models viewed/listed
    ModelViewed,

    /// Download started
    DownloadStarted,

    /// Download paused
    DownloadPaused,

    /// Download resumed
    DownloadResumed,

    /// Download cancelled
    DownloadCancelled,

    /// Download retried
    DownloadRetried,

    /// Download deleted
    DownloadDeleted,

    /// Completed downloads cleared
    DownloadsCleared,

    /// Cache cleared
    CacheCleared,

    /// Export operation performed
    DataExported,

    /// Import operation performed
    DataImported,

    /// Authentication attempt
    AuthAttempt,

    /// System startup
    SystemStartup,

    /// System shutdown
    SystemShutdown,

    /// Tag created
    TagCreated,

    /// Tag updated
    TagUpdated,

    /// Tag deleted
    TagDeleted,

    /// Tag accessed (read)
    TagAccessed,

    /// Document reindexed
    DocumentReindexed,

    /// Web content ingested and indexed
    WebContentIngested,

    /// Web content accessed (preview or extraction)
    WebContentAccessed,

    /// Health check performed
    HealthCheck,

    /// Document accessed (read operation)
    DocumentAccessed,

    /// Document updated
    DocumentUpdated,

    /// File accessed (opened, viewed)
    FileAccessed,

    /// Settings accessed (read)
    SettingsAccessed,

    /// Settings updated
    SettingsUpdated,

    /// Settings reset to defaults
    SettingsReset,

    /// Settings exported to file
    SettingsExported,

    /// Settings imported from file
    SettingsImported,

    /// Custom API endpoint configured
    EndpointConfigured,

    /// Conversation created
    ConversationCreated,

    /// Conversation accessed (read)
    ConversationAccessed,

    /// Conversation updated (renamed, modified)
    ConversationUpdated,

    /// Conversation deleted
    ConversationDeleted,

    /// Document added to favorites
    FavoriteAdded,

    /// Document removed from favorites
    FavoriteRemoved,

    /// Recent history cleared
    HistoryCleared,

    /// Mentions extracted from document
    MentionsExtracted,

    /// Application update check performed
    UpdateChecked,

    /// Custom action (with description in metadata)
    Custom(String),
}

impl AuditAction {
    /// Returns a human-readable description of the action.
    pub fn description(&self) -> &str {
        match self {
            AuditAction::FileIndexed => "File indexed",
            AuditAction::FileUpdated => "File updated",
            AuditAction::FileDeleted => "File deleted",
            AuditAction::SearchPerformed => "Search performed",
            AuditAction::QuestionAnswered => "Question answered",
            AuditAction::CredentialAccessed => "Credential accessed",
            AuditAction::CredentialStored => "Credential stored",
            AuditAction::CredentialDeleted => "Credential deleted",
            AuditAction::ConfigChanged => "Configuration changed",
            AuditAction::BackupCreated => "Backup created",
            AuditAction::BackupRestored => "Backup restored",
            AuditAction::WatcherStarted => "File watcher started",
            AuditAction::WatcherStopped => "File watcher stopped",
            AuditAction::ModelLoaded => "Embedding model loaded",
            AuditAction::ModelDownloaded => "LLM model downloaded",
            AuditAction::ModelDeleted => "LLM model deleted",
            AuditAction::ModelSetActive => "Active chat model set",
            AuditAction::AddCustomModel => "Custom model added",
            AuditAction::ValidateCustomModel => "Custom model validated",
            AuditAction::DeleteCustomModel => "Custom model deleted",
            AuditAction::ModelViewed => "Models viewed",
            AuditAction::DownloadStarted => "Download started",
            AuditAction::DownloadPaused => "Download paused",
            AuditAction::DownloadResumed => "Download resumed",
            AuditAction::DownloadCancelled => "Download cancelled",
            AuditAction::DownloadRetried => "Download retried",
            AuditAction::DownloadDeleted => "Download deleted",
            AuditAction::DownloadsCleared => "Completed downloads cleared",
            AuditAction::CacheCleared => "Cache cleared",
            AuditAction::DataExported => "Data exported",
            AuditAction::DataImported => "Data imported",
            AuditAction::AuthAttempt => "Authentication attempt",
            AuditAction::SystemStartup => "System startup",
            AuditAction::SystemShutdown => "System shutdown",
            AuditAction::TagCreated => "Tag created",
            AuditAction::TagUpdated => "Tag updated",
            AuditAction::TagDeleted => "Tag deleted",
            AuditAction::TagAccessed => "Tag accessed",
            AuditAction::DocumentReindexed => "Document reindexed",
            AuditAction::WebContentIngested => "Web content ingested",
            AuditAction::WebContentAccessed => "Web content accessed",
            AuditAction::HealthCheck => "Health check performed",
            AuditAction::DocumentAccessed => "Document accessed",
            AuditAction::DocumentUpdated => "Document updated",
            AuditAction::FileAccessed => "File accessed",
            AuditAction::SettingsAccessed => "Settings accessed",
            AuditAction::SettingsUpdated => "Settings updated",
            AuditAction::SettingsReset => "Settings reset to defaults",
            AuditAction::SettingsExported => "Settings exported",
            AuditAction::SettingsImported => "Settings imported",
            AuditAction::EndpointConfigured => "Custom endpoint configured",
            AuditAction::ConversationCreated => "Conversation created",
            AuditAction::ConversationAccessed => "Conversation accessed",
            AuditAction::ConversationUpdated => "Conversation updated",
            AuditAction::ConversationDeleted => "Conversation deleted",
            AuditAction::FavoriteAdded => "Favorite added",
            AuditAction::FavoriteRemoved => "Favorite removed",
            AuditAction::HistoryCleared => "Recent history cleared",
            AuditAction::MentionsExtracted => "Mentions extracted",
            AuditAction::UpdateChecked => "Application update checked",
            AuditAction::Custom(desc) => desc,
        }
    }
}

/// Result of an audited action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    /// Action completed successfully
    Success,

    /// Action failed
    Failure {
        /// Error message or reason for failure
        reason: String,
    },

    /// Action was denied (e.g., permission denied)
    Denied {
        /// Reason for denial
        reason: String,
    },
}

impl AuditResult {
    /// Create a success result.
    pub fn success() -> Self {
        AuditResult::Success
    }

    /// Create a failure result with the given reason.
    pub fn failure(reason: impl Into<String>) -> Self {
        AuditResult::Failure {
            reason: reason.into(),
        }
    }

    /// Create a denied result with the given reason.
    pub fn denied(reason: impl Into<String>) -> Self {
        AuditResult::Denied {
            reason: reason.into(),
        }
    }

    /// Check if the result is successful.
    pub fn is_success(&self) -> bool {
        matches!(self, AuditResult::Success)
    }

    /// Check if the result is a failure.
    pub fn is_failure(&self) -> bool {
        matches!(self, AuditResult::Failure { .. })
    }

    /// Check if the result is denied.
    pub fn is_denied(&self) -> bool {
        matches!(self, AuditResult::Denied { .. })
    }
}

/// Represents a single audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique identifier for this event
    #[serde(default = "uuid::Uuid::new_v4")]
    pub id: uuid::Uuid,

    /// Timestamp when the event occurred
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,

    /// Optional user identifier
    pub user_id: Option<String>,

    /// The action being audited
    pub action: AuditAction,

    /// Optional resource identifier (e.g., file path, document ID)
    pub resource_id: Option<String>,

    /// Result of the action
    pub result: AuditResult,

    /// Additional metadata as key-value pairs
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl AuditEvent {
    /// Create a new audit event builder.
    pub fn builder() -> AuditEventBuilder {
        AuditEventBuilder::default()
    }

    /// Create a new audit event with minimal required fields.
    pub fn new(action: AuditAction, result: AuditResult) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            user_id: None,
            action,
            resource_id: None,
            result,
            metadata: HashMap::new(),
        }
    }

    /// Add a metadata entry.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Add multiple metadata entries.
    pub fn with_metadata_map(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata.extend(metadata);
        self
    }

    /// Set the user ID.
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set the resource ID.
    pub fn with_resource_id(mut self, resource_id: impl Into<String>) -> Self {
        self.resource_id = Some(resource_id.into());
        self
    }
}

/// Builder for creating audit events.
#[derive(Default)]
pub struct AuditEventBuilder {
    user_id: Option<String>,
    action: Option<AuditAction>,
    resource_id: Option<String>,
    result: Option<AuditResult>,
    metadata: HashMap<String, String>,
}

impl AuditEventBuilder {
    /// Set the user ID.
    pub fn user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set the action.
    pub fn action(mut self, action: AuditAction) -> Self {
        self.action = Some(action);
        self
    }

    /// Set the resource ID.
    pub fn resource_id(mut self, resource_id: impl Into<String>) -> Self {
        self.resource_id = Some(resource_id.into());
        self
    }

    /// Set the result.
    pub fn result(mut self, result: AuditResult) -> Self {
        self.result = Some(result);
        self
    }

    /// Add a metadata entry.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Add multiple metadata entries.
    pub fn metadata_map(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata.extend(metadata);
        self
    }

    /// Build the audit event.
    ///
    /// # Errors
    ///
    /// Returns `BuilderError::MissingField` if action or result are not set.
    pub fn build(self) -> Result<AuditEvent, BuilderError> {
        Ok(AuditEvent {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            user_id: self.user_id,
            action: self.action.ok_or(BuilderError::MissingField("action"))?,
            resource_id: self.resource_id,
            result: self.result.ok_or(BuilderError::MissingField("result"))?,
            metadata: self.metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_action_description() {
        assert_eq!(AuditAction::FileIndexed.description(), "File indexed");
        assert_eq!(
            AuditAction::Custom("test".to_string()).description(),
            "test"
        );
    }

    #[test]
    fn test_audit_result_constructors() {
        let success = AuditResult::success();
        assert!(success.is_success());
        assert!(!success.is_failure());

        let failure = AuditResult::failure("error");
        assert!(failure.is_failure());
        assert!(!failure.is_success());

        let denied = AuditResult::denied("no permission");
        assert!(denied.is_denied());
        assert!(!denied.is_success());
    }

    #[test]
    fn test_audit_event_new() {
        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());

        assert_eq!(event.action, AuditAction::FileIndexed);
        assert!(event.result.is_success());
        assert!(event.user_id.is_none());
        assert!(event.resource_id.is_none());
        assert!(event.metadata.is_empty());
    }

    #[test]
    fn test_audit_event_builder() {
        let event = AuditEvent::builder()
            .action(AuditAction::SearchPerformed)
            .result(AuditResult::success())
            .user_id("user123")
            .resource_id("doc456")
            .metadata("query", "test search")
            .metadata("limit", "10")
            .build()
            .unwrap();

        assert_eq!(event.action, AuditAction::SearchPerformed);
        assert!(event.result.is_success());
        assert_eq!(event.user_id.as_deref(), Some("user123"));
        assert_eq!(event.resource_id.as_deref(), Some("doc456"));
        assert_eq!(
            event.metadata.get("query").map(|s| s.as_str()),
            Some("test search")
        );
        assert_eq!(event.metadata.get("limit").map(|s| s.as_str()), Some("10"));
    }

    #[test]
    fn test_audit_event_with_methods() {
        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
            .with_user_id("user123")
            .with_resource_id("/path/to/file.txt")
            .with_metadata("size", "1024")
            .with_metadata("type", "text/plain");

        assert_eq!(event.user_id.as_deref(), Some("user123"));
        assert_eq!(event.resource_id.as_deref(), Some("/path/to/file.txt"));
        assert_eq!(event.metadata.len(), 2);
    }

    #[test]
    fn test_audit_event_serialization() {
        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
            .with_user_id("user123")
            .with_metadata("test", "value");

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: AuditEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event.action, deserialized.action);
        assert_eq!(event.user_id, deserialized.user_id);
    }
}
