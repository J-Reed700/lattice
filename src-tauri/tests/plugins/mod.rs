//! Plugin smoke tests
//!
//! Smoke tests for plugin DTOs and basic functionality.
//! These tests verify that DTO structures can be created and serialized
//! without requiring full integration testing with tauri::State.

pub mod backup;
pub mod cache;
pub mod conversation;
pub mod extraction;
pub mod favorites;
pub mod health;
pub mod huggingface;
pub mod tags;
pub mod updates;
