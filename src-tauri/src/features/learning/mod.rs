//! Durable, subject-neutral Learning Studio program workflow.

pub mod assessment;
pub mod canvas;
pub mod dto;
pub mod lessons;
pub(crate) mod operations;
pub(crate) mod persistence;
pub mod planning;
pub mod plugin;
pub mod portability;
pub mod practice;
pub mod recall;
pub mod references;
pub mod repository;
pub mod runtime;
pub mod service;

// Public feature facade; implementations remain grouped by capability.
pub use assessment::assessment_engine;
pub use assessment::assessment_generation;
pub use assessment::assessment_repository;
pub use canvas::canvas_repository;
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
pub use practice::practical_dto;
pub use practice::practical_generation;
pub use practice::practical_repository;
pub use practice::practical_runs;
pub use practice::practice_generation;
pub use practice::practice_repository;
pub use recall::recall_repository;
pub use references::source_library;
pub use references::source_selector;
pub use references::sources;
pub use runtime::embedded_runtime;
pub use runtime::lab_runtime;
pub use runtime::python_runtime;
pub use runtime::runtime_catalog;

use lessons::answer_review;
pub(crate) use lessons::content_verification;
use lessons::generation_jobs;
use lessons::lesson_drafts;
use lessons::lesson_progress;
use lessons::review_evidence;
use lessons::teaching_review;
use planning::outline_citations;
use planning::outline_draft_repository;
use planning::outline_evidence;
use planning::outline_repair;
use planning::outline_research;
use planning::outline_review_scope;
use practice::practical_workspace;
use references::source_identity;
use references::source_workflows;

#[cfg(test)]
mod backend_boundary_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod consistency_tests;
