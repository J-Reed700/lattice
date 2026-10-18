//! The local checker's accuracy, measured from human verdicts (§15). Every
//! verdict the user gives on a claim the checker flagged is a labelled
//! example: a confirmation says the flag was false, a removal or an edit says
//! it was right. Examples export as the `claim-pairs-v1` JSONL that
//! `live_claim_pairs_eval` scores, so any candidate checker can be measured
//! on the user's own lessons.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §15.

use std::collections::BTreeMap;

use lattice_core::domain::learning::claim_review::{ClaimLedgerEntry, VerdictKind};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row, SqlitePool};

use super::repository::ShownPassage;
use crate::features::learning::persistence::{db, decode, encode, now};

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
    fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
            Self::RetrievalMiss => "retrieval_miss",
        }
    }

    fn from_column(value: &str) -> Option<Self> {
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

    fn from_column(value: &str) -> Option<Self> {
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

/// The file an export writes, under `evals/grounding/` (§15).
pub fn export_file_name(date: &str) -> String {
    format!("lesson-claims-{date}.jsonl")
}

#[derive(Clone)]
pub struct CheckerExamples {
    pool: SqlitePool,
}

impl CheckerExamples {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn record(&self, example: &CheckerExample) -> Result<()> {
        sqlx::query("INSERT INTO learning_checker_examples(id,program_id,lesson_id,claim_id,checker_model,context_window,policy,claim,passages_json,local_verdict,label,label_source,at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&example.id)
            .bind(&example.program_id)
            .bind(&example.lesson_id)
            .bind(&example.claim_id)
            .bind(&example.checker_model)
            .bind(example.context_window)
            .bind(&example.policy)
            .bind(&example.claim)
            .bind(encode(&example.passages)?)
            .bind(&example.local_verdict)
            .bind(example.label.as_str())
            .bind(example.label_source.as_str())
            .bind(example.at)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    /// Records a human verdict on a claim the local checker flagged (§15):
    /// a confirmation labels the claim true, a removal or an edit false.
    /// Returns `false`, writing nothing, when the checker's latest judgement
    /// of the claim was not a flag a judge made over passages (no source,
    /// could not judge, a general-knowledge proposal): those are not scored.
    pub async fn record_human_verdict(
        &self,
        program_id: &str,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
        passages: &[ShownPassage],
        source: LabelSource,
    ) -> Result<bool> {
        let local_verdict = match entry.latest_failure() {
            Some(VerdictKind::Unsupported) => "unsupported",
            Some(VerdictKind::Contradicted) => "contradicted",
            _ => return Ok(false),
        };
        let Some(basis) = entry.basis() else {
            return Ok(false);
        };
        let label = match source {
            LabelSource::UserConfirm => Label::True,
            LabelSource::UserRemove | LabelSource::UserEdit => Label::False,
            other => {
                return Err(AppError::InvalidInput(format!(
                    "{} is not a human verdict on a flagged claim.",
                    other.as_str()
                )))
            }
        };
        self.record(&CheckerExample {
            id: uuid::Uuid::new_v4().to_string(),
            program_id: program_id.into(),
            lesson_id: lesson_id.into(),
            claim_id: entry.claim_id().0.clone(),
            checker_model: basis.checker_model.clone(),
            context_window: i64::try_from(basis.window_tokens).unwrap_or(i64::MAX),
            policy: crate::features::learning::content_verification::evidence_policy().into(),
            claim: entry.statement().into(),
            passages: passages
                .iter()
                .map(|passage| passage.text.clone())
                .collect(),
            local_verdict: local_verdict.into(),
            label,
            label_source: source,
            at: now(),
        })
        .await?;
        Ok(true)
    }

    /// Records evidence the user added that the checker then found to
    /// support a claim it had flagged (§15, second row; the attach pipeline
    /// calls this with the check's passages). A flag over a passage the
    /// collection already held was the judge's error (`true`); one the
    /// collection could not have supported was retrieval's
    /// (`retrieval_miss`).
    pub async fn record_evidence_added(
        &self,
        program_id: &str,
        lesson_id: &str,
        flagged: &ClaimLedgerEntry,
        passages: &[ShownPassage],
        passage_was_in_collection: bool,
    ) -> Result<bool> {
        let local_verdict = match flagged.latest_failure() {
            Some(VerdictKind::Unsupported) => "unsupported",
            Some(VerdictKind::Contradicted) => "contradicted",
            Some(VerdictKind::NoSource) if !passage_was_in_collection => "no_source",
            _ => return Ok(false),
        };
        let Some(basis) = flagged.basis() else {
            return Ok(false);
        };
        self.record(&CheckerExample {
            id: uuid::Uuid::new_v4().to_string(),
            program_id: program_id.into(),
            lesson_id: lesson_id.into(),
            claim_id: flagged.claim_id().0.clone(),
            checker_model: basis.checker_model.clone(),
            context_window: i64::try_from(basis.window_tokens).unwrap_or(i64::MAX),
            policy: crate::features::learning::content_verification::evidence_policy().into(),
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
        .await?;
        Ok(true)
    }

    /// Forgets the examples an undone verdict gave (§5.1 Undo).
    pub async fn forget(&self, claim_id: &str, source: LabelSource) -> Result<()> {
        sqlx::query("DELETE FROM learning_checker_examples WHERE claim_id=? AND label_source=?")
            .bind(claim_id)
            .bind(source.as_str())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    /// Every example, oldest first; one program's with `program_id`.
    pub async fn list(&self, program_id: Option<&str>) -> Result<Vec<CheckerExample>> {
        let rows = sqlx::query("SELECT * FROM learning_checker_examples WHERE ?1 IS NULL OR program_id=?1 ORDER BY at,rowid")
            .bind(program_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|row| {
                let label: String = row.get("label");
                let source: String = row.get("label_source");
                Ok(CheckerExample {
                    id: row.get("id"),
                    program_id: row.get("program_id"),
                    lesson_id: row.get("lesson_id"),
                    claim_id: row.get("claim_id"),
                    checker_model: row.get("checker_model"),
                    context_window: row.get("context_window"),
                    policy: row.get("policy"),
                    claim: row.get("claim"),
                    passages: decode(row.get::<String, _>("passages_json").as_str())?,
                    local_verdict: row.get("local_verdict"),
                    label: Label::from_column(&label)
                        .ok_or_else(|| AppError::Database(format!("Unknown label {label}")))?,
                    label_source: LabelSource::from_column(&source).ok_or_else(|| {
                        AppError::Database(format!("Unknown label source {source}"))
                    })?,
                    at: row.get("at"),
                })
            })
            .collect()
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
