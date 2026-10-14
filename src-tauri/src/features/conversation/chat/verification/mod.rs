//! Response grounding verification.
//!
//! Two passes. The lexical pre-filter ([`lexical`]) scores every claim sentence
//! against the passages it cites — cheap, deterministic, and run on every turn.
//! The claim judge ([`judge`]) then asks a model about every cited sentence
//! except a near-verbatim copy of its passage that leaves nothing out. Matching
//! vocabulary and opposite meaning look identical to a token counter, so
//! overlap alone settles nothing.
//!
//! The judge is an improvement on the lexical verdict, never a precondition
//! for one. An escalated claim the judge never reached — the budget ran out,
//! the call failed — keeps its lexical verdict, except one carrying a number,
//! date or negation, or copying a passage sentence that carries one it left
//! out: overlap cannot settle those, so it reads as unverified
//! rather than borrowing a positive verdict from shared vocabulary. Unverified
//! is its own state and never counts as unsupported: "not checked" and "not
//! found in the source" are different findings. With no judge configured,
//! reports remain explicitly lexical.

use std::sync::Arc;

use tracing::{debug, info};

use crate::application::contracts::settings::LLMVerificationSettingsDto;
use crate::application::ports::LLMPort;
use crate::features::qa::dto::SourceDto;

mod background;
#[cfg(test)]
mod gate_tests;
mod judge;
mod lexical;

pub use self::background::VerificationReadyDto;
pub(super) use self::background::{pending_metadata, BackgroundVerification};

use self::judge::{evidence_for, ClaimJudge, ClaimJudgment, Evidence, MIN_VERDICT_CONFIDENCE};
use self::lexical::LexicalClaim;

use crate::application::services::claim_verification::ClaimVerdict;

impl ClaimVerdict {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Contradicted => "contradicted",
            Self::Unsupported => "unsupported",
            Self::Unverified => "unverified",
        }
    }
}

/// Why a claim was left unverified, so the UI can say which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnverifiedReason {
    /// The judge's time budget ran out before it reached the claim.
    Budget,
    /// The judge was asked and returned nothing usable.
    JudgeFailed,
    /// The cited source has no archived text to check against.
    NoText,
    /// The judge returned a label, but its probability was too low to present
    /// that label as a finding.
    LowConfidence,
}

impl UnverifiedReason {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Budget => "budget",
            Self::JudgeFailed => "judge_failed",
            Self::NoText => "no_text",
            Self::LowConfidence => "low_confidence",
        }
    }
}

/// Which pass produced a verdict. Surfaced so the UI can say how a claim was
/// checked rather than implying every verdict carries the same weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VerificationMethod {
    Lexical,
    Judge,
}

impl VerificationMethod {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Judge => "judge",
        }
    }
}

/// One claim sentence and what was concluded about it.
#[derive(Debug, Clone)]
pub(super) struct ClaimAssessment {
    /// The sentence as written, citation markers included.
    pub(super) sentence: String,
    /// Citation numbers the sentence carries, as the model wrote them.
    pub(super) citation_ids: Vec<u32>,
    pub(super) verdict: ClaimVerdict,
    /// A span from a cited passage, verified to actually appear in it.
    pub(super) evidence_quote: Option<String>,
    pub(super) method: VerificationMethod,
    /// The judge's probability for `verdict`, when the provider reports one.
    pub(super) confidence: Option<f32>,
    /// A short factual comparison supplied by the judge, when available.
    pub(super) reason: Option<String>,
    /// Set exactly when `verdict` is `Unverified`.
    pub(super) unverified_reason: Option<UnverifiedReason>,
}

impl ClaimAssessment {
    fn from_lexical(claim: &LexicalClaim) -> Self {
        Self {
            sentence: claim.sentence.clone(),
            citation_ids: claim.citation_ids.clone(),
            verdict: if claim.supported {
                ClaimVerdict::Supported
            } else {
                ClaimVerdict::Unsupported
            },
            evidence_quote: None,
            method: VerificationMethod::Lexical,
            confidence: None,
            reason: None,
            unverified_reason: None,
        }
    }

    fn mark_unverified(&mut self, reason: UnverifiedReason) {
        self.verdict = ClaimVerdict::Unverified;
        self.unverified_reason = Some(reason);
        self.evidence_quote = None;
        self.reason = None;
    }
}

/// The limits the UI's summary schema (`src/types/conversation.ts`) enforces.
/// It discards the whole summary on any violation, so everything emitted is
/// cut to fit here instead; the counts stay the true totals.
const MAX_EMITTED_VERDICTS: usize = 60;
const MAX_SUPPORTED_NOTES: usize = 40;
const MAX_LISTED_CLAIMS: usize = 20;
const MAX_SENTENCE_UNITS: usize = 2000;
const MAX_QUOTE_UNITS: usize = 1000;
const MAX_CLAIM_CITATIONS: usize = 24;
const CITATION_ID_RANGE: std::ops::RangeInclusive<u32> = 1..=1000;

/// Cut to at most `max` UTF-16 code units — the unit a JavaScript string's
/// length counts — without splitting a character.
fn fit_utf16(text: &str, max: usize) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= max
        })
        .collect()
}

fn fit_sentences(sentences: &[String], max: usize) -> Vec<String> {
    sentences
        .iter()
        .take(max)
        .map(|sentence| fit_utf16(sentence, MAX_SENTENCE_UNITS))
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct GroundingReport {
    pub(super) claims_evaluated: usize,
    pub(super) supported_claims: usize,
    pub(super) supported_claim_notes: Vec<String>,
    /// Everything not supported — contradicted claims included, because the
    /// existing badge counts this list and a contradiction is the worst case.
    pub(super) unsupported_claims: Vec<String>,
    pub(super) contradicted_claims: Vec<String>,
    /// Claims nothing checked. Not in `unsupported_claims`.
    pub(super) unverified_claims: usize,
    pub(super) claims: Vec<ClaimAssessment>,
    pub(super) judge_used: bool,
}

impl GroundingReport {
    fn from_assessments(claims: Vec<ClaimAssessment>, judge_used: bool) -> Self {
        let mut report = Self {
            claims_evaluated: claims.len(),
            judge_used,
            ..Self::default()
        };

        for claim in &claims {
            match claim.verdict {
                ClaimVerdict::Supported => {
                    report.supported_claims += 1;
                    report.supported_claim_notes.push(claim.sentence.clone());
                }
                ClaimVerdict::Contradicted => {
                    report.contradicted_claims.push(claim.sentence.clone());
                    report.unsupported_claims.push(claim.sentence.clone());
                }
                ClaimVerdict::Unsupported => {
                    report.unsupported_claims.push(claim.sentence.clone());
                }
                ClaimVerdict::Unverified => report.unverified_claims += 1,
            }
        }

        report.claims = claims;
        report
    }

    pub(super) fn unsupported_count(&self) -> usize {
        self.unsupported_claims.len()
    }

    pub(super) fn contradicted_count(&self) -> usize {
        self.contradicted_claims.len()
    }

    pub(super) fn judged_claim_count(&self) -> usize {
        self.claims
            .iter()
            .filter(|claim| claim.method == VerificationMethod::Judge)
            .count()
    }

    /// Supported over checked. An unchecked claim is neither grounded nor
    /// ungrounded, so it is left out of both sides; nothing checked is 1.0 in
    /// the same sense that nothing to check is.
    pub(super) fn grounded_ratio(&self) -> f32 {
        let checked = self.claims_evaluated.saturating_sub(self.unverified_claims);
        if checked == 0 {
            return 1.0;
        }
        self.supported_claims as f32 / checked as f32
    }

    /// What the "Checking the answer" step says it found.
    ///
    /// An answer with nothing to check is not an answer that failed its check,
    /// so it says so in those words rather than reporting "0 of 0 backed".
    pub(super) fn result_line(&self) -> String {
        if self.claims_evaluated == 0 {
            return "nothing to check against sources".to_string();
        }
        let backed = format!(
            "{} of {} claims backed",
            self.supported_claims, self.claims_evaluated
        );
        if self.unverified_claims == 0 {
            backed
        } else {
            format!("{backed}, {} not checked", self.unverified_claims)
        }
    }

    /// The metadata blob persisted with the assistant message.
    ///
    /// Additive: every key the UI already reads keeps its name and meaning.
    pub fn metadata_json(&self) -> serde_json::Value {
        let verdicts: Vec<serde_json::Value> = self
            .claims
            .iter()
            .take(MAX_EMITTED_VERDICTS)
            .map(|claim| {
                let citation_ids: Vec<u32> = claim
                    .citation_ids
                    .iter()
                    .copied()
                    .filter(|id| CITATION_ID_RANGE.contains(id))
                    .take(MAX_CLAIM_CITATIONS)
                    .collect();
                serde_json::json!({
                    "sentence": fit_utf16(&claim.sentence, MAX_SENTENCE_UNITS),
                    "citationIds": citation_ids,
                    "verdict": claim.verdict.as_str(),
                    "evidenceQuote": claim
                        .evidence_quote
                        .as_deref()
                        .map(|quote| fit_utf16(quote, MAX_QUOTE_UNITS)),
                    "method": claim.method.as_str(),
                    "confidence": claim.confidence,
                    "reason": claim
                        .reason
                        .as_deref()
                        .map(|reason| fit_utf16(reason, MAX_QUOTE_UNITS)),
                    "unverifiedReason": claim.unverified_reason.map(UnverifiedReason::as_str),
                })
            })
            .collect();

        serde_json::json!({
            "enabled": true,
            "claimsEvaluated": self.claims_evaluated,
            "supportedClaims": self.supported_claims,
            "supportedClaimNotes": fit_sentences(&self.supported_claim_notes, MAX_SUPPORTED_NOTES),
            "unsupportedClaims": fit_sentences(&self.unsupported_claims, MAX_LISTED_CLAIMS),
            "groundedRatio": self.grounded_ratio(),
            "contradictedClaims": fit_sentences(&self.contradicted_claims, MAX_LISTED_CLAIMS),
            "verdictCounts": {
                "supported": self.supported_claims,
                "contradicted": self.contradicted_claims.len(),
                "unsupported": self.claims_evaluated
                    .saturating_sub(self.supported_claims)
                    .saturating_sub(self.contradicted_claims.len())
                    .saturating_sub(self.unverified_claims),
                "unverified": self.unverified_claims,
            },
            "claimVerdicts": verdicts,
            "judgeUsed": self.judge_used,
        })
    }
}

/// Runs the grounding passes for one turn.
pub struct GroundingVerifier {
    judge: Option<ClaimJudge>,
}

impl GroundingVerifier {
    /// The judge is on whenever a handle exists; without one the turn is
    /// verified lexically and says so in the log.
    pub fn new(llm: Option<Arc<dyn LLMPort>>) -> Self {
        match llm {
            Some(llm) => {
                let judge = ClaimJudge::new(llm);
                // Debug, not info: this is the normal path and fires every turn.
                // Only the degraded paths below are worth a line in the log.
                debug!(
                    model = judge.model_name(),
                    "Grounding claim judge enabled for this turn"
                );
                Self { judge: Some(judge) }
            }
            None => {
                info!(
                    "Grounding claim judge skipped: no LLM handle available — claim verdicts are \
                     lexical overlap only and cannot detect a contradiction that reuses the \
                     source's wording"
                );
                Self { judge: None }
            }
        }
    }

    /// Opt out explicitly. Logs, because a silently lexical-only verdict is
    /// indistinguishable from a judged one in the persisted metadata.
    ///
    /// No production caller turns the judge off today — `new(None)` covers the
    /// "no handle" case — but the knob is the one place to route a future
    /// setting through, and tests use it to pin the lexical-only behaviour.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn with_judge_enabled(self, enabled: bool) -> Self {
        if enabled {
            return self;
        }
        if self.judge.is_some() {
            info!(
                "Grounding claim judge skipped: disabled by configuration — claim verdicts are \
                 lexical overlap only"
            );
        }
        Self { judge: None }
    }

    /// Apply the user's verification settings to the judge, if there is one.
    ///
    /// Without this the judge decodes greedily, which is the right default; the
    /// settings exist so someone can trade that determinism away deliberately
    /// rather than inheriting it from whatever the chat model is tuned to.
    pub(super) fn with_tuning(mut self, tuning: &LLMVerificationSettingsDto) -> Self {
        self.judge = self.judge.map(|judge| judge.with_tuning(tuning));
        self
    }

    pub async fn verify(&self, response: &str, sources: &[SourceDto]) -> GroundingReport {
        let claims = lexical::lexical_pass(response, sources);
        if claims.is_empty() {
            return GroundingReport::default();
        }

        let mut assessments: Vec<ClaimAssessment> =
            claims.iter().map(ClaimAssessment::from_lexical).collect();

        let Some(judge) = self.judge.as_ref() else {
            return GroundingReport::from_assessments(assessments, false);
        };

        // What each escalated claim can be judged against. A cited page with
        // no text is unverified whatever the lexical pass made of it: there
        // was nothing to find the claim in, so "not found" would be false.
        let mut requests = Vec::new();
        for (index, claim) in claims.iter().enumerate() {
            if !lexical::needs_judge(claim) {
                continue;
            }
            match evidence_for(claim, sources) {
                Evidence::Found(evidence) => {
                    requests.push((index, claim.claim_text.as_str(), evidence));
                }
                Evidence::NoText => {
                    if let Some(assessment) = assessments.get_mut(index) {
                        assessment.mark_unverified(UnverifiedReason::NoText);
                    }
                }
                Evidence::NoCitation => {}
            }
        }
        if requests.is_empty() {
            return GroundingReport::from_assessments(assessments, false);
        }

        let judgments = judge.judge_claims(&requests).await;
        for (index, judgment) in judgments {
            let Some(assessment) = assessments.get_mut(index) else {
                continue;
            };
            let unreached = match judgment {
                ClaimJudgment::Judged(outcome) => {
                    assessment.confidence = outcome.confidence;
                    assessment.method = VerificationMethod::Judge;
                    if outcome
                        .confidence
                        .is_some_and(|confidence| confidence < MIN_VERDICT_CONFIDENCE)
                    {
                        let confidence = outcome.confidence.unwrap_or_default();
                        assessment.mark_unverified(UnverifiedReason::LowConfidence);
                        // `mark_unverified` keeps the lexical method for
                        // failed calls; this one did run, it simply did not
                        // earn the right to present a red/green finding.
                        assessment.method = VerificationMethod::Judge;
                        assessment.reason = Some(format!(
                            "The checker was only {:.0}% sure, so this sentence was not marked as a finding.",
                            confidence * 100.0
                        ));
                        continue;
                    }
                    assessment.verdict = outcome.verdict;
                    assessment.evidence_quote = outcome.quote;
                    assessment.reason = outcome.reason;
                    continue;
                }
                ClaimJudgment::OutOfTime => UnverifiedReason::Budget,
                ClaimJudgment::Unusable | ClaimJudgment::Failed(_) => UnverifiedReason::JudgeFailed,
            };
            // Unreached: the lexical verdict stands where overlap can speak
            // for the claim. Where it cannot — a number, a date, a negation —
            // a match certifies nothing, and a miss is not a finding either.
            if claims
                .get(index)
                .is_some_and(|claim| claim.contradiction_prone)
            {
                assessment.mark_unverified(unreached);
            }
        }

        let judge_used = assessments
            .iter()
            .any(|claim| claim.method == VerificationMethod::Judge);
        GroundingReport::from_assessments(assessments, judge_used)
    }
}

/// Lexical-only grounding summary.
///
/// The synchronous path, kept for callers that have no LLM handle to offer.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn verify_response_grounding_summary(
    response: &str,
    sources: &[SourceDto],
) -> GroundingReport {
    GroundingReport::from_assessments(
        lexical::lexical_pass(response, sources)
            .iter()
            .map(ClaimAssessment::from_lexical)
            .collect(),
        false,
    )
}

#[cfg(test)]
pub(super) mod test_support {
    use crate::features::qa::dto::SourceDto;

    pub(in crate::features::conversation::chat::verification) fn source(
        content: &str,
    ) -> SourceDto {
        SourceDto {
            page_number: None,
            from_ocr: false,
            document_id: "doc-1".to_string(),
            chunk_id: "chunk-1".to_string(),
            content: content.to_string(),
            score: 1.0,
            path: None,
            position: None,
            file_name: "doc.txt".to_string(),
            file_path: "/tmp/doc.txt".to_string(),
            mime_type: "text/plain".to_string(),
            category: "Text File".to_string(),
            file_size_bytes: 100,
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            chunk_excerpts: None,
            citation_id: None,

            web_snapshot: None,
        }
    }
}

#[cfg(test)]
mod tests;
