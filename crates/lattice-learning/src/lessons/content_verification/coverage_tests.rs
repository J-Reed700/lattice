#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::content_verification::tests::{fixture_completion, fixture_prompts, fixture_task};
use crate::lessons::course_generation_tests::ScriptedModel;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};

#[test]
fn the_fidelity_contract_reads_what_it_renders() {
    use lattice_core::application::services::model_contract::test_support::assert_round_trip;
    for missing_assertions in [
        Vec::new(),
        vec![UnrepresentedAssertion {
            sentence_id: "s0".into(),
            statement: "Water boils at 100 degrees.".into(),
        }],
    ] {
        assert_round_trip::<FidelityAudit>(&FidelityFinding {
            missing_assertions,
            analysis: "Judged.".into(),
        });
    }
}

fn represented() -> String {
    json!({"analysis":"Every assertion is represented.","missingAssertions":[]}).to_string()
}
/// A judge's answer naming `sentence` as the sentence of each of `statements`.
fn missing(sentence: &str, statements: &[&str]) -> String {
    let gaps: Vec<Value> = statements
        .iter()
        .map(|statement| json!({"sentenceId":sentence,"statement":statement}))
        .collect();
    json!({"analysis":"These assertions are absent.","missingAssertions":gaps}).to_string()
}
fn scripted(outputs: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: std::sync::Mutex::new(outputs.into()),
        prompts: std::sync::Mutex::new(Vec::new()),
    }
}

struct ConcurrentFidelityModel(std::sync::atomic::AtomicUsize);

#[async_trait::async_trait]
impl LLMPort for ConcurrentFidelityModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            assert!(prompt.starts_with(FIDELITY_SYSTEM));
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                futures::future::pending::<()>().await;
            }
            replies.push(represented());
        }
        Ok(fixture_completion(request, replies))
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
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }
}

fn judged_of(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[tokio::test]
async fn a_slow_fidelity_request_does_not_stop_other_checks_from_starting() {
    let model = ConcurrentFidelityModel(std::sync::atomic::AtomicUsize::new(0));
    let unit = json!({"index":0,
        "passages":(0..4).map(|i|json!({"id":format!("p-{i}"),"text":"Equal inputs guarantee identical results."})).collect::<Vec<_>>(),
        "claims":[{"id":"claim-a","statement":"Equal inputs produce identical results."}]});
    tokio::select! {
        result = check_fidelity(&model, &unit, None) => {
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
async fn audit_fails_fast_when_one_section_errors_instead_of_draining_slow_siblings() {
    struct FailFastModel;

    #[async_trait::async_trait]
    impl LLMPort for FailFastModel {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            if crate::lessons::structured_call::is_draft_request(request) {
                return Ok(CompletionResponse::from_text(
                    crate::lessons::structured_call::FIXTURE_DRAFT,
                ));
            }
            let prompts = fixture_prompts(request);
            let data = fixture_task(&prompts[0]).unwrap();
            // Section 0 hangs forever; section 1 fails immediately with a cut-off answer.
            if data["targetPassage"]["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("unit-0-"))
            {
                futures::future::pending::<()>().await;
            }
            Err(AppError::AnswerCutOff("fixture cut off".into()))
        }
        fn model_name(&self) -> &str {
            "fail-fast-coverage-fixture"
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.len() / 4
        }
        fn max_context_tokens(&self) -> usize {
            32_000
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
        fn enforces(
            &self,
            _schema: &lattice_core::application::services::model_contract::shape::Schema,
        ) -> bool {
            false
        }
    }

    let content = vec![
        json!({"kind": "teaching", "body": "Section zero states a fact."}),
        json!({"kind": "teaching", "body": "Section one states a fact."}),
    ];
    let inventory = Inventory {
        units: (0..2)
            .map(|index| UnitClaims {
                index,
                claims: vec![Claim {
                    quote: "states a fact.".into(),
                    statement: "states a fact.".into(),
                }],
                non_factual_reason: String::new(),
            })
            .collect(),
    };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        audit(&FailFastModel, &content, &inventory, None),
    )
    .await
    .expect("audit must fail fast instead of draining a stalled sibling");
    let error = result.err().expect("audit must fail");
    assert!(matches!(error, AppError::AnswerCutOff(_)), "{error}");
}

#[tokio::test]
async fn complete_mapping_cannot_override_a_named_missing_assertion() {
    let unit = json!({"index":0,"passages":[{"id":"a","text":"Equal inputs guarantee identical results."}],"claims":[{"id":"claim-a","statement":"Inputs are equal."},{"id":"claim-other","statement":"Unselected assertion about another condition."}]});
    let model = scripted(vec![missing(
        "a-sentence-0",
        &["Equal inputs guarantee identical results."],
    )]);
    let checked = check_fidelity(&model, &unit, None).await.unwrap();
    assert!(!checked.complete);
    assert_eq!(
        checked.missing_assertions,
        vec![MissingAssertion {
            passage_id: "a".into(),
            sentence_id: Some("a-sentence-0".into()),
            statement: "Equal inputs guarantee identical results.".into()
        }]
    );
    assert!(checked.reason.contains("a: Equal inputs guarantee"));
    let data = fixture_task(&model.prompts.lock().unwrap()[0]).unwrap();
    assert_eq!(data["targetPassage"]["id"], "a");
    assert_eq!(data["inventory"].as_array().unwrap().len(), 2);
    assert_eq!(data["section"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_malformed_judge_answer_is_asked_for_once_more_then_fails_as_unusable() {
    let unit = json!({"index":0,"passages":[{"id":"a","text":"A fact."}],"claims":[{"id":"c","statement":"A fact holds."}]});
    let corrected = scripted(vec!["not json".into(), represented()]);
    let checked = check_fidelity(&corrected, &unit, None).await.unwrap();
    assert!(checked.complete);
    let prompts = corrected.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 2);
    let unusable = scripted(vec![
        json!({"analysis":"","missingAssertions":[{"sentenceId":"a-sentence-0","statement":"   "}]})
            .to_string(),
        "still not json".into(),
    ]);
    let error = check_fidelity(&unusable, &unit, None).await.err().unwrap();
    assert!(matches!(error, AppError::ModelAnswerUnusable(_)), "{error}");
}

#[tokio::test]
async fn prompt_snapshot_fidelity_audit() {
    let unit = json!({"index":0,"passages":[{"id":"a","text":"Equal inputs guarantee identical results. A rerun changes nothing."}],"claims":[{"id":"claim-a","statement":"Inputs are equal."},{"id":"claim-other","statement":"A rerun changes nothing."}]});
    let inner = scripted(vec![represented()]);
    let model = crate::lessons::course_generation_tests::RequestLog::new(&inner);
    check_fidelity(&model, &unit, None).await.unwrap();
    model.assert_snapshot::<FidelityAudit>();
}

#[tokio::test]
async fn a_long_reason_or_missing_assertion_from_the_judge_is_read_not_asked_for_again() {
    let unit = json!({"index":0,"passages":[{"id":"a","text":"A fact."}],"claims":[{"id":"c","statement":"Another fact."}]});
    let statement = format!(
        "A fact {}holds.",
        "that the passage states under its own condition ".repeat(30)
    );
    let reply =
        json!({"analysis":"The passage states it. ".repeat(40),"missingAssertions":[{"sentenceId":"a-sentence-0","statement":statement}]})
            .to_string();
    let model = scripted(vec![reply.clone(), reply]);
    let checked = check_fidelity(&model, &unit, None).await;
    assert!(checked.is_ok(), "{checked:?}");
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
    assert_eq!(checked.unwrap().missing_assertions[0].statement, statement);
}

#[test]
fn the_fidelity_schema_orders_analysis_alphabetically_before_missing_assertions() {
    let schema = fidelity_schema(&["a-sentence-0".to_owned()]);
    let keys: Vec<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, vec!["analysis", "missingAssertions"]);
    assert!(
        keys[0] < keys[1],
        "reasoning key must sort alphabetically before missingAssertions for llama.cpp grammars"
    );
}

#[test]
fn the_fidelity_schema_bounds_no_text_but_a_blank() {
    let bounds = crate::lessons::course_generation_tests::length_bounds(&fidelity_schema(&[
        "a-sentence-0".to_owned(),
    ]));
    assert!(bounds.is_empty(), "{bounds:?}");
}

#[test]
fn the_fidelity_schema_accepts_only_a_sentence_of_the_passage() {
    let judge =
        jsonschema::JSONSchema::compile(&fidelity_schema(&["a-sentence-0".to_owned()])).unwrap();
    assert!(judge.is_valid(&json!({"analysis":"Represented.","missingAssertions":[]})));
    assert!(judge.is_valid(&json!({"analysis":"Absent.","missingAssertions":[{"sentenceId":"a-sentence-0","statement":"A fact."}]})));
    assert!(!judge.is_valid(&json!({"analysis":"Absent.","missingAssertions":[{"sentenceId":"b-sentence-0","statement":"A fact."}]})));
    assert!(!judge.is_valid(&json!({"analysis":"Represented.","missingAssertions":[{"sentenceId":"a-sentence-0","statement":""}]})));
    assert!(!judge.is_valid(&json!({"verdict":"supported"})));
}

#[tokio::test]
async fn proposed_omissions_are_replaced_by_the_judges_own_list() {
    for (reply, complete) in [
        (represented(), true),
        (
            missing(
                "p0-sentence-0",
                &["The result follows only after the full sequence."],
            ),
            false,
        ),
    ] {
        let model = scripted(vec![reply]);
        let unit = json!({"index":0,"passages":[{"id":"p0","text":"The documented result follows from the sequence."}],"claims":[{"id":"c0","statement":"The sequence produces the documented result."}]});
        let checked = check_fidelity(&model, &unit, None).await.unwrap();
        assert_eq!(checked.complete, complete);
        assert_eq!(checked.missing_assertions.is_empty(), complete);
        assert_eq!(model.prompts.lock().unwrap().len(), 1);
    }
    // An empty inventory is judged too: its gaps become the whole passage.
    let model = scripted(vec![missing(
        "p0-sentence-0",
        &["An unsupported assertion."],
    )]);
    let unit =
        json!({"index":0,"passages":[{"id":"p0","text":"An unsupported assertion."}],"claims":[]});
    let checked = check_fidelity(&model, &unit, None).await.unwrap();
    assert!(!checked.complete);
    assert_eq!(
        checked.missing_assertions[0].statement,
        "An unsupported assertion."
    );
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn statements_already_in_the_inventory_are_represented_by_construction() {
    let unit = json!({"index":0,"passages":[{"id":"p0","text":"Water boils at 100 °C at sea level. Boiling depends on pressure."}],"claims":[{"id":"c0","statement":"Water boils at 100 °C at sea level."}]});
    let model = scripted(vec![missing(
        "p0-sentence-0",
        &[
            "water boils at 100 °C   at sea level",
            "The boiling point depends on pressure.",
            "The boiling point depends on pressure.",
        ],
    )]);
    let checked = check_fidelity(&model, &unit, None).await.unwrap();
    assert_eq!(
        checked
            .missing_assertions
            .iter()
            .map(|m| m.statement.as_str())
            .collect::<Vec<_>>(),
        vec!["The boiling point depends on pressure."]
    );
}

#[tokio::test]
async fn assessment_fidelity_keeps_the_scenario_separate_from_the_inventory() {
    let model = scripted(vec![represented()]);
    let unit = json!({"index":0,"kind":"assessment","correctIndex":0,
        "passages":[
            {"id":"a","field":"/explanation","text":"Additional solute remains undissolved once a solution is saturated."}],
        "context":[
            {"id":"q","field":"/prompt","text":"Given that this sample is fully saturated, what happens to additional solute at unchanged temperature?"},
            {"id":"o0","field":"/options/0","text":"It remains undissolved."},
            {"id":"o1","field":"/options/1","text":"The extra solute disappears."}],
        "claims":[{"id":"selected","statement":"Additional solute remains undissolved in a saturated solution at unchanged temperature."},
                  {"id":"other","statement":"Unselected assertion about another condition."}]} );
    let checked = check_fidelity(&model, &unit, None).await.unwrap();
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    let data = fixture_task(&prompts[0]).unwrap();
    assert_eq!(data["targetPassage"]["field"], "/explanation");
    assert_eq!(data["assessment"]["correctIndex"], 0);
    assert!(data["assessment"]["question"][0]
        .as_str()
        .unwrap()
        .starts_with("Given that this sample is fully saturated"));
    let options = data["assessment"]["options"].to_string();
    assert!(options.contains("/options/1") && options.contains("The extra solute disappears."));
    assert!(!data["inventory"]
        .to_string()
        .contains("The extra solute disappears."));
    assert!(data["inventory"]
        .to_string()
        .contains("Unselected assertion"));
    assert!(checked.complete);
}

#[tokio::test]
async fn teaching_fidelity_sends_the_full_inventory_and_section_without_other_sections() {
    let units = vec![
        json!({"index":0,"kind":"teaching",
            "passages":[{"id":"scope","field":"/title","text":"A specified experimental group"},
                {"id":"target","field":"/body","text":"All samples received the same treatment."}],
            "claims":[{"id":"selected","statement":"Samples in this group received the same treatment."},
                {"id":"other","statement":"Unselected inventory assertion"}]}),
        json!({"index":1,"passages":[{"id":"instruction","text":"Record your observations."}],
            "claims":[{"id":"foreign","statement":"UNRELATED SECOND SECTION ASSERTION"}]}),
    ];
    let judged = judged_of(&["target"]);
    let model = scripted(vec![represented()]);
    let mut checked = Vec::new();
    for unit in &units {
        checked.push(check_fidelity(&model, unit, Some(&judged)).await.unwrap());
    }
    assert!(checked.iter().all(|unit| unit.complete));
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1, "nonfactual passages are not judged");
    let data = fixture_task(&prompts[0]).unwrap();
    assert_eq!(data["targetPassage"]["id"], "target");
    assert!(data["section"]
        .to_string()
        .contains("A specified experimental group"));
    assert!(data["inventory"]
        .to_string()
        .contains("Unselected inventory assertion"));
    assert!(!prompts[0].contains("UNRELATED SECOND SECTION ASSERTION"));
}

#[test]
fn fidelity_request_orders_section_constant_fields_before_target_passage() {
    let teaching = json!({
        "index": 0,
        "kind": "teaching",
        "passages": [
            {"id": "p0", "text": "Alpha."},
            {"id": "p1", "text": "Beta."}
        ],
        "claims": [{"id": "c0", "statement": "Alpha holds."}]
    });
    let teaching_req = fidelity_request(&teaching, &teaching["passages"][0]);
    let teaching_keys: Vec<&str> = teaching_req
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(teaching_keys, vec!["section", "inventory", "targetPassage"]);

    let assessment = json!({
        "index": 1,
        "kind": "assessment",
        "correctIndex": 0,
        "passages": [{"id": "a", "field": "/explanation", "text": "Because alpha."}],
        "context": [
            {"id": "q", "field": "/prompt", "text": "Why?"},
            {"id": "o0", "field": "/options/0", "text": "Alpha."}
        ],
        "claims": [{"id": "c0", "statement": "Alpha holds."}]
    });
    let assessment_req = fidelity_request(&assessment, &assessment["passages"][0]);
    let assessment_keys: Vec<&str> = assessment_req
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        assessment_keys,
        vec!["assessment", "inventory", "targetPassage"]
    );

    let p0_json = teaching_req.to_string();
    let p1_json = fidelity_request(&teaching, &teaching["passages"][1]).to_string();
    let prefix = "\"targetPassage\":";
    assert_eq!(
        p0_json.split_once(prefix).unwrap().0,
        p1_json.split_once(prefix).unwrap().0
    );
}

#[tokio::test]
async fn local_models_run_same_section_fidelity_passages_sequentially() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct LocalPromptCacheModel {
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl LLMPort for LocalPromptCacheModel {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            let mut replies = Vec::new();
            for prompt in fixture_prompts(request) {
                assert!(prompt.starts_with(FIDELITY_SYSTEM));
                let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_in_flight.fetch_max(now, Ordering::SeqCst);
                self.calls.fetch_add(1, Ordering::SeqCst);
                tokio::task::yield_now().await;
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                replies.push(represented());
            }
            Ok(fixture_completion(request, replies))
        }
        fn model_name(&self) -> &str {
            "local-cache-fixture"
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.len() / 4
        }
        fn max_context_tokens(&self) -> usize {
            32_000
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
        fn enforces(
            &self,
            _schema: &lattice_core::application::services::model_contract::shape::Schema,
        ) -> bool {
            false
        }
        fn keeps_read_prompts(&self) -> bool {
            true
        }
    }

    let model = LocalPromptCacheModel {
        in_flight: AtomicUsize::new(0),
        max_in_flight: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
    };
    let unit = json!({
        "index": 0,
        "passages": (0..4)
            .map(|i| json!({"id": format!("p-{i}"), "text": format!("Passage {i} guarantees identical results.")}))
            .collect::<Vec<_>>(),
        "claims": [{"id": "claim-a", "statement": "Paraphrased inventory claim."}]
    });
    let checked = check_fidelity(&model, &unit, None).await.unwrap();
    assert!(checked.complete);
    assert_eq!(model.calls.load(Ordering::SeqCst), 4);
    assert_eq!(model.max_in_flight.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn verbatim_passages_skip_fidelity_audit_while_paraphrased_or_extra_sentences_call_it() {
    let verbatim_unit = json!({
        "index": 0,
        "passages": [
            {"id": "p0", "text": "Water boils at 100 °C at sea level. Ice melts at 0 °C."},
            {"id": "p1", "text": "The reader\u{2019}s buffer holds one row."}
        ],
        "claims": [
            {"id": "c0", "statement": "Water boils at 100 °C at sea level."},
            {"id": "c1", "statement": "ice melts at 0 °C"},
            {"id": "c2", "statement": "The reader's buffer holds one row."}
        ]
    });
    let silent = scripted(vec![]);
    let checked = check_fidelity(&silent, &verbatim_unit, None).await.unwrap();
    assert!(checked.complete);
    assert!(checked.missing_assertions.is_empty());
    assert!(silent.prompts.lock().unwrap().is_empty());

    let partial_unit = json!({
        "index": 0,
        "passages": [
            {"id": "p0", "text": "Water boils at 100 °C at sea level."},
            {"id": "p1", "text": "Water boils at 100 °C at sea level. Pressure alters the boiling point."}
        ],
        "claims": [
            {"id": "c0", "statement": "Water boils at 100 °C at sea level."}
        ]
    });
    let model = scripted(vec![missing(
        "p1-sentence-1",
        &["Pressure alters the boiling point."],
    )]);
    let checked = check_fidelity(&model, &partial_unit, None).await.unwrap();
    assert!(!checked.complete);
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
    let sent = fixture_task(&model.prompts.lock().unwrap()[0]).unwrap();
    assert_eq!(sent["targetPassage"]["id"], "p1");
    assert_eq!(
        checked.missing_assertions,
        vec![MissingAssertion {
            passage_id: "p1".into(),
            sentence_id: Some("p1-sentence-1".into()),
            statement: "Pressure alters the boiling point.".into(),
        }]
    );
}
