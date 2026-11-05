//! The source check: what a source passage says of one sentence. `ClaimSourceCheck` is an Adapter over the claim
//! checker in its strict policy, with deterministic sampling: a verdict that rests on the passage comes with an exact
//! quote of it, and a check that gives no verdict is an error, never a finding.
//!
//! One sentence goes to each call. The batch check locates its evidence by passage identifier, so its quote would be
//! the whole passage rather than the words a verdict rests on.

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::SamplingOverride;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::claim_verification::{
    CheckPolicy, ClaimChecker, ClaimEvidence, ClaimJudgment, ClaimVerdict, ANSWER_ROOM_TOKENS,
};
use lattice_core::shared::error::{AppError, Result};

/// What a source passage says of a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features::learning) enum SourceSays {
    Supported,
    Contradicted,
    Unsupported,
}

impl SourceSays {
    /// The verdict as a sentence says it.
    pub(in crate::features::learning) fn says(self) -> &'static str {
        match self {
            Self::Supported => "supports",
            Self::Contradicted => "contradicts",
            Self::Unsupported => "does not support",
        }
    }
}

/// What the check found for one sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::features::learning) struct SourceFinding {
    pub says: SourceSays,
    /// The exact words of the passage a supported or contradicted verdict rests on.
    pub quote: Option<String>,
    /// The checker's comparison of the sentence with the passage.
    pub reason: String,
}

/// Checks a sentence against a source passage.
#[async_trait]
pub(in crate::features::learning) trait SourceCheck:
    Send + Sync
{
    /// What `passage` says of `sentence`. A check that came to no verdict is an error.
    async fn check(&self, sentence: &str, passage: &str) -> Result<SourceFinding>;

    /// The model that checks, as the server names it.
    fn model_name(&self) -> &str;
}

/// The claim checker on `llm`, in its strict policy.
pub(in crate::features::learning) struct ClaimSourceCheck<'a> {
    llm: &'a dyn LLMPort,
}

impl<'a> ClaimSourceCheck<'a> {
    pub(in crate::features::learning) fn new(llm: &'a dyn LLMPort) -> Self {
        Self { llm }
    }
}

#[async_trait]
impl SourceCheck for ClaimSourceCheck<'_> {
    async fn check(&self, sentence: &str, passage: &str) -> Result<SourceFinding> {
        let checker = ClaimChecker::new(
            self.llm,
            SamplingOverride::deterministic(),
            u32::try_from(ANSWER_ROOM_TOKENS).unwrap_or(u32::MAX),
            CheckPolicy::Strict,
        );
        let evidence = ClaimEvidence {
            text: passage.to_owned(),
            quote: None,
        };
        finding_of(checker.check(sentence, &evidence).await)
    }

    fn model_name(&self) -> &str {
        self.llm.model_name()
    }
}

/// The finding a judgment comes to. A judgment the checker could not make, or a call that failed, is an error.
fn finding_of(judgment: ClaimJudgment) -> Result<SourceFinding> {
    let outcome = match judgment {
        ClaimJudgment::Judged(outcome) => outcome,
        ClaimJudgment::Failed(error) => return Err(error),
        ClaimJudgment::Unusable => {
            return Err(AppError::ModelAnswerUnusable(
                "The source check gave no verdict that could be used.".into(),
            ))
        }
    };
    let says = match outcome.verdict {
        ClaimVerdict::Supported => SourceSays::Supported,
        ClaimVerdict::Contradicted => SourceSays::Contradicted,
        ClaimVerdict::Unsupported => SourceSays::Unsupported,
        ClaimVerdict::Unverified => {
            return Err(AppError::ModelAnswerUnusable(
                "The source check left the sentence unverified.".into(),
            ))
        }
    };
    Ok(SourceFinding {
        says,
        quote: outcome.quote,
        reason: outcome.reason.unwrap_or_default(),
    })
}
