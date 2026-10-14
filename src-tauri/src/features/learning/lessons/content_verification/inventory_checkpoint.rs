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

/// The audit's own findings complete the inventory. Every assertion the
/// fidelity judge named as unrepresented becomes a claim located at the judged
/// passage, so representation closes here deterministically and the evidence
/// check decides whether the lesson may keep each one. Nothing is approved: an
/// adopted claim is checked, researched, corrected or removed like any other.
fn adopt_missing_assertions(
    inventory: &mut Inventory,
    coverage: &mut Coverage,
    content: &[Value],
) -> Result<usize> {
    let inputs = inventory::inputs(content);
    let mut adopted = 0;
    for unit in coverage.units.iter_mut().filter(|unit| !unit.complete) {
        let passages = inputs
            .get(unit.index)
            .and_then(|input| input["passages"].as_array())
            .ok_or_else(|| invalid("Unknown lesson section in coverage adoption."))?;
        let slot = inventory
            .units
            .iter_mut()
            .find(|current| current.index == unit.index)
            .ok_or_else(|| invalid("Unknown inventory section in coverage adoption."))?;
        let mut located = Vec::new();
        for missing in std::mem::take(&mut unit.missing_assertions) {
            let quote = passages
                .iter()
                .find(|passage| passage["id"] == missing.passage_id)
                .and_then(|passage| passage["text"].as_str())
                .ok_or_else(|| invalid("A coverage finding names an unknown passage."))?;
            let claim = Claim {
                quote: quote.into(),
                statement: missing.statement,
            };
            if !slot
                .claims
                .iter()
                .any(|existing| existing.quote == claim.quote && existing.statement == claim.statement)
            {
                slot.claims.push(claim);
                adopted += 1;
            }
            if !located.contains(&missing.passage_id) {
                located.push(missing.passage_id);
            }
        }
        unit.complete = true;
        unit.reason = format!(
            "Assertions the fidelity audit found missing from the inventory in {} were recorded as claims for the evidence check.",
            located.join(", ")
        );
    }
    Ok(adopted)
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
    let adopted = adopt_missing_assertions(&mut inventory, &mut coverage, content)?;
    if adopted > 0 {
        crate::features::learning::lesson_progress::stage(format!(
            "Recorded {adopted} assertions the coverage audit found missing; the evidence check decides them"
        ));
        validate_inventory(&inventory, content)?;
    }
    save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
    section_checkpoints::save(llm, content, &inventory.units, &coverage.units).await?;
    Ok((inventory, coverage))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    fn content() -> Vec<Value> {
        vec![
            json!({"kind":"teaching","body":"The documented result follows. Explain why in your own words."}),
            json!({"kind":"teaching","body":"Submit a short report."}),
        ]
    }
    fn inventory() -> Inventory {
        Inventory {
            units: vec![
                UnitClaims {
                    index: 0,
                    claims: vec![Claim {
                        quote: "The documented result follows. Explain why in your own words.".into(),
                        statement: "The documented result follows.".into(),
                    }],
                    non_factual_reason: String::new(),
                },
                UnitClaims {
                    index: 1,
                    claims: Vec::new(),
                    non_factual_reason: "A submission instruction.".into(),
                },
            ],
        }
    }
    fn missing(passage_id: &str, statement: &str) -> MissingAssertion {
        MissingAssertion {
            passage_id: passage_id.into(),
            statement: statement.into(),
        }
    }

    #[test]
    fn named_gaps_become_located_claims_and_complete_their_section() {
        let mut inventory = inventory();
        let mut coverage = Coverage {
            units: vec![
                CoverageUnit {
                    index: 0,
                    complete: false,
                    reason: "unit-0-passage-0: The result is documented.".into(),
                    missing_assertions: vec![
                        missing("unit-0-passage-0", "The result is documented."),
                        missing("unit-0-passage-0", "The result is documented."),
                        missing("unit-0-passage-0", "The documented result follows."),
                    ],
                },
                CoverageUnit {
                    index: 1,
                    complete: true,
                    reason: "Only an instruction.".into(),
                    missing_assertions: Vec::new(),
                },
            ],
        };
        assert!(valid_saved_coverage(&coverage, &content()));
        let adopted = adopt_missing_assertions(&mut inventory, &mut coverage, &content()).unwrap();
        assert_eq!(adopted, 1, "duplicates and existing statements are not re-added");
        assert_eq!(inventory.units[0].claims.len(), 2);
        assert_eq!(inventory.units[0].claims[1].statement, "The result is documented.");
        assert_eq!(
            inventory.units[0].claims[1].quote,
            "The documented result follows. Explain why in your own words."
        );
        assert!(coverage.units.iter().all(|unit| unit.complete));
        assert!(coverage.units[0].missing_assertions.is_empty());
        assert!(coverage.units[0].reason.contains("unit-0-passage-0"));
        assert_eq!(coverage.units[1].reason, "Only an instruction.");
        assert!(validate_inventory(&inventory, &content()).is_ok());
        assert!(valid_saved_coverage(&coverage, &content()));
    }

    #[test]
    fn an_unknown_passage_cannot_locate_an_adopted_claim() {
        let mut inventory = inventory();
        let mut coverage = Coverage {
            units: vec![CoverageUnit {
                index: 0,
                complete: false,
                reason: "unit-9-passage-9: Elsewhere.".into(),
                missing_assertions: vec![missing("unit-9-passage-9", "Elsewhere.")],
            }],
        };
        assert!(!valid_saved_coverage(&coverage, &content()));
        assert!(adopt_missing_assertions(&mut inventory, &mut coverage, &content()).is_err());
        let inconsistent = Coverage {
            units: vec![CoverageUnit {
                index: 0,
                complete: true,
                reason: "Claims complete but a gap remains listed.".into(),
                missing_assertions: vec![missing("unit-0-passage-0", "A gap.")],
            }],
        };
        assert!(!valid_saved_coverage(&inconsistent, &content()));
    }
}
