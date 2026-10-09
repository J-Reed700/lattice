//! Learner-safe verification report. Assessment claims, keys and explanations
//! stay on the backend; only teaching claims and original source passages leave.
use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

mod repository;
pub(in crate::features::learning) use repository::SavedEvidenceVersion;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningEvidencePassageDto {
    pub source_version_id: String,
    pub title: String,
    pub url: Option<String>,
    pub text: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub retrieval_kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningClaimEvidenceDto {
    pub section_index: usize,
    /// Exact lesson text selected by the claim inventory. This anchors the
    /// evidence to the generated section without asking the UI to guess.
    pub content_quote: String,
    pub claim: String,
    pub verdict: String,
    pub reason: String,
    pub supporting_quote: Option<String>,
    pub passages: Vec<LearningEvidencePassageDto>,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningLessonEvidenceDto {
    pub policy: String,
    pub checked_at: i64,
    pub checker_model: String,
    pub retrieval_mode: String,
    pub embedding_model: Option<String>,
    pub content_sha256: String,
    pub claim_count: usize,
    pub executed_examples: usize,
    pub unexecuted_languages: Vec<String>,
    pub sources_current: bool,
    pub teaching_claims: Vec<LearningClaimEvidenceDto>,
}

/// Read a single database snapshot, then project the typed private report.
pub async fn get(
    pool: &SqlitePool,
    program_id: &str,
    lesson_id: &str,
) -> Result<Option<LearningLessonEvidenceDto>> {
    let snapshot = repository::read(pool, program_id, lesson_id).await?;
    snapshot
        .report
        .as_ref()
        .map(|report| report.learner_view(&snapshot.lesson, &snapshot.versions))
        .transpose()
}
