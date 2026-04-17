//! # Embedding feature
//!
//! Text embedding generation via ONNX models (local) or remote APIs,
//! plus persistence of embeddings tied to document chunks. One of the
//! most cross-consumed features in the app — search, indexing, qa,
//! and mentions all depend on the shared ports this feature provides.
//!
//! ## File layout
//!
//! | Path                                | Canonical module path                                                     |
//! |-------------------------------------|---------------------------------------------------------------------------|
//! | `entity.rs`                         | `crate::domain::entities::embedding` (Embedding)                          |
//! | `dto.rs`                            | `crate::application::dtos::embedding_dto`                                 |
//! | `use_cases/`                        | `crate::application::use_cases::embedding`                                |
//! | `onnx_service.rs`                   | `crate::infrastructure::ml::onnx_embedding_service`                       |
//! | `remote_service.rs`                 | `crate::infrastructure::ml::remote_embedding_service`                     |
//! | `generator.rs`                      | `crate::infrastructure::ml::generator` (EmbeddingGenerator, ModelConfig)  |
//! | `validator.rs`                      | `crate::infrastructure::ml::validator`                                    |
//! | `persistence_mapper.rs`             | `crate::infrastructure::persistence::mappers::embedding_mapper`           |
//! | `repository.rs`                     | `crate::infrastructure::persistence::repositories::embedding_repository`  |
//! | `repository_tx/`                    | `crate::infrastructure::persistence::repositories::embedding` (tx-wrapper) |
//! | `service/`                          | `crate::infrastructure::services::embedding`                              |
//! | `trait_def.rs`                      | `crate::infrastructure::services::traits` (merged re-exports)             |
//! | `mocks.rs`                          | `crate::infrastructure::services::mocks` (merged re-exports)              |
//! | `commands.rs`                       | `crate::interfaces::commands::embeddings`                                 |
//! | `plugin.rs`                         | `crate::plugins::embeddings`                                              |
//!
//! ## Deliberately NOT moved (shared infra)
//!
//! Three ports stay in `application/ports/` — they are consumed across
//! many features:
//! - `EmbeddingPort`
//! - `EmbeddingRepositoryPort`
//! - `MockEmbeddingPort` (runtime fallback, not a test mock)
//!
//! Also staying:
//! - `domain/modules/embedding_constants.rs` — cross-feature constants
//!   (default model name, dimensions, etc.)
//! - `infrastructure/setup/embedding.rs` — app bootstrap wiring
//! - `infrastructure/ml/tokenizer.rs` and `model_manager.rs` — orphaned
//!   migration-placeholder code, nothing references them externally
//!
//! The `infrastructure/services/embedding_tests.rs` file is similarly
//! an orphan (not registered in any mod.rs). Left in place.
