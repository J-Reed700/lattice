//! The standing reach (claim sources design §7): before a claim is left
//! unsupported, insufficient or contradicted, it reads the course's own
//! highest-standing sources on its subject, and is weighed again against the
//! strongest the course has there (the ceiling, §6.1).
//!
//! The claims are those the gate left (§7.2); each one's scope is read from
//! stored facts (`Judgements::standing_scope`, §7.3), its walk pages that
//! scope as pass B pages the course (`Walk::standing`, `deepen`), every
//! version on a page it read is levelled for its subject, and what it found
//! joins its finding (`outcome::with_standing`). A claim settled before any
//! model check, or decided outside a preparation job, is not read on.
use super::outcome::with_standing;
use super::run::research_seconds;
use super::*;
use crate::features::learning::reference_collection::Scope;
use lattice_core::domain::provenance::{Available, ClaimProfile, Decision, TrustLevel};

impl Finding {
    /// Whether the gate leaves this finding for the standing reach (§7.2):
    /// unsupported, contradicted, or supported by support below its bar. A
    /// claim the judge could not judge is not read on.
    fn needs_standing(&self) -> bool {
        match self.verdict {
            ClaimVerdict::Unsupported | ClaimVerdict::Contradicted => true,
            ClaimVerdict::Supported => self
                .trust
                .as_ref()
                .is_some_and(|trust| matches!(trust.decision, Decision::Insufficient { .. })),
            ClaimVerdict::Unverified => false,
        }
    }
}

/// One claim the reach reads on: where its result is, its profile, the proxy
/// levels of the scope's versions its list returned, and what its section
/// cites.
struct Reading {
    index: usize,
    profile: ClaimProfile,
    proxies: HashMap<String, TrustLevel>,
    cited: Vec<String>,
}

/// Reads the course's strongest sources for every claim of `results` the
/// gate left (none of `decided`), then weighs each supported one again with
/// what the course has on its subject.
pub(super) async fn corroborate(
    context: &Context<'_, '_>,
    results: &mut [CheckedClaim],
    decided: &HashSet<usize>,
    labels: &Labels,
    answers: &mut AnswerSize,
) -> Result<()> {
    let llm = context.llm;
    let targets: Vec<usize> = results
        .iter()
        .enumerate()
        .filter(|(_, (ordinal, _, finding, _))| {
            !decided.contains(ordinal) && finding.needs_standing()
        })
        .map(|(index, _)| index)
        .collect();
    if targets.is_empty() {
        return Ok(());
    }
    let Some(mut judgements) = provenance_gate::Judgements::open(llm).await? else {
        return Ok(());
    };
    crate::features::learning::lesson_progress::stage(format!(
        "Reading the course's strongest sources for {} claims",
        targets.len()
    ));
    let claims: Vec<(usize, &str)> = targets
        .iter()
        .filter_map(|index| results.get(*index))
        .map(|(_, _, finding, _)| (finding.unit, finding.statement.as_str()))
        .collect();
    let profiles = judgements.profiles(llm, &claims).await?;
    let mut walks = Vec::new();
    let mut readings: BTreeMap<usize, Reading> = BTreeMap::new();
    for (index, profile) in targets.into_iter().zip(profiles) {
        let Some((ordinal, decision_key, finding, _)) = results.get(index) else {
            continue;
        };
        let shown: Vec<EvidencePassage> = finding
            .evidence
            .iter()
            .filter(|passage| passage.retrieval_kind != "execution")
            .cloned()
            .collect();
        // Reading one passage does not exhaust a document. Exclude only
        // sources already counted as support; claim_list excludes the exact
        // passages shown, so unread sections of official docs remain eligible.
        let supported_versions: Vec<String> = provenance_gate::supporting_passages(finding)
            .into_iter()
            .map(|passage| passage.source_id.clone())
            .collect();
        if profile.authority_subject().is_some() && !judgements.owner_judged(&profile).await? {
            // The scope's one model call: who owns the subject, asked of the
            // first course passage on it the claim was not shown.
            let course = context
                .references
                .claim_list(&finding.statement, &Scope::All, &shown)
                .await?;
            if let Some(first) = course.passages.first() {
                judgements
                    .judge_owner(llm, &first.source_id, &profile)
                    .await?;
            }
        }
        let scope = judgements
            .standing_scope(&profile, &supported_versions)
            .await?;
        let mut proxies = HashMap::new();
        if !scope.candidates.is_empty() {
            let claim = Claim {
                quote: finding.quote.clone(),
                statement: finding.statement.clone(),
            };
            let mut list = context
                .references
                .claim_list(
                    &claim.statement,
                    &Scope::Keys(scope.candidates.clone()),
                    &shown,
                )
                .await?;
            // The reach reads the course's strongest sources first (§7.4):
            // a stable sort, so relevance still orders each tier.
            list.passages
                .sort_by_key(|passage| scope.tier(&passage.source_id));
            for passage in &list.passages {
                if let Some(level) = scope.proxies.get(&passage.source_id) {
                    proxies.insert(passage.source_id.clone(), *level);
                }
            }
            if !list.passages.is_empty() {
                let checked = as_checked(&claim, context.citations, context.references);
                let budget = walk_budget(
                    &format!("standing:{decision_key}"),
                    PageBudget::of(llm, *answers),
                )
                .await?;
                walks.push(Walk::standing(
                    (*ordinal, finding.unit),
                    claim,
                    checked.statement,
                    shown,
                    scope.candidates,
                    list,
                    budget,
                ));
            }
        }
        let mut cited: Vec<String> = context
            .citations
            .of(finding.unit)
            .iter()
            .map(|passage| passage.source_id.clone())
            .collect();
        cited.sort();
        cited.dedup();
        readings.insert(
            *ordinal,
            Reading {
                index,
                profile,
                proxies,
                cited,
            },
        );
    }
    let (mut found, read) = if walks.is_empty() {
        Default::default()
    } else {
        let research = research_seconds(context).await?;
        deepen(
            llm,
            context.references,
            walks,
            (context.paging.depth, research),
            context.citations,
            labels,
            answers,
        )
        .await?
    };
    for (ordinal, reading) in readings {
        let versions = read.get(&ordinal).cloned().unwrap_or_default();
        let facts = judgements
            .facts(Some(llm), &versions, &reading.profile)
            .await?;
        let Some((_, _, finding, _)) = results.get_mut(reading.index) else {
            continue;
        };
        let levels: HashMap<String, TrustLevel> = facts
            .iter()
            .map(|(version, facts)| (version.clone(), facts.trust.level))
            .collect();
        let available = ceiling(finding, &levels, &reading.proxies);
        *finding = with_standing(finding.clone(), found.remove(&ordinal), &reading.cited);
        if finding.verdict == ClaimVerdict::Supported {
            provenance_gate::reweigh(
                &mut judgements,
                llm,
                &reading.profile,
                finding,
                Some(available),
            )
            .await?;
        }
    }
    Ok(())
}

/// What the course has on a claim's subject (§7.4): the highest level among
/// its supports and contrary passages as the gate levelled them, the
/// versions on the pages the reach read (`read`, levelled for the subject),
/// and the proxies of the scope's versions its list returned, Distrusted
/// excluded; `considered` counts the distinct versions behind it. A host
/// only believed to be the owner's never raises it: it has no proxy.
fn ceiling(
    finding: &Finding,
    read: &HashMap<String, TrustLevel>,
    proxies: &HashMap<String, TrustLevel>,
) -> Available {
    let mut levels: HashMap<&str, TrustLevel> = HashMap::new();
    if let Some(trust) = finding.trust.as_deref() {
        for support in &trust.supports {
            levels.insert(&support.source_version_id, support.trust.level);
        }
        for contrary in &trust.contrary {
            levels.insert(&contrary.source_version_id, contrary.trust.level);
        }
    }
    for (version, level) in read {
        levels.insert(version, *level);
    }
    for (version, level) in proxies {
        levels.entry(version).or_insert(*level);
    }
    levels.retain(|_, level| *level != TrustLevel::Distrusted);
    Available {
        ceiling: levels.values().copied().max(),
        considered: levels.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ceiling_is_the_highest_levelled_or_proxied_source_on_the_subject_and_hints_do_not_count()
    {
        let finding = Finding {
            unit: 0,
            quote: String::new(),
            statement: String::new(),
            verdict: ClaimVerdict::Unsupported,
            reason: String::new(),
            evidence: vec![],
            supporting_quote: None,
            resolution: None,
            trust: None,
            two_pass: Default::default(),
        };
        let read = HashMap::from([
            ("read".to_owned(), TrustLevel::Established),
            ("distrusted".to_owned(), TrustLevel::Distrusted),
        ]);
        // The proxy of a version a page read is replaced by its level; a
        // hinted host has no proxy, so it is not here at all.
        let proxies = HashMap::from([
            ("read".to_owned(), TrustLevel::Primary),
            ("listed".to_owned(), TrustLevel::Authoritative),
        ]);
        let available = ceiling(&finding, &read, &proxies);
        assert_eq!(available.ceiling, Some(TrustLevel::Authoritative));
        assert_eq!(
            available.considered, 2,
            "the distrusted page is not counted"
        );
        let none = ceiling(&finding, &HashMap::new(), &HashMap::new());
        assert_eq!((none.ceiling, none.considered), (None, 0));
    }
}
