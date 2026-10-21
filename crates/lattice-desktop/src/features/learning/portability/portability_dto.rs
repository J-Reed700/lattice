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

/// What an import does to a course that is already on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingAction {
    /// The import stops: the course here stays as it is.
    Block,
    /// The course here stays, and the import lands beside it.
    Keep,
    /// The course here is backed up and replaced.
    Replace,
}

impl LearningPackConflictPolicy {
    /// Every policy, most preferred first: moving keeps the most, a copy
    /// keeps the least of the pack's identity.
    pub const ALL: [Self; 3] = [Self::MergeSafe, Self::ReplaceAfterBackup, Self::CreateCopy];

    pub fn on_existing(self) -> ExistingAction {
        match self {
            Self::MergeSafe => ExistingAction::Block,
            Self::CreateCopy => ExistingAction::Keep,
            Self::ReplaceAfterBackup => ExistingAction::Replace,
        }
    }

    /// A copy gives every imported record a fresh identifier, so no record
    /// another course owns can collide with it.
    pub fn remaps_ids(self) -> bool {
        matches!(self, Self::CreateCopy)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPackPreviewStatus {
    Pending,
    Applied,
    Cancelled,
    Stale,
}

/// Why a preview cannot go ahead, or what it will do with a record that is
/// already on this computer. The renderer words each kind; `reason` carries
/// only the backend's own explanation when it has one.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LearningPackConflictKind {
    /// The pack was written by a different build of Lattice.
    PackSchema,
    /// The course itself is already on this computer.
    Program,
    /// The copy here has history no pack can restore, so it cannot be replaced.
    ProgramHistory,
    /// The copy here is still being prepared.
    ProgramJobs,
    /// A module another course on this computer owns.
    Module,
    /// A lesson another course on this computer owns.
    Lesson,
    /// A question another course on this computer owns.
    Question,
    /// A submitted assessment another course on this computer owns.
    Attempt,
    /// Any other record another course on this computer owns.
    Record,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackConflictDto {
    pub kind: LearningPackConflictKind,
    /// The local record's ID, when the conflict is about one.
    pub existing_id: Option<String>,
    /// The local record's title, when it has one.
    pub existing_title: Option<String>,
    /// The backend's explanation, when the kind alone does not say enough.
    pub reason: Option<String>,
    /// The policies under which this conflict goes away, most preferred
    /// first. Empty when no choice gets past it.
    pub clears_with: Vec<LearningPackConflictPolicy>,
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
    pub export_notice: LearningPackExportNoticeDto,
}

/// What an export of this course cannot carry, shown before exporting. The
/// renderer words both lists.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackExportNoticeDto {
    /// Local data the pack leaves on this computer; only kinds that have any.
    pub stays_behind: Vec<LearningPackStaysBehindDto>,
    /// Why an export would be refused right now. The same conflict a replace
    /// preview reports, so one fact has one shape.
    pub blockers: Vec<LearningPackConflictKind>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPackStaysBehindKind {
    /// Lesson notes, which live in the user's notebook.
    NotebookNotes,
    /// Unsaved lab drafts.
    PracticalDrafts,
    /// Unfinished lesson reviews, whose drafts live in job checkpoints.
    UnfinishedLessonReviews,
    /// Tangent chats, which stay with the rest of the user's chats.
    TangentChats,
    /// The Verifier audit consent, given on this computer only.
    VerifierConsent,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackStaysBehindDto {
    pub kind: LearningPackStaysBehindKind,
    pub count: u32,
}

/// What one export carries. The panel, save-as and export-all share it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackExportOptionsDto {
    pub include_evidence: bool,
    pub include_practical_artifacts: bool,
    pub include_source_bodies: bool,
    pub source_body_redistribution_confirmed: bool,
}

/// A save dialog either names a file or is dismissed; a dismissed dialog
/// writes and records nothing.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SaveLearningPackAsResultDto {
    Cancelled,
    Saved { export: Box<LearningPackExportDto> },
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportAllLearningPacksRequestDto {
    pub operation_id: String,
    pub options: LearningPackExportOptionsDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportedLearningPackDto {
    pub program_id: String,
    pub title: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FailedLearningPackExportDto {
    pub program_id: String,
    pub title: String,
    pub error: String,
}

/// A dismissed folder dialog writes and records nothing; a chosen folder
/// gets every course that could be exported, and says which could not.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ExportAllLearningPacksResultDto {
    Cancelled,
    Exported {
        #[serde(rename = "destinationDir")]
        destination_dir: String,
        exported: Vec<ExportedLearningPackDto>,
        failed: Vec<FailedLearningPackExportDto>,
    },
}

/// One course's export. Rust names the file from the course title; the
/// renderer never sends a file name.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportLearningPackRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub options: LearningPackExportOptionsDto,
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
