//! # HuggingFace feature
//!
//! HuggingFace integration: token storage (via OS keyring) and a Tauri
//! plugin exposing commands to set / get / check the HF auth token.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::huggingface::commands` — HF token operations
//! - `crate::features::huggingface::plugin::init()` — Tauri plugin
//!
//! The HuggingFace *model catalog* adapter is part of the
//! model_management feature, not this one (see
//! `crate::features::model_management::huggingface_adapter`).

pub mod commands;
pub mod plugin;
