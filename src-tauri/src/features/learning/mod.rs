//! Durable, subject-neutral Learning Studio program workflow.
pub mod assessment_engine;
pub mod assessment_generation;
pub mod assessment_repository;
pub mod canvas_repository;
pub(crate) mod content_verification;
pub mod curriculum;
pub mod curriculum_repository;
pub mod diagnostic_generation;
pub mod dto;
pub mod embedded_runtime;
pub mod generation;
mod generation_jobs;
pub mod lab_runtime;
pub mod lesson_evidence;
pub mod outline_progress;
pub mod pack;
pub mod pack_repository;
pub mod plan_dto;
pub mod plugin;
pub mod portability_dto;
pub mod practical_dto;
pub mod practical_generation;
pub mod practical_repository;
pub mod practical_runs;
mod practical_workspace;
pub mod practice_generation;
pub mod practice_repository;
pub mod python_runtime;
pub mod recall_repository;
pub mod reference_collection;
pub mod repository;
pub mod runtime_catalog;
pub mod service;
pub mod source_library;
pub mod source_selector;
pub mod sources;
pub mod teaching;

#[cfg(test)]
mod assessment_tests;
#[cfg(test)]
mod backend_boundary_tests;
#[cfg(test)]
mod canvas_tests;
#[cfg(test)]
mod pack_tests;
#[cfg(test)]
mod practical_tests;
#[cfg(test)]
mod practice_tests;
#[cfg(test)]
mod recall_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod course_generation_tests;
