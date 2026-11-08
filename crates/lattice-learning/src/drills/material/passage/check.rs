//! The source check: what a source passage says of one sentence. `ClaimSourceCheck` is an Adapter over the claim
//! checker in its strict policy, with deterministic sampling. The checker is shown the passage's sentences as numbered
//! passages and cites the one a verdict rests on, so the quote is that sentence, the source's own bytes, however the
//! source is wrapped or spaced. A finding says whether the checker was sure of it; a check that gives no usable
//! verdict is a finding of nothing, unsure, and only a call that fails is an error.
//!
//! One sentence goes to each call. The batch check shares one bank of passages among its claims and needs a context
//! sized for all of them; one claim a call keeps each check whole.

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::SamplingOverride;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::claim_verification::{
    CheckPolicy, ClaimChecker, ClaimJudgment, ClaimVerdict, ANSWER_ROOM_TOKENS,
    MIN_VERDICT_CONFIDENCE,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::text::segmentation::sentences;

/// Why a finding of nothing says nothing.
const NO_VERDICT: &str = "The source check gave no verdict that could be used.";

/// What a source passage says of a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceSays {
    Supported,
    Contradicted,
    Unsupported,
}

impl SourceSays {
    /// The verdict as a sentence says it.
    pub(crate) fn says(self) -> &'static str {
        match self {
            Self::Supported => "supports",
            Self::Contradicted => "contradicts",
            Self::Unsupported => "does not support",
        }
    }
}

/// What the check found for one sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFinding {
    pub says: SourceSays,
    /// The exact words of the passage a supported or contradicted verdict rests on.
    pub quote: Option<String>,
    /// The checker's comparison of the sentence with the passage.
    pub reason: String,
    /// Whether the checker was sure of `says`. A finding it was unsure of rests on nothing: it proves no error and
    /// decides no fix.
    pub confident: bool,
    /// The checker gave no verdict that could be used: `says` is then `Unsupported` and `confident` false.
    pub unusable: bool,
}

impl SourceFinding {
    /// Whether the checker was sure the source says `says`.
    pub(crate) fn surely(&self, says: SourceSays) -> bool {
        self.confident && self.says == says
    }
}

/// Checks a sentence against a source passage.
#[async_trait]
pub trait SourceCheck: Send + Sync {
    /// What `passage` says of `sentence`. A check that came to no usable verdict is an unusable finding; an error is
    /// a call that failed.
    async fn check(&self, sentence: &str, passage: &str) -> Result<SourceFinding>;

    /// The model that checks, as the server names it.
    fn model_name(&self) -> &str;
}

/// The claim checker on `llm`, in its strict policy.
pub struct ClaimSourceCheck<'a> {
    llm: &'a dyn LLMPort,
}

impl<'a> ClaimSourceCheck<'a> {
    pub fn new(llm: &'a dyn LLMPort) -> Self {
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
        let passages = source_sentences(passage);
        finding_of(
            checker
                .check_passages_without_deadline(sentence, &passages)
                .await,
        )
    }

    fn model_name(&self) -> &str {
        self.llm.model_name()
    }
}

/// The sentences of `passage`, each the exact bytes of the passage it spans. A line break with text on both sides,
/// as a hard-wrapped source has, does not end a sentence; a blank line does. A passage with no sentence is one.
fn source_sentences(passage: &str) -> Vec<String> {
    let flowed = flowed(passage);
    let found: Vec<String> = sentences(&flowed)
        .into_iter()
        .filter_map(|(start, end)| passage.get(start..end))
        .map(str::to_owned)
        .collect();
    if found.is_empty() {
        vec![passage.to_owned()]
    } else {
        found
    }
}

/// `text` with each line break inside a paragraph, and the carriage return before it, turned into a space: the same
/// number of bytes in the same places, so a span of it is a span of `text`.
fn flowed(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut flowed = String::with_capacity(text.len());
    for (index, line) in lines.iter().enumerate() {
        let Some(next) = lines.get(index + 1) else {
            flowed.push_str(line);
            break;
        };
        if line.trim().is_empty() || next.trim().is_empty() {
            flowed.push_str(line);
            flowed.push('\n');
            continue;
        }
        match line.strip_suffix('\r') {
            Some(bare) => {
                flowed.push_str(bare);
                flowed.push(' ');
            }
            None => flowed.push_str(line),
        }
        flowed.push(' ');
    }
    flowed
}

/// What a check that gave no usable verdict comes to.
fn nothing() -> SourceFinding {
    SourceFinding {
        says: SourceSays::Unsupported,
        quote: None,
        reason: NO_VERDICT.to_owned(),
        confident: false,
        unusable: true,
    }
}

/// The finding a judgment comes to. A judgment the checker could not make is a finding of nothing; only a call that
/// failed is an error.
pub(crate) fn finding_of(judgment: ClaimJudgment) -> Result<SourceFinding> {
    let outcome = match judgment {
        ClaimJudgment::Judged(outcome) => outcome,
        ClaimJudgment::Failed(error) => return Err(error),
        ClaimJudgment::Unusable => return Ok(nothing()),
    };
    let says = match outcome.verdict {
        ClaimVerdict::Supported => SourceSays::Supported,
        ClaimVerdict::Contradicted => SourceSays::Contradicted,
        ClaimVerdict::Unsupported => SourceSays::Unsupported,
        ClaimVerdict::Unverified => return Ok(nothing()),
    };
    let confident = !outcome.uncertain
        && outcome.confidence.is_none_or(|confidence| {
            confidence.is_finite() && confidence >= MIN_VERDICT_CONFIDENCE
        });
    Ok(SourceFinding {
        says,
        quote: outcome.quote,
        reason: outcome.reason.unwrap_or_default(),
        confident,
        unusable: false,
    })
}
