use super::*;

/// What a reading made of a statement, and the finding it leaves. Made only by
/// [`Read::new`]: the outcome decides the finding's verdict, so the two never disagree.
#[derive(Debug)]
pub(in crate::lessons::content_verification) struct Read {
    outcome: Outcome,
    finding: Finding,
    reused: bool,
}

impl Read {
    /// The reading `outcome` makes of `finding`: a statement that stands is supported,
    /// an unjudged one unverified, any other contradicted when the judge said so, else
    /// unsupported. Only a supported or contradicted finding keeps its quotation, and
    /// only support found elsewhere keeps `supported_by`. `reused`: no call was made.
    pub(in crate::lessons::content_verification) fn new(
        outcome: Outcome,
        mut finding: Finding,
        reused: bool,
    ) -> Self {
        finding.verdict = match outcome {
            Outcome::Stands | Outcome::StandsElsewhere => ClaimVerdict::Supported,
            Outcome::Unjudged => ClaimVerdict::Unverified,
            _ if finding.verdict == ClaimVerdict::Contradicted => ClaimVerdict::Contradicted,
            _ => ClaimVerdict::Unsupported,
        };
        if !matches!(
            finding.verdict,
            ClaimVerdict::Supported | ClaimVerdict::Contradicted
        ) {
            finding.supporting_quote = None;
        }
        if outcome != Outcome::StandsElsewhere {
            finding.two_pass.supported_by = None;
        }
        Self {
            outcome,
            finding,
            reused,
        }
    }

    pub(in crate::lessons::content_verification) fn outcome(&self) -> Outcome {
        self.outcome
    }

    pub(in crate::lessons::content_verification) fn finding(&self) -> &Finding {
        &self.finding
    }

    pub(in crate::lessons::content_verification) fn reused(&self) -> bool {
        self.reused
    }

    pub(in crate::lessons::content_verification) fn into_parts(self) -> (Outcome, Finding, bool) {
        (self.outcome, self.finding, self.reused)
    }
}
