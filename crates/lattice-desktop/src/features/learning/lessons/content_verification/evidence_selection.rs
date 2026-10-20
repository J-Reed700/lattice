//! Keep the evidence already compared with a claim stable as research grows.
//! New sources still participate in retrieval. Ranking changes among unchanged
//! sources do not replace the passages we have already selected and checked.
//! A claim research was asked for is the exception: what research found leads
//! its evidence, so the claim is checked again against it.
use super::*;
use crate::features::learning::reference_collection::{above_cut, best_per_source};

/// The retrieval kind of a passage research found for the claim it serves.
pub(super) const RESEARCHED: &str = "researched";

const RESEARCH_SCOPE_POLICY: &str = "claim-research-scope-v1";
/// How a claim's first page is built: its section's citations, then one
/// passage on its subject from each source the lesson cites, above its
/// ranking's cut (claim evidence pages §4). A selection saved under another
/// rule is retrieved anew.
const RANKING: &str = "rings-by-gap-v1";

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
                        .unwrap_or_else(|| references.mode().into()),
                    score: 0.0,
                    branch: None,
                    branch_score: 0.0,
                })
            })
            .collect()
    }
}

/// The selection that saves `evidence` against the `current` sources: each
/// passage's place and digest, and its kind when it is a citation or what
/// research found. Ranking scores, of either kind, are no part of it.
pub(super) fn selection_of(current: &[SourceBinding], evidence: &[EvidencePassage]) -> Selection {
    Selection {
        sources: current.to_vec(),
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
    }
}

pub(super) async fn select(
    llm: &dyn LLMPort,
    claim: &Claim,
    unit: usize,
    references: &ReferenceCollection<'_>,
    current: &[SourceBinding],
    executions: &[execution::Observation],
    retained: &ClaimChecks,
    citations: &Citations,
) -> Result<Vec<EvidencePassage>> {
    let selection_key = format!("claim-evidence-v2:{}", digest(&json!({
        "policy":POLICY, "model":llm.model_name(), "context":llm.max_context_tokens(),
        "unit":unit, "statement":claim.statement, "retrieval":references.mode(), "embedding":references.embedding_model(),
        "ranking":RANKING,
    }).to_string()));
    let memory = retained
        .selections
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&selection_key)
        .cloned();
    let mut saved = match memory {
        Some(saved) => Some(saved),
        None => crate::features::learning::lesson_drafts::checkpoint(&selection_key)
            .await?
            .and_then(|value| serde_json::from_value::<Selection>(value).ok()),
    };
    let migrated = saved.is_none();
    if migrated {
        // Prefer a completed comparison over an unfinished fresh retrieval
        // caused only by relocating the quote. Keep all completed comparisons
        // and genuinely added source versions, including pending counterevidence.
        let mut plans = Vec::new();
        for (index, old) in claim_reuse::identities(retained, unit, claim) {
            let old_key = format!("claim-evidence-v1:{}", digest(&json!({
                "policy":POLICY, "model":llm.model_name(), "context":llm.max_context_tokens(),
                "unit":index, "claim":old, "retrieval":references.mode(), "embedding":references.embedding_model(),
            }).to_string()));
            let Some(plan) = crate::features::learning::lesson_drafts::checkpoint(&old_key)
                .await?
                .and_then(|value| serde_json::from_value::<Selection>(value).ok())
            else {
                continue;
            };
            let Some(mut evidence) = plan.restore(references, current) else {
                continue;
            };
            canonicalize_evidence(&mut evidence);
            let complete = crate::features::learning::lesson_drafts::checkpoint(
                &claim_reuse::legacy_key(llm, index, &old, &evidence),
            )
            .await?
            .and_then(|value| serde_json::from_value::<ClaimReceipt>(value).ok())
            .and_then(|receipt| receipt.finding(index, &old, &evidence))
            .is_some();
            plans.push((complete, plan));
        }
        plans.sort_by_key(|(complete, _)| !complete);
        let has_completed = plans.iter().any(|(complete, _)| *complete);
        for (complete, plan) in plans {
            if let Some(saved) = &mut saved {
                let added: HashSet<_> = plan
                    .sources
                    .iter()
                    .filter(|source| !saved.sources.contains(source))
                    .map(|source| source.id.clone())
                    .collect();
                for source in plan.sources {
                    if !saved.sources.contains(&source) {
                        saved.sources.push(source);
                    }
                }
                saved.sources.sort_by(|a, b| a.id.cmp(&b.id));
                for passage in plan.passages {
                    if has_completed && !complete && !added.contains(&passage.source_id) {
                        continue;
                    }
                    if !saved.passages.iter().any(|old| {
                        old.source_id == passage.source_id
                            && old.start == passage.start
                            && old.end == passage.end
                            && old.sha256 == passage.sha256
                    }) {
                        saved.passages.push(passage);
                    }
                }
            } else {
                saved = Some(plan);
            }
        }
    }
    let restored = saved
        .as_ref()
        .and_then(|saved| saved.restore(references, current));
    let research_scope = research_scope(current, retained).await?;
    let (mut evidence, changed) =
        if let (Some(saved), Some(mut evidence)) = (saved.as_ref(), restored) {
            let added: HashSet<_> = current
                .iter()
                .filter(|source| !saved.sources.contains(source))
                .map(|source| source.id.as_str())
                .collect();
            let claim_is_research_target = research_scope
                .as_ref()
                .is_none_or(|targets| targets.contains(&(unit, claim.statement.clone())));
            if !added.is_empty() && claim_is_research_target {
                // Research was asked for because this evidence left the claim
                // unresolved. Each added source's best passage on the claim's
                // subject, above its ranking's cut, leads the page; the
                // earlier page's passages follow. The window bounds the check,
                // never this page.
                let found = references
                    .next_by_source(&claim.statement, &claim.statement, &[], |source| {
                        added.contains(source)
                    })
                    .await?;
                let (above, _) = above_cut(found);
                let mut researched: Vec<_> = best_per_source(above)
                    .into_iter()
                    .map(|passage| EvidencePassage {
                        retrieval_kind: RESEARCHED.into(),
                        ..passage
                    })
                    .collect();
                if !researched.is_empty() {
                    canonicalize_evidence(&mut evidence);
                    researched.extend(evidence);
                    evidence = researched;
                }
            }
            (evidence, saved.sources != current)
        } else {
            // This preserves exact keys for existing durable factual receipts.
            let evidence = first_page(
                llm,
                &claim.statement,
                unit,
                references,
                executions,
                citations,
            )
            .await?
            .into_iter()
            .filter(|passage| passage.retrieval_kind != "execution")
            .collect();
            (evidence, true)
        };
    evidence.sort_by(|a, b| {
        (&a.source_id, a.start_byte, a.end_byte).cmp(&(&b.source_id, b.start_byte, b.end_byte))
    });
    evidence.dedup_by(|a, b| {
        a.source_id == b.source_id
            && a.start_byte == b.start_byte
            && a.end_byte == b.end_byte
            && a.text == b.text
    });
    if changed || migrated {
        let selection = selection_of(current, &evidence);
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
    evidence.extend(
        executions
            .iter()
            .filter(|o| o.passed() && o.unit == Some(unit))
            .map(|o| EvidencePassage {
                source_id: o.id.clone(),
                text: o.evidence(),
                start_byte: 0,
                end_byte: 0,
                retrieval_kind: "execution".into(),
                score: 0.0,
                branch: None,
                branch_score: 0.0,
            }),
    );
    canonicalize_evidence(&mut evidence);
    Ok(evidence)
}
