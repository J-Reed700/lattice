//! # Metrics feature
//!
//! Application metrics snapshot command. Reads the shared `Metrics` service
//! (in `infrastructure/observability/metrics`) through `MetricsPort` and
//! exposes a snapshot to the frontend.
//!
//! ## File layout
//!
//! All files are loaded via `#[path]` redirects from their legacy module
//! locations so existing imports keep working (Strangler Fig):
//!
//! | File           | Canonical module path                                            |
//! |----------------|------------------------------------------------------------------|
//! | `dto.rs`       | `crate::application::dtos::metric_dto`                           |
//! | `use_cases/`   | `crate::application::use_cases::metrics`                         |
//! | `adapter.rs`   | `crate::infrastructure::observability::metrics_adapter`          |
//! | `commands.rs`  | `crate::interfaces::commands::metrics_commands`                  |
//!
//! The `MetricsPort` trait stays in `application/ports/` and the core
//! `Metrics` service (+ `MetricsSnapshot`) stays in
//! `infrastructure/observability/metrics` — both are shared infrastructure
//! used across many features.
//!
//! Metrics has no Tauri plugin of its own today; commands are registered
//! through the shared plugin infrastructure.
