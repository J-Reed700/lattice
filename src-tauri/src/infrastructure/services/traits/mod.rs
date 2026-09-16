//! Service trait definitions for dependency injection
//!
//! This module defines trait interfaces for cross-cutting / infrastructure
//! service types in the system. Feature-specific traits live with their
//! feature (e.g. `crate::features::tags::TagServiceTrait`).

mod article_extractor;
mod context;
mod model;

pub use article_extractor::*;
pub use context::*;
pub use model::*;
