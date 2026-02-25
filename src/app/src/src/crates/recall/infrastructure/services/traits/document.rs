//! Document Repository Trait - MIGRATED TO DDD
//!
//! This module documents the migration from legacy DocumentRepositoryTrait
//! to Domain-Driven Design architecture with proper ports and adapters.
//!
//! # Migration Status
//!
//! - ✅ Domain entity created (`domain/entities/document.rs`)
//! - ✅ Application ports defined (`application/ports/`)
//! - ✅ Repository implemented (`infrastructure/persistence/repositories/document_repository.rs`)
//! - ✅ Legacy trait removed (THIS PHASE)
//! - ✅ ServiceContainer refactored
//! - ✅ FunctionExecutor refactored
//! - ✅ All tests updated
//!
//! # OLD PATTERN (Deprecated)
//!
//! ```rust,ignore
//! use crate::shared::traits::DocumentRepositoryTrait;
//!
//! async fn old_way(repo: Arc<dyn DocumentRepositoryTrait>) -> Result<()> {
//!     // Old trait with flat interface
//!     let doc = repo.find_by_id("doc-123").await?;
//!     repo.update_status("doc-123", "indexed").await?;
//!     repo.delete("doc-456").await?;
//!     Ok(())
//! }
//! ```
//!
//! **Problems with old pattern:**
//! - Tight coupling to infrastructure
//! - No domain logic in Document type
//! - Hard to test (requires database mocks)
//! - Violates Dependency Inversion Principle
//!
//! # NEW PATTERN (DDD Architecture)
//!
//! ```rust,ignore
//! use crate::application::ports::{RepositoryPort, DocumentRepositoryPort};
//! use crate::domain::entities::Document;
//!
//! async fn new_way(
//!     repo: Arc<dyn RepositoryPort<Document> + DocumentRepositoryPort>
//! ) -> Result<()> {
//!     // Rich domain entity with business logic
//!     let doc = repo.find_by_id("doc-123").await?.unwrap();
//!
//!     // Business logic in domain entity
//!     if doc.needs_reindexing() {
//!         // Reindex logic
//!     }
//!
//!     // Document-specific operations
//!     let path = repo.find_file_path_by_id("doc-123").await?;
//!
//!     repo.delete("doc-456").await?;
//!     Ok(())
//! }
//! ```
//!
//! **Benefits of new pattern:**
//! - ✅ Clean separation: Domain → Ports → Infrastructure
//! - ✅ Rich domain entities with business logic
//! - ✅ Easy to test (mock implementations)
//! - ✅ SOLID principles compliance
//! - ✅ Flexible (swap implementations)
//!
//! # Architecture Layers
//!
//! ## Domain Layer (`src/domain/entities/document.rs`)
//!
//! Rich domain entity with business logic:
//!
//! ```rust,ignore
//! pub struct Document {
//!     id: DocumentId,
//!     file_path: ValidatedFilePath,
//!     metadata: FileMetadata,
//!     checksum: Checksum,
//!     status: DocumentStatus,
//!     indexed_at: DateTime<Utc>,
//! }
//!
//! impl Document {
//!     // Business logic
//!     pub fn is_indexed(&self) -> bool { ... }
//!     pub fn needs_reindexing(&self) -> bool { ... }
//!     pub fn is_text(&self) -> bool { ... }
//! }
//! ```
//!
//! ## Application Ports (`src/application/ports/`)
//!
//! ### Generic Port: `RepositoryPort<Document>`
//!
//! ```rust,ignore
//! #[async_trait]
//! pub trait RepositoryPort<T>: Send + Sync {
//!     async fn find_by_id(&self, id: &str) -> Result<Option<T>>;
//!     async fn save(&self, entity: &T) -> Result<()>;
//!     async fn delete(&self, id: &str) -> Result<()>;
//!     async fn count(&self) -> Result<usize>;
//!     // ... more CRUD operations
//! }
//! ```
//!
//! ### Specific Port: `DocumentRepositoryPort`
//!
//! ```rust,ignore
//! #[async_trait]
//! pub trait DocumentRepositoryPort: Send + Sync {
//!     async fn find_file_path_by_id(&self, document_id: &str) -> Result<String>;
//!     async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>>;
//!     async fn document_exists(&self, document_id: &str) -> Result<bool>;
//! }
//! ```
//!
//! ## Infrastructure (`src/infrastructure/persistence/repositories/`)
//!
//! ### Repository Implementation
//!
//! ```rust,ignore
//! pub struct DocumentRepository {
//!     pool: SqlitePool,
//! }
//!
//! impl RepositoryPort<Document> for DocumentRepository {
//!     // Implements generic CRUD...
//! }
//!
//! impl DocumentRepositoryPort for DocumentRepository {
//!     // Implements document-specific operations...
//! }
//! ```
//!
//! ### Mapper
//!
//! ```rust,ignore
//! pub struct DocumentMapper;
//!
//! impl DocumentMapper {
//!     pub fn to_entity(model: &DocumentModel) -> Document { ... }
//!     pub fn to_model(entity: &Document) -> DocumentModel { ... }
//! }
//! ```
//!
//! # Testing Pattern
//!
//! ## Using Mock Repository
//!
//! ```rust,ignore
//! use crate::infrastructure::persistence::repositories::mocks::MockDocumentRepository;
//! use crate::application::ports::{RepositoryPort, DocumentRepositoryPort};
//!
//! #[tokio::test]
//! async fn test_document_operations() {
//!     // Create mock
//!     let mock = Arc::new(MockDocumentRepository::new());
//!     let repo: Arc<dyn RepositoryPort<Document> + DocumentRepositoryPort> = mock;
//!
//!     // Create domain entity
//!     let doc = Document::new(
//!         DocumentId::new(),
//!         ValidatedFilePath::new(PathBuf::from("/test/file.txt"))?,
//!         file_metadata,
//!     );
//!
//!     // Test operations
//!     repo.save(&doc).await?;
//!     assert!(repo.exists(doc.id().as_str()).await?);
//!
//!     let found = repo.find_by_id(doc.id().as_str()).await?;
//!     assert!(found.is_some());
//! }
//! ```
//!
//! # ServiceContainer Integration
//!
//! ```rust,ignore
//! use crate::interfaces::di::ServiceContainer;
//! use crate::application::ports::{RepositoryPort, DocumentRepositoryPort};
//!
//! fn use_in_commands(container: &ServiceContainer) {
//!     // Get document repository from container
//!     let repo = container.document_repository();
//!
//!     // Type: Arc<dyn RepositoryPort<Document> + DocumentRepositoryPort>
//!
//!     // Use in command
//!     let doc = repo.find_by_id("doc-123").await?;
//! }
//! ```
//!
//! # Migration Checklist
//!
//! - [x] Domain entity exists with rich business logic
//! - [x] Generic port `RepositoryPort<Document>` defined
//! - [x] Specific port `DocumentRepositoryPort` defined
//! - [x] `DocumentRepository` implements both ports
//! - [x] `DocumentMapper` for entity ↔ model conversion
//! - [x] `ServiceContainer` refactored to use DDD ports
//! - [x] `FunctionExecutor` refactored to use DDD ports
//! - [x] `app.rs` initialization uses new pattern
//! - [x] All test files updated
//! - [x] Mock implementation created (`MockDocumentRepository`)
//! - [x] Legacy trait commented out in `shared/traits.rs`
//! - [x] Migration guides created
//!
//! # Files Modified (This Migration)
//!
//! 1. `src/shared/traits.rs` - Commented out legacy trait (+122 docs)
//! 2. `src/interfaces/di/service_container.rs` - DDD ports (~50 lines)
//! 3. `src/infrastructure/services/domains/function_executor.rs` - DDD ports (~15 lines)
//! 4. `src/infrastructure/setup/app.rs` - DDD initialization (~10 lines)
//! 5. `src/infrastructure/persistence/repositories/support/mocks.rs` - Mock (+180 lines)
//! 6. `src/infrastructure/services/traits/document.rs` - Migration guide (THIS FILE)
//! 7. `src/interfaces/di/tests.rs` - Updated mocks (~15 lines)
//! 8. `src/interfaces/commands/domains/function_calling_commands.rs` - Updated mocks (~10 lines)
//! 9. Additional test files as needed
//!
//! # Why This Migration?
//!
//! 1. **Domain-Driven Design**: Business logic lives in domain entities
//! 2. **Type Safety**: Value objects prevent invalid data
//! 3. **Security**: `ValidatedFilePath` prevents directory traversal (CWE-22)
//! 4. **Testability**: Port-based architecture enables easy mocking
//! 5. **Consistency**: Matches Tag (d97ddb4) and Chunk (0822a8c) migrations
//! 6. **SOLID Compliance**: Dependency Inversion Principle
//!
//! # See Also
//!
//! - `src/domain/entities/document.rs` - Rich domain entity
//! - `src/application/ports/repository_port.rs` - Generic port
//! - `src/application/ports/document_repository_port.rs` - Specific port
//! - `src/infrastructure/persistence/repositories/document_repository.rs` - Implementation
//! - `src/infrastructure/persistence/mappers/document_mapper.rs` - Mapper
//! - `src/infrastructure/persistence/repositories/support/mocks.rs` - Mock for testing
