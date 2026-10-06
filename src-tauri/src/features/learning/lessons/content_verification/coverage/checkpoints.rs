//! Keep completed coverage findings, including omissions, across interruptions.
//! Unlike a completed section inventory, these receipts bind the exact extracted
//! assertions as well as the original section. Corrections must be audited anew.
use super::*;

fn key(llm: &dyn LLMPort, input: &Value) -> String {
    format!(
        "coverage-unit-v1:{}",
        digest(
            &json!({
                "inventoryPolicy": INVENTORY_POLICY,
                "coveragePolicy": POLICY,
                "assessmentPolicy": ASSESSMENT_POLICY,
                "teachingPolicy": TEACHING_CONTEXT_POLICY,
                "model": llm.model_name(), "context": llm.max_context_tokens(),
                "unit": input,
            })
            .to_string()
        )
    )
}

pub(super) async fn load(
    llm: &dyn LLMPort,
    input: &Value,
    content: &[Value],
) -> Result<Option<CoverageUnit>> {
    Ok(
        crate::features::learning::lesson_drafts::checkpoint(&key(llm, input))
            .await?
            .and_then(|value| serde_json::from_value::<CoverageUnit>(value).ok())
            .filter(|saved| {
                input["index"].as_u64() == Some(saved.index as u64)
                    && !saved.reason.trim().is_empty()
                    && (saved.complete || !saved.unresolved_passages.is_empty())
                    && valid_saved_coverage(
                        &Coverage {
                            units: vec![saved.clone()],
                        },
                        content,
                    )
            }),
    )
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
        crate::features::learning::lesson_drafts::record_checkpoint(
            &key(llm, input),
            serde_json::to_value(checked)?,
        )
        .await?;
    }
    Ok(())
}
