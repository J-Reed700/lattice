// =============================================================================
// LINT CONFIGURATION - Oracle Week 1 Day 5: CI Check
// =============================================================================
// Phase 2 Week 1 Day 5: Allow unwrap/expect in test code only
// Production code (src/) has these lints denied in Cargo.toml
// Test code (#[cfg(test)] modules and tests/) is allowed to use unwrap for assertions
// Oracle's guidance: "A panic in a test is a valid failure signal"
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]
#![cfg_attr(test, allow(clippy::unwrap_in_result))]
#![cfg_attr(test, allow(clippy::indexing_slicing))]

//! # Recall Desktop - Personal Knowledge Management System
//!
//! A local-first knowledge management system with semantic search capabilities,
//! built with Domain-Driven Design (DDD) principles.
//!
//! ## Architecture
//!
//! This library follows **Domain-Driven Design (DDD)** with clear layer separation:
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │  Interfaces Layer (External Boundaries)                  │
//! │  - Tauri Commands (IPC)                                  │
//! │  - Event Handlers                                        │
//! │  - Dependency Injection Container                        │
//! └────────────────────┬─────────────────────────────────────┘
//!                      ↓ depends on
//! ┌──────────────────────────────────────────────────────────┐
//! │  Application Layer (Use Cases & Orchestration)           │
//! │  - Use Cases (workflows)                                 │
//! │  - DTOs (data transfer objects)                          │
//! │  - Ports (interface definitions for infrastructure)      │
//! │  - Mappers (domain ↔ DTO conversion)                     │
//! └────────────────────┬─────────────────────────────────────┘
//!                      ↓ depends on
//! ┌──────────────────────────────────────────────────────────┐
//! │  Domain Layer (Pure Business Logic)                      │
//! │  - Entities (Document, Chunk, SearchResult)              │
//! │  - Value Objects (FileMetadata, Checksum)                │
//! │  - Services (ChunkingService)                            │
//! │  ⚠️  ZERO external dependencies (only std + serde)       │
//! └──────────────────────────────────────────────────────────┘
//!                      ↑ implements
//! ┌──────────────────────────────────────────────────────────┐
//! │  Infrastructure Layer (Technical Implementations)        │
//! │  - Persistence (SQLite repositories)                     │
//! │  - Search (USearch HNSW, BM25, Hybrid)                    │
//! │  - ML (ONNX embeddings)                                  │
//! │  - LLM (Ollama, Anthropic)                               │
//! │  - File System (secure storage, watching)                │
//! │  - Security (keyring, validation, rate limiting)         │
//! └──────────────────────────────────────────────────────────┘
//!
//!          ┌──────────────────────────────────┐
//!          │  Shared Kernel (Foundation)      │
//!          │  - Error types                   │
//!          │  - Domain primitives (IDs)       │
//!          │  - Constants                     │
//!          │  - Utilities                     │
//!          └──────────────────────────────────┘
//! ```
//!
//! ## Dependency Rule
//!
//! Dependencies point **inward** only:
//!
//! - **Interfaces** depends on → Application, Domain, Infrastructure
//! - **Application** depends on → Domain (via ports)
//! - **Domain** depends on → NOTHING (pure business logic)
//! - **Infrastructure** depends on → Application (implements ports), Domain
//! - **Shared** is used by ALL layers
//!
//! The **Domain layer has ZERO external dependencies** and contains only pure business logic.
//!
//! ## Quick Start
//!
//! ### Using Domain Models
//!
//! ```rust,no_run
//! use recall_desktop::domain::{Document, ChunkingStrategy};
//! use recall_desktop::shared::{ValidatedFilePath, Result};
//! use std::path::PathBuf;
//!
//! # async fn example() -> Result<()> {
//! // Create a validated file path (prevents directory traversal)
//! let path = ValidatedFilePath::new(PathBuf::from("document.txt"))?;
//!
//! // Read file content
//! let content = std::fs::read_to_string(path.as_path())?;
//!
//! // Create document with chunking
//! let strategy = ChunkingStrategy::FixedSize { size: 512 };
//! let document = Document::from_file(path, content, strategy)?;
//!
//! // Access chunks
//! assert!(!document.chunks().is_empty());
//! # Ok(())
//! # }
//! ```
//!
//! ### Using Application Layer
//!
//! ```rust,no_run
//! use recall_desktop::application::{
//!     SearchRequestDto, SearchResponseDto,
//!     SemanticSearchUseCase,
//! };
//! use recall_desktop::shared::Result;
//!
//! # async fn example(use_case: SemanticSearchUseCase) -> Result<SearchResponseDto> {
//! let request = SearchRequestDto {
//!     query: "machine learning".to_string(),
//!     limit: Some(10),
//!     ..Default::default()
//! };
//!
//! let response = use_case.execute(request).await?;
//! # Ok(response)
//! # }
//! ```
//!
//! ## Public API Overview
//!
//! ### Shared Kernel (Foundation)
//!
//! ```rust
//! use recall_desktop::{
//!     AppError, Result,           // Error handling
//!     DocumentId, ChunkId,        // Type-safe IDs
//!     TagId, MentionId,           // More type-safe IDs
//!     ValidatedFilePath,          // Security-validated paths
//! };
//! ```
//!
//! ### Domain Layer (Pure Business Logic)
//!
//! ```rust
//! use recall_desktop::domain::{
//!     // Entities
//!     Document, DocumentStatus, Chunk, SearchResult, Embedding,
//!
//!     // Value Objects
//!     FileMetadata, Checksum, ChunkingStrategy,
//!     SearchQuery, SearchMode,
//!
//!     // Services
//!     ChunkingService,
//! };
//! ```
//!
//! ### Application Layer (Use Cases & DTOs)
//!
//! ```rust
//! use recall_desktop::application::{
//!     // Use Cases
//!     SemanticSearchUseCase, HybridSearchUseCase,
//!     IndexFileUseCase, IndexDirectoryUseCase,
//!     AskQuestionUseCase,
//!
//!     // DTOs
//!     SearchRequestDto, SearchResponseDto, SearchResultDto,
//!     IndexFileRequestDto, IndexFileResponseDto,
//!     QARequestDto, QAResponseDto,
//!     TagDto, CreateTagRequestDto, DocumentDto,
//!
//!     // Ports (interfaces for infrastructure)
//!     EmbeddingPort, VectorSearchPort, TextSearchPort,
//!     LLMPort, FileStoragePort, RepositoryPort,
//!     NotificationPort,
//! };
//! ```
//!
//! ### Infrastructure Layer (Concrete Implementations)
//!
//! ```rust
//! use recall_desktop::infrastructure::{
//!     // ML
//!     OnnxEmbeddingService,
//!
//!     // Search
//!     USearchVectorIndex, BM25Search,
//!
//!     // LLM
//!     OllamaClient,
//!
//!     // File System
//!     SecureFileStorage,
//!
//!     // Persistence
//!     DatabaseConnection, DocumentRepository, ChunkRepository,
//! };
//! ```
//!
//! ### Interfaces Layer (Commands & DI)
//!
//! ```rust
//! use recall_desktop::interfaces::{
//!     Container,          // Dependency injection container
//!     commands,           // Tauri command handlers
//!     event_handlers,     // Event handling
//! };
//! ```
//!
//! ## Feature Flags
//!
//! - `search`: Enable semantic search and embedding generation (default)
//! - `indexing`: Enable document processing and chunking (default)
//! - `qa`: Enable question-answering and LLM integration
//! - `extraction`: Enable content extraction from documents
//!
//! ## Migration from Old API
//!
//! The old API is still available but deprecated. Migration guide:
//!
//! ### Error Handling
//!
//! ```rust
//! // ❌ Old (deprecated)
//! use recall_desktop::error::{AppError, Result};
//!
//! // ✅ New (recommended)
//! use recall_desktop::shared::error::{AppError, Result};
//! // Or use the convenient re-exports:
//! use recall_desktop::{AppError, Result};
//! ```
//!
//! ### Domain Types
//!
//! ```rust
//! // ❌ Old (deprecated)
//! use recall_desktop::domain_types::DocumentId;
//!
//! // ✅ New (recommended)
//! use recall_desktop::shared::domain_types::DocumentId;
//! // Or use the convenient re-exports:
//! use recall_desktop::DocumentId;
//! ```
//!
//! ### Domain Models
//!
//! ```rust
//! // ❌ Old (deprecated - DocumentAggregate removed)
//! // use recall_desktop::domain::document::DocumentAggregate;
//!
//! // ✅ New (recommended)
//! use recall_desktop::domain::entities::document::Document;
//! // Or simpler:
//! use recall_desktop::domain::Document;
//! ```
//!
//! ## SOLID Principles
//!
//! This library strictly follows SOLID principles:
//!
//! - **Single Responsibility**: Each module has one clear purpose
//! - **Open/Closed**: Extend via ports/traits, not modification
//! - **Liskov Substitution**: All implementations respect port contracts
//! - **Interface Segregation**: Small, focused port traits
//! - **Dependency Inversion**: Depend on abstractions (ports), not concretions
//!
//! ## Security
//!
//! Security controls are layered throughout the architecture:
//!
//! - **Input Validation**: All inputs validated at command layer
//! - **Path Validation**: `ValidatedFilePath` prevents directory traversal (CWE-22)
//! - **Rate Limiting**: Resource-intensive operations protected (CWE-770)
//! - **Audit Logging**: Security events logged (CWE-778)
//! - **Secure Storage**: Credentials stored in OS keyring
//!
//! ## Testing
//!
//! - **Domain**: Pure unit tests (no mocks needed)
//! - **Application**: Use case tests with port mocks
//! - **Infrastructure**: Integration tests with real implementations
//! - **Interfaces**: Command tests with full DI container
//!
//! ## Version
//!
//! - Library: v0.2.0 (DDD architecture)
//! - Previous: v0.1.x (legacy architecture)

// Phase 2: Suppress warnings for cleanup phase (will be addressed in later phases)
#![allow(missing_docs)] // Phase 4: Documentation phase
#![allow(unused_imports)] // Phase 2: Clean up unused imports
#![allow(unused_variables)] // Phase 2: Clean up unused variables
#![allow(dead_code)] // Phase 2: Remove dead code
#![allow(deprecated)] // Phase 3: Update deprecated APIs
#![deny(unsafe_code)]
// Note: unused_crate_dependencies disabled - many crates are build/dev dependencies
// #![cfg_attr(test, deny(unused_crate_dependencies))]

// =============================================================================
// SHARED KERNEL - Foundation types used across all layers
// =============================================================================

/// Shared kernel module containing foundation types, errors, and utilities.
///
/// The shared kernel is used by ALL layers and has zero dependencies on
/// other application layers.
///
/// # Contents
///
/// - [`error`](shared::error) - Application error types
/// - [`domain_types`](shared::domain_types) - Type-safe domain primitives
/// - [`constants`](shared::constants) - Application-wide constants
/// - [`utils`](shared::utils) - Shared utilities
pub mod shared;

// Re-export commonly used shared types for convenience
pub use shared::{
    constants::*,
    domain_types::{ChunkId, DocumentId, MentionId, TagId, ValidatedFilePath},
    error::{AppError, ErrorResponse, Result, ResultExt},
    utils::{alignment, progress_emitter, retry},
};

// =============================================================================
// DOMAIN LAYER - Pure business logic (ZERO external dependencies)
// =============================================================================

/// Domain layer containing pure business logic with zero external dependencies.
///
/// # Architecture
///
/// The domain layer follows DDD patterns:
/// - **Aggregates**: Aggregate roots with business invariants
/// - **Entities**: Domain entities with identity
/// - **Value Objects**: Immutable, validated domain primitives
/// - **Services**: Pure business logic that doesn't fit in entities
///
/// # Zero Dependencies Rule
///
/// The domain layer depends ONLY on:
/// - Rust std library
/// - Common serialization (serde)
/// - Time handling (chrono)
///
/// NO infrastructure dependencies (database, HTTP, ML, etc.)
///
/// # Public API
///
/// ```rust
/// use recall_desktop::domain::{
///     DocumentAggregate, Document, DocumentStatus,
///     Chunk, SearchResult, Embedding,
///     FileMetadata, Checksum, ChunkingStrategy,
///     SearchQuery, SearchMode,
///     ChunkingService,
/// };
/// ```
pub mod domain;

pub use domain::{
    entities::{Chunk, Embedding, SearchResult},
    services::ChunkingService,
    value_objects::{
        Checksum, ChunkingStrategy, FileMetadata, IndexingOutcome, SearchMode, SearchQuery,
    },
    Document, DocumentStatus,
};

// =============================================================================
// APPLICATION LAYER - Use cases, DTOs, and port interfaces
// =============================================================================

/// Application layer containing use cases, DTOs, and port interfaces.
///
/// # Responsibilities
///
/// - **Use Case Orchestration**: Coordinate domain operations for workflows
/// - **DTO Management**: Define data structures for crossing boundaries
/// - **Mapping**: Convert between domain models and DTOs
/// - **Port Definitions**: Define interfaces for infrastructure (Hexagonal Architecture)
///
/// # Public API
///
/// ```rust
/// use recall_desktop::application::{
///     // Use Cases
///     SemanticSearchUseCase, HybridSearchUseCase,
///     IndexFileUseCase, IndexDirectoryUseCase,
///     AskQuestionUseCase,
///
///     // DTOs
///     SearchRequestDto, SearchResponseDto,
///     IndexFileRequestDto, QARequestDto,
///
///     // Ports
///     EmbeddingPort, VectorSearchPort, LLMPort,
/// };
/// ```
#[cfg(feature = "indexing")]
pub mod application;

#[cfg(feature = "indexing")]
pub use application::{
    dtos::{
        document_dto::DocumentDto,
        indexing_dto::{IndexFileRequestDto, IndexFileResponseDto},
        qa_dto::{QARequestDto, QAResponseDto},
        search_dto::{SearchRequestDto, SearchResponseDto, SearchResultDto},
        tag_dto::{CreateTagRequestDto, TagDto},
    },
    mappers,
    ports::{
        EmbeddingPort, FileStoragePort, LLMPort, NotificationPort, RepositoryPort, TextSearchPort,
        VectorSearchPort,
    },
    use_cases::{
        indexing::{IndexDirectoryUseCase, IndexFileUseCase},
        qa::AskQuestionUseCase,
        search::{HybridSearchUseCase, SemanticSearchUseCase},
    },
};

// =============================================================================
// INFRASTRUCTURE LAYER - Technical implementations (concrete adapters)
// =============================================================================

/// Infrastructure layer containing technical implementations of application ports.
///
/// # Implementations
///
/// - **Persistence**: SQLite repositories
/// - **Search**: HNSW vector search, BM25 text search, hybrid fusion
/// - **ML**: ONNX embedding generation
/// - **LLM**: Ollama and Anthropic clients
/// - **File System**: Secure file storage with validation
/// - **Security**: Keyring storage, rate limiting
/// - **Observability**: Tracing and monitoring
///
/// # Dependency Direction
///
/// Infrastructure implements Application ports and uses Domain entities.
/// Infrastructure should NOT be imported by Domain or Application layers.
///
/// # Public API
///
/// ```rust
/// use recall_desktop::infrastructure::{
///     OnnxEmbeddingService,
///     USearchVectorIndex, BM25Search,
///     OllamaClient,
///     SecureFileStorage,
///     DatabaseConnection, DocumentRepository,
/// };
/// ```
pub mod infrastructure;

pub use infrastructure::{
    audit, cache, extraction, file_system, llm, ml, observability, persistence, search, security,
    services, web,
};

// =============================================================================
// INTERFACES LAYER - External boundaries (commands, events, DI container)
// =============================================================================

/// Interfaces layer containing external boundaries and dependency injection.
///
/// # Responsibilities
///
/// - **Tauri Commands**: Thin controllers for IPC
/// - **Event Handlers**: Process domain and file system events
/// - **Dependency Injection**: Wire all layers together
///
/// # Command Pattern
///
/// Commands are thin controllers (< 50 lines) that:
/// 1. Apply rate limiting
/// 2. Validate inputs
/// 3. Delegate to use cases
/// 4. Log audit events
/// 5. Return responses
///
/// # Public API
///
/// ```rust
/// use recall_desktop::interfaces::{
///     Container,          // DI container
///     commands,           // Tauri commands
///     event_handlers,     // Event processing
/// };
/// ```
pub mod interfaces;

pub use interfaces::{di::Container, event_handlers};

// =============================================================================
// IPC LAYER - Anti-Corruption Boundary
// =============================================================================

/// IPC (Inter-Process Communication) Layer
///
/// Provides the transport boundary between Rust backend and TypeScript frontend.
/// Acts as an anti-corruption layer preventing transport concerns (Specta, Serde, Tauri)
/// from polluting domain and application layers.
///
/// **Phase 5: Crystal Conduit** - Oracle-mandated separation of transport and domain concerns.
///
/// # Key Types
///
/// - `ApiError`: Transport error contract with Specta bindings
///
/// # Architecture
///
/// ```text
/// TypeScript Frontend
///        ↕ Tauri IPC
///   Plugin Layer (ApiError) ← Transport boundary
///        ↕ From<AppError>
/// Shared Layer (AppError)  ← Unified error handling
///        ↕ From<DomainError>
/// Domain Layer             ← Pure business logic
/// ```

/// Tauri Plugin Infrastructure (Phase 1: Diamond Standard)
///
/// Domain-sharded plugins using tauri-specta v2 for type-safe IPC.
/// Coexists peacefully with the gateway pattern.
///
/// # Plugins
/// - **model**: 13 commands for model management
/// - **search**: 6 commands for search operations
/// - **file**: 12 commands for file operations
///
/// # Oracle Mandate
/// "Plugins are thin wrappers. ALL business logic stays in domain adapters."
pub mod plugins;

// =============================================================================
// COMPATIBILITY MODULES - Temporary shims for DDD migration
// =============================================================================

/// Compatibility module for Tag DDD migration.
///
/// This module provides backward-compatible exports during the Tag DDD migration.
/// It re-exports domain entities and application DTOs with legacy names.
///
/// # Migration Status
/// - Phase 1: ACTIVE - Compatibility shim (accept +32 regression)
/// - Phase 2-4: PENDING - Type fixes, DTOs, integration
/// - Phase 5: PENDING - Remove this module
///
/// # Note
/// This module will be removed in Phase 5 of the Tag DDD migration.
/// All code should eventually use domain/application types directly.
pub mod models;

// =============================================================================
// LEGACY MODULES - Deprecated but functional for backward compatibility
// =============================================================================

/// Legacy error module (DEPRECATED).
///
/// # Migration
///
/// ```rust
/// // ❌ Old
/// use recall_desktop::error::AppError;
///
/// // ✅ New
/// use recall_desktop::shared::error::AppError;
/// // Or simply:
/// use recall_desktop::AppError;
/// ```
#[deprecated(
    since = "0.2.0",
    note = "Use `shared::error` module instead, or use re-exported types at crate root"
)]
pub mod error {
    pub use crate::shared::error::*;
}

/// Legacy domain_types module (DEPRECATED).
///
/// # Migration
///
/// ```rust
/// // ❌ Old
/// use recall_desktop::domain_types::DocumentId;
///
/// // ✅ New
/// use recall_desktop::shared::domain_types::DocumentId;
/// // Or simply:
/// use recall_desktop::DocumentId;
/// ```
#[deprecated(
    since = "0.2.0",
    note = "Use `shared::domain_types` module instead, or use re-exported types at crate root"
)]
pub mod domain_types {
    pub use crate::shared::domain_types::*;
}

// =============================================================================
// PUBLIC UTILITY FUNCTIONS
// =============================================================================
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Get the directory where ML models are stored.
///
/// Returns the application data directory joined with "models".
///
/// # Errors
///
/// Returns an error if the app data directory cannot be determined.
pub fn get_model_dir(app: &AppHandle) -> Result<PathBuf> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("Failed to get app data directory: {}", e)))?;
    Ok(app_data.join("models"))
}

/// Get the path to the ONNX model file.
///
/// Returns the model directory joined with "model.onnx".
///
/// # Errors
///
/// Returns an error if the model directory cannot be determined.
pub fn get_model_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(get_model_dir(app)?.join("model.onnx"))
}

/// Get the path to the tokenizer file.
///
/// Returns the model directory joined with "tokenizer.json".
///
/// # Errors
///
/// Returns an error if the model directory cannot be determined.
pub fn get_tokenizer_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(get_model_dir(app)?.join("tokenizer.json"))
}

// =============================================================================
// LIBRARY METADATA
// =============================================================================

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Library name
pub const NAME: &str = env!("CARGO_PKG_NAME");

// =============================================================================
// TESTS
// =============================================================================

// Test modules (public for test utilities)
#[cfg(test)]
pub mod tests;

// Unit tests for lib.rs metadata
#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn test_version_exists() {
        assert!(!VERSION.is_empty());
        assert_eq!(NAME, "recall-desktop");
    }

    #[test]
    fn test_public_api_re_exports() {
        // Test that commonly used types are re-exported at crate root
        let _: Result<()> = Ok(());
        let _: AppError = AppError::Other("test".to_string());
    }
}
