//! # Application Use Cases
//!
//! Use cases orchestrate domain logic and infrastructure to implement application workflows.
//!
//! ## Hexagonal Architecture
//!
//! Use cases are the core of the application layer in Hexagonal Architecture:
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │              Presentation Layer                  │
//! │         (Tauri Commands, CLI, API)              │
//! └────────────────┬────────────────────────────────┘
//!                  │
//!                  ▼
//! ┌─────────────────────────────────────────────────┐
//! │             Application Layer                    │
//! │                                                  │
//! │  ┌─────────────────────────────────────────┐   │
//! │  │          Use Cases (This Module)         │   │
//! │  │  • Orchestrate domain operations        │   │
//! │  │  • Coordinate infrastructure services   │   │
//! │  │  • Define transaction boundaries        │   │
//! │  └─────────────────────────────────────────┘   │
//! │                                                  │
//! │  ┌─────────────┐         ┌─────────────────┐   │
//! │  │    DTOs     │         │     Mappers     │   │
//! │  │  (Input/    │◄────────┤   (Domain ↔    │   │
//! │  │   Output)   │         │     DTO)        │   │
//! │  └─────────────┘         └─────────────────┘   │
//! │                                                  │
//! │  ┌─────────────────────────────────────────┐   │
//! │  │    Ports (Infrastructure Interfaces)     │   │
//! │  │  • EmbeddingPort                        │   │
//! │  │  • VectorSearchPort                     │   │
//! │  │  • LLMPort                              │   │
//! │  │  • FileStoragePort                      │   │
//! │  │  • RepositoryPort                       │   │
//! │  └─────────────────────────────────────────┘   │
//! └────────────────┬─────────────────┬──────────────┘
//!                  │                 │
//!                  ▼                 ▼
//! ┌─────────────────────────┐ ┌─────────────────────┐
//! │    Domain Layer         │ │  Infrastructure     │
//! │  • Aggregates           │ │  • Adapters         │
//! │  • Entities             │ │  • Repositories     │
//! │  • Value Objects        │ │  • External APIs    │
//! │  • Domain Services      │ │  • File System      │
//! └─────────────────────────┘ └─────────────────────┘
//! ```
//!
//! ## Design Principles
//!
//! 1. **Dependency Inversion**: Use cases depend on ports (abstractions), not implementations
//! 2. **Single Responsibility**: Each use case handles ONE specific workflow
//! 3. **Thin Controllers**: Use cases orchestrate, domain logic lives in domain layer
//! 4. **Testability**: All dependencies injected via ports, easy to mock
//! 5. **Composability**: Use cases can call other use cases for complex workflows
//!
//! ## Use Case Organization
//!
//! ### Search Use Cases (`search/`)
//!
//! - **SemanticSearchUseCase**: Vector-based similarity search
//! - **HybridSearchUseCase**: Combines vector + BM25 search with RRF
//! - **FileSearchUseCase**: Search by filename and path patterns
//!
//! ### Indexing Use Cases (`indexing/`)
//!
//! - **IndexFileUseCase**: Index a single file
//! - **IndexDirectoryUseCase**: Batch index directory contents
//! - **ReindexDocumentUseCase**: Update existing document index
//!
//! ### Q&A Use Cases (`qa/`)
//!
//! - **AskQuestionUseCase**: RAG-based question answering with citations
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::search::SemanticSearchUseCase;
//! use vault_desktop::application::dtos::search_dto::{SearchRequestDto, SearchModeDto};
//! use std::sync::Arc;
//!
//! # async fn example(
//! #     embedder: Arc<dyn vault_desktop::application::ports::EmbeddingPort>,
//! #     searcher: Arc<dyn vault_desktop::application::ports::VectorSearchPort>
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! // 1. Create use case with injected dependencies
//! let search_use_case = SemanticSearchUseCase::new(embedder, searcher);
//!
//! // 2. Prepare request DTO
//! let request = SearchRequestDto {
//!     query: "machine learning algorithms".to_string(),
//!     limit: Some(10),
//!     threshold: Some(0.7),
//!     mode: SearchModeDto::Vector,
//! };
//!
//! // 3. Execute use case
//! let response = search_use_case.execute(request).await?;
//!
//! // 4. Use response
//! println!("Found {} results", response.total);
//! for result in response.results {
//!     println!("  - {} (score: {})", result.title, result.score);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Testing Use Cases
//!
//! Use cases are highly testable because all dependencies are injected via ports:
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::search::SemanticSearchUseCase;
//! use vault_desktop::application::ports::{EmbeddingPort, VectorSearchPort};
//! use async_trait::async_trait;
//! use std::sync::Arc;
//!
//! // Create mock implementations
//! struct MockEmbedder;
//!
//! #[async_trait]
//! impl EmbeddingPort for MockEmbedder {
//!     async fn embed_single(&self, text: &str) -> vault_desktop::error::Result<Vec<f32>> {
//!         Ok(vec![0.1, 0.2, 0.3]) // Mock embedding
//!     }
//!
//!     async fn embed_batch(&self, texts: &[String]) -> vault_desktop::error::Result<Vec<Vec<f32>>> {
//!         Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
//!     }
//!
//!     fn dimension(&self) -> usize { 3 }
//! }
//!
//! // ... (MockVectorSearch implementation)
//!
//! # async fn test_example() {
//! // Test with mocks
//! let use_case = SemanticSearchUseCase::new(
//!     Arc::new(MockEmbedder),
//!     Arc::new(MockVectorSearch),
//! );
//!
//! // Execute and assert
//! // ...
//! # }
//! ```
//!
//! ## Adding New Use Cases
//!
//! To add a new use case:
//!
//! 1. **Create the use case file** in the appropriate directory
//! 2. **Define dependencies** as `Arc<dyn Port>` fields
//! 3. **Implement `new()` constructor** accepting port dependencies
//! 4. **Implement `execute()` method** with DTO input/output
//! 5. **Add comprehensive tests** using mock port implementations
//! 6. **Export from module** in `mod.rs`
//!
//! ### Example Skeleton
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use crate::application::dtos::my_dto::{MyRequestDto, MyResponseDto};
//! use crate::application::ports::MyPort;
//! use crate::shared::error::Result;
//!
//! pub struct MyUseCase {
//!     my_service: Arc<dyn MyPort>,
//! }
//!
//! impl MyUseCase {
//!     pub fn new(my_service: Arc<dyn MyPort>) -> Self {
//!         Self { my_service }
//!     }
//!
//!     pub async fn execute(&self, request: MyRequestDto) -> Result<MyResponseDto> {
//!         // 1. Validate input
//!         // 2. Call domain services
//!         // 3. Coordinate infrastructure
//!         // 4. Build response
//!         todo!()
//!     }
//! }
//! ```

// Export use case modules
// Vertical-slice migration (backup): use cases live in features/backup/use_cases/.
#[path = "../../features/backup/use_cases/mod.rs"]
pub mod backup;
pub mod batch;
// Vertical-slice migration (cache): use cases live in features/cache/use_cases/.
#[path = "../../features/cache/use_cases/mod.rs"]
pub mod cache;
pub mod conversation;
pub mod credentials;
pub mod custom_model;
pub mod embedding;
pub mod extraction;
// Vertical-slice migration (favorites): use cases live in features/favorites/use_cases/.
#[path = "../../features/favorites/use_cases/mod.rs"]
pub mod favorites;
pub mod file;
pub mod function_calling;
// Vertical-slice migration (health): use cases live in features/health/use_cases/.
#[path = "../../features/health/use_cases/mod.rs"]
pub mod health;
pub mod indexing;
pub mod initialization;
pub mod llm;
// Vertical-slice migration (mentions): use cases live in features/mentions/use_cases/.
#[path = "../../features/mentions/use_cases/mod.rs"]
pub mod mentions;
// Vertical-slice migration (metrics): use cases live in features/metrics/use_cases/.
#[path = "../../features/metrics/use_cases/mod.rs"]
pub mod metrics;
pub mod model_management;
// Vertical-slice migration (qa): use cases live in features/qa/use_cases/.
#[path = "../../features/qa/use_cases/mod.rs"]
pub mod qa;
// Vertical-slice migration (recent): use cases live in features/recent/use_cases/.
#[path = "../../features/recent/use_cases/mod.rs"]
pub mod recent;
pub mod search;
pub mod settings;
// Vertical-slice migration (stats): use cases live in features/stats/use_cases/.
#[path = "../../features/stats/use_cases/mod.rs"]
pub mod stats;
// Vertical-slice migration (tags): use cases live in features/tags/use_cases/.
#[path = "../../features/tags/use_cases/mod.rs"]
pub mod tags;
// Vertical-slice migration (updates): use cases live in features/updates/use_cases/.
#[path = "../../features/updates/use_cases/mod.rs"]
pub mod updates;
pub mod web;

// Re-export all use cases for convenience
pub use batch::{GetBatchFileStatusUseCase, StartBatchFileImportUseCase};
pub use conversation::{
    CreateConversationUseCase, DeleteConversationUseCase, GetConversationMessagesUseCase,
    GetConversationUseCase, ListConversationsUseCase, RenameConversationUseCase,
};
pub use custom_model::{
    AddFromFileUseCase, AddFromUrlUseCase, DeleteUseCase as DeleteCustomModelUseCase,
    ListUseCase as ListCustomModelsUseCase, ValidateUseCase as ValidateCustomModelUseCase,
};
pub use embedding::{
    GenerateBatchEmbeddingsUseCase, GenerateSingleEmbeddingUseCase, GetEmbeddingModelInfoUseCase,
};
pub use extraction::{
    ExtractAndResolveLinksUseCase, ExtractDocumentTitleUseCase, ParseWikilinksUseCase,
    ResolveWikilinkUseCase,
};
pub use favorites::{
    AddFavoriteUseCase, IsFavoriteUseCase, ListFavoritesUseCase, RemoveFavoriteUseCase,
};
pub use file::{
    GetFileMetadataUseCase, GetFilePathByIdUseCase, OpenFileByIdUseCase, OpenFileUseCase,
    ReadFileContentUseCase, ShowInFolderUseCase,
};
pub use function_calling::{ExecuteFunctionUseCase, ListAvailableFunctionsUseCase};
pub use health::HealthCheckUseCase;
pub use indexing::{IndexDirectoryUseCase, IndexFileUseCase, ReindexDocumentUseCase};
pub use initialization::{InitializeDatabaseUseCase, InitializeModelsUseCase};
pub use llm::{
    CheckModelDownloadedUseCase, DeleteModelUseCase, DownloadModelUseCase,
    GetAvailableModelsUseCase, GetBestModelUseCase, GetModelPathUseCase,
    GetRecommendedModelsUseCase, GetSystemCapabilitiesUseCase, ListDownloadedModelsUseCase,
};
pub use mentions::{
    ExtractMentionsUseCase, GetBacklinksUseCase, GetMentionsByTypeUseCase, SearchMentionsUseCase,
};
pub use model_management::{
    CheckIsDownloadedUseCase, DeleteDownloadedModelUseCase, GetActiveChatModelUseCase,
    GetActiveEmbeddingModelUseCase, GetDownloadedModelsWithMetadataUseCase,
    SetActiveChatModelUseCase, SetActiveEmbeddingModelUseCase, TrackDownloadUseCase,
};
pub use qa::AskQuestionUseCase;
pub use recent::{ClearRecentHistoryUseCase, GetRecentDocumentsUseCase, TrackAccessUseCase};
pub use search::{FileSearchUseCase, HybridSearchUseCase, SemanticSearchUseCase};
pub use settings::{
    ExportSettingsUseCase, GetSettingsUseCase, ImportSettingsUseCase, ResetSettingsUseCase,
    UpdateSettingsUseCase, ValidateSettingsUseCase,
};
pub use stats::GetSystemStatsUseCase;
pub use tags::{
    ApplyTagsUseCase, AutoTagAllDocumentsUseCase, CreateTagUseCase, DeleteTagUseCase,
    GenerateTagsUseCase, GetTagsUseCase, RemoveTagFromDocumentUseCase, SearchByTagUseCase,
    UpdateTagUseCase,
};
pub use web::{CleanArticleContentUseCase, GetUrlPreviewUseCase, IngestWebUrlUseCase};
