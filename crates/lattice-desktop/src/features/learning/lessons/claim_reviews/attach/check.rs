//! Checking claims against one source the user attached (§5.4): the
//! claim's passages are retrieved from that version only, and judged by the
//! same strict publication judge a preparation pass uses. A passage counts
//! only when it mentions the claim's terms; a vector neighbour alone is
//! "no passage".

use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::claim_verification::{ClaimJudgment, ClaimVerdict};
use lattice_core::domain::learning::claim_review::{
    text_sha256, AttachOutcome, Check, CheckResult, Checker, EvidenceFingerprint, EvidenceSeen,
    VerdictKind,
};
use lattice_core::shared::error::{AppError, Result};

use crate::features::learning::lessons::content_verification::{
    bank_tokens, claim_share, evidence_policy, judge_for_publication, ranking, shown_of,
};
use crate::features::learning::reference_collection::{ReferenceCollection, ReferencePassage};

/// The passages of `source_version_id` the claim's check may read: those
/// that mention the claim's terms, best first, within the single-claim
/// judge's evidence room.
pub(super) async fn passages_from(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    source_version_id: &str,
    statement: &str,
) -> Result<Vec<ReferencePassage>> {
    let room = claim_share(llm, statement, 1);
    let page = references
        .retrieve_from(source_version_id, statement, |passages| {
            passages
                .iter()
                .map(|passage| bank_tokens(llm, &passage.text))
                .sum::<usize>()
                <= room
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
    let text = source
        .excerpt
        .get(start_byte..end_byte)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| {
            AppError::InvalidInput("The passage is not a span of that source's text.".into())
        })?;
    Ok(ReferencePassage {
        source_id: source_version_id.into(),
        text: text.into(),
        start_byte,
        end_byte,
        retrieval_kind: "user_selected".into(),
        score: 0.0,
    })
}

/// The check of `statement` against `passages`, as the ledger records it.
/// A provider outage is an error, never a verdict.
pub(super) async fn judge(
    llm: &dyn LLMPort,
    statement: &str,
    passages: &[ReferencePassage],
) -> Result<AttachOutcome> {
    let Some(first) = passages.first() else {
        return Ok(AttachOutcome::NoPassage);
    };
    let texts: Vec<String> = passages
        .iter()
        .map(|passage| passage.text.clone())
        .collect();
    let (judgment, shown) = judge_for_publication(llm, statement, &texts, &ranking(passages)).await;
    // A passage cut for the judge's window was never compared with the
    // claim: the check names only those it was shown.
    let passages = &shown_of(passages, &shown);
    let (result, reason) = match judgment {
        ClaimJudgment::Judged(outcome) => {
            let reason = outcome.reason.unwrap_or_default();
            let result = match outcome.verdict {
                ClaimVerdict::Supported => {
                    let quote = outcome.quote.unwrap_or_else(|| first.text.clone());
                    let source_version_id = passages
                        .iter()
                        .find(|passage| passage.text.contains(quote.trim()))
                        .unwrap_or(first)
                        .source_id
                        .clone();
                    CheckResult::Supported {
                        source_version_id,
                        supporting_quote: quote,
                    }
                }
                ClaimVerdict::Contradicted => CheckResult::Failed {
                    kind: VerdictKind::Contradicted,
                },
                ClaimVerdict::Unsupported => CheckResult::Failed {
                    kind: VerdictKind::Unsupported,
                },
                ClaimVerdict::Unverified => CheckResult::Failed {
                    kind: VerdictKind::CouldNotJudge,
                },
            };
            (result, reason)
        }
        ClaimJudgment::Unusable => (
            CheckResult::Failed {
                kind: VerdictKind::CouldNotJudge,
            },
            "The checker's answer held no usable verdict.".to_owned(),
        ),
        ClaimJudgment::Failed(error) => return Err(error),
    };
    let evidence: Vec<EvidenceSeen> = passages
        .iter()
        .map(|passage| EvidenceSeen {
            source_version_id: passage.source_id.clone(),
            start_byte: passage.start_byte,
            end_byte: passage.end_byte,
            text_sha256: text_sha256(&passage.text),
        })
        .collect();
    Ok(AttachOutcome::Checked(Check {
        checker: Checker::Local {
            model: llm.model_name().to_owned(),
        },
        result,
        reason,
        // A pass's own checks name the same policy, so a pass finds nothing
        // new in this check and reuses it (`claim_ledger::receipts`).
        fingerprint: EvidenceFingerprint::of(evidence_policy(), llm.model_name(), &evidence),
        evidence,
        window_tokens: llm.max_context_tokens(),
    }))
}
