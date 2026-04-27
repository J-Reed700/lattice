//! # Metrics feature
//!
//! Application metrics snapshot command. Reads the shared `Metrics`
//! service (in `infrastructure/observability/metrics`) through
//! `MetricsPort` and exposes a snapshot to the frontend.
//!
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::metrics::dto` — `MetricsSnapshotDto`
//! - `crate::features::metrics::use_cases::GetMetricsUseCase`
//! - `crate::features::metrics::adapter::MetricsAdapter` — wraps the
//!   shared `Metrics` service to satisfy `MetricsPort`
//! - `crate::features::metrics::commands` — Tauri command handlers
//!
//! `MetricsPort` stays in `application/ports/`. The core `Metrics`
//! service (+ `MetricsSnapshot`) stays in
//! `infrastructure/observability/metrics` — both are shared
//! observability infrastructure.
//!
//! Metrics has no Tauri plugin of its own; commands are registered
//! through the shared plugin infrastructure.

pub mod adapter;
pub mod commands;
pub mod di;
pub mod dto;
pub mod use_cases;
