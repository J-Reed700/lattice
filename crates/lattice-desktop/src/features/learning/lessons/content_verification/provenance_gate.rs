//! The provenance gate: whether a supported claim's support is enough for
//! its kind (source provenance §6.2) and the one rule that may take a
//! supported verdict away (§11).
//!
//! After the checker supports a claim, [`weigh`] levels each supporting
//! source for the claim's subject from cached judgements (standing,
//! ownership, the claim's profile) and asks lattice-core's `decide` whether
//! those independent supports meet the bar for the claim's kind. The result,
//! [`FindingTrust`], travels with the finding; [`ProvenanceGate`] turns it
//! into the claim ledger's sufficiency through `overturn`, so the checker's
//! verdict is never overturned by doubt: only by contrary evidence that
//! outranks the support, or by support that fails its bar. A support that
//! fails becomes an unsupported claim with the gate's reason, and research
//! follows its next steps ([`super::scrutiny`]) before repair or the user.
//!
//! The gate never calls the model to decide: the decision is a pure function
//! of judgements that are cached once per source and claim.
//! Design: docs/design/2026-10-18-source-provenance-and-trust.md §6, §10, §11.

use super::support_gate::{Sufficiency, Support as Supported, SupportGate};
use super::*;
use lattice_core::domain::learning::claim_review::{
    GateVerdict, SupportedBy, TrustAssessment, VerdictKind,
};
use lattice_core::domain::provenance::TRUST_POLICY_VERSION;
use lattice_core::domain::provenance::{
    citation_candidates, clusters, decide, effective_date, passage_merges, AddedBy, Available,
    ClaimProfile, ClusterId, Contrary, Decision, EffectiveBars, LinkStanding, Missing, OriginKey,
    OutboundLink, Overturn, PassageKind, Policy, Reason, SourceDates, SourceRelation, Support,
    TemporalScope, Trust, TrustLevel,
};
use std::sync::Arc;

mod context;
mod relevel;
mod strongest;
pub(super) use context::passage_labels;
pub(in crate::features::learning) use context::Judgements;
pub(in crate::features::learning) use context::{outline_trust, with_outline_trust, with_trust};
pub(in crate::features::learning) use relevel::relevel;
#[cfg(test)]
pub(super) use strongest::name_strongest;

#[cfg(test)]
#[path = "provenance_gate_tests.rs"]
mod tests;

/// The retrieval kind of a passage that is a code example's run, not a
/// source: it has no provenance and is never weighed.
const EXECUTION: &str = "execution";

/// What the gate weighed for one supported finding: the claim's profile,
/// each supporting source levelled for the claim's subject, the contrary
/// passages, and the decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::features::learning) struct FindingTrust {
    pub(super) profile: ClaimProfile,
    pub(super) supports: Vec<Support>,
    /// Contrary passages for the same claim, levelled the same way: what one
    /// of the claim's two passes found saying otherwise when the other
    /// supported it (course evidence index §5.1). The overturn rule weighs
    /// them against the supports.
    #[serde(default)]
    pub(super) contrary: Vec<Contrary>,
    pub(super) decision: Decision,
    /// What the course has on the claim's subject, when the standing reach
    /// weighed the claim (claim sources design §6.1, §7.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) available: Option<Available>,
}

impl FindingTrust {
    /// Whether research should look further for this claim (§11: scrutiny
    /// runs only on an insufficient decision or a contrary).
    pub(super) fn needs_scrutiny(&self) -> bool {
        matches!(self.decision, Decision::Insufficient { .. }) || !self.contrary.is_empty()
    }

    /// The claim's reason when its support is not enough: what it lacks,
    /// then the rules that fired, verbatim (the first names the claim's kind,
    /// its subject and the bar).
    pub(super) fn explain(&self) -> String {
        explain(&self.decision)
    }

    /// The claim ledger's record of this weighing, for the supported
    /// verdict it decided (`source_version_id` holds `supporting_quote`).
    pub(super) fn assessment(
        &self,
        source_version_id: &str,
        supporting_quote: &str,
    ) -> TrustAssessment {
        TrustAssessment {
            profile: self.profile.clone(),
            supports: self.supports.clone(),
            contrary: self.contrary.clone(),
            decision: self.decision.clone(),
            supported: SupportedBy {
                source_version_id: source_version_id.to_owned(),
                supporting_quote: supporting_quote.to_owned(),
            },
            policy: TRUST_POLICY_VERSION.to_owned(),
            available: self.available,
        }
    }
}

/// The claim's reason for what the gate made of its support: the contrary
/// that outranks it, or what the support lacks, then the rules that fired,
/// verbatim (the first names the claim's kind, its subject and the bar).
pub(in crate::features::learning) fn gate_reason(trust: &TrustAssessment) -> String {
    match trust.overturn() {
        Overturn::Contradicted { by, reasons } | Overturn::Escalated { by, reasons } => {
            format!(
                "A passage of {by} says otherwise.{}",
                reason_lines(&reasons)
            )
        }
        Overturn::None | Overturn::InsufficientStanding { .. } => explain(&trust.decision),
    }
}

fn explain(decision: &Decision) -> String {
    match decision {
        Decision::Sufficient { reasons, .. } => {
            format!(
                "Its support meets the bar for its kind.{}",
                reason_lines(reasons)
            )
        }
        Decision::Insufficient {
            missing, reasons, ..
        } => format!("Its support {}.{}", lacks(missing), reason_lines(reasons)),
    }
}

fn lacks(missing: &Missing) -> String {
    match missing {
        Missing::NoPrimaryForSubject => "does not come from the subject's owner".into(),
        Missing::NoMeasurementOrigin => "does not come from whoever measured it".into(),
        Missing::NoIndependentCorroboration { have } => format!(
            "comes from {have} independent origin{} and no source alone meets the bar",
            if *have == 1 { "" } else { "s" }
        ),
        Missing::NoIdentifiedSource => {
            "comes from sources whose credibility has not been established".into()
        }
        Missing::OnlyTestimonial => "only reports what another source says".into(),
        Missing::OnlyStale { newer_primary } => {
            format!("is older than the newest primary source ({newer_primary})")
        }
        Missing::StrongerSourceSilent { level } => format!(
            "is below a {} source the course has on this subject, which does not state it",
            level.as_str()
        ),
    }
}

fn reason_lines(reasons: &[Reason]) -> String {
    reasons
        .iter()
        .map(|reason| format!(" {}.", reason.text.trim_end_matches('.')))
        .collect()
}

/// The claim ledger's gate (CR §4 "P4 seam"): a supported verdict becomes
/// verified only when the quotation it rests on is in the passage it names
/// and the overturn rule leaves it standing.
pub(super) struct ProvenanceGate;

impl SupportGate for ProvenanceGate {
    fn judge(&self, support: &Supported<'_>) -> Sufficiency {
        let quoted = support.supporting_quote.is_empty()
            || support.passages.iter().any(|passage| {
                passage.source_id == support.source_version_id
                    && passage.text.contains(support.supporting_quote)
            });
        if !quoted {
            return Sufficiency::Insufficient {
                reason: format!(
                    "The checker supported \"{}\" with a quotation that is not in the passage it names.",
                    support.claim
                ),
                kind: None,
            };
        }
        // A finding the gate did not weigh: a quotation settled by its match,
        // a claim the user resolved with evidence, or a check outside a
        // preparation job, where no provenance is stored.
        let Some(trust) = support.trust else {
            return Sufficiency::Sufficient;
        };
        let assessed = trust.assessment(support.source_version_id, support.supporting_quote);
        match assessed.verdict() {
            GateVerdict::Sufficient => Sufficiency::Sufficient,
            GateVerdict::Fails(kind @ VerdictKind::InsufficientStanding { .. }) => {
                Sufficiency::Insufficient {
                    reason: gate_reason(&assessed),
                    kind: Some(kind),
                }
            }
            GateVerdict::Fails(_) => Sufficiency::Contradicted {
                reason: gate_reason(&assessed),
            },
        }
    }
}

/// A source version's saved text and the links in it (their byte offsets
/// index this text), shared by every claim it supports.
#[derive(Debug)]
pub(super) struct SavedText {
    pub(super) text: String,
    pub(super) links: Vec<OutboundLink>,
}

/// What the gate knows of one source version for one claim's subject.
#[derive(Debug, Clone)]
pub(super) struct VersionFacts {
    pub(super) origin: OriginKey,
    /// The version's level for the claim's subject, with its reasons.
    pub(super) trust: Trust,
    pub(super) dates: SourceDates,
    pub(super) added_by: AddedBy,
    pub(super) saved: Arc<SavedText>,
    /// Hosts where the subject's owner is believed to publish (§12.2).
    pub(super) owner_hints: Vec<String>,
}

/// Everything [`assess`] reads besides the finding: the facts of each
/// supporting version for the claim's subject, the course's relations and
/// the bars in force.
pub(super) struct Evidence<'a> {
    pub(super) facts: &'a HashMap<String, VersionFacts>,
    pub(super) relations: &'a [SourceRelation],
    pub(super) bars: &'a EffectiveBars,
    pub(super) policy: &'a Policy,
    /// What the course has on the claim's subject, once the standing reach
    /// has weighed it; `None` before.
    pub(super) available: Option<Available>,
}

/// The passages that support `finding`: one per source version, first those
/// holding the checker's supporting quotation, in evidence order, then the
/// other supports it carries (`also_supported`, claim sources design §8).
pub(super) fn supporting_passages(finding: &Finding) -> Vec<&EvidencePassage> {
    let quote = finding.supporting_quote.as_deref().unwrap_or_default();
    let mut seen = HashSet::new();
    let quoted = finding.evidence.iter().filter(|passage| {
        !quote.is_empty() && passage.retrieval_kind != EXECUTION && passage.text.contains(quote)
    });
    let others = finding
        .two_pass
        .also_supported
        .iter()
        .map(|other| &other.passage);
    quoted
        .chain(others)
        .filter(|passage| seen.insert(passage.source_id.as_str()))
        .collect()
}

/// Weighs a supported finding's support (§6.2): each supporting source is
/// levelled for the claim's subject, dated, read for whether it states the
/// fact or reports someone else's, and clustered by independent origin;
/// `decide` then compares the clusters with the bar for the claim's kind.
/// A supporting version without facts is not counted.
pub(super) fn assess(
    profile: &ClaimProfile,
    finding: &Finding,
    evidence: &Evidence<'_>,
) -> FindingTrust {
    let passages: Vec<(&EvidencePassage, &VersionFacts)> = supporting_passages(finding)
        .into_iter()
        .filter_map(|passage| {
            evidence
                .facts
                .get(&passage.source_id)
                .map(|facts| (passage, facts))
        })
        .collect();
    let ids: Vec<&str> = passages
        .iter()
        .map(|(passage, _)| passage.source_id.as_str())
        .collect();
    let origins: HashMap<String, OriginKey> = passages
        .iter()
        .map(|(passage, facts)| (passage.source_id.clone(), facts.origin.clone()))
        .collect();
    let texts: Vec<(&str, &str)> = passages
        .iter()
        .map(|(passage, _)| (passage.source_id.as_str(), passage.text.as_str()))
        .collect();
    let merges = passage_merges(&texts, evidence.policy);
    let clustered = clusters(&ids, evidence.relations, &origins, &merges);
    let cluster_of = |id: &str| {
        clustered
            .iter()
            .find(|cluster| cluster.members.iter().any(|member| member == id))
            .map_or_else(|| ClusterId(id.to_owned()), |cluster| cluster.id.clone())
    };
    // The origins clustered with a version publish what it publishes: a
    // link into one of them is not a citation (§8).
    let publisher = |id: &str| -> Vec<OriginKey> {
        clustered
            .iter()
            .find(|cluster| cluster.members.iter().any(|member| member == id))
            .map_or_else(|| vec![id.to_owned()], |cluster| cluster.members.clone())
            .iter()
            .filter_map(|member| origins.get(member).cloned())
            .collect()
    };
    // Staleness is relative to the newest primary support the claim has.
    let newest_primary = passages
        .iter()
        .filter(|(_, facts)| facts.trust.level == TrustLevel::Primary)
        .filter_map(|(passage, facts)| {
            effective_date(&facts.dates)
                .at
                .map(|at| (passage.source_id.as_str(), at))
        })
        .max_by_key(|(_, at)| *at);
    let supports = passages
        .iter()
        .map(|(passage, facts)| {
            let mentions_version = match &profile.temporal {
                TemporalScope::Since {
                    version: Some(version),
                    ..
                } => facts.saved.text.contains(version.as_str()),
                _ => false,
            };
            Support {
                source_version_id: passage.source_id.clone(),
                origin_cluster: cluster_of(&passage.source_id),
                trust: facts.trust.clone(),
                dates: facts.dates.clone(),
                passage: passage_kind(passage, facts),
                temporal: lattice_core::domain::provenance::validity(
                    &profile.temporal,
                    &facts.dates,
                    mentions_version,
                    newest_primary,
                ),
                added_by: facts.added_by,
                cited_urls: cited_urls(passage, facts, &publisher(&passage.source_id)),
                owner_hints: facts.owner_hints.clone(),
            }
        })
        .collect::<Vec<_>>();
    // What the other pass found saying otherwise, levelled the same way. A
    // contrary version without facts is not counted, as a support is not.
    let contrary: Vec<Contrary> = finding
        .two_pass
        .contrary
        .iter()
        .filter_map(|contrary| {
            let facts = evidence.facts.get(&contrary.passage.source_id)?;
            let mentions_version = match &profile.temporal {
                TemporalScope::Since {
                    version: Some(version),
                    ..
                } => facts.saved.text.contains(version.as_str()),
                _ => false,
            };
            Some(Contrary {
                source_version_id: contrary.passage.source_id.clone(),
                origin_cluster: cluster_of(&contrary.passage.source_id),
                trust: facts.trust.clone(),
                dates: facts.dates.clone(),
                temporal: lattice_core::domain::provenance::validity(
                    &profile.temporal,
                    &facts.dates,
                    mentions_version,
                    newest_primary,
                ),
                quote: contrary.quote.clone(),
            })
        })
        .collect();
    let decision = decide(
        profile,
        &supports,
        &contrary,
        evidence.bars,
        &evidence.available.unwrap_or_default(),
    );
    FindingTrust {
        profile: profile.clone(),
        supports,
        contrary,
        decision,
        available: evidence.available,
    }
}

/// How the supporting passage bears on the claim (§6.1 "Direct only"): a
/// passage that links to where the subject's owner publishes relays the
/// owner's account (`Testimonial`), which counts at Community until the
/// citation is followed (§8); otherwise the source states it directly. The
/// checker cites a whole passage, not a span, so a link anywhere in it
/// counts; a link to any other site is a reference, not an attribution.
fn passage_kind(passage: &EvidencePassage, facts: &VersionFacts) -> PassageKind {
    facts
        .saved
        .links
        .iter()
        .filter(|link| link.start_byte >= passage.start_byte && link.end_byte <= passage.end_byte)
        .filter(|link| owner_hinted(&link.url, &facts.owner_hints))
        .filter_map(|link| {
            lattice_core::domain::provenance::Origin::of_url(&link.url)
                .ok()
                .filter(|origin| origin.key != facts.origin)
                .map(|origin| origin.host.unwrap_or_else(|| origin.key.0.clone()))
        })
        .next()
        .map_or(PassageKind::Direct, |attributed_to| {
            PassageKind::Testimonial {
                attributed_to,
                borne_out: None,
            }
        })
}

/// The links worth following from a supporting passage, best first (§8):
/// links on a host where the subject's owner is believed to publish rank
/// as the owner's; links into `publisher`, the origins of the version's
/// cluster, are not citations.
fn cited_urls(
    passage: &EvidencePassage,
    facts: &VersionFacts,
    publisher: &[OriginKey],
) -> Vec<String> {
    citation_candidates(
        &facts.saved.text,
        (passage.start_byte, passage.end_byte),
        &facts.saved.links,
        publisher,
        |link| {
            if owner_hinted(&link.url, &facts.owner_hints) {
                LinkStanding::Owner
            } else {
                LinkStanding::Unknown
            }
        },
    )
    .into_iter()
    .map(|candidate| candidate.url)
    .collect()
}

/// Whether `url`'s host is one of `hints` or below one of them.
pub(super) fn owner_hinted(url: &str, hints: &[String]) -> bool {
    let Some(host) = url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
    else {
        return false;
    };
    hints.iter().any(|hint| {
        let hint = hint.trim().trim_end_matches('.').to_ascii_lowercase();
        !hint.is_empty() && (host == hint || host.ends_with(&format!(".{hint}")))
    })
}

/// Weighs every supported finding of a pass that the gate decides, in a
/// preparation job (where provenance is stored); elsewhere nothing is
/// weighed and the ledger's gate applies the quotation rule alone.
/// `settled` holds the ordinals of findings decided before any model check
/// (a quotation found in the source it names, evidence the user brought).
pub(super) async fn weigh(
    llm: &dyn LLMPort,
    results: &mut [CheckedClaim],
    settled: &HashSet<usize>,
) -> Result<()> {
    let targets: Vec<usize> = results
        .iter()
        .enumerate()
        .filter(|(_, (ordinal, _, finding, _))| {
            !settled.contains(ordinal)
                && finding.verdict == ClaimVerdict::Supported
                && !supporting_passages(finding).is_empty()
        })
        .map(|(index, _)| index)
        .collect();
    if targets.is_empty() {
        return Ok(());
    }
    let Some(mut judgements) = context::Judgements::open(llm).await? else {
        return Ok(());
    };
    crate::features::learning::lesson_progress::stage(format!(
        "Weighing where the support of {} claims comes from",
        targets.len()
    ));
    let claims: Vec<(usize, &str)> = targets
        .iter()
        .filter_map(|index| results.get(*index))
        .map(|(_, _, finding, _)| (finding.unit, finding.statement.as_str()))
        .collect();
    let profiles = judgements.profiles(llm, &claims).await?;
    for (index, profile) in targets.into_iter().zip(profiles) {
        let Some((_, _, finding, _)) = results.get_mut(index) else {
            continue;
        };
        reweigh(&mut judgements, llm, &profile, finding, None).await?;
    }
    Ok(())
}

/// Weighs a supported `finding` again with what the course has on its
/// subject (`available`, from the standing reach), and names the strongest
/// counted support as the claim's support (claim sources design §8).
pub(super) async fn reweigh(
    judgements: &mut context::Judgements,
    llm: &dyn LLMPort,
    profile: &ClaimProfile,
    finding: &mut Finding,
    available: Option<Available>,
) -> Result<()> {
    finding.trust = Some(Box::new(
        weighed(judgements, llm, profile, finding, available).await?,
    ));
    strongest::name_strongest(finding);
    Ok(())
}

/// One supported finding weighed for its profile: each supporting version
/// levelled for the claim's subject, then `assess`.
async fn weighed(
    judgements: &mut context::Judgements,
    llm: &dyn LLMPort,
    profile: &ClaimProfile,
    finding: &Finding,
    available: Option<Available>,
) -> Result<FindingTrust> {
    let mut versions: Vec<String> = supporting_passages(finding)
        .into_iter()
        .map(|passage| passage.source_id.clone())
        .chain(
            finding
                .two_pass
                .contrary
                .iter()
                .map(|contrary| contrary.passage.source_id.clone()),
        )
        .collect();
    versions.sort();
    versions.dedup();
    let facts = judgements.facts(Some(llm), &versions, profile).await?;
    Ok(assess(
        profile,
        finding,
        &Evidence {
            facts: &facts,
            relations: judgements.relations(),
            bars: judgements.bars(),
            policy: &lattice_core::domain::provenance::POLICY,
            available,
        },
    ))
}

/// Weighs one claim the checker supported outside a lesson's checks: the
/// outline's factual repair (P7). The same profile, levels and `decide` as
/// [`weigh`], inside the trust scope the job installed. `supported` names
/// the version and quotation the verdict rests on; the supports are the
/// passages of `page` that hold the quotation. `None` outside a trust
/// scope, or when no passage of the page holds the quotation.
pub(in crate::features::learning) async fn weigh_claim(
    llm: &dyn LLMPort,
    unit: usize,
    statement: &str,
    page: &[EvidencePassage],
    supported: SupportedBy,
) -> Result<Option<TrustAssessment>> {
    let finding = Finding {
        unit,
        quote: String::new(),
        statement: statement.to_owned(),
        verdict: ClaimVerdict::Supported,
        reason: String::new(),
        evidence: page.to_vec(),
        supporting_quote: Some(supported.supporting_quote.clone()),
        resolution: None,
        two_pass: Default::default(),
        trust: None,
    };
    if supporting_passages(&finding).is_empty() {
        return Ok(None);
    }
    let Some(mut judgements) = context::Judgements::open(llm).await? else {
        return Ok(None);
    };
    let Some(profile) = judgements
        .profiles(llm, &[(unit, statement)])
        .await?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let trust = weighed(&mut judgements, llm, &profile, &finding, None).await?;
    Ok(Some(trust.assessment(
        &supported.source_version_id,
        &supported.supporting_quote,
    )))
}
