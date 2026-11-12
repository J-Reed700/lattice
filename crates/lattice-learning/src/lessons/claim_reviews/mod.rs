//! Lesson claim review in the learning feature: the ledger's SQLite store,
//! the user's actions on claims, the checker-accuracy examples, the teaching
//! findings the repairs did not settle, and the DTOs the review panel and the
//! claim commands exchange. The ledger's types and rules are in
//! `lattice_core::domain::learning::claim_review`; its storage port is
//! `lattice_core::application::ports::claim_review`.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §7.

/// `n` as a count a review DTO carries, saturating at `u32::MAX`.
pub(crate) fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

mod actions;
pub mod attach;
pub mod checker_examples;
#[cfg(test)]
mod code_example_migration_tests;
pub mod dto;
#[cfg(test)]
mod dto_tests;
#[cfg(test)]
mod migration_tests;
mod published_edit;
mod published_remove;
#[cfg(test)]
mod published_remove_tests;
mod published_restore;
mod published_rewrite;
#[cfg(test)]
mod published_rewrite_tests;
mod put_back;
#[cfg(test)]
mod put_back_tests;
pub mod recheck;
pub mod repository;
mod restore;
pub mod service;
pub(crate) mod source_deleted;
mod teaching_claims;
pub mod teaching_findings;
mod teaching_summary;
pub mod trust_changed;
pub mod trust_overrides;
mod view;
