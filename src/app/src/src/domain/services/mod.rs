//! # Domain Services
//!
//! Pure business logic services with zero external dependencies.
//!
//! Domain services encapsulate business logic that doesn't naturally fit
//! within a single entity or value object. They operate only on domain
//! primitives and have no infrastructure dependencies.

pub mod chunking_service;
// Vertical-slice migration (search): ranking service lives in features/search/ranking_service.rs.
#[path = "../../features/search/ranking_service.rs"]
pub mod search_ranking_service;

// Re-export public types
pub use chunking_service::ChunkingService;
pub use search_ranking_service::SearchRankingService;
