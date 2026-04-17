//! # Download feature
//!
//! Model file download pipeline: fetch model weights/config from a
//! remote source (HuggingFace, GitHub release, etc.), verify checksums,
//! track progress, persist download state, emit events for UI.
//!
//! ~9 KLOC across 13 files.
//!
//! ## File layout
//!
//! | Path                                  | Canonical module path                                            |
//! |---------------------------------------|------------------------------------------------------------------|
//! | `domain/download.rs`                  | `crate::domain::download` (DownloadSession, state machine)       |
//! | `domain/snapshot.rs`                  | `crate::domain::download_snapshot`                               |
//! | `domain/downloaded_model.rs`          | `crate::domain::downloaded_model`                                |
//! | `domain/downloaded_model_repository.rs` | `crate::domain::repositories::downloaded_model_repository`     |
//! | `events/model_download_events.rs`     | `crate::domain::events::model_download_events`                   |
//! | `events/infra_events.rs`              | `crate::infrastructure::events::download_events`                 |
//! | `download_repository.rs`              | `crate::infrastructure::persistence::download_repository`        |
//! | `downloaded_model_repository.rs`      | `crate::infrastructure::persistence::repositories::downloaded_model_repository` |
//! | `saga.rs`                             | `crate::infrastructure::sagas::download_saga`                    |
//! | `engine.rs`                           | `crate::infrastructure::services::download_engine`               |
//! | `manager.rs`                          | `crate::infrastructure::services::download_manager`              |
//! | `commands.rs`                         | `crate::interfaces::commands::downloads`                         |
//! | `plugin.rs`                           | `crate::plugins::download_plugin`                                |
//!
//! No dedicated use cases under `application/use_cases/download/` —
//! download is consumed *through* the LLM and model-management
//! features, which orchestrate the download pipeline for model files.
//! Those use cases (e.g., `DownloadModelUseCase`, `TrackDownloadUseCase`)
//! will graduate into those features' slices.
