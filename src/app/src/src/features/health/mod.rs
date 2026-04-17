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
//! No ports, repositories, or traits — health is a thin diagnostics
//! feature.

pub mod commands;
pub mod dto;
pub mod plugin;
pub mod use_cases;
