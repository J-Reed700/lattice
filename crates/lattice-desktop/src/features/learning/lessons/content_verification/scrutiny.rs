//! Scrutiny (source provenance §11): what research does for a claim the
//! checker supported but whose support the provenance gate found
//! insufficient, or that a contrary passage disputes. It replaces the
//! evidence challenge and never decides a verdict: it turns the gate's
//! next steps into research — citations to follow (§8, followed
//! automatically by decision 3), searches on the hosts where the subject's
//! owner publishes, a search for an independent source or for newer
//! material — and the recheck that follows decides. The research planner is
//! its one model call. A search for an independent source keeps out the
//! origins the claim's supports already come from in code, not only by
//! asking the planner.
//! Design: docs/design/2026-10-18-source-provenance-and-trust.md §6.2, §8, §11.

use super::provenance_gate::FindingTrust;
use super::*;
use lattice_core::domain::provenance::{effective_date, NextStep, Origin};
use lattice_core::shared::http::url_identity::identity;

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
    /// Planner gaps, one per scrutinised claim (`web_research::plan_needs`).
    pub(super) gaps: Vec<Value>,
    /// Citations to capture before any search, best first, each page once
    /// (links to places in one page are that page).
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
    pub(super) fn scrutinised(&self) -> Option<&FindingTrust> {
        self.trust
            .as_deref()
            .filter(|trust| self.verdict == ClaimVerdict::Supported && trust.needs_scrutiny())
    }
}

/// The sites (registrable domain, or host when there is none) the saved
/// `sources` behind `trust`'s supports were published on, each once. A file
/// or pasted text has none: nothing on the web shares its origin.
fn support_sites(trust: &FindingTrust, sources: &[LearningSourceDto]) -> Vec<String> {
    let mut sites: Vec<String> = Vec::new();
    for support in &trust.supports {
        let site = sources
            .iter()
            .find(|source| source.id == support.source_version_id)
            .and_then(|source| source.url.as_deref())
            .and_then(|url| Origin::of_url(url).ok())
            .and_then(|origin| origin.site().map(str::to_owned));
        if let Some(site) = site.filter(|site| !sites.contains(site)) {
            sites.push(site);
        }
    }
    sites
}

/// The sites a search serving `gap` must not return results from: those of
/// the claim's current supports, when the gap asks for an independent
/// source.
pub(super) fn excluded_sites(gap: &Value) -> Vec<&str> {
    if gap.get("needsIndependentSource").and_then(Value::as_bool) != Some(true) {
        return Vec::new();
    }
    gap.get("independentOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// Whether `url` is published on one of `sites`. An archived copy is judged
/// as the page it archives.
pub(super) fn published_on(url: &str, sites: &[&str]) -> bool {
    !sites.is_empty()
        && Origin::of_url(url)
            .ok()
            .and_then(|origin| origin.site().map(|site| sites.contains(&site)))
            .unwrap_or(false)
}

/// The research `findings` ask for beyond "find evidence": only findings
/// whose support the gate found insufficient or disputed are scrutinised.
/// `sources` are the saved versions the supports name, so a request for an
/// independent source can say which origins it must avoid.
pub(super) fn review(findings: &[Finding], sources: &[LearningSourceDto]) -> Scrutiny {
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
                    let page = identity(url);
                    if !scrutiny
                        .follow
                        .iter()
                        .any(|follow| identity(&follow.url) == page)
                    {
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
            let sites = support_sites(trust, sources);
            if !sites.is_empty() {
                gap.insert("independentOf".into(), json!(sites));
            }
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
