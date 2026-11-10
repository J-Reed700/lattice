//! The local checker's accuracy, measured from human verdicts (§15). Every
//! verdict the user gives on a claim the checker flagged is a labelled
//! example: a confirmation says the flag was false, a removal or an edit says
//! it was right. Examples export as the `claim-pairs-v1` JSONL that
//! `live_claim_pairs_eval` scores, so any candidate checker can be measured
//! on the user's own lessons.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §15.

use std::collections::BTreeMap;

use lattice_core::domain::learning::claim_review::{
    AttachOutcome, Checker, ClaimEvent, ClaimLedgerEntry, VerdictKind,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, DatabaseHandle, WriteTx};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::repository::{self, ShownPassage};
use crate::persistence::now;

/// Whether the claim, as written, is true (§15 "Label").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    True,
    False,
    /// The supporting passage was not in the collection when the checker
    /// judged: retrieval missed it, the judge did not err.
    RetrievalMiss,
}

impl Label {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
            Self::RetrievalMiss => "retrieval_miss",
        }
    }

    pub(super) fn from_column(value: &str) -> Option<Self> {
        [Self::True, Self::False, Self::RetrievalMiss]
            .into_iter()
            .find(|label| label.as_str() == value)
    }
}

/// What gave the label (§15's rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelSource {
    UserConfirm,
    UserRemove,
    UserEdit,
    EvidenceAdded,
    VerifierAgree,
    VerifierDisagree,
}

impl LabelSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserConfirm => "user_confirm",
            Self::UserRemove => "user_remove",
            Self::UserEdit => "user_edit",
            Self::EvidenceAdded => "evidence_added",
            Self::VerifierAgree => "verifier_agree",
            Self::VerifierDisagree => "verifier_disagree",
        }
    }

    pub(super) fn from_column(value: &str) -> Option<Self> {
        [
            Self::UserConfirm,
            Self::UserRemove,
            Self::UserEdit,
            Self::EvidenceAdded,
            Self::VerifierAgree,
            Self::VerifierDisagree,
        ]
        .into_iter()
        .find(|source| source.as_str() == value)
    }
}

/// One labelled example of the local checker's behaviour.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckerExample {
    pub id: String,
    pub program_id: String,
    pub lesson_id: String,
    pub claim_id: String,
    pub checker_model: String,
    pub context_window: i64,
    pub policy: String,
    pub claim: String,
    /// The exact passage texts the checker compared.
    pub passages: Vec<String>,
    /// The local checker's verdict: `unsupported`, `contradicted`, or
    /// `supported` for a claim the Verifier disputed.
    pub local_verdict: String,
    pub label: Label,
    pub label_source: LabelSource,
    pub at: i64,
}

/// The checker's accuracy for one Learning model (§15 "Visible to the user").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckerAccuracyDto {
    pub checker_model: String,
    pub labelled: u32,
    /// Flags the user confirmed over, or that added evidence overturned.
    pub false_flags: u32,
    /// Flags the user agreed with by removing or editing the claim.
    pub true_flags: u32,
    /// `false_flags / (false_flags + true_flags)`; none without flags.
    pub false_flag_rate: Option<f64>,
    /// Claims the checker passed that the Verifier contradicted and the user
    /// then removed or edited.
    pub false_passes: u32,
    /// Examples labelled through a Verifier audit.
    pub verifier_labelled: u32,
    /// `false_passes / verifier_labelled`; none without Verifier examples.
    pub false_pass_rate: Option<f64>,
    pub retrieval_misses: u32,
}

/// The kind of the local checker's failing check that `entry` stands on: its
/// latest failure, when the latest check that judged it was the local
/// checker's. A claim the local checker passed and a Verifier disputed was
/// flagged by the Verifier, and a user's verdict on it says nothing of the
/// local checker's flags.
fn local_flag(entry: &ClaimLedgerEntry) -> Option<&VerdictKind> {
    let latest = entry
        .events()
        .iter()
        .rev()
        .find_map(|record| match &record.event {
            ClaimEvent::Checked(check)
            | ClaimEvent::EvidenceAttached {
                result: AttachOutcome::Checked(check),
                ..
            } if !check.result.is_unjudged() => Some(&check.checker),
            _ => None,
        });
    matches!(latest, Some(Checker::Local { .. }))
        .then(|| entry.latest_failure())
        .flatten()
}

/// The example evidence that supports a flagged claim gives, when the local
/// checker's latest judgement of it was a scored flag.
fn evidence_added_example(
    program_id: &str,
    lesson_id: &str,
    flagged: &ClaimLedgerEntry,
    passages: &[ShownPassage],
    passage_was_in_collection: bool,
) -> Option<CheckerExample> {
    let local_verdict = match local_flag(flagged) {
        Some(VerdictKind::Unsupported) => "unsupported",
        Some(VerdictKind::Contradicted) => "contradicted",
        Some(VerdictKind::NoSource) if !passage_was_in_collection => "no_source",
        _ => return None,
    };
    let basis = flagged.basis()?;
    Some(CheckerExample {
        id: uuid::Uuid::new_v4().to_string(),
        program_id: program_id.into(),
        lesson_id: lesson_id.into(),
        claim_id: flagged.claim_id().0.clone(),
        checker_model: basis.checker_model.clone(),
        context_window: i64::try_from(basis.window_tokens).unwrap_or(i64::MAX),
        policy: crate::content_verification::evidence_policy().into(),
        claim: flagged.statement().into(),
        passages: passages
            .iter()
            .map(|passage| passage.text.clone())
            .collect(),
        local_verdict: local_verdict.into(),
        label: if passage_was_in_collection {
            Label::True
        } else {
            Label::RetrievalMiss
        },
        label_source: LabelSource::EvidenceAdded,
        at: now(),
    })
}

/// The example a human verdict on `entry` gives, when the local checker's
/// latest judgement of it was a scored flag.
fn human_example(
    program_id: &str,
    lesson_id: &str,
    entry: &ClaimLedgerEntry,
    passages: &[ShownPassage],
    source: LabelSource,
) -> Result<Option<CheckerExample>> {
    let local_verdict = match local_flag(entry) {
        Some(VerdictKind::Unsupported) => "unsupported",
        Some(VerdictKind::Contradicted) => "contradicted",
        _ => return Ok(None),
    };
    let Some(basis) = entry.basis() else {
        return Ok(None);
    };
    let label = match source {
        LabelSource::UserConfirm => Label::True,
        LabelSource::UserRemove | LabelSource::UserEdit => Label::False,
        other => {
            return Err(AppError::InvalidInput(format!(
                "{} is not a human verdict on a flagged statement.",
                other.as_str()
            )))
        }
    };
    Ok(Some(CheckerExample {
        id: uuid::Uuid::new_v4().to_string(),
        program_id: program_id.into(),
        lesson_id: lesson_id.into(),
        claim_id: entry.claim_id().0.clone(),
        checker_model: basis.checker_model.clone(),
        context_window: i64::try_from(basis.window_tokens).unwrap_or(i64::MAX),
        policy: crate::content_verification::evidence_policy().into(),
        claim: entry.statement().into(),
        passages: passages
            .iter()
            .map(|passage| passage.text.clone())
            .collect(),
        local_verdict: local_verdict.into(),
        label,
        label_source: source,
        at: now(),
    }))
}

/// The file an export writes, under `evals/grounding/` (§15).
pub fn export_file_name(date: &str) -> String {
    format!("lesson-claims-{date}.jsonl")
}

#[derive(Clone)]
pub struct CheckerExamples {
    pool: Database,
}

impl CheckerExamples {
    pub fn new(pool: impl DatabaseHandle) -> Self {
        let pool = pool.database();
        Self { pool }
    }

    pub async fn record(&self, example: &CheckerExample) -> Result<()> {
        repository::checker_examples::insert(&self.pool, example).await
    }

    /// Records a human verdict on a claim the local checker flagged (§15):
    /// a confirmation labels the claim true, a removal or an edit false.
    /// Returns `false`, writing nothing, when the checker's latest judgement
    /// of the claim was not a flag a judge made over passages (no source,
    /// could not judge, a general-knowledge proposal, or a claim only the
    /// Verifier flagged): those are not scored.
    pub async fn record_human_verdict(
        &self,
        program_id: &str,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
        passages: &[ShownPassage],
        source: LabelSource,
    ) -> Result<bool> {
        let Some(example) = human_example(program_id, lesson_id, entry, passages, source)? else {
            return Ok(false);
        };
        self.record(&example).await?;
        Ok(true)
    }

    /// [`Self::record_human_verdict`] inside the caller's unit of work.
    pub(super) async fn record_human_verdict_in(
        tx: &mut WriteTx,
        program_id: &str,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
        passages: &[ShownPassage],
        source: LabelSource,
    ) -> Result<bool> {
        let Some(example) = human_example(program_id, lesson_id, entry, passages, source)? else {
            return Ok(false);
        };
        repository::checker_examples::insert_in(tx, &example).await?;
        Ok(true)
    }

    /// Records evidence the user added that the checker then found to
    /// support a claim it had flagged (§15, second row), inside the caller's unit
    /// of work. A flag over a passage the
    /// collection already held was the judge's error (`true`); one the
    /// collection could not have supported was retrieval's
    /// (`retrieval_miss`).
    pub(super) async fn record_evidence_added_in(
        tx: &mut WriteTx,
        program_id: &str,
        lesson_id: &str,
        flagged: &ClaimLedgerEntry,
        passages: &[ShownPassage],
        passage_was_in_collection: bool,
    ) -> Result<bool> {
        let Some(example) = evidence_added_example(
            program_id,
            lesson_id,
            flagged,
            passages,
            passage_was_in_collection,
        ) else {
            return Ok(false);
        };
        repository::checker_examples::insert_in(tx, &example).await?;
        Ok(true)
    }

    /// Forgets the examples an undone verdict gave (§5.1 Undo).
    pub async fn forget(&self, claim_id: &str, source: LabelSource) -> Result<()> {
        repository::checker_examples::delete_for_claim(&self.pool, claim_id, source).await
    }

    /// [`Self::forget`] inside the caller's unit of work.
    pub(super) async fn forget_in(
        tx: &mut WriteTx,
        claim_id: &str,
        source: LabelSource,
    ) -> Result<()> {
        repository::checker_examples::delete_for_claim_in(tx, claim_id, source).await
    }

    /// Every example, oldest first; one program's with `program_id`.
    pub async fn list(&self, program_id: Option<&str>) -> Result<Vec<CheckerExample>> {
        repository::checker_examples::list(&self.pool, program_id).await
    }

    /// The examples as `claim-pairs-v1` lines (`{id, claim, passage, label,
    /// difficulty}`), the shape `live_claim_pairs_eval` reads. A true claim
    /// is labelled `supported`; a false one keeps the checker's verdict.
    /// Retrieval misses and examples without passages are left out: no
    /// passage was judged.
    pub async fn export_lines(&self, program_id: Option<&str>) -> Result<Vec<String>> {
        Ok(self
            .list(program_id)
            .await?
            .iter()
            .filter(|example| !example.passages.is_empty())
            .filter_map(|example| {
                let label = match example.label {
                    Label::True => "supported",
                    Label::False => example.local_verdict.as_str(),
                    Label::RetrievalMiss => return None,
                };
                Some(
                    json!({
                        "id": example.id,
                        "claim": example.claim,
                        "passage": example.passages.join("\n\n"),
                        "label": label,
                        "difficulty": [example.label_source.as_str()],
                        "checker_model": example.checker_model,
                    })
                    .to_string(),
                )
            })
            .collect())
    }

    /// Writes [`Self::export_lines`] to `path`, returning how many pairs.
    pub async fn export(&self, program_id: Option<&str>, path: &std::path::Path) -> Result<usize> {
        let lines = self.export_lines(program_id).await?;
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        tokio::fs::write(path, text)
            .await
            .map_err(|error| AppError::FileStorage(error.to_string()))?;
        Ok(lines.len())
    }

    /// The accuracy of each checker model, from its labelled examples (§15).
    pub async fn accuracy(&self) -> Result<Vec<CheckerAccuracyDto>> {
        let mut models: BTreeMap<String, CheckerAccuracyDto> = BTreeMap::new();
        for example in self.list(None).await? {
            let tally = models
                .entry(example.checker_model.clone())
                .or_insert_with(|| CheckerAccuracyDto {
                    checker_model: example.checker_model.clone(),
                    labelled: 0,
                    false_flags: 0,
                    true_flags: 0,
                    false_flag_rate: None,
                    false_passes: 0,
                    verifier_labelled: 0,
                    false_pass_rate: None,
                    retrieval_misses: 0,
                });
            tally.labelled += 1;
            match (example.label_source, example.label) {
                (_, Label::RetrievalMiss) => tally.retrieval_misses += 1,
                (LabelSource::UserConfirm | LabelSource::EvidenceAdded, Label::True) => {
                    tally.false_flags += 1
                }
                (LabelSource::UserRemove | LabelSource::UserEdit, Label::False) => {
                    tally.true_flags += 1
                }
                (LabelSource::VerifierAgree | LabelSource::VerifierDisagree, label) => {
                    tally.verifier_labelled += 1;
                    if label == Label::False {
                        tally.false_passes += 1;
                    }
                }
                _ => {}
            }
        }
        let rate = |part: u32, whole: u32| (whole > 0).then(|| f64::from(part) / f64::from(whole));
        Ok(models
            .into_values()
            .map(|mut tally| {
                tally.false_flag_rate =
                    rate(tally.false_flags, tally.false_flags + tally.true_flags);
                tally.false_pass_rate = rate(tally.false_passes, tally.verifier_labelled);
                tally
            })
            .collect())
    }
}
