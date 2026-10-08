//! Retain independent representation checks for byte-identical sections across
//! repairs and restarts. These are not factual approvals: current references,
//! execution, teaching review, and the final publication gate still apply.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionCheckpoint {
    inventory: UnitClaims,
    coverage: CoverageUnit,
}

fn key(llm: &dyn LLMPort, content: &Value, index: usize) -> String {
    format!(
        "inventory-section-v1:{}",
        digest(
            &json!({
                "inventoryPolicy": INVENTORY_POLICY,
                "coveragePolicy": coverage::POLICY,
                "assessmentPolicy": coverage::ASSESSMENT_POLICY,
                "teachingPolicy": coverage::TEACHING_CONTEXT_POLICY,
                "model": llm.model_name(), "context": llm.max_context_tokens(),
                "index": index, "content": content,
            })
            .to_string()
        )
    )
}

impl SectionCheckpoint {
    fn valid(&self, content: &Value, index: usize) -> bool {
        let unit = &self.inventory;
        unit.index == index
            && self.coverage.index == index
            && self.coverage.complete
            && self.coverage.unresolved_passages.is_empty()
            && unit.claims.len() <= 24
            && unit.non_factual_reason.chars().count() <= 500
            && (!unit.claims.is_empty() || !unit.non_factual_reason.trim().is_empty())
            && unit.claims.iter().all(|claim| {
                !claim.quote.trim().is_empty()
                    && claim.quote.chars().count() <= 1600
                    && contains_text(content, &claim.quote)
                    && !claim.statement.trim().is_empty()
                    && claim.statement.chars().count() <= 2000
            })
    }
}

pub(super) async fn load(
    llm: &dyn LLMPort,
    content: &[Value],
) -> Result<(Vec<UnitClaims>, Coverage)> {
    let mut inventories = Vec::new();
    let mut audits = Vec::new();
    for (index, section) in content.iter().enumerate() {
        if let Some(saved) =
            crate::features::learning::lesson_drafts::checkpoint(&key(llm, section, index))
                .await?
                .and_then(|value| serde_json::from_value::<SectionCheckpoint>(value).ok())
                .filter(|saved| saved.valid(section, index))
        {
            inventories.push(saved.inventory);
            audits.push(saved.coverage);
        }
    }
    Ok((inventories, Coverage { units: audits }))
}

pub(super) async fn save(
    llm: &dyn LLMPort,
    content: &[Value],
    inventory: &[UnitClaims],
    coverage: &[CoverageUnit],
) -> Result<()> {
    for audit in coverage {
        let Some(section) = content.get(audit.index) else {
            continue;
        };
        let Some(unit) = inventory.iter().find(|unit| unit.index == audit.index) else {
            continue;
        };
        let saved = SectionCheckpoint {
            inventory: unit.clone(),
            coverage: audit.clone(),
        };
        if saved.valid(section, audit.index) {
            inventory::revisions::save(llm, content, unit).await?;
            crate::features::learning::lesson_drafts::record_checkpoint(
                &key(llm, section, audit.index),
                serde_json::to_value(saved)?,
            )
            .await?;
        }
    }
    Ok(())
}
