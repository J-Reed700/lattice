//! Pass B's first page for a claim, kept stable as research grows (course
//! evidence index §5.6): saved before its check and restored while the
//! course's sources are unchanged. For a research target, what research found
//! leads its page and its walk reads nothing else.
use super::*;
use crate::features::learning::reference_collection::Scope;

/// The retrieval kind of a passage research found for the claim it serves.
pub(super) const RESEARCHED: &str = "researched";

const RESEARCH_SCOPE_POLICY: &str = "claim-research-scope-v1";
/// How pass B's first page is ranked: one hybrid list over the course,
/// without what pass A shows, sliced by the page budget. A selection saved
/// under another ranking is retrieved anew, once.
const RANKING: &str = "two-pass-hybrid-v1";

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
    /// The reach the page was read from; `None` for a page research led or
    /// one with nothing on the claim's subject.
    #[serde(default)]
    reach: Option<Reach>,
    /// The versions research saved for the claim that lead the page: the
    /// claim's walk reads only these.
    #[serde(default)]
    researched: Vec<String>,
}

/// Pass B's first page for a claim, and where its walk goes on from.
pub(super) struct Selected {
    pub(super) evidence: Vec<EvidencePassage>,
    pub(super) reach: Option<Reach>,
    pub(super) researched: Vec<String>,
    /// What the page's list said, when it was read now rather than restored.
    pub(super) note: Option<evidence_checks::PageNote>,
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
                sha256: digest(&p.text),
                kind: [CITED, RESEARCHED]
                    .contains(&p.retrieval_kind.as_str())
                    .then(|| p.retrieval_kind.clone()),
            })
            .collect(),
        reach: None,
        researched: Vec::new(),
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
    format!("claim-evidence-v2:{}", digest(&json!({
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
            sha256: digest(&source.excerpt),
        })
        .collect();
    sources.sort_by(|a, b| a.id.cmp(&b.id));
    sources
}

fn research_scope_key(current: &[SourceBinding]) -> String {
    format!(
        "claim-research-scope-v1:{}",
        digest(&json!({"policy":RESEARCH_SCOPE_POLICY,"sources":current}).to_string())
    )
}

pub(super) async fn record_research_scope(
    references: &ReferenceCollection<'_>,
    findings: &[Finding],
    retained: &ClaimChecks,
) -> Result<()> {
    let current = sources(references);
    let mut targets: Vec<_> = findings
        .iter()
        .filter(|finding| finding.wants_research())
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
    crate::features::learning::lesson_drafts::record_checkpoint(
        &key,
        serde_json::to_value(ResearchScope {
            policy: RESEARCH_SCOPE_POLICY.into(),
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
    let Some(scope) = crate::features::learning::lesson_drafts::checkpoint(&key)
        .await?
        .and_then(|value| serde_json::from_value::<ResearchScope>(value).ok())
    else {
        return Ok(None);
    };
    if scope.policy != RESEARCH_SCOPE_POLICY
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
                if digest(text) != passage.sha256 {
                    return None;
                }
                Some(EvidencePassage {
                    source_id: source.id.clone(),
                    text: text.into(),
                    start_byte: passage.start,
                    end_byte: passage.end,
                    retrieval_kind: passage
                        .kind
                        .clone()
                        .unwrap_or_else(|| references.mode().label().into()),
                    score: 0.0,
                })
            })
            .collect()
    }
}

/// Pass B's first page for `claim`: restored when saved under this ranking
/// with every source unchanged, otherwise read from the walk's first reach,
/// and never overlapping `attribution`. Saved before any model work; `None`,
/// saving nothing, while `sizing` is unmeasured.
pub(super) async fn select(
    llm: &dyn LLMPort,
    (claim, checked, unit): (&Claim, &str, usize),
    references: &ReferenceCollection<'_>,
    current: &[SourceBinding],
    retained: &ClaimChecks,
    (citations, attribution, paging, sizing): (
        &Citations,
        &[EvidencePassage],
        Paging,
        &evidence_checks::Sizing,
    ),
) -> Result<Option<Selected>> {
    let budget = match sizing {
        evidence_checks::Sizing::Sized(budget) => Some(budget),
        evidence_checks::Sizing::Unmeasured => None,
    };
    let selection_key = selection_key(llm, references, (unit, &claim.statement), RANKING);
    let memory = retained
        .selections
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&selection_key)
        .cloned();
    let saved = match memory {
        Some(saved) => Some(saved),
        None => crate::features::learning::lesson_drafts::checkpoint(&selection_key)
            .await?
            .and_then(|value| serde_json::from_value::<Selection>(value).ok()),
    };
    let restored = saved
        .as_ref()
        .and_then(|saved| saved.restore(references, current));
    let research_scope = research_scope(current, retained).await?;
    let mut note = None;
    let (mut evidence, mut reach, mut researched, changed) =
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
                let Some(budget) = budget else {
                    return Ok(None);
                };
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
                Some(page) => {
                    let mut added: Vec<String> = added.into_iter().collect();
                    added.sort();
                    (page, Some(Reach::Researched), added, true)
                }
                None => (
                    evidence,
                    saved.reach,
                    saved.researched.clone(),
                    saved.sources != current,
                ),
            }
        } else {
            let Some(budget) = budget else {
                return Ok(None);
            };
            let (page, read) = evidence_checks::page_one(
                llm,
                references,
                &claim.statement,
                checked,
                citations,
                attribution,
                paging.reaches,
                budget,
            )
            .await?;
            let reach = read.as_ref().map(|note| note.reach);
            note = read;
            (page, reach, Vec::new(), true)
        };
    // Pass B never shows what pass A does, even when the section's citation
    // moved onto a passage the saved page holds.
    evidence.retain(|passage| !attribution.iter().any(|shown| overlaps(shown, passage)));
    evidence.sort_by(|a, b| {
        (&a.source_id, a.start_byte, a.end_byte).cmp(&(&b.source_id, b.start_byte, b.end_byte))
    });
    evidence.dedup_by(|a, b| {
        a.source_id == b.source_id
            && a.start_byte == b.start_byte
            && a.end_byte == b.end_byte
            && a.text == b.text
    });
    if reach.is_none() && !researched.is_empty() {
        reach = Some(Reach::Researched);
    }
    if researched.is_empty() && reach == Some(Reach::Researched) {
        reach = None;
        researched.clear();
    }
    if changed || saved.is_none() {
        let mut selection = selection_of(current, &evidence);
        selection.reach = reach;
        selection.researched = researched.clone();
        // Save the plan before model work. An interrupted comparison will use
        // exactly this evidence again, but cannot inherit an older approval.
        crate::features::learning::lesson_drafts::record_checkpoint(
            &selection_key,
            serde_json::to_value(&selection)?,
        )
        .await?;
        retained
            .selections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(selection_key, selection);
    }
    canonicalize_evidence(&mut evidence);
    Ok(Some(Selected {
        evidence,
        reach,
        researched,
        note,
    }))
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
            retrieval_kind: RESEARCHED.into(),
            ..passage
        })
        .collect();
    canonicalize_evidence(&mut earlier);
    for passage in earlier {
        if page.iter().any(|shown| overlaps(shown, &passage)) {
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
