//! Lesson claim review in the learning feature: the ledger's SQLite store,
//! the user's actions on claims, the checker-accuracy examples, and the DTOs
//! the review panel and the claim commands exchange. The ledger's types and
//! rules are in `lattice_core::domain::learning::claim_review`; its storage
//! port is `lattice_core::application::ports::claim_review`.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §7.

pub mod attach;
pub mod checker_examples;
pub mod dto;
#[cfg(test)]
mod dto_tests;
#[cfg(test)]
mod migration_tests;
pub mod repository;
pub mod service;
mod view;
