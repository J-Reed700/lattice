//! Infrastructure Layer
//!
//! This layer contains technical implementations of application ports:
//! - **Persistence**: Database access (repositories, models, migrations)
//! - **Search**: Search engines (HNSW vector search, BM25 text search, hybrid fusion)
//! - **ML**: Machine learning services (ONNX embeddings, model management)
//! - **LLM**: Large language model clients (Ollama, Anthropic, local models)
//! - **File System**: File operations (storage, watching, validation)
//! - **Extraction**: Content extraction (PDF, DOCX, HTML)
//! - **Web**: Web content ingestion and scraping
//! - **Cache**: Caching mechanisms (query cache, LLM cache)
//! - **Security**: Security services (keyring, validation, rate limiting, auth)
//! - **Observability**: Monitoring and logging (tracing, metrics)
//! - **Audit**: Audit logging and compliance
//!
//! # Dependency Direction
//! Infrastructure implements Application ports and uses Domain entities.
//! Infrastructure should NOT be imported by Domain or Application layers.
//!
//! # Clean Architecture Compliance
//! - Infrastructure depends on: Application (ports), Domain (entities)
//! - Infrastructure provides: Concrete implementations of ports
//! - Infrastructure must NOT: Be imported by Domain or Application layers
//!
//! # Migration Status
//! Phase 3: Directory structure created, implementations pending migration

pub mod adapters;
pub mod audit;
pub mod cache;
pub mod crash;
pub mod event_bus;
pub mod events;
pub mod extraction;
pub mod file_system;
// Vertical-slice migration (indexing): pipeline lives in features/indexing/engine/.
#[path = "../features/indexing/engine/mod.rs"]
pub mod indexing;
// Vertical-slice migration (llm): engine lives in features/llm/engine/.
#[path = "../features/llm/engine/mod.rs"]
pub mod llm;
pub mod ml;
pub mod observability;
pub mod persistence;
// Vertical-slice migration (qa): QA engine lives in features/qa/engine/.
#[path = "../features/qa/engine/mod.rs"]
pub mod qa;
pub mod sagas;
// Vertical-slice migration (search): retrieval engine lives in features/search/engine/.
#[path = "../features/search/engine/mod.rs"]
pub mod search;
pub mod security;
pub mod services;
pub mod setup;
pub mod storage;
pub mod system;
pub mod system_info_adapter;
pub mod updates;
// Vertical-slice migration (web): infrastructure web module lives in features/web/infra_mod.rs.
#[path = "../features/web/infra_mod.rs"]
pub mod web;

// Model catalog infrastructure
pub mod huggingface_adapter;
pub mod model_cache_adapter;
pub mod model_catalog_cache;
