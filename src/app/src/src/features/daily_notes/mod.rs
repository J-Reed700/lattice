//! # Daily notes feature
//!
//! Daily journal workspace management — creating per-day entries in
//! a designated workspace folder.
//!
//! ## File layout
//!
//! | File          | Canonical module path                                    |
//! |---------------|----------------------------------------------------------|
//! | `commands.rs` | `crate::interfaces::commands::daily_notes_workspace`     |
//! | `plugin.rs`   | `crate::plugins::daily_notes_plugin`                     |
//!
//! Thin feature — all logic lives in the command file. No DTO,
//! use case, port, or repository.
