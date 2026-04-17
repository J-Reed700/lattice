//! # Model Management feature
//!
//! Model lifecycle: tracking which models are downloaded, which are
//! the active chat / embedding models, model metadata catalog, and
//! adapters bridging the LLM/HuggingFace catalogs to the persistent
//! model repository. The "model" plugin (Tauri) lives here too.
//!
//! ## File layout
//!
//! | Path                                       | Canonical module path                                                          |
//! |--------------------------------------------|--------------------------------------------------------------------------------|
//! | `domain.rs`                                | `crate::domain::model_management`                                              |
//! | `use_cases/`                               | `crate::application::use_cases::model_management`                              |
//! | `commands.rs`                              | `crate::interfaces::commands::model_management`                                |
//! | `commands_extra.rs`                        | `crate::interfaces::commands::model_management_commands`                       |
//! | `repository_tx/`                           | `crate::infrastructure::persistence::repositories::model` (tx wrapper)         |
//! | `huggingface_adapter.rs`                   | `crate::infrastructure::huggingface_adapter`                                   |
//! | `cache_adapter.rs`                         | `crate::infrastructure::model_cache_adapter`                                   |
//! | `catalog_cache.rs`                         | `crate::infrastructure::model_catalog_cache`                                   |
//! | `plugin/mod.rs`                            | `crate::plugins::model` (directory plugin)                                     |
//!
//! ## Deliberately NOT moved (shared infra)
//!
//! - DI wiring in `interfaces/di/{modules,container}.rs` — per oracle
//!   ruling. The DI container constructs the adapters and wires them
//!   to ports.
//! - Application ports for model storage / catalog stay in
//!   `application/ports/`.
//! - No DTO file — model_management commands speak in domain types
//!   directly.
//! - No services/{traits,mocks} — service abstractions are use case
//!   level here, not service-trait level.
//!
//! ## Adapters that graduated from infrastructure/
//!
//! Three top-level files in `infrastructure/` were adopted into this
//! feature because they are model-management specific:
//! - `infrastructure/huggingface_adapter.rs` (the ModelCatalogPort
//!   impl backed by HuggingFace's catalog)
//! - `infrastructure/model_cache_adapter.rs` (the in-memory model
//!   cache wrapper around HF + curated models)
//! - `infrastructure/model_catalog_cache.rs` (the persistent
//!   model catalog cache)
//!
//! These are renamed without the `model_` prefix inside the feature
//! since the feature directory already provides that namespace.
//!
//! ## LLM ↔ download ↔ model_management entanglement
//!
//! See `features/llm/mod.rs` — the `engine/model_catalog_adapter.rs`
//! and `engine/model_storage_adapter.rs` files there are LLM-side
//! adapters into model_management. Per the oracle ruling those stay
//! with the LLM engine for now (they are constructed from the LLM
//! factory). Cross-feature use case calls (e.g. LLM use cases call
//! into download) continue to resolve through Strangler Fig.
//!
//! ## Use case rename
//!
//! All `*_use_case.rs` files were renamed to drop the suffix during
//! this migration (e.g. `check_is_downloaded_use_case.rs` →
//! `check_is_downloaded.rs`), matching the convention used by
//! conversation, embedding, and other features. The `use_cases/mod.rs`
//! file was rewritten to point at the new filenames; type names
//! (e.g. `CheckIsDownloadedUseCase`) are unchanged.
