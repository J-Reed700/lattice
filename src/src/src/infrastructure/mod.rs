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
//! Some legacy implementations remain outside this module while migration is
//! in progress.

pub mod adapters;
pub mod audit;
pub mod command_channel;
pub mod conversation_context;
pub mod crash;
pub mod document_scope;
pub(crate) mod embedding_loading;
pub(crate) mod embedding_runtime;
pub mod event_bus;
pub mod events;
pub mod extraction;
pub mod file_library;
pub mod file_system;
pub mod ml;
pub(crate) mod model_cache;
pub(crate) mod model_loading;
pub mod observability;
pub mod persistence;
pub mod sagas;
pub mod security;
pub mod services;
pub mod setup;
pub mod storage;
pub mod system_info_adapter;

// Model catalog infrastructure
