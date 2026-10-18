//! # Download feature
//!
//! Model file download pipeline: fetch model weights/config from a
//! remote source (HuggingFace, GitHub release, etc.), verify checksums,
//! track progress, persist download state, emit events for UI.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::download::download_repository` — download state repo
//! - `crate::features::download::engine` — DownloadEngine
//! - `crate::features::download::manager` — DownloadManagerService, which
//!   implements core's `DownloadManager` port
//! - `crate::features::download::saga` — DownloadSaga
//! - `crate::features::download::commands` — Tauri command handlers
//! - `crate::features::download::plugin::init()` — Tauri plugin
//! - `crate::features::download::events` — model download domain events
//!   and outbound UI event payloads
//!
//! Domain types (DownloadSession, DownloadError, DownloadedModel, the
//! DownloadedModelRepository port) live in the domain layer under
//! `lattice_core::domain::{download, models, repositories}` because the application layer and several other
//! features depend on them. The SQLite store of downloaded models is
//! `crate::infrastructure::persistence::repositories::downloaded_models`, and
//! artifact identity hashing is `crate::infrastructure::ml::artifact_identity`.
//!
//! No use cases here — download is orchestrated *through* the LLM and
//! model-management features, which own the model-download use cases.

pub mod commands;
pub mod download_repository;
pub mod engine;
pub mod events;
pub mod manager;
pub mod plugin;
pub mod saga;
