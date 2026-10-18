//! Source provenance and trust in Learning Studio: the store over the trust
//! tables, the capture hook every source-saving path calls inside its own
//! transaction, the override changes that tell the ledger which sources to
//! re-decide, and the rows a course pack carries.
//!
//! The rules (origin, signals, levels, relations) are lattice-core's
//! `domain::provenance`; storage is its `application::ports::SourceTrustStore`
//! port, implemented here over the pool as the other learning repositories
//! are. Nothing here imports another feature.
//! Design: docs/design/2026-10-18-source-provenance-and-trust.md §16-§18.

pub mod capture;
pub mod overrides;
pub mod pack;
pub mod repository;
mod rows;

pub use repository::SqliteSourceTrustStore;

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod overrides_tests;
#[cfg(test)]
mod repository_tests;
#[cfg(test)]
mod test_support;
