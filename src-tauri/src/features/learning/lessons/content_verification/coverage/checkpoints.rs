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

pub(super) fn mapping_key(llm: &dyn LLMPort, inputs: &[Value]) -> String {
    format!("coverage-mapping-v1:{}", key(llm, &json!(inputs)))
}

pub(super) fn fidelity_key(
    llm: &dyn LLMPort,
    unit: &Value,
    passage: &Value,
    statements: &[String],
) -> String {
    format!(
        "coverage-fidelity-v1:{}",
        key(
            llm,
            &json!({"unit":unit,"passage":passage,"selectedStatements":statements})
        )
    )
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedFidelity {
    verdict: ClaimVerdict,
    quote: Option<String>,
    reason: Option<String>,
    confidence: Option<f32>,
}

pub(super) async fn load_fidelity(key: &str) -> Result<Option<JudgeOutcome>> {
    Ok(crate::features::learning::lesson_drafts::checkpoint(key)
        .await?
        .and_then(|raw| serde_json::from_value::<SavedFidelity>(raw).ok())
        .filter(|saved| {
            saved.verdict != ClaimVerdict::Unverified
                && saved
                    .confidence
                    .is_none_or(|confidence| confidence.is_finite())
        })
        .map(|saved| JudgeOutcome {
            verdict: saved.verdict,
            quote: saved.quote,
            reason: saved.reason,
            confidence: saved.confidence,
        }))
}

pub(super) async fn save_fidelity(key: &str, finding: &JudgeOutcome) -> Result<()> {
    if finding.verdict != ClaimVerdict::Unverified {
        crate::features::learning::lesson_drafts::record_checkpoint(
            key,
            serde_json::to_value(SavedFidelity {
                verdict: finding.verdict,
                quote: finding.quote.clone(),
                reason: finding.reason.clone(),
                confidence: finding.confidence,
            })?,
        )
        .await?;
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconciledCoverage {
    finding_policy: String,
    coverage: CoverageUnit,
}

fn saved_coverage(value: Value) -> Option<CoverageUnit> {
    if let Ok(saved) = serde_json::from_value::<ReconciledCoverage>(value.clone()) {
        return (saved.finding_policy == "coverage-findings-v1").then_some(saved.coverage);
    }
    // Preserve completed coverage. Legacy negative mappings need the new
    // independent omission check before they can request another repair.
    serde_json::from_value::<CoverageUnit>(value)
        .ok()
        .filter(|saved| saved.complete)
}

pub(super) async fn load(
    llm: &dyn LLMPort,
    input: &Value,
    content: &[Value],
) -> Result<Option<CoverageUnit>> {
    Ok(
        crate::features::learning::lesson_drafts::checkpoint(&key(llm, input))
            .await?
            .and_then(saved_coverage)
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
            serde_json::to_value(ReconciledCoverage {
                finding_policy: "coverage-findings-v1".into(),
                coverage: checked.clone(),
            })?,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_positive_coverage_survives_but_negative_mappings_need_reconciliation() {
        let unit = |complete| CoverageUnit {
            index: 0,
            complete,
            unresolved_passages: if complete { vec![] } else { vec!["p0".into()] },
            reason: "A saved coverage finding.".into(),
        };
        assert!(saved_coverage(json!(unit(true))).is_some());
        assert!(saved_coverage(json!(unit(false))).is_none());
        let current = ReconciledCoverage {
            finding_policy: "coverage-findings-v1".into(),
            coverage: unit(false),
        };
        assert!(saved_coverage(json!(current)).is_some());
    }
}
