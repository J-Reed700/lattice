//! Recall capabilities owned by Learning Studio: program recall cards, study
//! decks, and the FSRS scheduler both share.

pub(crate) mod recall_content;
pub mod recall_repository;
#[cfg(test)]
pub(crate) mod recall_tests;
pub(crate) mod schedule;
pub mod study_dto;
mod study_generation;
pub(in crate::features::learning) use study_generation::deck_output_tokens;
pub mod study_repository;
#[cfg(test)]
mod study_repository_tests;
pub(crate) mod study_service;
#[cfg(test)]
mod study_tests;
