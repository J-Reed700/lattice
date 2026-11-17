//! A claim as the review screen shows it (UI design §5.1): the statement in
//! its sentence and paragraph, the support the checker found (or the passage
//! that contradicts it), and what research tried.

use serde::{Deserialize, Serialize};

use super::{SourceKindDto, SourceRefDto};
use crate::dto::LearningTrustLevel;
use crate::lesson_evidence::LearningEvidencePassageDto;

/// The claim as the review shows it: plain text, in its sentence and
/// paragraph.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaimPresentationDto {
    /// The checked statement, markdown and citation markers removed.
    pub statement: String,
    /// The sentence of the lesson field the statement was extracted from,
    /// plain.
    pub sentence: String,
    /// The paragraph around the sentence, plain, split at it.
    pub context_before: String,
    pub context_after: String,
    /// Where the claim is: "Section 2 · Defining Tool Schemas",
    /// "Question 3", "Question 3 · answer key".
    pub place: String,
}

/// The best support the checker found, or the contradicting passage.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaimSupportDto {
    pub role: ClaimSupportRoleDto,
    pub source: SourceRefDto,
    /// The registrable domain, else the host; null for a file or pasted
    /// text.
    pub site: Option<String>,
    pub kind: SourceKindDto,
    /// The source's level for this claim's subject, when the gate weighed
    /// it.
    pub standing: Option<LearningTrustLevel>,
    pub counts: ClaimSupportCountsDto,
    /// The supporting (or contradicting) sentences, as the judge cited them.
    pub quote: String,
    /// `quote` in its passage.
    pub excerpt: ClaimExcerptDto,
    pub passage: LearningEvidencePassageDto,
}

/// A quote in its passage as the review shows it (UI design §3.4): the
/// sentences that hold it, with the sentence before and after them when it
/// is prose, never code.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaimExcerptDto {
    pub before: String,
    /// The quote as the passage words it; the quote as given when the
    /// passage does not hold it.
    pub quote: String,
    pub after: String,
    /// More of the passage comes before `before`.
    pub clipped_start: bool,
    /// More of the passage comes after `after`.
    pub clipped_end: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ClaimSupportRoleDto {
    Supports,
    Contradicts,
}

/// Whether a support counted toward the claim's bar.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ClaimSupportCountsDto {
    Enough,
    /// Counted, but below the bar for this claim on its own.
    NotEnough,
    /// Excluded (distrusted, ruled out by its date), with the plain reason.
    NotCounted {
        reason: String,
    },
}

/// What research did for the claim, summed up.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResearchSummaryDto {
    /// Research rounds with at least one query.
    pub searches: u32,
    /// Distinct source versions research saved for this claim.
    pub pages_read: u32,
    /// Of those, the versions whose passages the checker compared for this
    /// claim.
    pub pages_mentioning: u32,
    /// A page research saved supports the claim now.
    pub stated: bool,
    /// The `pages_mentioning` versions, newest first.
    pub relevant: Vec<SourceRefDto>,
}
