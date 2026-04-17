//! Mention Repository Trait - MIGRATED TO DDD
//!
//! This module documents the migration from legacy MentionRepositoryTrait
//! to Domain-Driven Design architecture with proper ports pattern.
//!
//! # Migration Status
//!
//! - ✅ Application ports defined (`application/ports/mention_repository_port.rs`)
//! - ✅ Repository implemented (`infrastructure/persistence/repositories/mention_repository.rs`)
//! - ✅ Legacy trait removed (THIS PHASE)
//! - ✅ ServiceContainer refactored
//! - ✅ Mock implementation updated
//! - ✅ All consumers updated
//!
//! # OLD PATTERN (Deprecated)
//!
//! ```rust,ignore
//! use crate::infrastructure::services::traits::MentionRepositoryTrait;
//!
//! async fn old_way(repo: Arc<dyn MentionRepositoryTrait>) -> Result<()> {
//!     // Old trait with infrastructure coupling
//!     let mention = repo.create_mention("John Doe", "person", None).await?;
//!     let mentions = repo.search_mentions("John", 10).await?;
//!     let doc_mentions = repo.get_mentions_for_document("doc-123").await?;
//!     repo.delete_mention("mention-456").await?;
//!     Ok(())
//! }
//! ```
//!
//! **Problems with old pattern:**
//! - Tight coupling to infrastructure layer
//! - Mixed in services/traits instead of application/ports
//! - Returns infrastructure types (Result from shared::error)
//! - Hard to test (requires database mocks)
//! - Violates Dependency Inversion Principle
//!
//! # NEW PATTERN (DDD Architecture)
//!
//! ```rust,ignore
//! use crate::application::ports::mention_repository_port::MentionRepositoryPort;
//! use crate::shared::error::AppError;
//!
//! async fn new_way(repo: Arc<dyn MentionRepositoryPort>) -> Result<(), AppError> {
//!     // Clean port interface with domain types
//!     let mention = repo.create_mention("John Doe", "person", None).await?;
//!     let mentions = repo.search_mentions("John", 10).await?;
//!     let doc_mentions = repo.get_mentions_for_document("doc-123").await?;
//!     repo.delete_mention("mention-456").await?;
//!     Ok(())
//! }
//! ```
//!
//! **Benefits of new pattern:**
//! - ✅ Clean separation: Application → Ports → Infrastructure
//! - ✅ Domain-focused data types (MentionData, MentionWithContextData)
//! - ✅ Easy to test (mock implementations via ports)
//! - ✅ SOLID principles compliance
//! - ✅ Flexible (swap implementations)
//!
//! # Architecture Layers
//!
//! ## Application Ports (`src/application/ports/mention_repository_port.rs`)
//!
//! Clean interface defining mention operations:
//!
//! ```rust,ignore
//! #[derive(Debug, Clone)]
//! pub struct MentionData {
//!     pub id: String,
//!     pub name: String,
//!     pub mention_type: String,
//!     pub metadata: Option<String>,
//!     pub created_at: String,
//! }
//!
//! #[derive(Debug, Clone)]
//! pub struct MentionWithContextData {
//!     pub mention: MentionData,
//!     pub context: Option<String>,
//!     pub position: Option<i64>,
//! }
//!
//! #[async_trait]
//! pub trait MentionRepositoryPort: Send + Sync {
//!     async fn create_mention(
//!         &self,
//!         name: &str,
//!         mention_type: &str,
//!         metadata: Option<&str>,
//!     ) -> Result<MentionData, AppError>;
//!
//!     async fn find_mention_by_name(&self, name: &str) -> Result<Option<MentionData>, AppError>;
//!     async fn search_mentions(&self, query: &str, limit: i64) -> Result<Vec<MentionData>, AppError>;
//!     async fn get_mentions_by_type(&self, mention_type: &str) -> Result<Vec<MentionData>, AppError>;
//!     async fn get_mentions_for_document(&self, document_id: &str) -> Result<Vec<MentionWithContextData>, AppError>;
//!     async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>, AppError>;
//!     async fn extract_and_store_mentions(&self, document_id: &str, text: &str) -> Result<Vec<MentionWithContextData>, AppError>;
//!     async fn delete_mention(&self, id: &str) -> Result<(), AppError>;
//! }
//! ```
//!
//! ## Infrastructure (`src/infrastructure/persistence/repositories/`)
//!
//! ### Repository Implementation
//!
//! ```rust,ignore
//! pub struct MentionRepository {
//!     pool: SqlitePool,
//! }
//!
//! impl MentionRepositoryPort for MentionRepository {
//!     async fn create_mention(
//!         &self,
//!         name: &str,
//!         mention_type: &str,
//!         metadata: Option<&str>,
//!     ) -> Result<MentionData, AppError> {
//!         // SQLite implementation
//!         let mention_id = Uuid::new_v4().to_string();
//!         sqlx::query!(
//!             "INSERT INTO mentions (id, name, type, metadata) VALUES (?, ?, ?, ?)",
//!             mention_id, name, mention_type, metadata
//!         )
//!         .execute(&self.pool)
//!         .await?;
//!
//!         Ok(MentionData {
//!             id: mention_id,
//!             name: name.to_string(),
//!             mention_type: mention_type.to_string(),
//!             metadata: metadata.map(String::from),
//!             created_at: Utc::now().to_rfc3339(),
//!         })
//!     }
//!
//!     // ... other methods
//! }
//! ```
//!
//! # Testing Pattern
//!
//! ## Using Mock Repository
//!
//! ```rust,ignore
//! use crate::infrastructure::services::mocks::MockMentionRepository;
//! use crate::application::ports::mention_repository_port::MentionRepositoryPort;
//!
//! #[tokio::test]
//! async fn test_mention_operations() {
//!     let mock = Arc::new(MockMentionRepository::new()) as Arc<dyn MentionRepositoryPort>;
//!
//!     // Create mention
//!     let mention = mock.create_mention("John Doe", "person", None).await.unwrap();
//!     assert_eq!(mention.name, "John Doe");
//!
//!     // Search mentions
//!     let results = mock.search_mentions("John", 10).await.unwrap();
//!     assert_eq!(results.len(), 1);
//!
//!     // Extract mentions from text
//!     let text = "Met with @[John Doe] and discussed [[Project Alpha]].";
//!     let extracted = mock.extract_and_store_mentions("doc-123", text).await.unwrap();
//!     assert_eq!(extracted.len(), 2);
//! }
//! ```
//!
//! # ServiceContainer Integration
//!
//! ## OLD USAGE (Deprecated)
//!
//! ```rust,ignore
//! use crate::infrastructure::services::traits::MentionRepositoryTrait;
//!
//! pub struct ServiceContainer {
//!     mention_repository: Arc<dyn MentionRepositoryTrait>,
//! }
//!
//! impl ServiceContainer {
//!     pub fn mention_repository(&self) -> Arc<dyn MentionRepositoryTrait> {
//!         Arc::clone(&self.mention_repository)
//!     }
//! }
//! ```
//!
//! ## NEW USAGE (DDD Ports)
//!
//! ```rust,ignore
//! use crate::application::ports::mention_repository_port::MentionRepositoryPort;
//!
//! pub struct ServiceContainer {
//!     mention_repository: Arc<dyn MentionRepositoryPort>,
//! }
//!
//! impl ServiceContainer {
//!     pub fn mention_repository(&self) -> Arc<dyn MentionRepositoryPort> {
//!         Arc::clone(&self.mention_repository)
//!     }
//! }
//! ```
//!
//! # Migration Guide
//!
//! ## Method-by-Method Migration
//!
//! ### 1. create_mention
//!
//! **Old:**
//! ```rust,ignore
//! use crate::shared::error::Result;
//! async fn create_mention(&self, name: &str, mention_type: &str, metadata: Option<&str>)
//!     -> Result<MentionData>;
//! ```
//!
//! **New:**
//! ```rust,ignore
//! use crate::shared::error::AppError;
//! async fn create_mention(&self, name: &str, mention_type: &str, metadata: Option<&str>)
//!     -> Result<MentionData, AppError>;
//! ```
//!
//! **Changes:** Explicit error type (AppError)
//!
//! ### 2. find_mention_by_name
//!
//! **Old:**
//! ```rust,ignore
//! async fn find_mention_by_name(&self, name: &str) -> Result<Option<MentionData>>;
//! ```
//!
//! **New:**
//! ```rust,ignore
//! async fn find_mention_by_name(&self, name: &str) -> Result<Option<MentionData>, AppError>;
//! ```
//!
//! **Changes:** Explicit error type (AppError)
//!
//! ### 3. search_mentions
//!
//! **Old:**
//! ```rust,ignore
//! async fn search_mentions(&self, query: &str, limit: i64) -> Result<Vec<MentionData>>;
//! ```
//!
//! **New:**
//! ```rust,ignore
//! async fn search_mentions(&self, query: &str, limit: i64) -> Result<Vec<MentionData>, AppError>;
//! ```
//!
//! **Changes:** Explicit error type (AppError)
//!
//! ### 4. get_mentions_by_type
//!
//! **Old:**
//! ```rust,ignore
//! async fn get_mentions_by_type(&self, mention_type: &str) -> Result<Vec<MentionData>>;
//! ```
//!
//! **New:**
//! ```rust,ignore
//! async fn get_mentions_by_type(&self, mention_type: &str) -> Result<Vec<MentionData>, AppError>;
//! ```
//!
//! **Changes:** Explicit error type (AppError)
//!
//! ### 5. get_mentions_for_document
//!
//! **Old:**
//! ```rust,ignore
//! async fn get_mentions_for_document(&self, document_id: &str)
//!     -> Result<Vec<MentionWithContextData>>;
//! ```
//!
//! **New:**
//! ```rust,ignore
//! async fn get_mentions_for_document(&self, document_id: &str)
//!     -> Result<Vec<MentionWithContextData>, AppError>;
//! ```
//!
//! **Changes:** Explicit error type (AppError)
//!
//! ### 6. get_documents_with_mention
//!
//! **Old:**
//! ```rust,ignore
//! async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>>;
//! ```
//!
//! **New:**
//! ```rust,ignore
//! async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>, AppError>;
//! ```
//!
//! **Changes:** Explicit error type (AppError)
//!
//! ### 7. extract_and_store_mentions
//!
//! **Old:**
//! ```rust,ignore
//! async fn extract_and_store_mentions(&self, document_id: &str, text: &str)
//!     -> Result<Vec<MentionWithContextData>>;
//! ```
//!
//! **New:**
//! ```rust,ignore
//! async fn extract_and_store_mentions(&self, document_id: &str, text: &str)
//!     -> Result<Vec<MentionWithContextData>, AppError>;
//! ```
//!
//! **Changes:** Explicit error type (AppError)
//!
//! ### 8. delete_mention
//!
//! **Old:**
//! ```rust,ignore
//! async fn delete_mention(&self, id: &str) -> Result<()>;
//! ```
//!
//! **New:**
//! ```rust,ignore
//! async fn delete_mention(&self, id: &str) -> Result<(), AppError>;
//! ```
//!
//! **Changes:** Explicit error type (AppError)
//!
//! # Complete Migration Checklist
//!
//! - [x] Update imports: `use crate::application::ports::mention_repository_port::MentionRepositoryPort`
//! - [x] Change type: `Arc<dyn MentionRepositoryTrait>` → `Arc<dyn MentionRepositoryPort>`
//! - [x] Update ServiceContainer field types
//! - [x] Update ServiceContainer getter return types
//! - [x] Update mock implementations to implement MentionRepositoryPort
//! - [x] Update all test code to use new port
//! - [x] Update app.rs initialization to use MentionRepositoryPort
//! - [x] Verify error types are AppError (not generic Result)
//!
//! # Pattern Consistency
//!
//! This migration follows the same pattern used for:
//! - Tag migration (commit d97ddb4)
//! - Chunk migration (commit 0822a8c)
//! - Document migration (commit 44c1ebf)
//!
//! All persistence domains now use consistent DDD architecture with clean ports pattern.

// ============================================================================
// LEGACY TRAIT DEFINITION (Commented Out)
// ============================================================================
//
// The trait below has been DEPRECATED and replaced with MentionRepositoryPort.
// It is preserved here for reference during the migration period.
//
// use crate::shared::error::Result;
// use async_trait::async_trait;
//
// #[async_trait]
// pub trait MentionRepositoryTrait: Send + Sync {
//     /// Create a new mention
//     ///
//     /// # Arguments
//     /// * `name` - Name of the mention (e.g., "John Doe" or "Project Alpha")
//     /// * `mention_type` - Type of mention ("person", "wikilink", etc.)
//     /// * `metadata` - Optional JSON metadata
//     ///
//     /// # Returns
//     /// Created mention data
//     async fn create_mention(
//         &self,
//         name: &str,
//         mention_type: &str,
//         metadata: Option<&str>,
//     ) -> Result<crate::application::ports::mention_repository_port::MentionData>;
//
//     /// Find mention by name
//     ///
//     /// # Arguments
//     /// * `name` - Name to search for
//     ///
//     /// # Returns
//     /// Mention if found, None otherwise
//     async fn find_mention_by_name(
//         &self,
//         name: &str,
//     ) -> Result<Option<crate::application::ports::mention_repository_port::MentionData>>;
//
//     /// Search mentions by query
//     ///
//     /// # Arguments
//     /// * `query` - Search query (supports partial matching)
//     /// * `limit` - Maximum number of results
//     ///
//     /// # Returns
//     /// List of matching mentions
//     async fn search_mentions(
//         &self,
//         query: &str,
//         limit: i64,
//     ) -> Result<Vec<crate::application::ports::mention_repository_port::MentionData>>;
//
//     /// Get mentions by type
//     ///
//     /// # Arguments
//     /// * `mention_type` - Type to filter by ("person", "wikilink", etc.)
//     ///
//     /// # Returns
//     /// List of mentions of the specified type
//     async fn get_mentions_by_type(
//         &self,
//         mention_type: &str,
//     ) -> Result<Vec<crate::application::ports::mention_repository_port::MentionData>>;
//
//     /// Get mentions for a document
//     ///
//     /// # Arguments
//     /// * `document_id` - Document ID
//     ///
//     /// # Returns
//     /// List of mentions with context and position information
//     async fn get_mentions_for_document(
//         &self,
//         document_id: &str,
//     ) -> Result<Vec<crate::application::ports::mention_repository_port::MentionWithContextData>>;
//
//     /// Get documents that contain a mention (backlinks)
//     ///
//     /// # Arguments
//     /// * `mention_id` - Mention ID
//     ///
//     /// # Returns
//     /// List of document IDs that reference this mention
//     async fn get_documents_with_mention(
//         &self,
//         mention_id: &str,
//     ) -> Result<Vec<String>>;
//
//     /// Extract and store mentions from document text
//     ///
//     /// # Arguments
//     /// * `document_id` - Document ID
//     /// * `text` - Document text to extract mentions from
//     ///
//     /// # Returns
//     /// List of extracted mentions with context
//     async fn extract_and_store_mentions(
//         &self,
//         document_id: &str,
//         text: &str,
//     ) -> Result<Vec<crate::application::ports::mention_repository_port::MentionWithContextData>>;
//
//     /// Delete a mention
//     ///
//     /// # Arguments
//     /// * `id` - Mention ID to delete
//     async fn delete_mention(&self, id: &str) -> Result<()>;
// }
