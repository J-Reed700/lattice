//! Renderer contracts for portable program packs, source completion, and
//! versioned recall. Pack bytes, credentials, private chat, answer keys, and
//! hidden practical checks never cross these contracts.

use crate::features::learning::recall::study_dto::StudyRating;
use crate::features::learning::{
    dto::{LearningSourceVersionSummaryDto, LearningSourceWorkspaceDto},
    pack::LearningPackManifest,
    source_selector::{LearningQuoteMatchStatus, LearningTextQuoteSelector},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPackConflictPolicy {
    CreateCopy,
    MergeSafe,
    ReplaceAfterBackup,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPackPreviewStatus {
    Pending,
    Applied,
    Cancelled,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackConflictDto {
    pub entity_kind: String,
    pub incoming_id: String,
    pub incoming_title: String,
    pub existing_id: String,
    pub existing_title: String,
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackChangeDto {
    pub entity_kind: String,
    pub entity_id: String,
    pub action: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackExportDto {
    pub id: String,
    pub program_id: String,
    pub destination_path: String,
    pub manifest: LearningPackManifest,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackImportPreviewDto {
    pub id: String,
    pub source_path: String,
    pub manifest: LearningPackManifest,
    pub incoming_program_id: String,
    pub incoming_program_title: String,
    pub conflict_policy: LearningPackConflictPolicy,
    pub conflicts: Vec<LearningPackConflictDto>,
    pub changes: Vec<LearningPackChangeDto>,
    pub warnings: Vec<String>,
    pub status: LearningPackPreviewStatus,
    pub can_apply: bool,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackImportResultDto {
    pub id: String,
    pub preview_id: String,
    pub imported_program_id: String,
    pub backup_id: Option<String>,
    pub applied_changes: Vec<LearningPackChangeDto>,
    pub imported_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPortabilityWorkspaceDto {
    pub program_id: String,
    pub exports: Vec<LearningPackExportDto>,
    pub import_previews: Vec<LearningPackImportPreviewDto>,
    pub imports: Vec<LearningPackImportResultDto>,
    pub source_workspace: LearningSourceWorkspaceDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportLearningPackRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub file_name: String,
    pub include_evidence: bool,
    pub include_practical_artifacts: bool,
    pub include_source_bodies: bool,
    pub source_body_redistribution_confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLearningPackImportRequestDto {
    pub operation_id: String,
    pub preview_id: String,
    pub source_path: String,
    pub conflict_policy: LearningPackConflictPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyLearningPackImportRequestDto {
    pub operation_id: String,
    pub preview_id: String,
    pub expected_root_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CancelLearningPackImportPreviewRequestDto {
    pub operation_id: String,
    pub preview_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteLearningSourceRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub source_id: String,
    pub expected_revision: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReimportLearningSourceRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub source_id: String,
    pub version_id: String,
    pub expected_revision: i64,
    pub replacement_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceSelectorDto {
    pub id: String,
    pub source_id: String,
    pub source_version_id: String,
    pub selector: LearningTextQuoteSelector,
    pub match_status: LearningQuoteMatchStatus,
    pub start_byte: Option<usize>,
    pub end_byte: Option<usize>,
    pub candidate_count: usize,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateLearningSourceSelectorRequestDto {
    pub operation_id: String,
    pub selector_id: String,
    pub program_id: String,
    pub source_id: String,
    pub source_version_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MatchLearningSourceSelectorRequestDto {
    pub program_id: String,
    pub selector_id: String,
    pub target_version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchLearningSourcesSemanticallyRequestDto {
    pub program_id: String,
    pub query: String,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceSemanticSearchResultDto {
    pub source_id: String,
    pub version: LearningSourceVersionSummaryDto,
    pub excerpt: String,
    pub score: f64,
    pub retrieval_kind: String,
    pub selector: LearningTextQuoteSelector,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningRecallCardFormat {
    MultipleChoice,
    QuestionAnswer,
    Cloze,
    Reverse,
    CodePrediction,
    Reconstruction,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningRecallContentDto {
    pub prompt: String,
    pub answer: String,
    pub explanation: String,
    /// Required for multiple-choice recall; empty for every other format.
    pub options: Vec<String>,
    /// Required for multiple-choice recall; empty for every other format.
    pub correct_option_index: Option<usize>,
    pub language: Option<String>,
    pub cloze_deletions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningRecallSchedulerStateDto {
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    pub last_reviewed_at: Option<i64>,
    pub due_at: i64,
    pub interval_days: i64,
    pub review_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningRecallCardVersionDto {
    pub revision: i64,
    pub format: LearningRecallCardFormat,
    pub content: LearningRecallContentDto,
    pub source_version_ids: Vec<String>,
    pub change_reason: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningRecallCardDto {
    pub id: String,
    pub format: LearningRecallCardFormat,
    pub content: LearningRecallContentDto,
    pub source_version_ids: Vec<String>,
    pub content_revision: i64,
    pub scheduler: LearningRecallSchedulerStateDto,
    pub versions: Vec<LearningRecallCardVersionDto>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningRecallDuplicateStatus {
    Pending,
    Confirmed,
    Dismissed,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningRecallDuplicateSuggestionDto {
    pub id: String,
    pub card_id: String,
    pub possible_duplicate_card_id: String,
    pub reason: String,
    pub similarity: Option<f64>,
    pub status: LearningRecallDuplicateStatus,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningRecallWorkspaceDto {
    pub program_id: String,
    pub cards: Vec<LearningRecallCardDto>,
    pub duplicates: Vec<LearningRecallDuplicateSuggestionDto>,
    pub due_count: i64,
    pub scheduler_disclosure: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveLearningRecallCardRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub card_id: String,
    pub expected_content_revision: Option<i64>,
    pub format: LearningRecallCardFormat,
    pub content: LearningRecallContentDto,
    pub source_version_ids: Vec<String>,
    pub change_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DecideLearningRecallDuplicateRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub suggestion_id: String,
    pub accept: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewLearningRecallCardRequestDto {
    pub review_id: String,
    pub program_id: String,
    pub card_id: String,
    pub expected_review_count: i64,
    pub rating: StudyRating,
    pub selected_option: Option<usize>,
}
