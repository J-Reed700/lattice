//! Keep the evidence already compared with a claim stable as research grows.
//! New sources still participate in retrieval. Ranking changes among unchanged
//! sources do not replace the passages we have already selected and checked.
use super::*;

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
                    retrieval_kind: references.mode().into(),
                    score: 0.0,
                })
            })
            .collect()
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
) -> Result<Vec<EvidencePassage>> {
    let selection_key = format!("claim-evidence-v2:{}", digest(&json!({
        "policy":POLICY, "model":llm.model_name(), "context":llm.max_context_tokens(),
        "unit":unit, "statement":claim.statement, "retrieval":references.mode(), "embedding":references.embedding_model(),
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
    let (mut evidence, changed) =
        if let (Some(saved), Some(mut evidence)) = (saved.as_ref(), restored) {
            let added: HashSet<_> = current
                .iter()
                .filter(|source| !saved.sources.contains(source))
                .map(|source| source.id.as_str())
                .collect();
            if !added.is_empty() {
                // Use normal hybrid retrieval over the current collection to locate
                // relevant new sources. Keep the old comparison inputs verbatim.
                // Do not trim new evidence to the old character budget: that could
                // drop counterevidence. The checker enforces its real context size.
                evidence.extend(
                    references
                        .retrieve_for_verification(&claim.statement, 8)
                        .await?
                        .into_iter()
                        .filter(|passage| added.contains(passage.source_id.as_str())),
                );
            }
            (evidence, saved.sources != current)
        } else {
            // This preserves exact keys for existing durable factual receipts.
            let evidence = evidence_for(&claim.statement, unit, references, executions)
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
        let selection = Selection {
            sources: current.to_vec(),
            passages: evidence
                .iter()
                .map(|p| Passage {
                    source_id: p.source_id.clone(),
                    start: p.start_byte,
                    end: p.end_byte,
                    sha256: digest(&p.text),
                })
                .collect(),
        };
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
            }),
    );
    canonicalize_evidence(&mut evidence);
    Ok(evidence)
}
