//! What one lesson's audit sends (§14.4): its draft's digest, its units in
//! the report's unit order, and each claim's passages as its card shows them
//! (`ClaimReviewRepository::shown_passages`), with each source's title.

use std::collections::HashMap;

use serde_json::{json, Value};

use lattice_core::application::ports::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{ClaimId, LessonReviewState};
use lattice_core::shared::error::{AppError, Result};

use super::repository;
use super::{AuditPassage, AuditUnit};
use crate::content_verification::unit_texts;
use crate::curriculum_repository::LearningCurriculumRepository;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::persistence::db;
use crate::repository::LearningRepository;
use crate::source_library::workspace_in;
use lattice_core::shared::persistence::Database;

pub struct AuditMaterial {
    pub draft_sha256: String,
    pub units: Vec<AuditUnit>,
    pub passages: HashMap<ClaimId, Vec<AuditPassage>>,
}

impl AuditMaterial {
    /// `candidate`, the draft a preparation is about to publish, whose
    /// digest is `draft_sha256`.
    pub(crate) async fn of_draft(
        database: &Database,
        program_id: &str,
        lesson_id: &str,
        candidate: &Value,
        draft_sha256: String,
    ) -> Result<Self> {
        Ok(Self {
            draft_sha256,
            units: units(candidate),
            passages: passages(database, program_id, lesson_id).await?,
        })
    }

    /// The lesson as its review stands: the draft that waits for review, or
    /// the published lesson once it is ready ("Audit this lesson now").
    pub async fn of_lesson(database: &Database, program_id: &str, lesson_id: &str) -> Result<Self> {
        let review = ClaimReviewRepository::new(database.clone())
            .load(lesson_id)
            .await?
            .review
            .filter(|review| review.program_id == program_id)
            .ok_or_else(|| AppError::NotFound("This lesson has no statement review yet.".into()))?;
        let waiting = if review.state == LessonReviewState::Ready {
            None
        } else {
            LearningCurriculumRepository::new(database.clone())
                .lesson_draft(&review.job_id, lesson_id)
                .await?
        };
        let candidate = match waiting {
            Some(draft) => serde_json::from_str(&draft.candidate)
                .map_err(|error| AppError::Serialization(error.to_string()))?,
            None => published(database, program_id, lesson_id).await?,
        };
        Ok(Self {
            draft_sha256: review.draft_sha256,
            units: units(&candidate),
            passages: passages(database, program_id, lesson_id).await?,
        })
    }
}

fn units(candidate: &Value) -> Vec<AuditUnit> {
    unit_texts(candidate)
        .into_iter()
        .enumerate()
        .map(|(index, (title, text))| AuditUnit { index, title, text })
        .collect()
}

/// The published lesson in the draft's shape, its reviewed questions and
/// their explanations included, so its units index as the report's do.
async fn published(database: &Database, program_id: &str, lesson_id: &str) -> Result<Value> {
    let program = LearningRepository::new(database.clone())
        .get(program_id)
        .await?;
    let lesson = program
        .modules
        .iter()
        .flat_map(|module| &module.lessons)
        .find(|lesson| lesson.id == lesson_id)
        .ok_or_else(|| AppError::NotFound("This lesson is not in the course.".into()))?;
    // The lesson's own reviewed questions live in its quiz bank.
    let questions = repository::lesson_questions(database, program_id, lesson_id).await?;
    Ok(json!({ "blocks": lesson.blocks, "questions": questions }))
}

/// Each claim's passages as its card shows them, with each source
/// version's title (deleted sources included: a card still names them).
async fn passages(
    database: &Database,
    program_id: &str,
    lesson_id: &str,
) -> Result<HashMap<ClaimId, Vec<AuditPassage>>> {
    let claims = ClaimReviewRepository::new(database.clone());
    let shown = claims.shown_passages(lesson_id).await?;
    let mut read = database.begin_read().await.map_err(db)?;
    let workspace = workspace_in(&mut read, program_id, true).await?;
    read.commit().await.map_err(db)?;
    let titles: HashMap<String, String> = workspace
        .sources
        .into_iter()
        .flat_map(|source| source.versions)
        .map(|version| (version.id, version.title))
        .collect();
    Ok(shown
        .into_iter()
        .map(|(claim_id, passages)| {
            let passages = passages
                .into_iter()
                .map(|passage| AuditPassage {
                    title: titles
                        .get(&passage.source_version_id)
                        .cloned()
                        .unwrap_or_default(),
                    source_version_id: passage.source_version_id,
                    start_byte: passage.start_byte,
                    end_byte: passage.end_byte,
                    text: passage.text,
                })
                .collect();
            (claim_id, passages)
        })
        .collect())
}

#[cfg(test)]
#[path = "material_tests.rs"]
mod tests;
