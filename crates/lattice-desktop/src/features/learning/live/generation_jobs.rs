//! Stands in for `lattice_learning::lessons::generation_jobs`, which declared these evals.

use lattice_core::shared::error::{AppError, Result};
use lattice_learning::curriculum_repository::LearningCurriculumRepository;
use lattice_learning::dto::{LearningPreparation, LearningProgramStatus};
use lattice_learning::lessons::generation_jobs::*;
use lattice_learning::repository::LearningRepository;
use lattice_learning::service;
use sqlx::SqlitePool;
use std::sync::Arc;

#[path = "../lessons/live_lesson_tests.rs"]
mod live_lesson_tests;
