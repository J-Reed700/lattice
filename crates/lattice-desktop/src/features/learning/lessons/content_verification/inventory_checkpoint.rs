//! Resuming, completing and checkpointing the claim inventory and its coverage audit.
use super::*;

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
            && unit.complete == unit.missing_assertions.is_empty()
            && unit.missing_assertions.iter().all(|missing| {
                ids.contains(missing.passage_id.as_str())
                    && !missing.statement.trim().is_empty()
                    && missing.statement.chars().count() <= 1000
            })
    })
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
            .extend(coverage::audit(llm, content, &pending, None).await?.units);
        coverage.units.sort_by_key(|unit| unit.index);
    }
    save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
    section_checkpoints::save(llm, content, &inventory.units, &coverage.units).await?;
    Ok((inventory, coverage))
}
