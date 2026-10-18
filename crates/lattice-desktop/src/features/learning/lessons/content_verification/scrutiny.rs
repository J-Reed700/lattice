//! Scrutiny (source provenance §11): what research does for a claim the
//! checker supported but whose support the provenance gate found
//! insufficient, or that a contrary passage disputes. It replaces the
//! evidence challenge and never decides a verdict: it turns the gate's
//! next steps into research — citations to follow (§8, followed
//! automatically by decision 3), searches on the hosts where the subject's
//! owner publishes, a search for an independent source or for newer
//! material — and the recheck that follows decides. The research planner is
//! its one model call.
//! Design: docs/design/2026-10-18-source-provenance-and-trust.md §6.2, §8, §11.

use super::provenance_gate::FindingTrust;
use super::*;
use lattice_core::domain::provenance::{effective_date, NextStep};

/// A cited page to fetch for a claim, with the supporting version that cites
/// it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Follow {
    pub(super) url: String,
    pub(super) from: String,
}

/// The research a set of findings asks for.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Scrutiny {
    /// Planner gaps, one per scrutinised claim (`web_research::plan_queries`).
    pub(super) gaps: Vec<Value>,
    /// Citations to capture before any search, best first, each once.
    pub(super) follow: Vec<Follow>,
}

impl Finding {
    /// Whether research should look for evidence for this finding: the
    /// checker found none or found it contradicted, or the gate found its
    /// support insufficient or disputed.
    pub(super) fn wants_research(&self) -> bool {
        matches!(
            self.verdict,
            ClaimVerdict::Unsupported | ClaimVerdict::Contradicted
        ) || self.scrutinised().is_some()
    }

    /// The gate's account of a supported finding that needs scrutiny.
    fn scrutinised(&self) -> Option<&FindingTrust> {
        self.trust
            .as_deref()
            .filter(|trust| self.verdict == ClaimVerdict::Supported && trust.needs_scrutiny())
    }
}

/// The research `findings` ask for beyond "find evidence": only findings
/// whose support the gate found insufficient or disputed are scrutinised.
pub(super) fn review(findings: &[Finding]) -> Scrutiny {
    let mut scrutiny = Scrutiny::default();
    for (finding, trust) in findings
        .iter()
        .filter_map(|finding| finding.scrutinised().map(|trust| (finding, trust)))
    {
        let steps = match &trust.decision {
            lattice_core::domain::provenance::Decision::Insufficient { next, .. } => {
                next.as_slice()
            }
            lattice_core::domain::provenance::Decision::Sufficient { .. } => &[],
        };
        let mut owner_hints: Vec<String> = Vec::new();
        let mut independent = false;
        let mut newer = false;
        for step in steps {
            match step {
                NextStep::FollowCitation { url, from } => {
                    if !scrutiny.follow.iter().any(|follow| &follow.url == url) {
                        scrutiny.follow.push(Follow {
                            url: url.clone(),
                            from: from.clone(),
                        });
                    }
                }
                NextStep::SearchOwner { origin_hints } => {
                    for hint in origin_hints {
                        if !owner_hints.contains(hint) {
                            owner_hints.push(hint.clone());
                        }
                    }
                }
                NextStep::SearchIndependent => independent = true,
                NextStep::FetchNewer { .. } => newer = true,
                NextStep::AskUser => {}
            }
        }
        // A contrary passage is checked at the owner's account of the point.
        if !trust.contrary.is_empty() {
            for hint in trust
                .supports
                .iter()
                .flat_map(|support| &support.owner_hints)
            {
                if !owner_hints.contains(hint) {
                    owner_hints.push(hint.clone());
                }
            }
        }
        let newest_support = trust
            .supports
            .iter()
            .filter_map(|support| effective_date(&support.dates).at)
            .max()
            .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
            .map(|at| at.format("%Y-%m-%d").to_string());
        let mut gap = serde_json::Map::new();
        gap.insert("claim".into(), json!(finding.statement));
        gap.insert("missingEvidence".into(), json!(trust.explain()));
        gap.insert("kind".into(), json!(trust.profile.kind.as_str()));
        if let Some(subject) = &trust.profile.subject {
            gap.insert("subject".into(), json!(subject.name));
        }
        if let Some(origin) = &trust.profile.measurement_origin {
            gap.insert("measuredBy".into(), json!(origin));
        }
        if let Some(attributed) = &trust.profile.attributed_to {
            gap.insert("attributedTo".into(), json!(attributed));
        }
        if !owner_hints.is_empty() {
            gap.insert("ownerOriginHints".into(), json!(owner_hints));
        }
        if independent {
            gap.insert("needsIndependentSource".into(), json!(true));
        }
        if newer {
            gap.insert("needsNewerThan".into(), json!(newest_support));
        }
        scrutiny.gaps.push(Value::Object(gap));
    }
    scrutiny
}

#[cfg(test)]
#[path = "scrutiny_tests.rs"]
mod tests;
