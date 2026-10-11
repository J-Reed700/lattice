//! # QA feature
//!
//! Question-answering helpers the conversation chat builds on: the model
//! health checks and chat-starters command the chat surface uses, and the
//! source DTOs answers cite.
//!
//! ## Public surface
//!
//! - `crate::features::qa::dto` — QA DTOs
//! - `crate::features::qa::commands` — Tauri command handlers
//! - `crate::features::qa::plugin::init()` — Tauri plugin
//!
//! Domain types (HyDEInterpretation, QueryType, ChatResponse, ...) live in
//! `crate::domain::qa` because the application layer depends on them.

pub mod commands;
pub mod dto;
pub mod plugin;
