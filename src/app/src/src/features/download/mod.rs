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
//! - `crate::features::download::downloaded_model_repository` —
//!   DownloadedModelRepository (implements the domain port)
//! - `crate::features::download::engine` — DownloadEngine
//! - `crate::features::download::manager` — DownloadManager, DownloadEvent
//! - `crate::features::download::saga` — DownloadSaga
//! - `crate::features::download::commands` — Tauri command handlers
//! - `crate::features::download::plugin::init()` — Tauri plugin
//!
//! Domain types remain accessible via the shared `crate::domain::*`
//! paths (DownloadSession, DownloadError, DownloadedModel, etc.) —
//! their Strangler Fig redirects in `domain/mod.rs`,
//! `domain/events/mod.rs`, and `domain/repositories/mod.rs` are kept
//! because the `domain` namespace is a shared aggregator consumed
//! broadly across features (shared/error, llm, model_management,
//! infrastructure/services, interfaces/commands).
//!
//! No use cases here — download is orchestrated *through* the LLM and
//! model-management features, which own the model-download use cases.

pub mod commands;
pub mod download_repository;
pub mod downloaded_model_repository;
pub mod engine;
pub mod events;
pub mod manager;
pub mod plugin;
pub mod saga;
