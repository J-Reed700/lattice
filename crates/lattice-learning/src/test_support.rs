//! The database and program every test starts from, shared with lattice-desktop's live evals.

#![allow(clippy::indexing_slicing)]

use crate::dto::*;
use lattice_core::shared::error::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

#[cfg(test)]
pub mod query_counts;

pub async fn pool() -> Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        // A long model call must not retire the only in-memory database connection.
        .idle_timeout(None)
        .max_lifetime(None)
        .connect(":memory:")
        .await
        .map_err(|e| lattice_core::shared::error::AppError::Database(e.to_string()))?;
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .map_err(|e| lattice_core::shared::error::AppError::Database(e.to_string()))?;
    Ok(pool)
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn fixture() -> LearningProgramDto {
    let mut modules = Vec::new();
    for mi in 0..2 {
        let mid = id();
        let mut lessons = Vec::new();
        for li in 0..2 {
            lessons.push(LearningLessonDto {
                id: id(),
                title: format!("Lesson {mi}-{li}"),
                objective: "Explain the central idea".into(),
                estimated_minutes: 20,
                preparation: LearningPreparation::Outline,
                blocks: vec![],
                completed: false,
            });
        }
        modules.push(LearningModuleDto {
            prerequisite_module_ids: vec![],
            project: None,
            id: mid,
            title: format!("Module {mi}"),
            summary: "A subject-neutral module".into(),
            outcomes: vec!["Describe a concept".into()],
            lessons,
        });
    }
    let current = modules[0].lessons[0].id.clone();
    LearningProgramDto {
        outline_review: None,
        summary: LearningProgramSummaryDto {
            id: id(),
            title: "A generic learning program".into(),
            goal: "Learn a subject".into(),
            status: LearningProgramStatus::Draft,
            revision: 0,
            module_count: 2,
            lesson_count: 4,
            completed_lessons: 0,
            current_lesson_id: Some(current),
            created_at: chrono::Utc::now().timestamp_millis(),
        },
        prior_knowledge: "Some familiarity".into(),
        minutes_per_session: 30,
        model_name: "fixture-model".into(),
        modules,
        sources: vec![LearningSourceDto {
            id: id(),
            title: "A selected source".into(),
            url: None,
            excerpt: "A bounded passage used by this generic course.".into(),
            acquired_at: 1,
        }],
    }
}
