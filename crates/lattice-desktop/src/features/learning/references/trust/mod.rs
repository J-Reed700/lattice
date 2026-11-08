//! Source provenance and trust in Learning Studio: the store over the trust
//! tables, the capture hook source-saving paths call in their own transaction,
//! override changes, the outcomes and calibrated bars verdicts make, the rows
//! a pack carries, and the commands' reads. The rules are lattice-core's
//! `domain::provenance`; storage is its `SourceTrustStore` port.
//! Design: docs/design/2026-10-18-source-provenance-and-trust.md §16-§18.

pub mod calibration;
pub mod calibration_view;
pub mod capture;
pub(in crate::features::learning) mod dto_map;
pub mod eval_export;
pub mod judgements;
pub mod outcomes;
pub mod override_changes;
pub mod overrides;
pub mod pack;
pub mod repository;
mod rows;
pub mod source_view;

pub use repository::SqliteSourceTrustStore;

#[cfg(test)]
mod calibration_tests;
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
