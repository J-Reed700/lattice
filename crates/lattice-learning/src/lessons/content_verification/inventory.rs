//! Claim locations are selected from lossless, app-owned text passages.
use super::*;
use lattice_core::application::services::model_contract::{Contract, ContractId, PromptInput};
use lattice_core::shared::text::segmentation::{
    partition, Chars, Overflow, Plan, Prose, Rung, Strength,
};
use std::collections::BTreeMap;

pub(super) mod revisions;

mod batches;
use batches::ask;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExtractedClaim {
    sentence_id: String,
    statement: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExtractedUnit {
    index: usize,
    claims: Vec<ExtractedClaim>,
    non_factual_reason: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExtractionResponse {
    #[serde(deserialize_with = "unique_units")]
    units: BTreeMap<String, Value>,
}

/// The call that reads a batch of sections for their claims.
pub(crate) struct InventoryExtraction;

impl Contract for InventoryExtraction {
    const ID: ContractId = ContractId::new("inventory-extraction");
    type Input = PromptInput;
}

impl InventoryExtraction {
    /// The units of a reply, or what is wrong with its shape.
    fn read(raw: &str) -> Result<ExtractionResponse> {
        crate::generation::parse_json(raw).map_err(|error| {
            AppError::InvalidInput(format!(
                "Return a units object with unique requested unit-N keys: {error}"
            ))
        })
    }
}

#[cfg(any(test, feature = "test-support"))]
impl lattice_core::application::services::model_contract::test_support::RenderAnswer
    for InventoryExtraction
{
    type Answer = ExtractionResponse;
    fn render(answer: &ExtractionResponse) -> String {
        serde_json::to_string(answer).unwrap_or_default()
    }
    fn read(text: &str) -> Result<ExtractionResponse> {
        Self::read(text)
    }
}

fn unique_units<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, Value>, D::Error> {
    struct Units;
    impl<'de> serde::de::Visitor<'de> for Units {
        type Value = BTreeMap<String, Value>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("one inventory per application-assigned unit key")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut units = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, Value>()? {
                if units.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate unit inventory"));
                }
            }
            Ok(units)
        }
    }
    deserializer.deserialize_map(Units)
}

#[cfg(test)]
pub(super) fn fixture_response(units: Vec<Value>) -> Value {
    let units: serde_json::Map<_, _> = units
        .into_iter()
        .map(|mut unit| {
            let index = unit
                .as_object_mut()
                .and_then(|fields| fields.remove("index"))
                .unwrap_or(Value::Null);
            (format!("unit-{index}"), unit)
        })
        .collect();
    json!({"units": units})
}

/// How a string field is cut into passages: at the latest paragraph break that
/// leaves one past 600 characters, else at the latest sentence break within
/// 1,400; a sentence longer than that is a passage of its own.
const PASSAGE_PLAN: Plan<'static> = Plan {
    max: 1_400,
    ladder: &[
        Rung {
            at_least: Strength::Paragraph,
            floor: 0.43,
        },
        Rung {
            at_least: Strength::Sentence,
            floor: 0.0,
        },
    ],
    overflow: Overflow::Extend,
};

fn passages(value: &Value, path: &str, index: usize, output: &mut Vec<Value>) {
    match value {
        Value::String(text) => {
            for span in partition(text, &Prose, &Chars, &PASSAGE_PLAN) {
                output.push(json!({"id":format!("unit-{index}-passage-{}",output.len()),"field":path,"text":text.get(span).unwrap_or_default()}));
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                passages(item, &format!("{path}/{i}"), index, output);
            }
        }
        Value::Object(items) => {
            for (key, item) in items.iter().filter(|(key, _)| key.as_str() != "kind") {
                passages(item, &format!("{path}/{key}"), index, output);
            }
        }
        _ => {}
    }
}

/// What a check unit's passages are, for the extractor.
const CHECK_ROLE: &str = "This section is a check. Before its 'Check your thinking' line it poses a case for the learner to reason through: the case's inputs are lesson-internal, not universal facts. After that line it gives the answer, the reasoning, and misconceptions a learner might hold, each named as wrong. Extract what the lesson asserts about a misconception (that it fails, and why it fails); never extract the misconception itself as a true fact.";

/// What the extractor is told of a question and its options.
const CONTEXT_ROLE: &str = "The question and its options, for reading its explanation by. They are not passages and no claim is located in them: never extract an option as a fact. The selected answer is correctIndex.";

/// Why a section with nothing to read has no claims.
const NOTHING_TO_READ: &str = "Nothing in this section is read for claims: it asks something of the learner and states no fact.";

/// A question and its options as context, each field with an id of its own.
fn context(unit: &Value, index: usize) -> Vec<Value> {
    let options = unit["options"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(ordinal, option)| (format!("/options/{ordinal}"), option));
    std::iter::once(("/prompt".to_owned(), &unit["prompt"]))
        .chain(options)
        .filter_map(|(field, text)| text.as_str().map(|text| (field, text)))
        .enumerate()
        .map(|(position, (field, text))| {
            json!({"id":format!("unit-{index}-context-{position}"),"field":field,"text":text})
        })
        .collect()
}

/// What the extractor reads: a block's body and a question's answer
/// explanation, which is where the lesson states what is so. A heading, a
/// rubric, a reflection's prompts and a question's stem and options are the
/// lesson's own scaffolding, never read for claims (claim classes design §3.4).
pub(crate) fn inputs(content: &[Value]) -> Vec<Value> {
    content.iter().enumerate().map(|(index, unit)| {
        let mut text = Vec::new();
        if unit["role"] != "reflection" {
            passages(&unit["body"], "/body", index, &mut text);
        }
        passages(&unit["explanation"], "/explanation", index, &mut text);
        for (slot, reason) in unit["rationales"].as_array().into_iter().flatten().enumerate() {
            if reason.as_str().is_some_and(|reason| !reason.trim().is_empty()) {
                passages(reason, &format!("/rationales/{slot}"), index, &mut text);
            }
        }
        let mut input = json!({"index":index,"kind":unit.get("kind"),"correctIndex":unit.get("correctIndex"),"passages":text});
        // A check names misconceptions as wrong below its reveal line
        // (learning science lessons §3.3), as a quiz names wrong options: the
        // extractor must not take one for an assertion and send it to be
        // "corrected" into a true statement.
        let is_check = unit["kind"] == "teaching"
            && unit["body"].as_str().and_then(crate::generation::split_check).is_some();
        if let (true, Some(object)) = (is_check, input.as_object_mut()) {
            object.insert("checkRole".into(), json!(CHECK_ROLE));
        }
        if let (true, Some(object)) = (unit["kind"] == "assessment", input.as_object_mut()) {
            object.insert("context".into(), json!(context(unit, index)));
            object.insert("contextRole".into(), json!(CONTEXT_ROLE));
        }
        input
    }).collect()
}

fn schema(indices: &[usize], input: &[Value]) -> Value {
    // App-owned slots prevent a constrained decoder from spending an array
    // entry on a duplicate index or choosing a branch before its index field.
    let slots: serde_json::Map<_, _> = indices.iter().map(|index| {
        let ids = claim_sentences::ids(
            input.get(*index)
                .and_then(|unit| unit.get("passages")).and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice),
        );
        (format!("unit-{index}"), json!({"type":"object","additionalProperties":false,"required":["claims","nonFactualReason"],"properties":{"nonFactualReason":{"type":"string"},"claims":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["sentenceId","statement"],"properties":{"sentenceId":{"type":"string","enum":ids},"statement":{"type":"string","minLength":1}}}}}}))
    }).collect();
    json!({"type":"object","additionalProperties":false,"required":["units"],"properties":{"units":{"type":"object","additionalProperties":false,"required":slots.keys().collect::<Vec<_>>(),"properties":slots}}})
}

fn resolve(unit: ExtractedUnit, input: &[Value]) -> std::result::Result<UnitClaims, String> {
    let passages = input
        .get(unit.index)
        .and_then(|v| v.get("passages"))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Unknown section {}", unit.index))?;
    if unit.claims.is_empty() && unit.non_factual_reason.trim().is_empty() {
        return Err(format!(
            "Section {} needs claims or an explicit nonfactual reason",
            unit.index
        ));
    }
    let mut claims = Vec::new();
    for claim in unit.claims {
        let (_, sentence) =
            claim_sentences::quote(passages, &claim.sentence_id).ok_or_else(|| {
                format!(
                    "Section {} has an unknown sentence ID {}",
                    unit.index, claim.sentence_id
                )
            })?;
        if claim.statement.trim().is_empty() {
            return Err(format!("Section {} has an empty statement", unit.index));
        }
        claims.push(Claim {
            quote: sentence,
            statement: claim.statement,
        });
    }
    Ok(UnitClaims {
        index: unit.index,
        claims,
        non_factual_reason: unit.non_factual_reason,
    })
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) async fn extract(llm: &dyn LLMPort, content: &[Value]) -> Result<Inventory> {
    extract_scoped(
        llm,
        content,
        &(0..content.len()).collect::<Vec<_>>(),
        BTreeMap::new(),
    )
    .await
}

pub(super) async fn extract_remaining(
    llm: &dyn LLMPort,
    content: &[Value],
    retained: Vec<UnitClaims>,
) -> Result<Inventory> {
    let mut retained: BTreeMap<_, _> = retained
        .into_iter()
        .map(|unit| (unit.index, unit))
        .collect();
    for index in 0..content.len() {
        if let std::collections::btree_map::Entry::Vacant(entry) = retained.entry(index) {
            if let Some(updated) = revisions::update(llm, content, index).await? {
                entry.insert(updated);
            }
        }
    }
    let missing: Vec<_> = (0..content.len())
        .filter(|index| !retained.contains_key(index))
        .collect();
    extract_scoped(llm, content, &missing, retained).await
}

async fn extract_scoped(
    llm: &dyn LLMPort,
    content: &[Value],
    indices: &[usize],
    mut accepted: BTreeMap<usize, UnitClaims>,
) -> Result<Inventory> {
    let input = inputs(content);
    // A section with nothing to read has no claims to ask the model for.
    for index in indices {
        let unread = input
            .get(*index)
            .and_then(|unit| unit["passages"].as_array())
            .is_some_and(|passages| claim_sentences::ids(passages).is_empty());
        if unread {
            accepted.insert(
                *index,
                UnitClaims {
                    index: *index,
                    claims: Vec::new(),
                    non_factual_reason: NOTHING_TO_READ.into(),
                },
            );
        }
    }
    let pending: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|index| !accepted.contains_key(index))
        .collect();
    ask(
        llm,
        (content, input.as_slice()),
        &pending,
        &mut accepted,
        true,
    )
    .await?;
    let inventory = Inventory {
        units: accepted.into_values().collect(),
    };
    validate_inventory(&inventory, content)?;
    Ok(inventory)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;
    use crate::lessons::course_generation_tests::ScriptedModel;
    use lattice_core::shared::persistence::Database;
    use std::sync::Mutex;

    /// An answer that locates one claim at the first sentence of `passage`.
    fn answer(index: usize, passage: &str) -> Value {
        json!({"index":index,"claims":[{"sentenceId":format!("{passage}-sentence-0"),"statement":"A scoped factual statement."}],"nonFactualReason":""})
    }
    fn model(responses: Vec<Value>) -> ScriptedModel {
        ScriptedModel {
            outputs: Mutex::new(responses.into_iter().map(|v| v.to_string()).collect()),
            prompts: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn the_extraction_contract_reads_what_it_renders() {
        let answer = ExtractionResponse {
            units: BTreeMap::from([(
                "unit-0".to_owned(),
                json!({"claims":[],"nonFactualReason":"A label."}),
            )]),
        };
        lattice_core::application::services::model_contract::test_support::assert_round_trip::<
            InventoryExtraction,
        >(&answer);
    }

    #[tokio::test]
    async fn an_unreadable_inventory_fails_the_job_instead_of_deferring() -> Result<()> {
        let pool = crate::tests::pool().await?;
        let (repo, job, lesson) =
            crate::content_verification::tests::batch_scheduler_tests::setup(&pool).await?;
        let content = vec![json!({"body":"The first section states a fact."})];
        let unreadable = model(vec![json!({"units":{}}), json!({"units":{}})]);
        crate::lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
            let error = extract_remaining(&unreadable, &content, vec![])
                .await
                .err()
                .unwrap();
            assert!(matches!(error, AppError::ModelAnswerUnusable(_)), "{error}");
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn lesson_internal_statements_are_nonfactual_but_the_facts_they_rely_on_are_claims(
    ) -> Result<()> {
        let content = vec![
            json!({"body":"In the provided code example, the handler calls UpsertOrderSummary."}),
            json!({"body":"The handler awaits SaveChangesAsync, which writes tracked changes to the database."}),
        ];
        let llm = model(vec![fixture_response(vec![
            json!({"index":0,"claims":[],"nonFactualReason":"lesson-internal"}),
            json!({"index":1,"claims":[{"sentenceId":"unit-1-passage-0-sentence-0","statement":"SaveChangesAsync writes tracked changes to the database."}],"nonFactualReason":""}),
        ])]);

        let inventory = extract_remaining(&llm, &content, vec![]).await?;

        assert!(inventory.units[0].claims.is_empty());
        assert_eq!(inventory.units[0].non_factual_reason, "lesson-internal");
        assert_eq!(inventory.units[1].claims.len(), 1);
        let prompts = llm.prompts.lock().unwrap();
        assert!(prompts[0].contains("with the reason \"lesson-internal\""));
        assert!(prompts[0].contains("still extract as claims any real-world fact they rely on"));
        Ok(())
    }

    #[tokio::test]
    async fn a_fact_mixed_with_example_names_is_extracted_without_them() -> Result<()> {
        let content = vec![
            json!({"body":"The Infrastructure layer implements data access interfaces defined by the Application Core, such as IOrderRepository, using technologies like EF Core."}),
        ];
        let llm = model(vec![fixture_response(vec![
            json!({"index":0,"claims":[{"sentenceId":"unit-0-passage-0-sentence-0","statement":"In Clean Architecture, the Infrastructure layer implements data access interfaces that the Application Core defines, using technologies such as EF Core."}],"nonFactualReason":""}),
        ])]);

        let inventory = extract_remaining(&llm, &content, vec![]).await?;

        let statement = &inventory.units[0].claims[0].statement;
        assert!(!statement.contains("IOrderRepository"), "{statement}");
        let prompts = llm.prompts.lock().unwrap();
        assert!(prompts[0].contains("names this lesson invented for its examples"));
        assert!(
            prompts[0].contains("extract the real-world fact in general terms without those names")
        );
        Ok(())
    }

    #[test]
    fn dense_sections_keep_every_claim_in_schema_resolution_and_validation() {
        let content = vec![json!({"body":"The measurements describe individual samples."})];
        let input = inputs(&content);
        let claims: Vec<_> = (0..64).map(|i| json!({"sentenceId":"unit-0-passage-0-sentence-0","statement":format!("Sample {i} has measured value {i}.")})).collect();
        let response = fixture_response(vec![
            json!({"index":0,"claims":claims,"nonFactualReason":""}),
        ]);
        assert!(jsonschema::JSONSchema::compile(&schema(&[0], &input))
            .unwrap()
            .is_valid(&response));
        let mut value = response["units"]["unit-0"].clone();
        value["index"] = json!(0);
        let unit = resolve(serde_json::from_value(value).unwrap(), &input).unwrap();
        assert_eq!(unit.claims.len(), 64);
        validate_inventory(&Inventory { units: vec![unit] }, &content).unwrap();
    }

    #[test]
    fn generation_schema_rejects_cross_section_passages_before_resolution() {
        let input = inputs(&[
            json!({"body":"First section"}),
            json!({"body":"Omitted section"}),
            json!({"body":"Third section"}),
        ]);
        let schema = schema(&[0, 2], &input);
        let validator = jsonschema::JSONSchema::compile(&schema).unwrap();
        let valid = fixture_response(vec![
            answer(0, "unit-0-passage-0"),
            answer(2, "unit-2-passage-0"),
        ]);
        assert!(validator.is_valid(&valid));
        let crossed = fixture_response(vec![
            answer(0, "unit-2-passage-0"),
            answer(2, "unit-2-passage-0"),
        ]);
        assert!(!validator.is_valid(&crossed));
        let unrequested = fixture_response(vec![
            answer(0, "unit-0-passage-0"),
            answer(1, "unit-1-passage-0"),
        ]);
        assert!(!validator.is_valid(&unrequested));
        let mut renumbered = valid;
        renumbered["units"]["unit-0"]["index"] = json!(2);
        assert!(!validator.is_valid(&renumbered));
    }

    #[test]
    fn repeated_unit_keys_are_rejected_before_they_can_hide_a_missing_section() {
        let raw = r#"{"units":{"unit-8":{"claims":[],"nonFactualReason":"Instruction only"},"unit-8":{"claims":[],"nonFactualReason":"Another instruction"}}}"#;
        assert!(crate::generation::parse_json::<ExtractionResponse>(raw).is_err());
    }

    #[test]
    fn passage_locations_preserve_all_bytes_and_cannot_cross_units() {
        let body = "🦀  code\r\n    print(' a  b ')\n".repeat(110);
        let content = vec![
            json!({"kind":"teaching","body":body}),
            json!({"body":"Other unit"}),
        ];
        let input = inputs(&content);
        let reconstructed: String = input[0]["passages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["text"].as_str().unwrap())
            .collect();
        assert_eq!(reconstructed, body);
        assert!(resolve(
            serde_json::from_value(answer(0, "unit-1-passage-0")).unwrap(),
            &input
        )
        .is_err());
        let valid = resolve(
            serde_json::from_value(answer(0, "unit-0-passage-0")).unwrap(),
            &input,
        )
        .unwrap();
        assert!(claim_ledger::stands_in_part(
            &content[0],
            &valid.claims[0].quote
        ));
    }

    /// A check's reveal names misconceptions as wrong; the extractor is told
    /// so for that unit, and only for a body that is a check.
    #[test]
    fn a_check_tells_the_extractor_its_misconceptions_are_named_as_wrong() {
        let check = format!(
            "A sealed jar of air is carried from the beach up a mountain. What happens to its lid?\n{}\nIt bulges. If you said it caves in, you assumed the air inside loses pressure; it is the outside that does.",
            crate::generation::CHECK_REVEAL
        );
        let content = vec![
            json!({"kind":"teaching","title":"Your turn","body":check}),
            json!({"kind":"teaching","title":"The idea","body":"Pressure falls with altitude."}),
            json!({"kind":"assessment","correctIndex":0,"prompt":"Check your thinking?","options":["A","B"],"explanation":"Because."}),
        ];
        let input = inputs(&content);
        assert_eq!(input[0]["checkRole"], CHECK_ROLE);
        assert!(input[0]["passages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["text"]
                .as_str()
                .unwrap()
                .contains("If you said it caves in")));
        assert!(input[1].get("checkRole").is_none());
        assert!(input[2].get("checkRole").is_none());
    }

    #[test]
    fn an_assessments_question_and_options_are_context_and_never_passages() {
        let wrong = "Unendorsed alternative. ".repeat(150);
        let content = vec![json!({"kind":"assessment","correctIndex":1,
            "prompt":"Which result follows under the stated conditions?",
            "options":[wrong,"The selected factual answer."],
            "explanation":"An empirical reason the other answer fails."})];
        let input = inputs(&content);
        let targets = input[0]["passages"].as_array().unwrap();
        assert!(targets.iter().all(|p| p["field"] == "/explanation"));
        assert!(!targets.is_empty());
        let context = input[0]["context"].as_array().unwrap();
        let fields: Vec<&str> = context
            .iter()
            .map(|p| p["field"].as_str().unwrap())
            .collect();
        assert_eq!(fields, ["/prompt", "/options/0", "/options/1"]);
        assert_eq!(context[1]["text"], wrong);
        assert_eq!(input[0]["contextRole"], CONTEXT_ROLE);
        // A provider cannot turn an option or the question into a claim, even
        // if it ignores instructions or does not enforce the schema.
        for id in context.iter().map(|p| p["id"].as_str().unwrap()) {
            let bad = answer(0, id);
            assert!(resolve(serde_json::from_value(bad.clone()).unwrap(), &input).is_err());
            let validator = jsonschema::JSONSchema::compile(&schema(&[0], &input)).unwrap();
            assert!(!validator.is_valid(&fixture_response(vec![bad])));
        }
    }

    #[test]
    fn only_bodies_and_explanations_are_read_and_a_reflection_is_not() {
        let content = vec![
            json!({"kind":"teaching","role":"explanation","title":"A heading that states a fact.",
                "body":"The body states the lesson.","rubric":[{"title":"Cites","description":"A rubric line."}]}),
            json!({"kind":"teaching","role":"reflection","title":"Reflect",
                "body":"Explain from memory what the lesson taught."}),
            json!({"kind":"assessment","correctIndex":0,"prompt":"Stem?","options":["A","B"],
                "explanation":"Because the lesson says so."}),
        ];
        let input = inputs(&content);
        let fields = |unit: usize| -> Vec<&str> {
            input[unit]["passages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["field"].as_str().unwrap())
                .collect()
        };
        assert_eq!(fields(0), ["/body"]);
        assert!(fields(1).is_empty(), "a reflection holds no claims");
        assert_eq!(fields(2), ["/explanation"]);
        assert!(input[0].get("context").is_none());
    }

    #[test]
    fn a_wrong_choices_reason_is_a_passage_of_its_own_beside_the_explanation() {
        let content = vec![
            json!({"kind":"assessment","correctIndex":0,"prompt":"Stem?",
            "options":["A","B","C","D"],
            "explanation":"A holds because the lesson says so.",
            "rationales":["","B fails for one reason.","C fails for another.","D fails for a third."]}),
        ];
        let input = inputs(&content);
        let found: Vec<(&str, &str)> = input[0]["passages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["field"].as_str().unwrap(), p["text"].as_str().unwrap()))
            .collect();
        assert_eq!(
            found,
            [
                ("/explanation", "A holds because the lesson says so."),
                ("/rationales/1", "B fails for one reason."),
                ("/rationales/2", "C fails for another."),
                ("/rationales/3", "D fails for a third."),
            ]
        );
        assert!(input[0]["context"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| !p["field"].as_str().unwrap().starts_with("/rationales")));
    }

    #[tokio::test]
    async fn a_section_with_nothing_to_read_is_not_put_to_the_model() -> Result<()> {
        let content = vec![
            json!({"kind":"teaching","role":"reflection","body":"Explain from memory."}),
            json!({"kind":"teaching","role":"explanation","body":"The second section states a fact."}),
        ];
        let llm = model(vec![fixture_response(vec![answer(1, "unit-1-passage-0")])]);

        let inventory = extract_remaining(&llm, &content, vec![]).await?;

        assert!(inventory.units[0].claims.is_empty());
        assert_eq!(inventory.units[0].non_factual_reason, NOTHING_TO_READ);
        assert_eq!(inventory.units[1].claims.len(), 1);
        let prompts = llm.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("\"requestedIndices\":[1]"));
        Ok(())
    }

    fn two_sections() -> Vec<Value> {
        vec![
            json!({"body":"First assertion."}),
            json!({"body":"Second assertion."}),
        ]
    }

    #[tokio::test]
    async fn a_correction_reply_is_used_alone_and_nothing_of_the_refused_reply_is_kept(
    ) -> Result<()> {
        let mut stale = answer(0, "unit-0-passage-0");
        stale["claims"][0]["statement"] = json!("A statement only the refused reply made.");
        let model = model(vec![
            fixture_response(vec![stale, answer(1, "unit-0-passage-0")]),
            fixture_response(vec![
                answer(0, "unit-0-passage-0"),
                answer(1, "unit-1-passage-0"),
            ]),
        ]);
        let inventory = extract(&model, &two_sections()).await?;
        assert_eq!(inventory.units.len(), 2);
        assert_eq!(
            inventory.units[0].claims[0].statement,
            "A scoped factual statement."
        );
        let prompts = model.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 2, "one answer and one correction");
        assert!(prompts[1].contains("\"requestedIndices\":[0,1]"));
        Ok(())
    }

    #[tokio::test]
    async fn a_correction_reply_leaving_a_section_out_fails_even_when_the_refused_reply_had_it() {
        let model = model(vec![
            fixture_response(vec![
                answer(0, "unit-0-passage-0"),
                answer(1, "unit-0-passage-0"),
            ]),
            fixture_response(vec![answer(1, "unit-1-passage-0")]),
        ]);
        let error = extract(&model, &two_sections()).await.err().unwrap();
        assert!(matches!(error, AppError::ModelAnswerUnusable(_)), "{error}");
        assert!(error.to_string().contains("Unit 0 needs an inventory"));
        assert!(!error.to_string().contains("Unit 1"), "{error}");
        assert_eq!(model.prompts.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn missing_duplicate_and_foreign_passages_cannot_approve() {
        for response in [
            fixture_response(vec![]),
            json!({"units":{"unit-0":answer(0,"unit-0-passage-0")}}),
            fixture_response(vec![answer(0, "invented")]),
        ] {
            assert!(extract(
                &model(vec![response.clone(), response]),
                &[json!({"body":"A fact."})]
            )
            .await
            .is_err());
        }
    }

    /// The passages the extractor is given for a section whose body is `body`.
    fn body_passages(body: &str) -> Vec<String> {
        let input = inputs(&[json!({"body":body})]);
        input[0]["passages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["text"].as_str().unwrap().to_owned())
            .collect()
    }

    /// Sentence `n` of a body: 83 characters.
    fn sentence(n: usize) -> String {
        format!(
            "Sentence {n:02} of the lesson states one more checkable fact about the subject at hand."
        )
    }

    /// `count` sentences from sentence `first` on, in one paragraph.
    fn paragraph(first: usize, count: usize) -> String {
        (first..first + count)
            .map(sentence)
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn a_field_is_cut_on_the_latest_paragraph_that_leaves_a_passage_past_600_characters() {
        // Nine paragraphs of 335 characters, a blank line between each.
        let body = (0..9)
            .map(|k| paragraph(4 * k, 4))
            .collect::<Vec<_>>()
            .join("\n\n");
        assert_eq!(body.chars().count(), 3_031);
        let passages = body_passages(&body);
        assert_eq!(passages.concat(), body);
        let sizes: Vec<usize> = passages.iter().map(|p| p.chars().count()).collect();
        assert_eq!(sizes.len(), 3, "{sizes:?}");
        for passage in &passages[..2] {
            let size = passage.chars().count();
            assert!(
                passage.ends_with("\n\n"),
                "a paragraph ends it: {passage:?}"
            );
            assert!((602..=1_400).contains(&size), "{size}");
            assert!(
                size + 337 > 1_400,
                "the next paragraph would have fit: {size}"
            );
        }
        let input = inputs(&[json!({"body":body})]);
        let named: Vec<_> = input[0]["passages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["id"].as_str().unwrap(), p["field"].as_str().unwrap()))
            .collect();
        assert_eq!(
            named,
            [
                ("unit-0-passage-0", "/body"),
                ("unit-0-passage-1", "/body"),
                ("unit-0-passage-2", "/body")
            ]
        );
    }

    #[test]
    fn with_no_paragraph_past_600_characters_to_cut_on_a_passage_ends_on_a_sentence() {
        // A paragraph of 503 characters, then one of thirty sentences.
        let body = format!("{}\n\n{}", paragraph(0, 6), paragraph(6, 30));
        let passages = body_passages(&body);
        assert_eq!(passages.concat(), body);
        assert!(passages.len() >= 3, "{}", passages.len());
        for (n, passage) in passages.iter().enumerate() {
            assert!(passage.chars().count() <= 1_400, "passage {n}");
            assert!(
                passage.trim_end().ends_with('.'),
                "passage {n} ends on a sentence: {passage:?}"
            );
            assert!(
                n == 0 || passage.starts_with("Sentence"),
                "passage {n} starts on a sentence: {passage:?}"
            );
        }
        assert!(
            passages[0].chars().count() > 600,
            "the blank line at 505 is too early to cut on"
        );
    }

    #[test]
    fn a_sentence_longer_than_the_room_is_one_passage() {
        let long = format!("Alpha {}omega.", "beta gamma delta ".repeat(100));
        assert_eq!(long.chars().count(), 1_712);
        let body = format!("{long} Next sentence follows here.");
        assert_eq!(
            body_passages(&body),
            [format!("{long} "), "Next sentence follows here.".to_owned()]
        );
    }

    /// The one claim `statement`, located at sentence `sentence`.
    fn extracted(sentence: &str, statement: &str) -> ExtractedUnit {
        serde_json::from_value(json!({"index":0,"claims":[{"sentenceId":sentence,"statement":statement}],"nonFactualReason":""})).unwrap()
    }

    #[test]
    fn a_claims_quote_is_the_sentence_it_names_and_never_the_passage() {
        let body = "Alpha holds the line. Bravo stands firm today. Charlie rests in the shade.";
        let content = vec![json!({"body":body})];
        let unit = resolve(
            extracted("unit-0-passage-0-sentence-1", "Bravo stands firm today."),
            &inputs(&content),
        )
        .unwrap();
        assert_eq!(unit.claims[0].quote, "Bravo stands firm today.");
        validate_inventory(&Inventory { units: vec![unit] }, &content).unwrap();

        let history = "The Peace of Westphalia was signed in 1648 in the cities of Osnabrück and Münster. It ended the Thirty Years' War in the Holy Roman Empire. It also closed the Eighty Years' War between Spain and the Dutch Republic. Historians often date the modern order of sovereign states from these treaties.";
        let unit = resolve(
            extracted(
                "unit-0-passage-0-sentence-3",
                "Signed in 1648, the Peace of Westphalia ended the Thirty Years' War and is often seen by historians as the start of the order of sovereign states.",
            ),
            &inputs(&[json!({"body":history})]),
        )
        .unwrap();
        assert_eq!(
            unit.claims[0].quote,
            "Historians often date the modern order of sovereign states from these treaties."
        );
    }

    #[test]
    fn a_long_statement_or_nonfactual_reason_is_read_not_refused() {
        let body = "Alpha holds the line. Bravo stands firm today.";
        let content = vec![json!({"body":body})];
        let statement = format!(
            "Alpha holds the line {}today.",
            "under every condition the section states ".repeat(60)
        );
        let unit: ExtractedUnit = serde_json::from_value(json!({"index":0,
            "claims":[{"sentenceId":"unit-0-passage-0-sentence-0","statement":statement}],
            "nonFactualReason":"The section frames its scope first. ".repeat(20)}))
        .unwrap();
        let unit = resolve(unit, &inputs(&content));
        assert!(unit.is_ok(), "{:?}", unit.as_ref().err());
        let unit = unit.unwrap();
        assert_eq!(unit.claims[0].statement, statement);
        let inventory = Inventory { units: vec![unit] };
        let validated = validate_inventory(&inventory, &content);
        assert!(validated.is_ok(), "{:?}", validated.as_ref().err());
    }

    #[test]
    fn the_inventory_schema_bounds_no_text_but_a_blank() {
        let input = inputs(&[json!({"body":"The measurements describe individual samples."})]);
        let bounds = crate::lessons::course_generation_tests::length_bounds(&schema(&[0], &input));
        assert!(bounds.is_empty(), "{bounds:?}");
    }

    #[test]
    fn a_long_sentence_is_quoted_whole() {
        let long = format!("Alpha {}omega.", "beta gamma delta ".repeat(100));
        let content = vec![json!({"body":long})];
        let input =
            vec![json!({"passages":[{"id":"unit-0-passage-0","field":"/body","text":long}]})];
        // The named sentence is the quote, whatever the statement says.
        for statement in [
            "Alpha beta gamma delta omega.",
            "A scoped factual statement.",
        ] {
            let unit =
                resolve(extracted("unit-0-passage-0-sentence-0", statement), &input).unwrap();
            let quote = &unit.claims[0].quote;
            assert_eq!(quote, &long, "{statement}");
            validate_inventory(&Inventory { units: vec![unit] }, &content).unwrap();
        }
    }

    #[test]
    fn an_inventory_may_quote_a_long_contained_span() {
        let long = format!("Alpha {}omega.", "beta gamma delta ".repeat(100));
        let content = vec![json!({"body":long})];
        let inventory = Inventory {
            units: vec![UnitClaims {
                index: 0,
                claims: vec![Claim {
                    quote: long.clone(),
                    statement: "A scoped factual statement.".into(),
                }],
                non_factual_reason: String::new(),
            }],
        };
        let validated = validate_inventory(&inventory, &content);
        assert!(validated.is_ok(), "{:?}", validated.as_ref().err());
    }
}
