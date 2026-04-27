//! # Application Mappers
//!
//! Mappers for converting between domain models and DTOs.
//!
//! ## Purpose
//!
//! Mappers provide clean conversion between:
//! - Domain models (rich business logic, invariants, behavior)
//! - DTOs (flat data structures for serialization)
//!
//! This separation ensures:
//! - **Decoupling**: API changes don't affect domain models
//! - **Flexibility**: Can evolve DTOs for API versioning
//! - **Testing**: Easy to test conversions in isolation
//!
//! ## Organization
//!
//! - `conversation_mapper` (re-export) — used by chat-side conversation creation

// Re-export mappers
pub use crate::features::conversation::mapper::ConversationDtoMapper;
