//! Learner-safe verification report. Assessment claims, keys and explanations
//! stay on the backend; only teaching claims and original source passages leave.
use lattice_core::domain::learning::claim_review::UnverifiedReason;
use lattice_core::shared::error::Result;
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
    /// The level of this passage's source for the claim's subject, when the
    /// passage supports the claim and its support was weighed.
    pub trust: Option<crate::features::learning::dto::LearningSourceTrustDto>,
}
/// How a published claim stands (§5.6): verified against a source, confirmed
/// by the user, or verified against a source since removed. A string union,
/// like the review panel's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LearningClaimResolutionDto {
    Verified,
    ConfirmedByYou,
    SourceRemoved,
    NotVerified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UnverifiedReasonDto {
    Unchecked,
    Unsupported,
    Contradicted,
}

impl From<UnverifiedReason> for UnverifiedReasonDto {
    fn from(reason: UnverifiedReason) -> Self {
        match reason {
            UnverifiedReason::Unchecked => Self::Unchecked,
            UnverifiedReason::Unsupported => Self::Unsupported,
            UnverifiedReason::Contradicted => Self::Contradicted,
        }
    }
}

/// The class a published statement was checked as (claim classes design
/// §3.1): what the learner is told of the check it met.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LearningStatementClassDto {
    /// Checked against the source passage its sentence cites.
    Sourced,
    /// Checked for consistency with the passages its section cites.
    Reasoning,
    /// Standard knowledge, put to the model alone.
    Common,
}

/// What the part-wise check found of one part of a statement (§3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LearningClaimPartKindDto {
    /// A passage states it.
    Stated,
    /// No passage states it, and it is standard knowledge of the field.
    Common,
    /// No passage gives a specific it states.
    SpecificMissing,
    /// A passage says otherwise.
    Contradicted,
}

/// One part of a statement as the part-wise check read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningClaimPartDto {
    pub part: String,
    pub kind: LearningClaimPartKindDto,
    /// The saved source version that states the part; none for another.
    pub source_version_id: Option<String>,
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
    /// How the claim stands in the published lesson (§5.6).
    pub resolution: LearningClaimResolutionDto,
    /// The user's note on a claim confirmed by them.
    pub note: Option<String>,
    /// Whether the claim's support met the bar for its kind; `None` when it
    /// was not weighed.
    pub sufficiency: Option<crate::features::learning::dto::LearningSufficiencyDto>,
    /// The class the statement was checked as; none for a lesson prepared
    /// before statements were sorted.
    #[serde(default)]
    pub class: Option<LearningStatementClassDto>,
    /// How the provenance gate labelled its support, by the label's name
    /// (`attributed`, `practice`, `secondary`...); none when it takes none.
    #[serde(default)]
    pub label: Option<String>,
    /// The parts a Sourced statement's cited passage was read as, which say
    /// what a passage states and what is common knowledge beside it.
    #[serde(default)]
    pub parts: Vec<LearningClaimPartDto>,
    #[serde(default)]
    pub unverified_reason: Option<UnverifiedReasonDto>,
    /// The claim behind this statement, for acting on it; none in a report
    /// from before findings named theirs.
    #[serde(default)]
    pub claim_id: Option<String>,
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
    /// Of `claim_count`, the claims the user confirmed without a supporting
    /// passage; shown apart from the machine-verified ones (§5.6).
    pub confirmed_count: usize,
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
        .map(|report| report.learner_view(&snapshot.lesson, &snapshot.versions, &snapshot.claims))
        .transpose()
}
