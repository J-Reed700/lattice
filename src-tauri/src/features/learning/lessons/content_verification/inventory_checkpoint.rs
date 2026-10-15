//! Resuming, completing and checkpointing the claim inventory and its coverage audit.
use super::*;

async fn audit_coverage(
    llm: &dyn LLMPort,
    content: &[Value],
    inventory: &Inventory,
) -> Result<Coverage> {
    coverage::audit(llm, content, inventory, None).await
}

async fn save_inventory(
    llm: &dyn LLMPort,
    candidate: &str,
    inventory: &Inventory,
    coverage: Option<&Coverage>,
) -> Result<()> {
    let checkpoint = InventoryCheckpoint {
        policy: INVENTORY_POLICY.into(),
        model: llm.model_name().into(),
        model_context: llm.max_context_tokens(),
        content_sha256: digest(candidate),
        inventory: inventory.clone(),
        coverage: coverage.cloned().unwrap_or(Coverage { units: Vec::new() }),
        coverage_policy: coverage.map(|_| coverage::POLICY.into()),
        assessment_policy: coverage.map(|_| coverage::ASSESSMENT_POLICY.into()),
        teaching_context_policy: coverage.map(|_| coverage::TEACHING_CONTEXT_POLICY.into()),
    };
    crate::features::learning::lesson_drafts::record_claim_inventory(
        candidate,
        serde_json::to_string(&checkpoint)?,
    )
    .await
}

pub(super) async fn extract_audited_inventory(
    llm: &dyn LLMPort,
    content: &[Value],
    candidate: &str,
) -> Result<(Inventory, Coverage)> {
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Inventory,
        "Extracting factual claims from the lesson and assessments",
    );
    let (retained, coverage) = section_checkpoints::load(llm, content).await?;
    if !retained.is_empty() {
        crate::features::learning::lesson_progress::stage(format!(
            "Reusing completed claim inventories for {} unchanged sections",
            retained.len()
        ));
    }
    let inventory = inventory::extract_remaining(llm, content, retained).await?;
    complete_inventory(llm, content, inventory, candidate, Some(coverage)).await
}

pub(super) fn valid_saved_coverage(coverage: &Coverage, content: &[Value]) -> bool {
    let inputs = inventory::inputs(content);
    let mut seen = HashSet::new();
    coverage.units.iter().all(|unit| {
        let Some(input) = inputs.get(unit.index) else {
            return false;
        };
        let ids: HashSet<_> = input["passages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|passage| passage["id"].as_str())
            .collect();
        seen.insert(unit.index)
            && (!unit.complete || unit.unresolved_passages.is_empty())
            && unit
                .unresolved_passages
                .iter()
                .all(|id| ids.contains(id.as_str()))
            && unit
                .unresolved_passages
                .iter()
                .collect::<HashSet<_>>()
                .len()
                == unit.unresolved_passages.len()
    })
}

pub(super) fn retain_current_assessment_checks(
    coverage: &mut Coverage,
    content: &[Value],
    policy: Option<&str>,
) {
    if policy != Some(coverage::ASSESSMENT_POLICY) {
        // v3 adds choice references. Positive v2 comparisons remain valid;
        // negative ones can have lacked a referent. Older/unknown assessment
        // protocols still require rechecking. Teaching audits are unaffected.
        coverage.units.retain(|unit| {
            content.get(unit.index).is_some_and(|section| {
                section["kind"] != "assessment"
                    || (policy == Some("assessment-fidelity-v2") && unit.complete)
            })
        });
    }
}

pub(super) fn retain_current_teaching_checks(
    coverage: &mut Coverage,
    content: &[Value],
    policy: Option<&str>,
) {
    if policy != Some(coverage::TEACHING_CONTEXT_POLICY) {
        coverage.units.retain(|unit| {
            content.get(unit.index).is_some_and(|section| {
                section["kind"] != "teaching"
                    // v2 only removes normative teaching requirements from
                    // empirical coverage. A positive comparison from v1 (or
                    // the original context-free policy) remains valid. Redo
                    // their negative findings, and distrust unknown policies.
                    || (matches!(policy, None | Some("teaching-context-v1"))
                        && unit.complete)
            })
        });
    }
}

// This strictly decreasing, bounded tuple prevents correction cycles without
// imposing a time or attempt limit. More complete sections come first, then
// fewer unresolved passages, then more captured claims within those passages.
fn coverage_work(
    coverage: &Coverage,
    inventory: &Inventory,
    content: &[Value],
) -> (usize, usize, std::cmp::Reverse<usize>) {
    let inputs = inventory::inputs(content);
    let incomplete: Vec<_> = coverage
        .units
        .iter()
        .filter(|unit| !unit.complete)
        .collect();
    let passages = incomplete
        .iter()
        .map(|unit| {
            if unit.unresolved_passages.is_empty() {
                // Older checkpoints retain their findings as data. An absent
                // location list is never interpreted as zero unresolved work.
                inputs
                    .get(unit.index)
                    .and_then(|input| input["passages"].as_array())
                    .map_or(0, Vec::len)
            } else {
                unit.unresolved_passages.len()
            }
        })
        .sum();
    (
        incomplete.len(),
        passages,
        std::cmp::Reverse(inventory.units.iter().map(|unit| unit.claims.len()).sum()),
    )
}

pub(super) async fn complete_inventory(
    llm: &dyn LLMPort,
    content: &[Value],
    mut inventory: Inventory,
    candidate: &str,
    saved_coverage: Option<Coverage>,
) -> Result<(Inventory, Coverage)> {
    let mut coverage = saved_coverage.unwrap_or(Coverage { units: Vec::new() });
    // A later batch can fail after individual sections were saved but before
    // the whole-candidate checkpoint was updated. Prefer those completed,
    // current-policy sections when resuming the older aggregate checkpoint.
    let (retained, audits) = section_checkpoints::load(llm, content).await?;
    for unit in retained {
        inventory
            .units
            .retain(|current| current.index != unit.index);
        inventory.units.push(unit);
    }
    for audit in audits.units {
        coverage
            .units
            .retain(|current| current.index != audit.index);
        coverage.units.push(audit);
    }
    inventory.units.sort_by_key(|unit| unit.index);
    coverage.units.sort_by_key(|unit| unit.index);
    validate_inventory(&inventory, content)?;
    section_checkpoints::save(llm, content, &inventory.units, &coverage.units).await?;
    save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Coverage,
        "Checking that every factual claim was captured",
    );
    let pending = Inventory {
        units: inventory
            .units
            .iter()
            .filter(|unit| !coverage.units.iter().any(|check| check.index == unit.index))
            .cloned()
            .collect(),
    };
    if !pending.units.is_empty() {
        coverage
            .units
            .extend(audit_coverage(llm, content, &pending).await?.units);
        coverage.units.sort_by_key(|unit| unit.index);
    }
    save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
    while coverage.units.iter().any(|unit| !unit.complete) {
        let previous_work = coverage_work(&coverage, &inventory, content);
        crate::features::learning::lesson_progress::stage(
            "Completing missing claims identified by the coverage audit",
        );
        inventory = inventory::correct_coverage(llm, content, &inventory, &coverage).await?;
        crate::features::learning::lesson_progress::stage(
            "Rechecking the completed claim inventory",
        );
        // correct_coverage retains complete section inventories verbatim, and
        // lesson content is immutable here. Recheck only corrected sections;
        // unrelated approved inventories have not changed their audit inputs.
        let unchanged: HashSet<_> = coverage
            .units
            .iter()
            .filter(|unit| unit.complete)
            .map(|unit| unit.index)
            .collect();
        let corrected = Inventory {
            units: inventory
                .units
                .iter()
                .filter(|unit| !unchanged.contains(&unit.index))
                .cloned()
                .collect(),
        };
        // Save completed, unchanged units even if the next model call fails.
        coverage.units.retain(|unit| unit.complete);
        save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
        section_checkpoints::save(llm, content, &inventory.units, &coverage.units).await?;
        let checked = audit_coverage(llm, content, &corrected).await?;
        coverage.units.extend(checked.units);
        coverage.units.sort_by_key(|unit| unit.index);
        save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
        if coverage.units.iter().any(|unit| !unit.complete)
            && coverage_work(&coverage, &inventory, content) >= previous_work
        {
            return Err(AppError::Other(format!(
                "Claim coverage stopped making progress: {}. The draft and completed checks are saved; no lesson was published.",
                coverage
                    .units
                    .iter()
                    .filter(|unit| !unit.complete)
                    .map(|unit| format!("Section {}: {}", unit.index + 1, unit.reason))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
    }
    Ok((inventory, coverage))
}
