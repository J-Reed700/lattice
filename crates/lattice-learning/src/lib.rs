//! Durable, subject-neutral Learning Studio program workflow.

#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]
#![cfg_attr(test, allow(clippy::unwrap_in_result))]
#![cfg_attr(test, allow(clippy::indexing_slicing))]
#![cfg_attr(test, allow(clippy::panic))]
#![deny(unsafe_code)]
pub mod activity;
pub mod assessment;
pub mod canvas;
pub mod drills;
pub mod dto;
pub mod lessons;
mod model_call;
pub mod model_requirements;
pub(crate) mod operations;
pub mod patient_model;
pub mod persistence;
pub mod planning;
pub mod portability;
pub mod practice;
pub mod recall;
pub mod references;
pub mod repository;
pub mod runtime;
pub mod service;
pub mod tutor;

// Public feature facade; implementations remain grouped by capability.
pub use assessment::assessment_engine;
pub use assessment::assessment_generation;
pub use assessment::assessment_repository;
pub(crate) use assessment::bank_writing;
pub use assessment::grade_rationale;
pub(crate) use assessment::key_review;
pub(crate) use assessment::question_review;
pub use assessment::quiz_bank_jobs;
pub use assessment::quiz_blueprints;
pub use canvas::canvas_repository;
pub use lessons::course_plan;
pub use lessons::course_setup;
pub use lessons::generation;
pub use lessons::lesson_evidence;
pub use lessons::reference_collection;
pub use lessons::teaching;
pub use planning::curriculum;
pub use planning::curriculum_repository;
pub use planning::diagnostic_generation;
pub use planning::outline_draft;
pub use planning::outline_evidence_view;
pub use planning::outline_progress;
pub use planning::plan_dto;
pub use portability::pack;
pub use portability::pack_repository;
pub use portability::portability_dto;
pub use practice::coding;
pub use practice::interview_research;
pub use practice::practical_dto;
pub use practice::practical_generation;
pub use practice::practical_repository;
pub use practice::practical_runs;
pub use practice::practice_fit;
pub use practice::practice_generation;
pub use practice::practice_repository;
pub use practice::simulation_policy;
pub use recall::recall_repository;
pub use references::source_library;
pub use references::source_selector;
pub use references::sources;
pub use runtime::embedded_runtime;
pub use runtime::lab_runner;
pub use runtime::lab_runtime;
pub use runtime::python_runtime;
pub use runtime::runtime_catalog;

pub use lessons::content_verification;
pub use lessons::generation_jobs;
pub use lessons::lesson_drafts;
use lessons::lesson_progress;
pub use lessons::review_evidence;
pub use lessons::review_findings;
pub use lessons::teaching_review;
use planning::outline_citations;
use planning::outline_draft_repository;
pub use planning::outline_evidence;
pub use planning::outline_linking;
use planning::outline_references;
use planning::outline_repair;
use planning::outline_review;
use planning::outline_review_scope;
pub use practice::practical_workspace;
pub use references::source_identity;
pub use references::source_workflows;

#[cfg(test)]
mod backend_boundary_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod clean_slate_tests;
#[cfg(test)]
mod consistency_tests;
#[cfg(test)]
mod content_caps_tests;
#[cfg(test)]
mod found_by_search_tests;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
