//! # LLM feature
//!
//! Local + remote LLM inference: clients (Ollama HTTP, llama.cpp via
//! mistral.rs local), circuit breaker, model catalog, system
//! capabilities (GPU/CPU detection), and model download orchestration.
//!
//! ## File layout
//!
//! | Path                                       | Canonical module path                                                |
//! |--------------------------------------------|-----------------------------------------------------------------------|
//! | `dto.rs`                                   | `crate::application::dtos::llm_dto`                                   |
//! | `use_cases/`                               | `crate::application::use_cases::llm`                                  |
//! | `commands.rs`                              | `crate::interfaces::commands::llm`                                    |
//! | `engine/`                                  | `crate::infrastructure::llm` (LLM clients, factory, catalog, system)  |
//! | `engine/{circuit_breaker,factory,…,types}.rs` | `crate::infrastructure::llm::*` (formerly `infrastructure/llm/modules/`) |
//! | `engine/inference/`                        | `crate::infrastructure::llm::inference` (mistral.rs local engine)     |
//! | `engine/system/`                           | `crate::infrastructure::llm::system` (GPU/platform detection)         |
//!
//! ## Deliberately NOT moved (shared infra)
//!
//! - `application/ports/llm_port.rs` — heavily-shared `LLMPort` trait,
//!   consumed by qa, conversation, function_calling, and others.
//! - DI wiring in `interfaces/di/{modules,container}.rs` — per oracle
//!   ruling.
//! - No dedicated plugin — LLM commands flow through the model plugin.
//! - `infrastructure/cache/llm_cache.rs` — orphan placeholder stub
//!   (per handoff §4.3); the real cache lives in `features/cache/`.
//!
//! ## LLM ↔ download ↔ model_management entanglement
//!
//! Use cases like `DownloadModelUseCase` live here in
//! `use_cases/download_model.rs` but call into `features/download/`
//! and `features/model_management/`. These are downstream cross-feature
//! calls; they continue to resolve through Strangler Fig redirects.
//!
//! `engine/model_catalog_adapter.rs` (HardcodedModelCatalog) and
//! `engine/model_storage_adapter.rs` (FilesystemModelStorage) are
//! adapters between LLM-side catalogs and the model_management /
//! download features. Per handoff, these may eventually graduate to
//! `features/model_management/` — but they stay with the LLM engine
//! for this commit because they're invoked from the LLM factory.
//!
//! ## Engine flattening
//!
//! Same pattern as search/indexing: the legacy
//! `infrastructure/llm/modules/` filesystem-organization wart was
//! dissolved during this migration. 10 files now live as direct
//! children of `engine/`, and the `#[path = "modules/foo.rs"]`
//! attributes in `engine/mod.rs` are now plain `pub mod foo;`.
//! The `inference/` and `system/` subdirectories preserve their
//! structure because they group cohesive concerns.
