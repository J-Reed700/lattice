//! # Vault portability
//!
//! Mirrors notes to plain markdown files on disk so users can manage them
//! with their own tools (Obsidian, ripgrep, git, iCloud, etc.). The vault
//! folder is the user-facing source of truth; SQLite remains the
//! query/index layer.
//!
//! This module provides a thin write-through layer that runs after a
//! successful SQL note write. Failures are logged but non-fatal — the
//! database commit always wins. A user with the feature disabled (the
//! default) sees zero overhead because [`writeback::sync_note_to_vault`]
//! short-circuits on the settings flag.
//!
//! Reverse direction (filesystem watcher → SQL re-import) lives in a
//! follow-up task; v1 ships one-way export only.

pub mod writeback;
