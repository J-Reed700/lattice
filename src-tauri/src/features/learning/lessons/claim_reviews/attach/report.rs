//! What attaching evidence returns to the claim card (§5.4): the source it
//! was checked against, the check, the other claims checked, the frozen
//! copy, and the card's sentences.

use lattice_core::domain::learning::claim_review::{
    AttachOutcome, CheckResult, Revision, VerdictKind,
};
use serde::{Deserialize, Serialize};

use super::failures::AttachFailureDto;
use crate::features::learning::lessons::claim_reviews::dto::{AttachOutcomeDto, SourceRefDto};
use crate::features::learning::source_library::SavedSource;

/// Where the frozen copy of an attached page stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SnapshotNoteDto {
    /// Not a web page.
    NotApplicable,
    Ready,
    /// Being captured; the reader shows the saved text until it is ready.
    Pending,
    /// The text was saved; "Capture again" retries the copy.
    Failed {
        reason: String,
    },
}

/// The other open claims of the lesson checked against the attached source.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OtherClaimsCheckedDto {
    /// Open claims tried against the source.
    pub tried: usize,
    pub verified: usize,
    /// Checked and still not supported (or not judged).
    pub not_verified: usize,
    /// No passage of the source mentions the claim's terms.
    pub no_passage: usize,
    /// Why the remaining claims were not checked, when the model stopped
    /// answering part-way.
    pub stopped: Option<String>,
}

/// What `resolve_learning_claim` returns for an attach action.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaimAttachReportDto {
    /// The review's revision after the attach: send it with the next action.
    pub revision: i64,
    /// The saved source the claim was checked against; null when the link
    /// could not be saved.
    pub source: Option<SourceRefDto>,
    /// The evidence was already in the course's references.
    pub already_saved: bool,
    /// The targeted check of the claim; null when nothing was saved.
    pub outcome: Option<AttachOutcomeDto>,
    pub failure: Option<AttachFailureDto>,
    pub snapshot: SnapshotNoteDto,
    pub others: OtherClaimsCheckedDto,
    /// The card's lines, in order.
    pub lines: Vec<String>,
}

pub(super) fn source_ref(saved: &SavedSource) -> SourceRefDto {
    SourceRefDto {
        source_version_id: saved.version_id.clone(),
        title: saved.title.clone(),
        url: saved.url.clone(),
        added_by: saved.added_by.into(),
    }
}

/// The card's sentence for the targeted check.
pub(super) fn outcome_line(title: &str, outcome: &AttachOutcome) -> String {
    match outcome {
        AttachOutcome::NoPassage => format!(
            "No passage in {title} mentions this claim's terms. The source is in your references; the claim is unchanged."
        ),
        AttachOutcome::Checked(check) => match &check.result {
            CheckResult::Supported { .. } => format!("Verified against {title}."),
            CheckResult::Failed { kind } => {
                let verdict = match kind {
                    VerdictKind::Contradicted => "says otherwise",
                    VerdictKind::CouldNotJudge => "could not be judged",
                    _ => "does not support this claim",
                };
                let reason = check.reason.trim();
                if reason.is_empty() {
                    format!("{title} {verdict}.")
                } else {
                    format!("{title} {verdict}: {reason}")
                }
            }
        },
    }
}

fn others_line(others: &OtherClaimsCheckedDto) -> Option<String> {
    if others.tried == 0 {
        return None;
    }
    let mut parts = vec![format!("{} verified", others.verified)];
    if others.not_verified > 0 {
        parts.push(format!("{} not verified", others.not_verified));
    }
    parts.push(format!("{} no passage", others.no_passage));
    let noun = if others.tried == 1 { "claim" } else { "claims" };
    let mut line = format!(
        "Also checked {} other open {noun} against this source: {}.",
        others.tried,
        parts.join(", ")
    );
    if let Some(reason) = &others.stopped {
        line.push_str(&format!(" The rest were not checked: {reason}"));
    }
    Some(line)
}

/// The report for a link that could not be saved: nothing was recorded.
pub(super) fn not_saved(revision: Revision, failure: AttachFailureDto) -> ClaimAttachReportDto {
    ClaimAttachReportDto {
        revision: revision.0,
        source: None,
        already_saved: false,
        outcome: None,
        lines: vec![failure.message.clone()],
        failure: Some(failure),
        snapshot: SnapshotNoteDto::NotApplicable,
        others: OtherClaimsCheckedDto::default(),
    }
}

/// The report for a source saved (or found saved) and checked.
pub(super) fn checked(
    revision: Revision,
    saved: &SavedSource,
    already_saved: bool,
    outcome: &AttachOutcome,
    snapshot: SnapshotNoteDto,
    others: OtherClaimsCheckedDto,
) -> ClaimAttachReportDto {
    let mut lines = Vec::new();
    if already_saved {
        lines.push(format!(
            "Already in your references as {}; checking against it.",
            saved.title
        ));
    }
    lines.push(outcome_line(&saved.title, outcome));
    lines.extend(others_line(&others));
    if let SnapshotNoteDto::Failed { reason } = &snapshot {
        lines.push(format!(
            "Frozen copy not captured: {reason}; the text was saved."
        ));
    }
    ClaimAttachReportDto {
        revision: revision.0,
        source: Some(source_ref(saved)),
        already_saved,
        outcome: Some(outcome.into()),
        failure: None,
        snapshot,
        others,
        lines,
    }
}
