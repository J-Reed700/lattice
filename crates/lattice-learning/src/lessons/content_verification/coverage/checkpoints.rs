//! Keep completed coverage findings, including omissions, across interruptions.
//! Unlike a completed section inventory, these receipts bind the exact extracted
//! assertions as well as the original section. A section whose audit found a gap
//! keeps its receipt: the gap is a finding, not an addition to the inventory.
use super::*;
use lattice_core::shared::hash::sha256_hex;

fn key(llm: &dyn LLMPort, input: &Value) -> String {
    format!(
        "coverage-unit-v2:{}",
        sha256_hex(
            json!({
                "inventoryPolicy": INVENTORY_POLICY,
                "coveragePolicy": POLICY,
                "model": llm.model_name(), "context": llm.max_context_tokens(),
                "unit": input,
            })
            .to_string()
        )
    )
}

pub(super) fn mapping_key(llm: &dyn LLMPort, inputs: &[Value]) -> String {
    format!("coverage-mapping-v2:{}", key(llm, &json!(inputs)))
}

pub(super) fn fidelity_key(llm: &dyn LLMPort, unit: &Value, passage: &Value) -> String {
    format!(
        "coverage-fidelity-v2:{}",
        key(llm, &json!({"unit":unit,"passage":passage}))
    )
}

pub(super) async fn load_fidelity(key: &str) -> Result<Option<FidelityFinding>> {
    Ok(crate::lesson_drafts::checkpoint(key)
        .await?
        .and_then(|raw| serde_json::from_value::<FidelityFinding>(raw).ok())
        .filter(FidelityFinding::valid))
}

pub(super) async fn save_fidelity(key: &str, finding: &FidelityFinding) -> Result<()> {
    crate::lesson_drafts::record_checkpoint(key, serde_json::to_value(finding)?).await
}

pub(super) async fn load(
    llm: &dyn LLMPort,
    input: &Value,
    content: &[Value],
) -> Result<Option<CoverageUnit>> {
    Ok(crate::lesson_drafts::checkpoint(&key(llm, input))
        .await?
        .and_then(|value| serde_json::from_value::<CoverageUnit>(value).ok())
        .filter(|saved| {
            input["index"].as_u64() == Some(saved.index as u64)
                && !saved.reason.trim().is_empty()
                && valid_saved_coverage(
                    &Coverage {
                        units: vec![saved.clone()],
                    },
                    content,
                )
        }))
}

pub(super) async fn save(
    llm: &dyn LLMPort,
    inputs: &[Value],
    coverage: &[CoverageUnit],
) -> Result<()> {
    for checked in coverage {
        let input = inputs
            .iter()
            .find(|input| input["index"].as_u64() == Some(checked.index as u64))
            .ok_or_else(failure)?;
        crate::lesson_drafts::record_checkpoint(&key(llm, input), serde_json::to_value(checked)?)
            .await?;
    }
    Ok(())
}
