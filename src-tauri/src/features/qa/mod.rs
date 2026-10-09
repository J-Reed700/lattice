//! # QA feature
//!
//! Question-answering helpers the conversation chat builds on: HyDE query
//! interpretation, corpus-derived chat starters, the model health checks the
//! chat surface uses, and the source DTOs answers cite.
//!
//! ## Public surface
//!
//! - `crate::features::qa::dto` — QA DTOs
//! - `crate::features::qa::commands` — Tauri command handlers
//! - `crate::features::qa::plugin::init()` — Tauri plugin
//! - `crate::features::qa::engine` — token counting
//! - `crate::features::qa::hyde` — HyDE retrieval
//!
//! Domain types (HyDEInterpretation, QueryType, ChatResponse, ...) live in
//! `crate::domain::qa` because the application layer depends on them.

pub mod commands;
pub mod dto;
pub mod plugin;
pub mod starters;
pub mod starters_dto;

pub mod engine;
pub mod hyde;
