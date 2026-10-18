//! Storage infrastructure implementations.
//!
//! This module provides storage adapters for the application layer.

pub mod content_addressed_storage;

pub use content_addressed_storage::ContentAddressedStorage;
