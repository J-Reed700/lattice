//! The program source library: versions, checks, search and source requests.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSourceKind {
    Web,
    Document,
    Pasted,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSourcePolicy {
    Fixed,
    Manual,
    BeforeUse,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSourceCheckStatus {
    Unchanged,
    UpdateAvailable,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceVersionSummaryDto {
    pub id: String,
    pub version_number: i64,
    pub title: String,
    pub publisher: Option<String>,
    pub resolved_url: Option<String>,
    pub excerpt: String,
    pub content_sha256: String,
    pub word_count: usize,
    pub truncated: bool,
    pub extraction_version: String,
    pub acquired_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceCheckDto {
    pub operation_id: String,
    pub status: LearningSourceCheckStatus,
    pub checked_at: i64,
    pub active_digest: Option<String>,
    pub pending_version_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceLibraryItemDto {
    pub id: String,
    pub kind: LearningSourceKind,
    pub origin: String,
    pub requested_url: Option<String>,
    pub freshness_policy: LearningSourcePolicy,
    pub active_version_id: Option<String>,
    pub pending_version_id: Option<String>,
    pub revision: i64,
    pub deleted_at: Option<i64>,
    pub deletion_reason: Option<String>,
    pub active_version: Option<LearningSourceVersionSummaryDto>,
    pub pending_version: Option<LearningSourceVersionSummaryDto>,
    pub versions: Vec<LearningSourceVersionSummaryDto>,
    pub latest_check: Option<LearningSourceCheckDto>,
    pub checks: Vec<LearningSourceCheckDto>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceWorkspaceDto {
    pub program_id: String,
    pub sources: Vec<LearningSourceLibraryItemDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceUsageDto {
    pub lesson_id: String,
    pub lesson_title: String,
    pub reference_kind: String,
    pub reference_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceVersionDto {
    pub source_id: String,
    pub version: LearningSourceVersionSummaryDto,
    pub full_text: String,
    pub usage: Vec<LearningSourceUsageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetLearningSourceVersionRequestDto {
    pub program_id: String,
    pub source_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchLearningSourcesRequestDto {
    pub program_id: String,
    pub query: String,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceSearchResultDto {
    pub source_id: String,
    pub version_id: String,
    pub title: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddLearningWebSourceRequestDto {
    pub operation_id: String,
    pub source_id: String,
    pub version_id: String,
    pub program_id: String,
    pub url: String,
    pub freshness_policy: LearningSourcePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddLearningDocumentSourceRequestDto {
    pub operation_id: String,
    pub source_id: String,
    pub version_id: String,
    pub program_id: String,
    pub document_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddLearningTextSourceRequestDto {
    pub operation_id: String,
    pub source_id: String,
    pub version_id: String,
    pub program_id: String,
    pub title: String,
    pub publisher: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RefreshLearningSourceRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub source_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AdoptLearningSourceVersionRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub source_id: String,
    pub version_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLearningSourcePolicyRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub source_id: String,
    pub freshness_policy: LearningSourcePolicy,
    pub expected_revision: i64,
}
