//! # Health feature
//!
//! System health check + version info + database initialization.
//! Self-contained vertical slice — exposes its modules directly.
//!
//! ## Public surface
//!
//! - `crate::features::health::dto` — DTOs (HealthCheckResponseDto, SystemStatsDto)
//! - `crate::features::health::use_cases::HealthCheckUseCase`
//! - `crate::features::health::commands` — Tauri command handlers
//! - `crate::features::health::plugin` — Tauri plugin (`init()`)
//!
//! `backend_logs` (private) reads the newest lines of the backend's file log
//! for the `get_backend_logs` command.
//!
//! No ports, repositories, or traits — health is a thin diagnostics
//! feature.

mod backend_logs;
pub mod commands;
pub mod di;
pub mod dto;
pub mod plugin;
pub mod use_cases;
