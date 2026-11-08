//! Lesson claim review in the learning feature: the ledger's SQLite store,
//! the user's actions on claims, the checker-accuracy examples, the teaching
//! findings the repairs did not settle, and the DTOs the review panel and the
//! claim commands exchange. The ledger's types and rules are in
//! `lattice_core::domain::learning::claim_review`; its storage port is
//! `lattice_core::application::ports::claim_review`.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §7.

mod actions;
pub mod attach;
pub mod checker_examples;
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
mod put_back;
#[cfg(test)]
mod put_back_tests;
pub mod recheck;
pub mod repository;
mod restore;
pub mod service;
pub mod teaching_findings;
pub mod trust_changed;
pub mod trust_overrides;
mod view;
