//! Checking claims against one source the user attached (§5.4): the
//! claim's passages are retrieved from that version only, and judged by the
//! same strict publication judge a preparation pass uses. A passage counts
//! only when it mentions the claim's terms; a vector neighbour alone is
//! "no passage".

use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::claim_verification::ClaimJudgment;
use lattice_core::domain::learning::claim_review::{
    AttachOutcome, Check, CheckResult, Checker, EvidenceFingerprint, EvidenceSeen, VerdictKind,
};
use lattice_core::shared::error::{AppError, Result};

use crate::lessons::content_verification::{
    fits_window, judge_for_publication, ranking, shown_of, supporting_passage, Judged,
};
use crate::reference_collection::{PassageKind, ReferenceCollection, ReferencePassage};

const UNUSABLE: &str = "The checker's answer held no usable verdict.";

/// The passages of `source_version_id` the claim's check may read: those
/// that mention the claim's terms, best first, within the single-claim
/// judge's evidence room.
pub(super) async fn passages_from(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    source_version_id: &str,
    statement: &str,
) -> Result<Vec<ReferencePassage>> {
    let page = references
        .retrieve_from(source_version_id, statement, |passages| {
            fits_window(llm, statement, passages)
        })
        .await?;
    let subject = references.subject_of(statement).await?;
    Ok(page
        .into_iter()
        .filter(|passage| subject.mentioned_in(&passage.text))
        .collect())
}

/// The passage the user picked, as saved.
pub(super) fn picked_passage(
    references: &ReferenceCollection<'_>,
    source_version_id: &str,
    (start_byte, end_byte): (usize, usize),
) -> Result<ReferencePassage> {
    let source = references
        .sources
        .iter()
        .find(|source| source.id == source_version_id)
        .ok_or_else(|| AppError::NotFound("Source version not found in this course".into()))?;
    ReferencePassage::of(source, start_byte..end_byte, PassageKind::UserSelected, 0.0)
        .filter(|passage| !passage.text.trim().is_empty())
        .ok_or_else(|| {
            AppError::InvalidInput("The passage is not a span of that source's text.".into())
        })
}

/// The check of `statement` against `passages`, as the ledger records it.
/// A provider outage is an error, never a verdict.
pub(super) async fn judge(
    llm: &dyn LLMPort,
    statement: &str,
    passages: &[ReferencePassage],
) -> Result<AttachOutcome> {
    if passages.is_empty() {
        return Ok(AttachOutcome::NoPassage);
    }
    let texts: Vec<String> = passages
        .iter()
        .map(|passage| passage.text.clone())
        .collect();
    let judged = judge_for_publication(llm, statement, &texts, &ranking(passages)).await;
    // A passage cut for the judge's window was never compared with the
    // claim: the check names only those it was shown.
    let passages = &shown_of(passages, &judged.shown);
    let (result, reason) = match judged.judgment {
        ClaimJudgment::Judged(outcome) => {
            let reason = outcome.reason.unwrap_or_default();
            match outcome.verdict.failure(true) {
                Some(kind) => (CheckResult::Failed { kind }, reason),
                None => match outcome
                    .quote
                    .as_deref()
                    .and_then(|quote| supporting_passage(quote, passages, None))
                {
                    Some((passage, words)) => (
                        CheckResult::Supported {
                            source_version_id: passage.source_id.clone(),
                            supporting_quote: words.to_owned(),
                        },
                        reason,
                    ),
                    None => (
                        CheckResult::Failed {
                            kind: VerdictKind::CouldNotJudge,
                        },
                        UNUSABLE.to_owned(),
                    ),
                },
            }
        }
        ClaimJudgment::Unusable => (
            CheckResult::Failed {
                kind: VerdictKind::CouldNotJudge,
            },
            UNUSABLE.to_owned(),
        ),
        ClaimJudgment::Failed(error) => return Err(error),
    };
    let evidence: Vec<EvidenceSeen> = passages
        .iter()
        .map(|passage| {
            EvidenceSeen::of(
                &passage.source_id,
                passage.start_byte,
                passage.end_byte,
                &passage.text,
            )
        })
        .collect();
    Ok(AttachOutcome::Checked(Check {
        checker: Checker::Local {
            model: llm.model_name().to_owned(),
        },
        result,
        reason,
        fingerprint: EvidenceFingerprint::of(
            Judged::Passages.policy(),
            llm.model_name(),
            &evidence,
        ),
        evidence,
        window_tokens: llm.max_context_tokens(),
    }))
}
