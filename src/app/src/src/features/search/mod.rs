//! # Search feature
//!
//! Hybrid retrieval combining vector (USearch HNSW), keyword (SQLite
//! FTS5 BM25), filename, and recency search — with reciprocal-rank
//! fusion, optional reranking, and query expansion. The single most
//! cross-consumed feature in the app: qa, mentions, indexing, and
//! conversation chat all rely on the shared ports this feature
//! provides.
//!
//! ## File layout
//!
//! | Path                                              | Canonical module path                                                          |
//! |---------------------------------------------------|--------------------------------------------------------------------------------|
//! | `dto.rs`                                          | `crate::application::dtos::search_dto`                                         |
//! | `mapper.rs`                                       | `crate::application::mappers::search_mapper`                                   |
//! | `commands.rs`                                     | `crate::interfaces::commands::search_commands`                                 |
//! | `plugin/mod.rs`                                   | `crate::plugins::search` (directory plugin)                                    |
//! | `entity.rs`                                       | `crate::domain::entities::search_result` (SearchResult)                        |
//! | `ranking_service.rs`                              | `crate::domain::services::search_ranking_service`                              |
//! | `repository.rs`                                   | `crate::domain::repositories::search_repository`                               |
//! | `repository_tx/`                                  | `crate::infrastructure::persistence::repositories::search` (tx wrapper)        |
//! | `value_objects/mode.rs`                           | `crate::domain::value_objects::search_mode`                                    |
//! | `value_objects/query.rs`                          | `crate::domain::value_objects::search_query`                                   |
//! | `use_cases/`                                      | `crate::application::use_cases::search`                                        |
//! | `enrichment_service.rs`                           | `crate::infrastructure::services::search_enrichment_service`                   |
//! | `trait_def.rs`                                    | `crate::infrastructure::services::traits` (merged re-exports)                  |
//! | `mocks.rs`                                        | `crate::infrastructure::services::mocks` (merged re-exports)                   |
//! | `engine/`                                         | `crate::infrastructure::search` (the retrieval engine)                         |
//! | `engine/{bm25,builder,…,vector_ops}.rs`           | `crate::infrastructure::search::*` (formerly `infrastructure/search/modules/`) |
//! | `engine/hybrid/`                                  | `crate::infrastructure::search::hybrid`                                        |
//! | `engine/query_expansion/`                         | `crate::infrastructure::search::query_expansion`                               |
//! | `engine/query_expansion/dictionaries/`            | `crate::infrastructure::search::query_expansion::dictionaries`                 |
//! | `engine/strategies/`                              | `crate::infrastructure::search::strategies`                                    |
//! | `engine/text_search/`                             | `crate::infrastructure::search::text_search`                                   |
//! | `engine/vector_search/usearch_index.rs`           | `crate::infrastructure::search::vector_search::usearch_index`                  |
//!
//! ## Deliberately NOT moved (shared infra)
//!
//! These ports stay in `application/ports/` — they are consumed across
//! many features:
//! - `VectorSearchPort` (consumed by qa, mentions, indexing, conversation chat)
//! - `TextSearchPort`
//!
//! Other shared bits left in place:
//! - DI wiring in `interfaces/di/{modules,container}.rs` — per oracle
//!   ruling, do not dismantle the DI container during migration.
//!   Imports continue to resolve through Strangler Fig.
//! - `lib.rs` re-export `pub use infrastructure::search as search;` —
//!   keeps `crate::search::*` consumer paths working unchanged.
//! - `tests/plugins/search.rs` — integration test stays under `tests/`.
//!
//! ## Engine flattening
//!
//! The legacy `infrastructure/search/modules/` subdirectory was a
//! filesystem-organization wart. During this migration its contents
//! were *flattened* into `engine/` directly, and the
//! `#[path = "modules/foo.rs"]` attributes in `engine/mod.rs` were
//! replaced with plain `pub mod foo;` declarations.
//!
//! Other subdirectories (`hybrid/`, `query_expansion/`, `strategies/`,
//! `text_search/`, `vector_search/`) preserve their structure because
//! they group cohesive concerns rather than reflecting accidental
//! layout.
//!
//! ## Critical: `usearch_index.rs` single-load discipline
//!
//! `engine/vector_search/usearch_index.rs` is the in-process retrieval
//! index. The DI container holds a single `Arc<USearchVectorIndex>`.
//! It must be loaded at exactly one Rust module path. The Strangler
//! Fig redirect chain is:
//!
//!   `infrastructure/mod.rs` (`#[path]`)
//!     → `engine/mod.rs`
//!       → `engine/vector_search/mod.rs` (`pub mod`)
//!         → `engine/vector_search/usearch_index.rs`
//!
//! This file is **not** declared anywhere else (e.g. no `pub mod`
//! inside `features/search/mod.rs`). Adding a second registration
//! would silently duplicate the index type and break retrieval.
