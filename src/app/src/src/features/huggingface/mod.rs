//! # HuggingFace feature
//!
//! HuggingFace integration: token storage (via OS keyring) and a Tauri
//! plugin exposing commands to set / get / check the HF auth token.
//!
//! ## File layout
//!
//! | File           | Canonical module path                                |
//! |----------------|------------------------------------------------------|
//! | `commands.rs`  | `crate::interfaces::commands::hf_settings`           |
//! | `plugin.rs`    | `crate::plugins::huggingface`                        |
//!
//! The HuggingFace *model catalog* adapter
//! (`infrastructure/huggingface_adapter.rs`, implements `ModelCatalogPort`)
//! is NOT part of this feature — it's model-management infrastructure
//! and stays with `infrastructure/`. It will graduate alongside the
//! model-management feature migration.
