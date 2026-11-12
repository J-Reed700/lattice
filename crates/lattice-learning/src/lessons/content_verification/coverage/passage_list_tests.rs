use super::*;
use crate::lessons::course_generation_tests::{property_count, ScriptedModel};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::services::model_contract::CORRECTION_LEAD;

fn passage(id: &str) -> Value {
    json!({"id":id,"text":"A fact."})
}

fn claim(id: &str) -> Value {
    json!({"id":id,"statement":"A fact."})
}

fn units(passages: usize) -> Vec<Value> {
    vec![json!({"index":0,"kind":"teaching",
        "passages":(0..passages).map(|i| passage(&format!("p{i}"))).collect::<Vec<_>>(),
        "claims":(0..passages).map(|i| claim(&format!("c{i}"))).collect::<Vec<_>>()})]
}

/// Two sections, each with one passage and its own claim.
fn two_sections() -> Vec<Value> {
    vec![
        json!({"index":0,"kind":"teaching","passages":[passage("p0")],"claims":[claim("c0")]}),
        json!({"index":1,"kind":"teaching","passages":[passage("p1")],"claims":[claim("c1")]}),
    ]
}

fn covered(id: &str, claims: &[&str]) -> Value {
    json!({"passageId":id,"claimIds":claims,"nonFactualReason":"","missingClaims":[]})
}

fn answer(decisions: Vec<Value>) -> String {
    json!({ "passages": decisions }).to_string()
}

/// A model that gives `answers` in turn.
fn model(answers: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: std::sync::Mutex::new(answers.into()),
        prompts: Default::default(),
    }
}

/// A decision that reports an omission, so it differs from [`covered`].
fn omitted(id: &str) -> Value {
    json!({"passageId":id,"claimIds":[],"nonFactualReason":"","missingClaims":["An omitted fact."]})
}

/// Passes each request on to `inner` and keeps it.
struct Sent<'a> {
    inner: &'a ScriptedModel,
    requests: std::sync::Mutex<Vec<CompletionRequest>>,
}

#[async_trait::async_trait]
impl LLMPort for Sent<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.requests.lock().unwrap().push(request.clone());
        self.inner.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The last user message of each request that is a correction.
fn corrections(requests: &[CompletionRequest]) -> Vec<String> {
    requests
        .iter()
        .filter_map(|request| {
            request.input.iter().rev().find_map(|item| match item {
                CompletionInput::Message { role, content } if role == "user" => {
                    Some(content.clone())
                }
                _ => None,
            })
        })
        .filter(|last| last.starts_with(CORRECTION_LEAD))
        .collect()
}

/// The mapping of a model that answers with `first`, and with `second` when
/// corrected, and every request it was sent.
async fn mapped(
    units: &[Value],
    first: String,
    second: String,
) -> (Result<String>, Vec<CompletionRequest>) {
    let scripted = model(vec![first, second]);
    let sent = Sent {
        inner: &scripted,
        requests: Default::default(),
    };
    let mapping = mapping::review(&sent, units, None).await;
    let requests = sent.requests.lock().unwrap().clone();
    (mapping, requests)
}

#[test]
fn the_schema_is_as_large_for_fifty_passages_as_for_one() {
    let (one, fifty) = (schema(&units(1)), schema(&units(50)));
    assert_eq!(property_count(&one), property_count(&fifty));
    let listed = &fifty["properties"]["passages"]["items"]["properties"]["passageId"]["enum"];
    assert_eq!(listed.as_array().map(Vec::len), Some(50));
}

#[test]
fn the_schema_accepts_one_entry_per_passage_and_only_known_ids() {
    let units = two_sections();
    let validator = jsonschema::JSONSchema::compile(&schema(&units)).unwrap();
    let complete = json!({"passages":[covered("p0", &["c0"]), covered("p1", &["c1"])]});
    assert!(validator.is_valid(&complete));
    let unknown_passage = json!({"passages":[covered("p9", &["c0"])]});
    assert!(!validator.is_valid(&unknown_passage));
    let unknown_claim = json!({"passages":[covered("p0", &["c9"])]});
    assert!(!validator.is_valid(&unknown_claim));
}

#[tokio::test]
async fn a_complete_answer_is_kept_by_passage() {
    let units = two_sections();
    let model = model(vec![answer(vec![
        covered("p0", &["c0"]),
        covered("p1", &["c1"]),
    ])]);
    let raw = mapping::review(&model, &units, None).await.unwrap();
    let kept: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(kept["p0"]["claimIds"], json!(["c0"]));
    assert_eq!(kept["p1"]["claimIds"], json!(["c1"]));
    assert!(kept["p0"].get("passageId").is_none());
    assert!(resolve(&raw, &units).is_ok());
}

#[tokio::test]
async fn a_passage_left_out_is_named_and_the_correction_alone_is_kept() {
    let units = two_sections();
    let first = answer(vec![covered("p0", &["c0"])]);
    let second = answer(vec![omitted("p0"), covered("p1", &["c1"])]);
    let (mapping, requests) = mapped(&units, first, second).await;
    let asked = corrections(&requests);
    assert_eq!(requests.len(), 2);
    assert_eq!(asked.len(), 1);
    assert!(asked[0].contains("p1"), "{}", asked[0]);
    assert!(!asked[0].contains("p0"), "{}", asked[0]);
    let kept: Value = serde_json::from_str(&mapping.unwrap()).unwrap();
    assert_eq!(kept["p0"]["missingClaims"], json!(["An omitted fact."]));
    assert_eq!(kept["p0"]["claimIds"], json!([]));
    assert_eq!(kept["p1"]["claimIds"], json!(["c1"]));
}

#[tokio::test]
async fn a_passage_answered_twice_is_named_and_the_correction_alone_is_kept() {
    let units = two_sections();
    let first = answer(vec![
        covered("p0", &["c0"]),
        covered("p0", &["c0"]),
        covered("p1", &["c1"]),
    ]);
    let second = answer(vec![omitted("p0"), covered("p1", &["c1"])]);
    let (mapping, requests) = mapped(&units, first, second).await;
    let asked = corrections(&requests);
    assert_eq!(requests.len(), 2);
    assert_eq!(asked.len(), 1);
    assert!(asked[0].contains("p0"), "{}", asked[0]);
    assert!(!asked[0].contains("p1"), "{}", asked[0]);
    let kept: Value = serde_json::from_str(&mapping.unwrap()).unwrap();
    assert_eq!(kept["p0"]["missingClaims"], json!(["An omitted fact."]));
    assert_eq!(kept["p0"]["claimIds"], json!([]));
    assert_eq!(kept["p1"]["claimIds"], json!(["c1"]));
}

#[tokio::test]
async fn a_claim_from_another_section_is_named_with_its_passage() {
    let units = two_sections();
    let first = answer(vec![covered("p0", &["c1"]), covered("p1", &["c1"])]);
    let second = answer(vec![omitted("p0"), covered("p1", &["c1"])]);
    let (mapping, requests) = mapped(&units, first, second).await;
    let asked = corrections(&requests);
    assert_eq!(requests.len(), 2);
    assert_eq!(asked.len(), 1);
    assert!(asked[0].contains("p0"), "{}", asked[0]);
    assert!(!asked[0].contains("p1"), "{}", asked[0]);
    let kept: Value = serde_json::from_str(&mapping.unwrap()).unwrap();
    assert_eq!(kept["p0"]["missingClaims"], json!(["An omitted fact."]));
    assert_eq!(kept["p0"]["claimIds"], json!([]));
    assert_eq!(kept["p1"]["claimIds"], json!(["c1"]));
}

#[tokio::test]
async fn a_correction_leaving_a_passage_out_again_fails_without_the_first_answer() {
    let units = two_sections();
    let first = answer(vec![covered("p0", &["c0"])]);
    let second = answer(vec![covered("p1", &["c1"])]);
    let (mapping, requests) = mapped(&units, first, second).await;
    assert_eq!(requests.len(), 2, "one correction, then the call fails");
    let error = mapping.unwrap_err();
    assert!(matches!(error, AppError::ModelAnswerUnusable(_)), "{error}");
    assert!(error.to_string().contains("p0"), "{error}");
    assert!(!error.to_string().contains("p1"), "{error}");
}

#[tokio::test]
async fn a_correction_is_asked_in_the_same_conversation_about_every_passage() {
    let units = two_sections();
    let first = answer(vec![covered("p0", &["c0"])]);
    let second = answer(vec![covered("p0", &["c0"]), covered("p1", &["c1"])]);
    let (mapping, requests) = mapped(&units, first.clone(), second).await;
    assert!(resolve(&mapping.unwrap(), &units).is_ok());
    assert_eq!(requests.len(), 2);
    let said: Vec<String> = requests[1]
        .input
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect();
    let said = said.join(" ");
    assert!(said.contains(&first), "the refused reply is carried");
    assert!(said.contains("\"id\":\"p0\""), "{said}");
    assert!(said.contains("\"id\":\"p1\""), "{said}");
}
