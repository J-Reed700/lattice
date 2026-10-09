//! Mock implementations for testing
//!
//! Mocks for cross-cutting / infrastructure service traits. Feature-specific
//! mocks live with their feature (e.g. `crate::features::tags::mocks::MockTagService`).

mod mock_model;
mod mock_search_enrichment;

#[allow(unused_imports)]
pub use mock_model::*;
pub use mock_search_enrichment::MockSearchEnrichmentService;
