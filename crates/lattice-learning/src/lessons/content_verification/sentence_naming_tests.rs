//! A claim names the sentence it sits in, and its quote is that sentence's
//! exact text: the extractor and the coverage audit are shown each passage as
//! its sentences and answer with a sentence id, so no sentence is guessed
//! from the words a statement shares with it.
use super::*;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use std::sync::Mutex;

const FIRST: &str = "Alpha holds the line.";
const SECOND: &str = "Bravo stands firm today.";
const THIRD: &str = "Charlie rests in the shade.";
const BODY: &str = "Alpha holds the line. Bravo stands firm today. Charlie rests in the shade.";
const FIRST_ID: &str = "unit-0-passage-0-sentence-0";
const SECOND_ID: &str = "unit-0-passage-0-sentence-1";
const THIRD_ID: &str = "unit-0-passage-0-sentence-2";
/// Statements that share no word with the sentence they are named at.
const ABOUT_FIRST: &str = "Quartz cleaves along flat planes.";
const ABOUT_THIRD: &str = "Mica splits into thin sheets.";

/// A model that answers every request with `answer` of its prompt and keeps
/// the prompt and the schema of each.
struct Naming {
    answer: fn(&str) -> String,
    asked: Mutex<Vec<(String, Value)>>,
}

impl Naming {
    fn new(answer: fn(&str) -> String) -> Self {
        Self {
            answer,
            asked: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl LLMPort for Naming {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            self.asked.lock().unwrap().push((
                prompt.clone(),
                request.json_schema.clone().unwrap_or(Value::Null),
            ));
            replies.push((self.answer)(&prompt));
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "naming-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn candidate() -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Sample","body":BODY,"rubric":[]}],"questions":[]})
}

fn reply(claims: Vec<Value>) -> String {
    inventory::fixture_response(vec![
        json!({"index":0,"claims":claims,"nonFactualReason":""}),
    ])
    .to_string()
}

/// An extractor that names the third sentence for a statement that shares no
/// word with it.
fn names_the_third(_: &str) -> String {
    reply(vec![json!({"sentenceId":THIRD_ID,"statement":ABOUT_THIRD})])
}

/// An extractor that names the first and the third sentence.
fn names_the_first_and_the_third(_: &str) -> String {
    reply(vec![
        json!({"sentenceId":FIRST_ID,"statement":ABOUT_FIRST}),
        json!({"sentenceId":THIRD_ID,"statement":ABOUT_THIRD}),
    ])
}

fn names_no_such_sentence(_: &str) -> String {
    reply(vec![
        json!({"sentenceId":"unit-0-passage-0-sentence-9","statement":ABOUT_THIRD}),
    ])
}

fn answers_nothing(_: &str) -> String {
    json!({"units":{}}).to_string()
}

/// The claims `answer` extracts from the sample lesson, each section's index
/// and claim, or why extraction failed.
async fn extracted(answer: fn(&str) -> String) -> std::result::Result<Vec<(usize, Claim)>, String> {
    let content = units(&candidate()).map_err(|error| error.to_string())?;
    let model = Naming::new(answer);
    let inventory = inventory::extract(&model, &content)
        .await
        .map_err(|error| error.to_string())?;
    Ok(inventory
        .units
        .into_iter()
        .flat_map(|unit| {
            let index = unit.index;
            unit.claims.into_iter().map(move |claim| (index, claim))
        })
        .collect())
}

#[tokio::test]
async fn a_claim_at_a_passages_third_sentence_is_quoted_by_exactly_that_sentence() {
    let quotes: std::result::Result<Vec<String>, String> = extracted(names_the_third)
        .await
        .map(|claims| claims.into_iter().map(|(_, claim)| claim.quote).collect());
    assert_eq!(quotes, Ok(vec![THIRD.to_owned()]));
}

#[tokio::test]
async fn the_request_shows_every_sentence_with_its_id_and_the_schema_allows_exactly_those_ids() {
    let model = Naming::new(answers_nothing);
    let content = units(&candidate()).unwrap();
    // The answer names nothing, so extraction fails: the request is the point.
    let _ = inventory::extract(&model, &content).await;

    let asked = model.asked.lock().unwrap();
    let (prompt, schema) = asked.first().expect("the extractor was asked");
    let shown: Value = serde_json::from_str(prompt.rsplit("\n\n").next().unwrap()).unwrap();
    assert_eq!(
        shown["units"][0]["passages"][0]["sentences"],
        json!([
            {"id":FIRST_ID,"text":FIRST},
            {"id":SECOND_ID,"text":SECOND},
            {"id":THIRD_ID,"text":THIRD},
        ])
    );
    let item = "/properties/units/properties/unit-0/properties/claims/items";
    assert_eq!(
        schema.pointer(&format!("{item}/properties/sentenceId/enum")),
        Some(&json!([FIRST_ID, SECOND_ID, THIRD_ID]))
    );
    assert_eq!(
        schema.pointer(&format!("{item}/required")),
        Some(&json!(["sentenceId", "statement"]))
    );
    assert_eq!(
        schema
            .pointer(&format!("{item}/properties"))
            .and_then(Value::as_object)
            .map(|properties| properties.len()),
        Some(2)
    );
}

/// What a repair of the first sentence puts in the lesson beside the
/// third's claim, and how many corrections were put back: the repair is
/// confined to the failed first sentence, and the third is protected.
async fn repair_beside_the_protected_third() -> std::result::Result<(String, usize), String> {
    let candidate = candidate();
    let claims = extracted(names_the_first_and_the_third).await?;
    let model = Naming::new(answers_nothing);
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}")
        .await
        .map_err(|error| error.to_string())?;
    let found = ledger.extracted(&candidate, &claims, &[]);
    let matched = ledger.match_pass(&candidate, &found);
    let ids = ledger
        .identify(&StandardPolicy::new(), &candidate, found, matched)
        .await
        .map_err(|error| error.to_string())?;
    let targets = vec![ids.first().cloned().ok_or("no first claim")?];
    let repaired = json!({
        "kind":"explanation","title":"Sample","rubric":[],
        "body":"Alpha now holds the line. Bravo stands firm today. Charlie rests in the shade.",
    });
    let confined = confine(
        &candidate["blocks"][0],
        &repaired,
        &Protected::of_claims(&ledger, &targets),
        &Protected::of_ledger(&ledger, &targets),
    )
    .map_err(|error| error.to_string())?;
    Ok((
        confined.section["body"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        confined.put_back.len(),
    ))
}

#[tokio::test]
async fn protecting_a_claim_protects_its_sentence_and_not_the_passage_it_is_in() {
    assert_eq!(
        repair_beside_the_protected_third().await,
        Ok((
            "Alpha now holds the line. Bravo stands firm today. Charlie rests in the shade."
                .to_owned(),
            0
        ))
    );
}

/// What the coverage audit's judge says is missing: the third sentence's
/// assertion. The mapping before it accounts for every passage.
fn audits(prompt: &str) -> String {
    if prompt.starts_with("Audit inventory fidelity.") {
        return json!({
            "missingAssertions":[{"sentenceId":THIRD_ID,"statement":ABOUT_THIRD}],
            "reason":"One assertion is absent.",
        })
        .to_string();
    }
    let data: Value = serde_json::from_str(prompt.rsplit("\n\n").next().unwrap()).unwrap();
    let decisions: Vec<Value> = data["units"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|unit| {
            let claims: Vec<Value> = unit["claims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| claim["id"].clone())
                .collect();
            unit["passages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|passage| json!({"passageId":passage["id"],"claimIds":claims,"nonFactualReason":"","missingClaims":[]}))
                .collect::<Vec<_>>()
        })
        .collect();
    json!({"passages":decisions}).to_string()
}

/// The quotes of the claims the audit adds to an inventory that holds a claim
/// on the first sentence, or why the audit failed.
async fn added_quotes() -> std::result::Result<Vec<String>, String> {
    let candidate = candidate();
    let content = units(&candidate).map_err(|error| error.to_string())?;
    let mut inventory = Inventory {
        units: vec![UnitClaims {
            index: 0,
            claims: vec![Claim {
                quote: FIRST.into(),
                statement: ABOUT_FIRST.into(),
            }],
            non_factual_reason: String::new(),
        }],
    };
    let model = Naming::new(audits);
    let mut coverage = coverage::audit(&model, &content, &inventory, None)
        .await
        .map_err(|error| error.to_string())?;
    let rewrites = claim_ledger::Rewrites::default();
    inventory_checkpoint::adopt_missing(
        &mut inventory,
        &mut coverage,
        &content,
        (&candidate, &rewrites),
    );
    Ok(inventory.units[0]
        .claims
        .iter()
        .skip(1)
        .map(|claim| claim.quote.clone())
        .collect())
}

#[tokio::test]
async fn a_statement_the_coverage_audit_adds_is_quoted_by_the_sentence_the_judge_named() {
    assert_eq!(added_quotes().await, Ok(vec![THIRD.to_owned()]));
}

#[test]
fn an_inventory_saved_under_the_earlier_policy_is_not_the_current_one() {
    const EARLIER: &str = "lesson-evidence-v8";
    assert!(!INVENTORY_POLICY.names(EARLIER));
    let saved = InventoryCheckpoint {
        policy: EARLIER.into(),
        model: String::new(),
        model_context: 0,
        content_sha256: String::new(),
        inventory: Inventory { units: Vec::new() },
        coverage: Coverage { units: Vec::new() },
        coverage_policy: None,
    };
    assert!(!INVENTORY_POLICY.names(&saved.policy));
}

#[tokio::test]
async fn a_sentence_id_the_request_did_not_show_is_refused_by_name() {
    let refused = extracted(names_no_such_sentence).await;
    let reason = refused.err().unwrap_or_default();
    assert!(
        reason.contains("unit-0-passage-0-sentence-9"),
        "the refusal names the id: {reason}"
    );
}

#[test]
fn a_named_sentence_that_stands_twice_in_its_field_has_the_place_it_first_stands() {
    let body = format!("{FIRST} {SECOND} {FIRST}");
    let lesson = json!({"blocks":[{"kind":"explanation","title":"Sample","body":body,"rubric":[]}],"questions":[]});

    let location = claim_ledger::locate(&lesson, 0, FIRST, Some(ABOUT_FIRST));

    assert_eq!(location.quote, FIRST);
    assert_eq!(location.span, Some((0, FIRST.len())));
}

/// The text of a passage as a model is shown it: its sentences, in order,
/// joined by a space.
pub(super) fn passage_text(passage: &Value) -> String {
    passage["sentences"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|sentence| sentence["text"].as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The ID of the sentence of a shown `passage` that holds `statement`, the
/// first when none does: how a fixture extractor names the sentence a claim
/// sits in.
pub(super) fn sentence_id(passage: &Value, statement: &str) -> Value {
    let sentences = passage["sentences"].as_array().cloned().unwrap_or_default();
    sentences
        .iter()
        .find(|sentence| {
            sentence["text"]
                .as_str()
                .is_some_and(|text| text.contains(statement))
        })
        .or(sentences.first())
        .map_or(Value::Null, |sentence| sentence["id"].clone())
}
