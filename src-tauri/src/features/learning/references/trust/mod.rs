//! Source provenance and trust in Learning Studio: the store over the trust
//! tables, the capture hook every source-saving path calls inside its own
//! transaction, the override changes that tell the ledger which sources to
//! re-decide, and the rows a course pack carries.
//!
//! The commands' reads and changes (P6) sit beside them: the trust view of
//! a course's sources and of a claim's supports, override changes with what
//! they reopen, the calibration Settings shows, the two judgements the user
//! can ask for again, and the eval export.
//!
//! The rules (origin, signals, levels, relations) are lattice-core's
//! `domain::provenance`; storage is its `application::ports::SourceTrustStore`
//! port, implemented here over the pool as the other learning repositories
//! are. Nothing here imports another feature.
//! Design: docs/design/2026-10-18-source-provenance-and-trust.md §16-§18.

pub mod calibration_view;
pub mod capture;
mod dto_map;
pub mod eval_export;
pub mod judgements;
pub mod override_changes;
pub mod overrides;
pub mod pack;
pub mod repository;
mod rows;
pub mod source_view;
mod waiting_claims;

pub use repository::SqliteSourceTrustStore;

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
mod dto_map_tests;
#[cfg(test)]
mod eval_export_tests;
#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod overrides_tests;
#[cfg(test)]
mod repository_tests;
#[cfg(test)]
mod source_view_tests;
#[cfg(test)]
mod test_support;
