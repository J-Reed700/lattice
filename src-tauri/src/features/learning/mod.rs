//! Durable, subject-neutral Learning Studio program workflow.
pub mod assessment_engine;
pub mod assessment_generation;
pub mod assessment_repository;
pub mod canvas_repository;
pub mod curriculum;
pub mod curriculum_repository;
pub mod dto;
pub mod embedded_runtime;
pub mod generation;
pub mod lab_runtime;
pub mod pack;
pub mod pack_repository;
pub mod plan_dto;
pub mod plugin;
pub mod portability_dto;
pub mod practical_dto;
pub mod practical_generation;
pub mod practical_repository;
pub mod practice_generation;
pub mod practice_repository;
pub mod python_runtime;
pub mod recall_repository;
pub mod repository;
pub mod runtime_catalog;
pub mod service;
pub mod source_library;
pub mod source_selector;
pub mod sources;

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
