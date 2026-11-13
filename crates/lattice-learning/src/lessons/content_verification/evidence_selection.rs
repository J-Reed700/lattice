//! The course's first page for a claim, kept stable as research grows (course
//! evidence index §5.6): saved before its check and restored while the
//! course's sources are unchanged. For a research target, what research found
//! leads its page.
use super::*;
use crate::reference_collection::{PassageKind, Scope};

const RESEARCH_SCOPE_POLICY: Protocol = lattice_core::protocol!("claim-research-scope", 1);
const EVIDENCE_KEYS: Protocol = lattice_core::protocol!("claim-evidence", 2);
/// How the first page of the course is ranked: one hybrid list over the
/// course, without what the claim's citation shows, sliced by the page budget.
/// A selection saved under another ranking is retrieved anew, once.
const RANKING: Protocol = lattice_core::protocol!("two-pass-hybrid", 3);

/// Automatic research is requested for specific failed comparisons. Bind the
/// resulting source snapshot to those claims so adding a broadly relevant page
/// cannot reopen unrelated approvals. A later user-supplied source has no such
/// scope and therefore retains the conservative normal invalidation behavior.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResearchScope {
    policy: String,
    sources: Vec<SourceBinding>,
    targets: Vec<ResearchTarget>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResearchTarget {
    unit: usize,
    statement: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Selection {
    sources: Vec<SourceBinding>,
    passages: Vec<Passage>,
    /// The reach the page was read from; `None` for a page with nothing on
    /// the claim's subject.
    #[serde(default)]
    reach: Option<Reach>,
}

/// The first page of the course for a claim, and where it was read from.
pub(super) struct Selected {
    pub(super) evidence: Vec<EvidencePassage>,
    pub(super) reach: Option<Reach>,
}

/// The saved form of a first page of `evidence` over `sources`.
pub(super) fn selection_of(sources: &[SourceBinding], evidence: &[EvidencePassage]) -> Selection {
    Selection {
        sources: sources.to_vec(),
        passages: evidence
            .iter()
            .map(|p| Passage {
                source_id: p.source_id.clone(),
                start: p.start_byte,
                end: p.end_byte,
                sha256: sha256_hex(&p.text),
                kind: matches!(p.kind(), PassageKind::Cited | PassageKind::Researched)
                    .then(|| p.retrieval_kind.clone()),
            })
            .collect(),
        reach: None,
    }
}

/// The checkpoint key of `statement`'s first page in section `unit`, under
/// `ranking`.
fn selection_key(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    (unit, statement): (usize, &str),
    ranking: &str,
) -> String {
    EVIDENCE_KEYS.key(sha256_hex(json!({
        "policy":POLICY, "model":llm.model_name(), "context":llm.max_context_tokens(),
        "unit":unit, "statement":statement, "retrieval":references.mode().label(), "embedding":references.embedding_model(),
        "ranking":ranking,
    }).to_string()))
}

/// The key a first page saved under an earlier `ranking` has, for tests of
/// the ranking bump.
#[cfg(test)]
pub(super) fn selection_key_under(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    claim: (usize, &str),
    ranking: &str,
) -> String {
    selection_key(llm, references, claim, ranking)
}

#[derive(Clone, Serialize, Deserialize)]
struct Passage {
    source_id: String,
    start: usize,
    end: usize,
    sha256: String,
    /// How the passage was chosen, so a section's cited passages stay first.
    #[serde(default)]
    kind: Option<String>,
}

pub(super) fn sources(references: &ReferenceCollection<'_>) -> Vec<SourceBinding> {
    let mut sources: Vec<_> = references
        .sources
        .iter()
        .map(|source| SourceBinding {
            id: source.id.clone(),
            sha256: sha256_hex(&source.excerpt),
        })
        .collect();
    sources.sort_by(|a, b| a.id.cmp(&b.id));
    sources
}

fn research_scope_key(current: &[SourceBinding]) -> String {
    RESEARCH_SCOPE_POLICY.key(sha256_hex(
        json!({"policy":RESEARCH_SCOPE_POLICY,"sources":current}).to_string(),
    ))
}

/// Saves that what research added to `references` is for the claims of
/// `findings`, the ones the round looked for.
pub(super) async fn record_research_scope(
    references: &ReferenceCollection<'_>,
    findings: &[Finding],
    retained: &ClaimChecks,
) -> Result<()> {
    let current = sources(references);
    let mut targets: Vec<_> = findings
        .iter()
        .map(|finding| ResearchTarget {
            unit: finding.unit,
            statement: finding.statement.clone(),
        })
        .collect();
    targets.sort();
    targets.dedup();
    if targets.is_empty() {
        return Ok(());
    }
    let key = research_scope_key(&current);
    retained
        .research_scopes
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(
            key.clone(),
            targets
                .iter()
                .map(|target| (target.unit, target.statement.clone()))
                .collect(),
        );
    crate::lesson_drafts::record_checkpoint(
        &key,
        serde_json::to_value(ResearchScope {
            policy: RESEARCH_SCOPE_POLICY.to_string(),
            sources: current,
            targets,
        })?,
    )
    .await
}

async fn research_scope(
    current: &[SourceBinding],
    retained: &ClaimChecks,
) -> Result<Option<HashSet<(usize, String)>>> {
    let key = research_scope_key(current);
    if let Some(targets) = retained
        .research_scopes
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&key)
        .cloned()
    {
        return Ok(Some(targets));
    }
    let Some(scope) = crate::lesson_drafts::checkpoint(&key)
        .await?
        .and_then(|value| serde_json::from_value::<ResearchScope>(value).ok())
    else {
        return Ok(None);
    };
    if !RESEARCH_SCOPE_POLICY.names(&scope.policy)
        || scope.sources != current
        || scope
            .targets
            .iter()
            .any(|target| target.statement.trim().is_empty())
    {
        return Ok(None);
    }
    let targets: HashSet<_> = scope
        .targets
        .into_iter()
        .map(|target| (target.unit, target.statement))
        .collect();
    retained
        .research_scopes
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(key, targets.clone());
    Ok(Some(targets))
}

impl Selection {
    fn restore(
        &self,
        references: &ReferenceCollection<'_>,
        current: &[SourceBinding],
    ) -> Option<Vec<EvidencePassage>> {
        // A replacement/removal invalidates comparisons that used that source.
        // Other claims keep their evidence, including earlier counterevidence.
        self.passages
            .iter()
            .map(|passage| {
                let binding = self
                    .sources
                    .iter()
                    .find(|source| source.id == passage.source_id)?;
                if !current.contains(binding) {
                    return None;
                }
                let source = references
                    .sources
                    .iter()
                    .find(|source| source.id == passage.source_id)?;
                let text = source.excerpt.get(passage.start..passage.end)?;
                if sha256_hex(text) != passage.sha256 {
                    return None;
                }
                let kind = passage
                    .kind
                    .as_deref()
                    .map_or(PassageKind::Retrieved(references.mode()), PassageKind::of);
                EvidencePassage::of(source, passage.start..passage.end, kind, 0.0)
            })
            .collect()
    }
}

/// The first page saved under `key`, in the job's memory or else in its
/// checkpoints.
async fn saved_selection(retained: &ClaimChecks, key: &str) -> Result<Option<Selection>> {
    let memory = retained
        .selections
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(key)
        .cloned();
    match memory {
        Some(saved) => Ok(Some(saved)),
        None => Ok(crate::lesson_drafts::checkpoint(key)
            .await?
            .and_then(|value| serde_json::from_value::<Selection>(value).ok())),
    }
}

/// Whether the first page of `claim` is saved and restores whatever the page
/// budget is: no source research added since is for this claim.
pub(super) async fn is_saved(
    llm: &dyn LLMPort,
    (claim, unit): (&Claim, usize),
    references: &ReferenceCollection<'_>,
    current: &[SourceBinding],
    retained: &ClaimChecks,
) -> Result<bool> {
    let key = selection_key(llm, references, (unit, &claim.statement), RANKING.as_str());
    let Some(saved) = saved_selection(retained, &key).await? else {
        return Ok(false);
    };
    if saved.restore(references, current).is_none() {
        return Ok(false);
    }
    if current.iter().all(|source| saved.sources.contains(source)) {
        return Ok(true);
    }
    let for_research = research_scope(current, retained)
        .await?
        .is_none_or(|targets| targets.contains(&(unit, claim.statement.clone())));
    Ok(!for_research)
}

/// The first page of the course for `claim`: restored when saved under this
/// ranking with every source unchanged, otherwise read from the course's list,
/// and never overlapping `attribution`, the passages its citation shows. Saved
/// before any model work.
pub(super) async fn select(
    llm: &dyn LLMPort,
    (claim, checked, unit): (&Claim, &str, usize),
    references: &ReferenceCollection<'_>,
    current: &[SourceBinding],
    retained: &ClaimChecks,
    (attribution, budget): (&[EvidencePassage], &evidence_checks::PageBudget),
) -> Result<Selected> {
    let selection_key = selection_key(llm, references, (unit, &claim.statement), RANKING.as_str());
    let saved = saved_selection(retained, &selection_key).await?;
    let restored = saved
        .as_ref()
        .and_then(|saved| saved.restore(references, current));
    let research_scope = research_scope(current, retained).await?;
    let (mut evidence, reach, changed) =
        if let (Some(saved), Some(evidence)) = (saved.as_ref(), restored) {
            let added: HashSet<String> = current
                .iter()
                .filter(|source| !saved.sources.contains(source))
                .map(|source| source.id.clone())
                .collect();
            let claim_is_research_target = research_scope
                .as_ref()
                .is_none_or(|targets| targets.contains(&(unit, claim.statement.clone())));
            let led = if !added.is_empty() && claim_is_research_target {
                research_led(
                    llm,
                    (claim, checked),
                    references,
                    &added,
                    attribution,
                    budget.tokens,
                    evidence.clone(),
                )
                .await?
            } else {
                None
            };
            match led {
                Some(page) => (page, Some(Reach::Researched), true),
                None => (evidence, saved.reach, saved.sources != current),
            }
        } else {
            let (page, reach) = evidence_checks::page_one(
                llm,
                references,
                &claim.statement,
                checked,
                attribution,
                budget,
            )
            .await?;
            (page, reach, true)
        };
    // The page never shows what the citation does, even when the section's
    // citation moved onto a passage the saved page holds.
    evidence.retain(|passage| !attribution.iter().any(|shown| shown.overlaps(passage)));
    evidence.sort_by(|a, b| {
        (&a.source_id, a.start_byte, a.end_byte).cmp(&(&b.source_id, b.start_byte, b.end_byte))
    });
    evidence.dedup_by(|a, b| a.same_place(b));
    if changed || saved.is_none() {
        let mut selection = selection_of(current, &evidence);
        selection.reach = reach;
        // Save the plan before model work. An interrupted comparison will use
        // exactly this evidence again, but cannot inherit an older approval.
        crate::lesson_drafts::record_checkpoint(&selection_key, serde_json::to_value(&selection)?)
            .await?;
        retained
            .selections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(selection_key, selection);
    }
    canonicalize_evidence(&mut evidence);
    Ok(Selected { evidence, reach })
}

/// The first page of a claim research was asked for, once research saved
/// `added` versions: the best of what it added, in the claim's list over
/// those versions, leads the page, and the earlier page's passages, in
/// their order, fill what the budget and the window leave. `None` when
/// research added nothing on the claim's subject.
async fn research_led(
    llm: &dyn LLMPort,
    (claim, checked): (&Claim, &str),
    references: &ReferenceCollection<'_>,
    added: &HashSet<String>,
    attribution: &[EvidencePassage],
    budget: Option<usize>,
    mut earlier: Vec<EvidencePassage>,
) -> Result<Option<Vec<EvidencePassage>>> {
    let list = references
        .claim_list(&claim.statement, &Scope::Keys(added.clone()), attribution)
        .await?;
    let (found, _) = evidence_checks::page_of(llm, checked, &list.passages, attribution, budget);
    if found.is_empty() {
        return Ok(None);
    }
    let mut page: Vec<EvidencePassage> = found
        .into_iter()
        .map(|passage| EvidencePassage {
            retrieval_kind: PassageKind::Researched.label().into(),
            ..passage
        })
        .collect();
    canonicalize_evidence(&mut earlier);
    for passage in earlier {
        if page.iter().any(|shown| shown.overlaps(&passage)) {
            continue;
        }
        let mut grown = page.clone();
        grown.push(passage);
        if evidence_checks::within_page(llm, checked, &grown, budget) {
            page = grown;
        }
    }
    Ok(Some(page))
}
