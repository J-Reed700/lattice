//! Program-scoped immutable source snapshots and refresh history.
use crate::{
    dto::*,
    portability_dto::{DeleteLearningSourceRequestDto, ReimportLearningSourceRequestDto},
};
use crate::{
    operations::Operation,
    persistence::{hash, now},
};
use futures::TryStreamExt;
use lattice_core::domain::learning::claim_review::SourceAddedBy;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, WriteTx};
#[cfg(test)]
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

mod admission;
#[cfg(test)]
mod admission_tests;
pub use admission::{RefusedSource, ShortSource};
mod capture;
mod lifecycle;
#[cfg(test)]
mod lifecycle_tests;
mod reads;
pub use reads::ActiveVersion;
mod repository;
pub(crate) use repository::reads::workspace_in;
mod user_evidence;
pub use user_evidence::{ChosenVersion, SavedSource};

const MAX_EXCERPT_CHARS: usize = 2_400;
/// The extraction a pasted text is recorded with: the text as written.
pub const PASTED_TEXT_EXTRACTION: &str = "pasted_text_v1";
const LEGACY_EXTRACTION: &str = "legacy_bounded_extraction_v1";
const MAX_CHECKS_IN_WORKSPACE: i64 = 50;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
fn conflict(message: impl Into<String>) -> AppError {
    AppError::Conflict(message.into())
}
fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_owned()
}
fn excerpt(text: &str) -> String {
    text.chars().take(MAX_EXCERPT_CHARS).collect()
}
fn words(text: &str) -> usize {
    text.split_whitespace().count()
}
fn uuid(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| invalid(format!("Invalid {label} ID")))
}
fn owned_id(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 512 {
        return Err(invalid(format!("Invalid {label} ID")));
    }
    Ok(())
}
fn policy(value: &LearningSourcePolicy) -> &'static str {
    match value {
        LearningSourcePolicy::Fixed => "fixed",
        LearningSourcePolicy::Manual => "manual",
        LearningSourcePolicy::BeforeUse => "before_use",
    }
}
fn parse_policy(value: &str) -> Result<LearningSourcePolicy> {
    match value {
        "fixed" => Ok(LearningSourcePolicy::Fixed),
        "manual" => Ok(LearningSourcePolicy::Manual),
        "before_use" => Ok(LearningSourcePolicy::BeforeUse),
        _ => Err(AppError::Database("Invalid source freshness policy".into())),
    }
}
fn parse_kind(value: &str) -> Result<LearningSourceKind> {
    match value {
        "web" => Ok(LearningSourceKind::Web),
        "document" => Ok(LearningSourceKind::Document),
        "pasted" => Ok(LearningSourceKind::Pasted),
        _ => Err(AppError::Database("Invalid source kind".into())),
    }
}
fn parse_added_by(value: &str) -> Result<SourceAddedBy> {
    SourceAddedBy::from_column(value)
        .ok_or_else(|| AppError::Database("Invalid source provenance".into()))
}
fn parse_check(value: &str) -> Result<LearningSourceCheckStatus> {
    match value {
        "unchanged" => Ok(LearningSourceCheckStatus::Unchanged),
        "update_available" => Ok(LearningSourceCheckStatus::UpdateAvailable),
        "failed" => Ok(LearningSourceCheckStatus::Failed),
        _ => Err(AppError::Database("Invalid source check status".into())),
    }
}
fn check_name(value: &LearningSourceCheckStatus) -> &'static str {
    match value {
        LearningSourceCheckStatus::Unchanged => "unchanged",
        LearningSourceCheckStatus::UpdateAvailable => "update_available",
        LearningSourceCheckStatus::Failed => "failed",
    }
}

#[derive(Clone)]
pub struct LearningSourceLibraryRepository {
    database: Database,
}

/// Who is adding a source and where it came from. Source capture owns this
/// fixed provenance plus caller-supplied stable IDs; naming each field keeps
/// replay validation auditable.
#[derive(Debug, Clone)]
pub struct NewLearningSource<'a> {
    pub operation_id: &'a str,
    pub source_id: &'a str,
    pub version_id: &'a str,
    pub program_id: &'a str,
    pub kind: LearningSourceKind,
    pub operation_kind: &'a str,
    pub origin: &'a str,
    pub requested_url: Option<&'a str>,
    pub freshness: LearningSourcePolicy,
}

#[derive(Debug, Clone)]
pub struct CapturedLearningSource {
    pub title: String,
    pub publisher: Option<String>,
    pub requested_url: Option<String>,
    pub resolved_url: Option<String>,
    pub text: String,
    pub truncated: bool,
    pub extraction_version: String,
    /// What capture read about where the text came from; the capture hook
    /// stores it with the version (`trust::capture::record`).
    pub signals: lattice_core::domain::provenance::ProvenanceSignals,
}

/// What saving a page research selected did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedSaved {
    Added,
    /// The course already holds the page, as this version.
    Held {
        version_id: String,
    },
}

impl LearningSourceLibraryRepository {
    pub fn before_use_allows(status: &LearningSourceCheckStatus) -> Result<()> {
        match status {
            LearningSourceCheckStatus::Unchanged => Ok(()),
            LearningSourceCheckStatus::UpdateAvailable => Err(invalid(
                "A newer version is available. Adopt it or change the freshness policy before preparing a lesson.",
            )),
            LearningSourceCheckStatus::Failed => Err(AppError::ServiceNotAvailable(
                "The before-use source check failed; retry after the source is reachable.".into(),
            )),
        }
    }
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// Captures a source research found (`added_by = research`); a source the
    /// user adds goes through [`Self::add_by`].
    pub async fn add(
        &self,
        source: NewLearningSource<'_>,
        captured: CapturedLearningSource,
        payload_hash: String,
    ) -> Result<()> {
        self.add_by(source, captured, payload_hash, SourceAddedBy::Research)
            .await
    }
    fn validate_program_id(id: &str) -> Result<()> {
        uuid(id, "program")
    }

    pub async fn preflight_replay(
        &self,
        operation_id: &str,
        program_id: &str,
        source_id: &str,
        kind: &str,
        payload_hash: &str,
    ) -> Result<bool> {
        uuid(operation_id, "operation")?;
        let mut read = self.database.begin_read().await?;
        let seen = source_operation(operation_id, program_id, source_id, kind, payload_hash)
            .seen(&mut read)
            .await?;
        read.commit().await?;
        Ok(seen)
    }

    pub async fn preflight_new_source(
        &self,
        program_id: &str,
        source_id: &str,
        version_id: &str,
    ) -> Result<()> {
        Self::validate_program_id(program_id)?;
        uuid(source_id, "source")?;
        uuid(version_id, "source version")?;
        let mut read = self.database.begin_read().await?;
        if !repository::active_program_exists(&mut read, program_id).await? {
            return Err(AppError::NotFound(
                "Active learning program not found".into(),
            ));
        }
        if repository::ids_in_use(&mut read, program_id, source_id, version_id).await? {
            return Err(invalid(
                "The requested source or version ID is already in use.",
            ));
        }
        read.commit().await?;
        Ok(())
    }

    /// Finalize SQL-migrated legacy snapshots before exposing metadata. This is
    /// idempotent and only touches rows explicitly marked as legacy bounded.
    async fn backfill_legacy(&self, program_id: &str) -> Result<()> {
        repository::backfill_legacy(&self.database, program_id).await
    }
}

fn find_case_insensitive(text: &str, query: &str) -> Option<usize> {
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return None;
    }
    let folded: Vec<(char, usize)> = text
        .chars()
        .enumerate()
        .flat_map(|(index, ch)| ch.to_lowercase().map(move |lower| (lower, index)))
        .collect();
    folded
        .windows(needle.len())
        .position(|window| window.iter().map(|(ch, _)| *ch).eq(needle.iter().copied()))
        .and_then(|position| folded.get(position).map(|(_, index)| *index))
}
fn context_excerpt(text: &str, match_char: usize) -> String {
    let begin = match_char.saturating_sub(180);
    text.chars().skip(begin).take(420).collect()
}

async fn lock_program(tx: &mut WriteTx, program_id: &str) -> Result<()> {
    if !repository::lock_active_program(tx, program_id).await? {
        return Err(AppError::NotFound(
            "Active learning program not found".into(),
        ));
    }
    Ok(())
}
fn source_operation<'a>(
    operation_id: &'a str,
    program_id: &'a str,
    source_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "source",
        kind,
        program_id: Some(program_id),
        subject_id: Some(source_id),
        payload_hash,
        conflict: "Operation ID was already used with different source data.",
    }
}

/// What a source operation produced: the version it captured, if any, and
/// the source revision it left.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceOperationResult<'a> {
    version_id: Option<&'a str>,
    revision: i64,
}

fn portability_operation<'a>(
    operation_id: &'a str,
    program_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "portability",
        kind,
        program_id: Some(program_id),
        subject_id: None,
        payload_hash,
        conflict: "Operation ID was reused with different source data.",
    }
}

async fn require_active_program(tx: &mut WriteTx, program_id: &str) -> Result<()> {
    if !repository::active_program_exists(tx, program_id).await? {
        return Err(AppError::NotFound(
            "Active learning program not found".into(),
        ));
    }
    Ok(())
}
