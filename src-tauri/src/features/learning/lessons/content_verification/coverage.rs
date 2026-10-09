//! Account for every lossless lesson passage, not just a section-level summary.
use super::*;
use crate::features::learning::outline_progress::OutlineProgress;
use std::collections::BTreeMap;

mod checkpoints;
mod mapping;

pub(in crate::features::learning) const POLICY: &str = "passage-coverage-v5";
// Assessment interpretation changes independently of identical teaching checks.
pub(super) const ASSESSMENT_POLICY: &str = "assessment-fidelity-v3";
// Only previously negative teaching comparisons take this additional path.
pub(super) const TEACHING_CONTEXT_POLICY: &str = "teaching-context-v2";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PassageCheck {
    claim_ids: Vec<String>,
    non_factual_reason: String,
    missing_claims: Vec<String>,
}

fn failure() -> AppError {
    invalid("The coverage audit did not account for every lesson passage with valid claim locations. The draft is saved; no lesson was published.")
}

fn unit_input(input: &Value, unit: &UnitClaims) -> Value {
    let mut input = input.clone();
    if let Some(object) = input.as_object_mut() {
        object.insert("claims".into(), json!(unit.claims.iter().enumerate().map(|(ordinal, claim)| json!({"id":format!("unit-{}-claim-{ordinal}",unit.index),"statement":claim.statement})).collect::<Vec<_>>()));
        object.insert("nonFactualReason".into(), json!(unit.non_factual_reason));
    }
    input
}

fn schema(units: &[Value]) -> Value {
    let mut properties = serde_json::Map::new();
    for unit in units {
        let claim_ids: Vec<_> = unit["claims"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["id"].as_str())
            .collect();
        let claims = if claim_ids.is_empty() {
            json!({"type":"array","maxItems":0,"items":{"type":"string"}})
        } else {
            json!({"type":"array","maxItems":claim_ids.len(),"uniqueItems":true,"items":{"type":"string","enum":claim_ids}})
        };
        for passage in unit["passages"].as_array().into_iter().flatten() {
            if let Some(id) = passage["id"].as_str() {
                properties.insert(id.to_owned(), json!({"type":"object","additionalProperties":false,"required":["claimIds","nonFactualReason","missingClaims"],"properties":{
                    "claimIds":claims,"nonFactualReason":{"type":"string","maxLength":500},
                    "missingClaims":{"type":"array","items":{"type":"string","minLength":1,"maxLength":1000}}
                }}));
            }
        }
    }
    json!({"type":"object","additionalProperties":false,"required":properties.keys().collect::<Vec<_>>(),"properties":properties})
}

fn resolve(raw: &str, units: &[Value]) -> Result<Vec<CoverageUnit>> {
    let checks: BTreeMap<String, PassageCheck> =
        crate::features::learning::generation::parse_json(raw)?;
    let expected: usize = units
        .iter()
        .map(|unit| unit["passages"].as_array().map_or(0, Vec::len))
        .sum();
    if checks.len() != expected {
        return Err(failure());
    }
    let mut result = Vec::new();
    for unit in units {
        let index = unit["index"]
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .ok_or_else(failure)?;
        let claims: HashSet<_> = unit["claims"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["id"].as_str())
            .collect();
        let mut missing = Vec::new();
        let mut unresolved_passages = Vec::new();
        for passage in unit["passages"].as_array().into_iter().flatten() {
            let id = passage["id"].as_str().ok_or_else(failure)?;
            let check = checks.get(id).ok_or_else(failure)?;
            let selected: HashSet<_> = check.claim_ids.iter().collect();
            if selected.len() != check.claim_ids.len()
                || selected.iter().any(|id| !claims.contains(id.as_str()))
                || check.non_factual_reason.chars().count() > 500
                || check
                    .missing_claims
                    .iter()
                    .any(|s| s.trim().is_empty() || s.chars().count() > 1000)
                || (selected.is_empty()
                    && check.non_factual_reason.trim().is_empty()
                    && check.missing_claims.is_empty())
            {
                return Err(failure());
            }
            if !check.missing_claims.is_empty() {
                unresolved_passages.push(id.to_owned());
            }
            missing.extend(
                check
                    .missing_claims
                    .iter()
                    .map(|reason| format!("{id}: {reason}")),
            );
        }
        result.push(CoverageUnit {
            index,
            unresolved_passages,
            complete: missing.is_empty(),
            reason: if missing.is_empty() {
                "Every supplied passage has an explicit claim-coverage or nonfactual decision."
                    .into()
            } else {
                missing.join("; ")
            },
        });
    }
    Ok(result)
}

/// The mapping review is not its own proof of faithful representation. Reuse
/// the strict entailment judge on original bytes against the assertions selected
/// by that mapping; external source checking remains a separate later gate.
async fn check_fidelity(
    llm: &dyn LLMPort,
    raw: &str,
    units: &[Value],
    coverage: &mut [CoverageUnit],
) -> Result<()> {
    let decisions: BTreeMap<String, PassageCheck> =
        crate::features::learning::generation::parse_json(raw)?;
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Fidelity,
    );
    let mut checks = Vec::new();
    let mut unresolved = BTreeMap::<(usize, String), String>::new();
    for unit in units {
        let inventory_statements: Vec<String> = unit["claims"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|claim| claim["statement"].as_str().map(str::to_owned))
            .collect();
        let index = unit["index"]
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .ok_or_else(failure)?;
        for passage in unit["passages"].as_array().into_iter().flatten() {
            let id = passage["id"].as_str().ok_or_else(failure)?;
            let decision = decisions.get(id).ok_or_else(failure)?;
            if !decision.missing_claims.is_empty() {
                unresolved.insert(
                    (index, id.to_owned()),
                    format!("{id}: {}", decision.missing_claims.join("; ")),
                );
            }
            // A mapping omission is a proposal, not proof that the inventory
            // lacks the assertion. Judge the complete original passage against
            // the same-section inventory, even when the mapper selected no IDs.
            if inventory_statements.is_empty()
                || decision.claim_ids.is_empty() && decision.missing_claims.is_empty()
            {
                continue;
            }
            let text = passage["text"].as_str().ok_or_else(failure)?;
            // Keep the mapping in the receipt identity for compatibility with
            // saved checks. The comparison uses the complete same-section
            // inventory and interpretation context in one request.
            let statements: Vec<String> = unit["claims"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|claim| {
                    claim["id"]
                        .as_str()
                        .is_some_and(|id| decision.claim_ids.iter().any(|selected| selected == id))
                })
                .filter_map(|claim| claim["statement"].as_str().map(str::to_owned))
                .collect();
            let checker = &checker;
            let inventory_statements = inventory_statements.clone();
            let assessment_context = (unit["kind"] == "assessment").then(|| {
                json!({
                    "targetField": passage["field"],
                    "correctIndex": unit["correctIndex"],
                    "question": unit["passages"].as_array().into_iter().flatten()
                        .filter(|p| p["field"] == "/prompt")
                        .map(|p| p["text"].clone()).collect::<Vec<_>>(),
                    "options": unit["passages"].as_array().into_iter().flatten()
                        .chain(unit["distractors"].as_array().into_iter().flatten())
                        .filter(|p| p["field"].as_str().is_some_and(|f| f.starts_with("/options/")))
                        .map(|p| json!({"field":p["field"],"text":p["text"]})).collect::<Vec<_>>()
                })
                .to_string()
            });
            let teaching_context = (unit["kind"] == "teaching").then(|| {
                json!({"targetField":passage["field"],"section":unit["passages"]}).to_string()
            });
            let receipt = checkpoints::fidelity_key(llm, unit, passage, &statements);
            checks.push(async move {
                if let Some(saved) = checkpoints::load_fidelity(&receipt).await? {
                    return Ok::<_, AppError>((index, id, ClaimJudgment::Judged(saved)));
                }
                let _model_call = crate::features::learning::lesson_progress::model_call();
                let judgment = if let Some(context) = &assessment_context {
                    checker
                        .check_fidelity_in_context(text, &inventory_statements, context)
                        .await
                } else if let Some(context) = &teaching_context {
                    // Context interprets wording; it is never evidence that an
                    // omitted assertion exists in the extracted inventory.
                    checker
                        .check_fidelity_in_teaching_context(text, &inventory_statements, context)
                        .await
                } else {
                    checker
                        .check_passages_without_deadline(text, &inventory_statements)
                        .await
                };
                if let ClaimJudgment::Judged(finding) = &judgment {
                    checkpoints::save_fidelity(&receipt, finding).await?;
                }
                Ok((index, id, judgment))
            });
        }
    }
    let total = checks.len();
    let mut stream = futures::stream::iter(checks).buffer_unordered(3);
    let mut completed = 0;
    crate::features::learning::lesson_progress::stage(format!(
        "Comparing extracted claims with original wording · 0 of {total} passages"
    ));
    while let Some(result) = stream.next().await {
        let (index, id, result) = result?;
        let finding = match result {
            ClaimJudgment::Judged(finding) => finding,
            ClaimJudgment::Failed(error) => return Err(error),
            _ => return Err(AppError::ServiceNotAvailable("The claim-fidelity checker could not finish. The extracted claims and lesson draft are saved; no lesson was published.".into())),
        };
        if finding.verdict == ClaimVerdict::Supported {
            unresolved.remove(&(index, id.to_owned()));
        } else {
            let reason = format!(
                "{id}: The inventory does not fully represent the original assertions: {}",
                finding.reason.unwrap_or_default()
            );
            unresolved.insert((index, id.to_owned()), reason);
        }
        completed += 1;
        crate::features::learning::lesson_progress::stage(format!(
            "Comparing extracted claims with original wording · {completed} of {total} passages"
        ));
    }
    for unit in coverage {
        let findings: Vec<_> = unresolved
            .iter()
            .filter(|((index, _), _)| *index == unit.index)
            .collect();
        unit.unresolved_passages = findings.iter().map(|((_, id), _)| id.clone()).collect();
        unit.complete = findings.is_empty();
        unit.reason = if unit.complete {
            "Every factual passage is represented in the inventory; reported omissions were independently checked.".into()
        } else {
            findings
                .into_iter()
                .map(|(_, reason)| reason.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        };
    }
    Ok(())
}

pub(in crate::features::learning) async fn audit(
    llm: &dyn LLMPort,
    content: &[Value],
    inventory: &Inventory,
    progress: Option<&OutlineProgress>,
) -> Result<Coverage> {
    let inputs = inventory::inputs(content);
    let units = inventory
        .units
        .iter()
        .map(|unit| {
            inputs
                .get(unit.index)
                .map(|input| unit_input(input, unit))
                .ok_or_else(failure)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut audited = Vec::new();
    let mut pending = Vec::new();
    for unit in &units {
        if let Some(saved) = checkpoints::load(llm, unit, content).await? {
            audited.push(saved);
        } else {
            pending.push(unit.clone());
        }
    }
    for batch in pending.chunks(4) {
        crate::features::learning::lesson_progress::stage(format!(
            "Checking claim coverage · {} of {} sections reviewed",
            audited.len(),
            units.len()
        ));
        let mapping_key = checkpoints::mapping_key(llm, batch);
        let saved = crate::features::learning::lesson_drafts::checkpoint(&mapping_key)
            .await?
            .and_then(|value| value.as_str().map(str::to_owned))
            .filter(|raw| resolve(raw, batch).is_ok());
        let raw = if let Some(saved) = saved {
            saved
        } else {
            mapping::review(llm, batch, progress).await?
        };
        let mut checked = resolve(&raw, batch)?;
        // This mapping is unfinished representation work, never factual
        // approval. Save it before fidelity so a disconnect cannot change its
        // inputs and force completed passage comparisons to run again.
        crate::features::learning::lesson_drafts::record_checkpoint(&mapping_key, json!(raw))
            .await?;
        check_fidelity(llm, &raw, batch, &mut checked).await?;
        checkpoints::save(llm, batch, &checked).await?;
        // Persist a finished batch before the next model request. A later
        // interruption must not discard these independently audited sections.
        section_checkpoints::save(llm, content, &inventory.units, &checked).await?;
        audited.extend(checked);
    }
    audited.sort_by_key(|unit| unit.index);
    Ok(Coverage { units: audited })
}

#[cfg(test)]
pub(in crate::features::learning) async fn live_mapping_fixture(
    llm: &dyn LLMPort,
    fixture: &Value,
) -> Result<Value> {
    let units = fixture["units"].as_array().ok_or_else(failure)?;
    let raw = fixture["mapping"].to_string();
    let mut coverage = resolve(&raw, units)?;
    check_fidelity(llm, &raw, units, &mut coverage).await?;
    Ok(serde_json::to_value(coverage)?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    struct FidelityModel(&'static str);
    #[async_trait::async_trait]
    impl LLMPort for FidelityModel {
        async fn generate(
            &self,
            prompt: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<String> {
            assert!(prompt.starts_with("Audit claim fidelity."));
            assert!(prompt.contains("Claim: Equal inputs guarantee identical results."));
            Ok(self.0.into())
        }
        async fn generate_streaming(
            &self,
            _: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unreachable!()
        }
        fn model_name(&self) -> &str {
            "fidelity-protocol-fixture"
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.len() / 4
        }
        fn max_context_tokens(&self) -> usize {
            32000
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    struct ConcurrentFidelityModel(std::sync::atomic::AtomicUsize);

    #[async_trait::async_trait]
    impl LLMPort for ConcurrentFidelityModel {
        async fn generate(
            &self,
            prompt: &str,
            context: &[String],
            stops: Option<Vec<String>>,
        ) -> Result<String> {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                futures::future::pending::<()>().await;
            }
            FidelityModel("supported\nReason: The fixture assertions are represented.\nSource passage: passage-0")
                .generate(prompt, context, stops).await
        }
        async fn generate_streaming(
            &self,
            _: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unreachable!()
        }
        fn model_name(&self) -> &str {
            "concurrent-fidelity-fixture"
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.len() / 4
        }
        fn max_context_tokens(&self) -> usize {
            32000
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn a_slow_fidelity_request_does_not_stop_other_checks_from_starting() {
        let model = ConcurrentFidelityModel(std::sync::atomic::AtomicUsize::new(0));
        let units = vec![json!({"index":0,
            "passages":(0..4).map(|i|json!({"id":format!("p-{i}"),"text":"Equal inputs guarantee identical results."})).collect::<Vec<_>>(),
            "claims":[{"id":"claim-a","statement":"Equal inputs guarantee identical results."}]})];
        let raw = json!((0..4)
            .map(|i| (
                format!("p-{i}"),
                json!({"claimIds":["claim-a"],"nonFactualReason":"","missingClaims":[]})
            ))
            .collect::<BTreeMap<_, _>>())
        .to_string();
        let mut checked = resolve(&raw, &units).unwrap();
        tokio::select! {
            result = check_fidelity(&model, &raw, &units, &mut checked) => {
                panic!("The deliberately blocked first request finished: {result:?}")
            }
            result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while model.0.load(std::sync::atomic::Ordering::SeqCst) < 4 {
                    tokio::task::yield_now().await;
                }
            }) => result.expect("The fourth check was blocked by completion ordering"),
        }
    }

    #[tokio::test]
    async fn complete_mapping_cannot_override_missing_assertions_or_failed_fidelity() {
        let units = vec![
            json!({"index":0,"passages":[{"id":"a","text":"Equal inputs guarantee identical results."}],"claims":[{"id":"claim-a","statement":"Inputs are equal."},{"id":"claim-other","statement":"Unselected assertion about another condition."}]}),
        ];
        let raw = json!({"a":{"claimIds":["claim-a"],"nonFactualReason":"","missingClaims":[]}})
            .to_string();
        let mut coverage = resolve(&raw, &units).unwrap();
        assert!(coverage[0].complete);
        check_fidelity(&FidelityModel("unsupported\nReason: Equal inputs do not establish the claimed identical results.\nSource passage: none"), &raw, &units, &mut coverage).await.unwrap();
        assert!(!coverage[0].complete);
        assert_eq!(coverage[0].unresolved_passages, vec!["a"]);
        assert!(coverage[0].reason.contains("identical results"));
        assert!(check_fidelity(
            &FidelityModel("incomplete reply"),
            &raw,
            &units,
            &mut coverage
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn proposed_omissions_need_independent_fidelity_and_cannot_use_an_empty_inventory() {
        use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
        for (verdict, complete) in [("supported", true), ("unsupported", false)] {
            let model = ScriptedModel {
                outputs: std::sync::Mutex::new(vec![format!("{verdict}\nReason: Compared every original assertion against the complete inventory.\nSource passage: passage-0")].into()),
                prompts: Default::default(),
            };
            let units = vec![
                json!({"index":0,"passages":[{"id":"p0","text":"The documented result follows from the sequence."}],"claims":[{"id":"c0","statement":"The sequence produces the documented result."}]}),
            ];
            let raw = json!({"p0":{"claimIds":[],"missingClaims":["The result is allegedly absent."],"nonFactualReason":""}}).to_string();
            let mut checked = resolve(&raw, &units).unwrap();
            check_fidelity(&model, &raw, &units, &mut checked)
                .await
                .unwrap();
            assert_eq!(checked[0].complete, complete);
            assert_eq!(checked[0].unresolved_passages.is_empty(), complete);
            assert_eq!(model.prompts.lock().unwrap().len(), 1);
            assert!(!checked[0].reason.contains("allegedly"));
        }
        let empty = ScriptedModel {
            outputs: Default::default(),
            prompts: Default::default(),
        };
        let units = vec![
            json!({"index":0,"passages":[{"id":"p0","text":"An unsupported assertion."}],"claims":[]}),
        ];
        let raw = json!({"p0":{"claimIds":[],"missingClaims":["The assertion is missing."],"nonFactualReason":""}}).to_string();
        let mut checked = resolve(&raw, &units).unwrap();
        check_fidelity(&empty, &raw, &units, &mut checked)
            .await
            .unwrap();
        assert!(!checked[0].complete);
        assert_eq!(checked[0].unresolved_passages, vec!["p0"]);
        assert!(empty.prompts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn assessment_fidelity_keeps_the_scenario_separate_from_selected_evidence() {
        use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
        let model = ScriptedModel {
            outputs: std::sync::Mutex::new(vec!["supported\nReason: The answer preserves the supplied scenario.\nSource passage: passage-0".into()].into()),
            prompts: std::sync::Mutex::new(Vec::new()),
        };
        let units = vec![json!({"index":0,"kind":"assessment","correctIndex":0,
            "passages":[
                {"id":"q","field":"/prompt","text":"Given that this sample is fully saturated, what happens to additional solute at unchanged temperature?"},
                {"id":"a","field":"/options/0","text":"It remains undissolved."}],
            "distractors":[{"id":"wrong","field":"/options/1","text":"The extra solute disappears."}],
            "claims":[{"id":"selected","statement":"Additional solute remains undissolved in a saturated solution at unchanged temperature."},
                      {"id":"other","statement":"Unselected assertion about another condition."}]} )];
        let raw = json!({"q":{"claimIds":[],"nonFactualReason":"The exercise stipulates a sample.","missingClaims":[]},
            "a":{"claimIds":["selected"],"nonFactualReason":"","missingClaims":[]}}).to_string();
        let mut coverage = resolve(&raw, &units).unwrap();
        check_fidelity(&model, &raw, &units, &mut coverage)
            .await
            .unwrap();
        let prompts = model.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("Original assessment context"));
        assert!(prompts[0].contains("Given that this sample is fully saturated"));
        assert!(prompts[0].contains("\"targetField\":\"/options/0\""));
        assert!(prompts[0].contains("Claim: It remains undissolved."));
        let (context, evidence) = prompts[0].split_once("Source passages:").unwrap();
        assert!(context.contains("\"field\":\"/options/0\""));
        assert!(context.contains("\"field\":\"/options/1\""));
        assert!(context.contains("The extra solute disappears."));
        assert!(!evidence.contains("The extra solute disappears."));
        assert!(evidence.contains("Unselected assertion"));
        assert!(coverage[0].complete);
    }

    #[tokio::test]
    async fn teaching_fidelity_uses_full_inventory_and_context_in_one_comparison() {
        use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
        let units = vec![json!({"index":0,"kind":"teaching",
            "passages":[{"id":"scope","field":"/title","text":"A specified experimental group"},
                {"id":"target","field":"/body","text":"All samples received the same treatment."}],
            "claims":[{"id":"selected","statement":"Samples in this group received the same treatment."},
                {"id":"other","statement":"Unselected inventory assertion"}]})];
        let raw = json!({"scope":{"claimIds":[],"nonFactualReason":"Sets the section scope.","missingClaims":[]},
            "target":{"claimIds":["selected"],"nonFactualReason":"","missingClaims":[]}}).to_string();
        for (verdict, expected) in [("supported", true), ("unsupported", false)] {
            let response = |label| {
                format!("{label}\nReason: The comparison applies to the specified scope.\nSource passage: passage-0")
            };
            let model = ScriptedModel {
                outputs: std::sync::Mutex::new(std::iter::once(verdict).map(response).collect()),
                prompts: std::sync::Mutex::new(Vec::new()),
            };
            let mut checked = resolve(&raw, &units).unwrap();
            check_fidelity(&model, &raw, &units, &mut checked)
                .await
                .unwrap();
            assert_eq!(checked[0].complete, expected);
            let prompts = model.prompts.lock().unwrap();
            assert_eq!(prompts.len(), 1);
            assert!(prompts[0].contains("Original teaching context"));
            assert!(prompts[0].contains("Claim: All samples received the same treatment."));
            let (context, evidence) = prompts[0].split_once("Source passages:").unwrap();
            assert!(context.contains("A specified experimental group"));
            assert!(!evidence.contains("A specified experimental group"));
            assert!(evidence.contains("Unselected inventory assertion"));
        }
    }

    #[tokio::test]
    async fn incomplete_mapping_checks_the_same_section_inventory_once_without_borrowing_other_sections(
    ) {
        use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
        for (fallback_verdict, complete) in [("supported", true), ("unsupported", false)] {
            let model = ScriptedModel {
                outputs: std::sync::Mutex::new(vec![
                    format!("{fallback_verdict}\nReason: Checked the complete same-section inventory.\nSource passage: passage-1"),
                ].into()),
                prompts: std::sync::Mutex::new(Vec::new()),
            };
            let units = vec![
                json!({"index":0,"passages":[{"id":"target","text":"The sample contains dissolved material."}],
                    "claims":[{"id":"selected","statement":"The sample is liquid."},
                        {"id":"omitted-from-mapping","statement":"The sample contains dissolved material."}]}),
                json!({"index":1,"passages":[{"id":"instruction","text":"Record your observations."}],
                    "claims":[{"id":"foreign","statement":"UNRELATED SECOND SECTION ASSERTION"}]}),
            ];
            let raw = json!({"target":{"claimIds":["selected"],"nonFactualReason":"","missingClaims":[]},
                "instruction":{"claimIds":[],"nonFactualReason":"An exercise instruction.","missingClaims":[]}}).to_string();
            let mut checked = resolve(&raw, &units).unwrap();
            check_fidelity(&model, &raw, &units, &mut checked)
                .await
                .unwrap();
            assert_eq!(checked[0].complete, complete);
            let prompts = model.prompts.lock().unwrap();
            assert_eq!(prompts.len(), 1);
            assert!(prompts[0].contains("The sample contains dissolved material."));
            assert!(prompts[0].contains("[passage-1]"));
            assert!(prompts
                .iter()
                .all(|prompt| !prompt.contains("UNRELATED SECOND SECTION ASSERTION")));
        }
    }

    #[test]
    fn every_passage_needs_a_decision_and_claim_ids_cannot_cross_sections() {
        let units = vec![
            json!({"index":0,"passages":[{"id":"a","text":"A fact."},{"id":"b","text":"Another fact."}],"claims":[{"id":"claim-a"}]}),
            json!({"index":1,"passages":[{"id":"c","text":"Different fact."}],"claims":[{"id":"claim-c"}]}),
        ];
        let decision = |id: &str| json!({"claimIds":[id],"nonFactualReason":"","missingClaims":[]});
        let valid =
            json!({"a":decision("claim-a"),"b":decision("claim-a"),"c":decision("claim-c")});
        assert!(resolve(&valid.to_string(), &units).is_ok());
        let validator = jsonschema::JSONSchema::compile(&schema(&units)).unwrap();
        assert!(validator.is_valid(&valid));
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("b");
        assert!(resolve(&missing.to_string(), &units).is_err());
        assert!(!validator.is_valid(&missing));
        let mut wrong = valid.clone();
        wrong["b"] = decision("claim-c");
        assert!(resolve(&wrong.to_string(), &units).is_err());
        assert!(!validator.is_valid(&wrong));
        let mut empty = valid.clone();
        empty["b"]["claimIds"] = json!([]);
        assert!(resolve(&empty.to_string(), &units).is_err());
        let mut incomplete = valid;
        incomplete["b"]["missingClaims"] =
            json!(["The guaranteed consequence is missing from the inventory."]);
        let checked = resolve(&incomplete.to_string(), &units).unwrap();
        assert!(!checked[0].complete);
        assert!(checked[1].complete);
        assert!(checked[0].reason.contains("b: The guaranteed consequence"));
    }
}
