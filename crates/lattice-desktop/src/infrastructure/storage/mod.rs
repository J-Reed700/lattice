//! Storage infrastructure implementations.
//!
//! This module provides storage adapters for the application layer.

pub mod content_addressed_storage;
pub mod file_intents;

pub use content_addressed_storage::ContentAddressedStorage;
pub use file_intents::{FileIntents, FileStore, IntentKind};
