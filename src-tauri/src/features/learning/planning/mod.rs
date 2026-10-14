//! Planning capabilities owned by Learning Studio.

pub mod curriculum;
pub mod curriculum_repository;
pub mod diagnostic_generation;
pub(crate) mod outline_linking;
pub(crate) mod outline_citations;
pub mod outline_draft;
pub(crate) mod outline_draft_repository;
pub(crate) mod outline_evidence;
pub mod outline_evidence_view;
pub mod outline_progress;
pub(crate) mod outline_references;
#[cfg(test)]
mod outline_reference_tests;
#[cfg(test)]
mod outline_review_scope_tests;
pub(crate) mod outline_repair;
#[cfg(test)]
mod outline_repair_tests;
pub(crate) mod outline_review;
pub(crate) mod outline_review_scope;
#[cfg(test)]
mod outline_review_tests;
pub mod plan_dto;
