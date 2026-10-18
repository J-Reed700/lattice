//! # QA feature
//!
//! Question-answering helpers the conversation chat builds on: the model
//! health checks and chat-starters command the chat surface uses. The source
//! DTOs answers cite live in `lattice_core::application::contracts::sources`.
//!
//! ## Public surface
//!
//! - `crate::features::qa::dto` — the health check's response DTO
//! - `crate::features::qa::commands` — Tauri command handlers
//! - `crate::features::qa::plugin::init()` — Tauri plugin
//!
//! Domain types (HyDEInterpretation, QueryType, ChatResponse, ...) live in
//! `lattice_core::domain::qa` because the application layer depends on them.

pub mod commands;
pub mod dto;
pub mod plugin;
