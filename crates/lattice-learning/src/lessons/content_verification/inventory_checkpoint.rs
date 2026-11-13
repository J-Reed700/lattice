//! Resuming, completing and checkpointing the claim inventory and its coverage audit.
use super::claim_ledger::Rewrites;
use super::*;
use lattice_core::domain::learning::claim_review::{identities, ClaimLedgerEntry, ExtractedClaim};

async fn save_inventory(
    llm: &dyn LLMPort,
    candidate: &str,
    inventory: &Inventory,
    coverage: Option<&Coverage>,
) -> Result<()> {
    let checkpoint = InventoryCheckpoint {
        policy: INVENTORY_POLICY.to_string(),
        model: llm.model_name().into(),
        model_context: llm.max_context_tokens(),
        content_sha256: sha256_hex(candidate),
        inventory: inventory.clone(),
        coverage: coverage.cloned().unwrap_or(Coverage { units: Vec::new() }),
        coverage_policy: coverage.map(|_| coverage::POLICY.to_string()),
    };
    crate::lesson_drafts::record_claim_inventory(candidate, serde_json::to_string(&checkpoint)?)
        .await
}

pub(super) async fn extract_audited_inventory(
    llm: &dyn LLMPort,
    content: &[Value],
    candidate: (&str, &Value),
    rewrites: &Rewrites,
) -> Result<(Inventory, Coverage)> {
    crate::lesson_progress::phase(
        crate::lesson_progress::Phase::Inventory,
        "Extracting factual statements from the lesson and assessments",
    );
    let (retained, coverage) = section_checkpoints::load(llm, content).await?;
    if !retained.is_empty() {
        crate::lesson_progress::stage(format!(
            "Reusing completed statement inventories for {} unchanged sections",
            retained.len()
        ));
    }
    let inventory = inventory::extract_remaining(llm, content, retained).await?;
    complete_inventory(llm, content, inventory, candidate, Some(coverage), rewrites).await
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
        let sentences: HashSet<String> =
            claim_sentences::ids(input["passages"].as_array().map_or(&[][..], Vec::as_slice))
                .into_iter()
                .collect();
        seen.insert(unit.index)
            && unit.complete == unit.missing_assertions.is_empty()
            && unit.missing_assertions.iter().all(|missing| {
                ids.contains(missing.passage_id.as_str())
                    && missing
                        .sentence_id
                        .as_ref()
                        .is_none_or(|sentence| sentences.contains(sentence))
                    && !missing.statement.trim().is_empty()
            })
    })
}

/// A gap the audit found in a passage the inventory reads is a statement the
/// lesson makes: it joins the inventory here, before the checks, and is
/// located, sorted and checked like any other. After the first pass only a gap
/// in a sentence written since the last is adopted, so the audit cannot widen
/// the inventory again and again (claim classes design §3.4).
pub(super) fn adopt_missing(
    inventory: &mut Inventory,
    coverage: &mut Coverage,
    content: &[Value],
    (candidate, rewrites): (&Value, &Rewrites),
) -> usize {
    let inputs = inventory::inputs(content);
    let mut adopted = 0;
    for unit in coverage.units.iter_mut().filter(|unit| !unit.complete) {
        let Some(passages) = inputs
            .get(unit.index)
            .and_then(|input| input["passages"].as_array())
        else {
            continue;
        };
        let Some(slot) = inventory
            .units
            .iter_mut()
            .find(|current| current.index == unit.index)
        else {
            continue;
        };
        let mut kept = Vec::new();
        let mut located: Vec<String> = Vec::new();
        for missing in std::mem::take(&mut unit.missing_assertions) {
            let claim = missing
                .sentence_id
                .as_deref()
                .and_then(|sentence| claim_sentences::quote(passages, sentence))
                .filter(|(passage, _)| *passage == missing.passage_id)
                .map(|(_, quote)| Claim {
                    quote,
                    statement: missing.statement.clone(),
                })
                .filter(|claim| {
                    let extracted = ExtractedClaim {
                        location: claim_ledger::locate(
                            candidate,
                            unit.index,
                            &claim.quote,
                            Some(&claim.statement),
                        ),
                        statement: claim.statement.clone(),
                    };
                    rewrites.is_new(candidate, &extracted)
                });
            let Some(claim) = claim else {
                kept.push(missing);
                continue;
            };
            if !slot.claims.iter().any(|existing| {
                existing.quote == claim.quote && existing.statement == claim.statement
            }) {
                slot.claims.push(claim);
                adopted += 1;
            }
            if !located.contains(&missing.passage_id) {
                located.push(missing.passage_id);
            }
        }
        *unit = CoverageUnit::from_missing(
            unit.index,
            kept,
            &format!(
                "Statements the audit found missing from the inventory in {} were recorded as claims for the checks.",
                located.join(", ")
            ),
        );
    }
    adopted
}

/// A claim the ledger holds whose sentence stands in the lesson stays in the
/// inventory, as it was extracted from that same text, when no statement of
/// the inventory continues it: an extraction that misses it is at fault, and
/// the claim is not ended for it (acceptance design §2.9). Who continues whom
/// is the pass's own rule, so what is kept is what identification would
/// otherwise end. How many claims were kept.
fn keep_held(inventory: &mut Inventory, candidate: &Value, held: &[ClaimLedgerEntry]) -> usize {
    if held.is_empty() {
        return 0;
    }
    let found: Vec<ExtractedClaim> = inventory
        .units
        .iter()
        .flat_map(|unit| {
            unit.claims.iter().map(move |claim| ExtractedClaim {
                location: claim_ledger::locate(
                    candidate,
                    unit.index,
                    &claim.quote,
                    Some(&claim.statement),
                ),
                statement: claim.statement.clone(),
            })
        })
        .collect();
    let texts: HashMap<(usize, String), String> = held
        .iter()
        .filter_map(|entry| {
            let at = entry.location();
            claim_ledger::field_text(candidate, at.unit_index, &at.field)
                .map(|text| ((at.unit_index, at.field.clone()), text))
        })
        .collect();
    let prior: Vec<&ClaimLedgerEntry> = held.iter().collect();
    let left = identities(&prior, &[], &found, |unit, field| {
        texts.get(&(unit, field.to_owned())).map(String::as_str)
    })
    .orphaned;
    let mut kept = 0;
    for entry in held.iter().filter(|entry| left.contains(entry.claim_id())) {
        let at = entry.location();
        let stands = claim_ledger::stands(candidate, at.unit_index, &at.field, &at.quote);
        let Some(unit) = inventory
            .units
            .iter_mut()
            .find(|unit| unit.index == at.unit_index)
            .filter(|_| stands)
        else {
            continue;
        };
        unit.claims.push(Claim {
            quote: at.quote.clone(),
            statement: entry.statement().to_owned(),
        });
        kept += 1;
    }
    kept
}

pub(super) async fn complete_inventory(
    llm: &dyn LLMPort,
    content: &[Value],
    mut inventory: Inventory,
    (candidate, parsed): (&str, &Value),
    saved_coverage: Option<Coverage>,
    rewrites: &Rewrites,
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
    let kept = keep_held(&mut inventory, parsed, rewrites.held());
    if kept > 0 {
        tracing::info!(
            "claim.inventory: kept {kept} claims the extraction left out, whose sentences stand"
        );
    }
    validate_inventory(&inventory, content)?;
    section_checkpoints::save(llm, content, &inventory.units, &coverage.units).await?;
    save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
    crate::lesson_progress::phase(
        crate::lesson_progress::Phase::Coverage,
        "Checking that every factual statement was captured",
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
    let adopted = adopt_missing(&mut inventory, &mut coverage, content, (parsed, rewrites));
    if adopted > 0 {
        crate::lesson_progress::stage(format!(
            "Recorded {adopted} statements the coverage audit found missing; the checks decide them"
        ));
        validate_inventory(&inventory, content)?;
        save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
    }
    section_checkpoints::save(llm, content, &inventory.units, &coverage.units).await?;
    Ok((inventory, coverage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::domain::learning::claim_review::{
        Actor, ClaimLocation, EventProvenance, UnitKind,
    };

    #[test]
    fn a_held_claim_with_a_long_quote_stays_held() {
        let long = format!("Alpha {}omega.", "beta gamma delta ".repeat(100));
        let candidate = json!({"blocks": [{"kind": "explanation", "title": "A", "body": long}], "questions": []});
        let held = ClaimLedgerEntry::extracted(
            ClaimId("c1".into()),
            EventProvenance {
                actor: Actor::Machine,
                at: 1,
                job_id: None,
                app_version: "test".into(),
                checker_model: None,
            },
            ClaimLocation::locate(0, UnitKind::Block, "body", long.clone(), &long),
            "A scoped factual statement.".into(),
        )
        .unwrap();
        let mut inventory = Inventory {
            units: vec![UnitClaims {
                index: 0,
                claims: vec![],
                non_factual_reason: "A section.".into(),
            }],
        };

        let kept = keep_held(&mut inventory, &candidate, &[held]);

        assert_eq!(kept, 1);
    }
}
