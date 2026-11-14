#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::services::model_contract::{CORRECTION_LEAD, CUT_OFF_REFUSAL};
use lattice_core::domain::learning::acceptance::{Outcome, Reading, StandardPolicy};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::JobStore;
use readings_tests::{claim, read_all, read_one, world_of};
use sentence_naming_tests::{passage_text, sentence_id};
use std::sync::Mutex;
use std::time::Duration;

#[path = "batch_scheduler_tests.rs"]
pub(in crate::lessons) mod batch_scheduler_tests;

#[path = "incremental_tests.rs"]
mod incremental_tests;

const BAD: &str = "DictReader strips field-name whitespace by default.";
const GOOD: &str = "DictReader preserves field-name whitespace by default.";
/// What a repair writes for [`BAD`]: the same fact in the lesson's own words,
/// since a repair may not write a sentence that copies a source.
const FIXED: &str = "By default, DictReader preserves field-name whitespace.";
const SOURCE: &str = "DictReader preserves field-name whitespace by default. Missing values in nonblank rows default to None. The optional skipinitialspace setting ignores spaces immediately after delimiters; it defaults to false.";

/// A deterministic protocol fixture, not evidence of a live model's accuracy.
/// Execution behavior is covered separately with real bundled guests.
struct Model {
    name: &'static str,
    repaired: Mutex<bool>,
    judge_calls: Mutex<usize>,
    failing_judge: bool,
    incomplete_coverage: bool,
    hold_bad_claim: bool,
    /// Fail every batched check as a server that cannot finish a group does,
    /// so the claims are checked one at a time.
    batch_unavailable: bool,
    extracted_sections: Mutex<Vec<Vec<usize>>>,
    audited_sections: Mutex<Vec<Vec<usize>>>,
    /// Fail the coverage batch that starts at this section.
    fail_coverage_at: Option<usize>,
    coverage_gap_in: Option<usize>,
}
impl Model {
    fn new() -> Self {
        Self {
            name: "scripted-evidence-checker",
            repaired: Mutex::new(false),
            judge_calls: Mutex::new(0),
            failing_judge: false,
            incomplete_coverage: false,
            hold_bad_claim: false,
            batch_unavailable: false,
            extracted_sections: Mutex::new(Vec::new()),
            audited_sections: Mutex::new(Vec::new()),
            fail_coverage_at: None,
            coverage_gap_in: None,
        }
    }
}
/// The task a fixture prompt carries, as JSON: its last paragraph before the correction an ask-again adds.
pub(crate) fn fixture_task(prompt: &str) -> Option<Value> {
    let task = prompt
        .split_once(&format!("\n\n{CORRECTION_LEAD}"))
        .map_or(prompt, |(task, _)| task);
    serde_json::from_str(task.rsplit("\n\n").next()?).ok()
}
/// A typed request as the prompts the scripted fixtures answer: one per claim
/// of a batched check, otherwise one. A generation request reads as its system
/// instructions followed by the user's request; a single claim check reads as
/// its user prompt, which is where the check states the claim.
pub(crate) fn fixture_prompts(request: &CompletionRequest) -> Vec<String> {
    // A two-step call's prose step is answered by `fixture_completion` alone,
    // so scripted replies and call counts belong to the structured answer.
    if crate::lessons::structured_call::is_draft_request(request) {
        return Vec::new();
    }
    if let Some(schema) = &request.json_schema {
        assert_schema_request_holds_the_shape(request, schema);
    }
    let (system, user) = fixture_messages(request);
    if system.starts_with(crate::lessons::teaching::contract::SYSTEM)
        || system.starts_with(crate::lessons::teaching::contract::confirm::SYSTEM)
    {
        return Vec::new();
    }
    if let Some(batch) = fixture_batch(user) {
        return batch
            .into_iter()
            .map(|(claim, passages, _)| {
                let evidence = passages
                    .iter()
                    .enumerate()
                    .map(|(index, text)| format!("[passage-{index}]\n{text}"))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                format!("Source passages:\n{evidence}\n\nClaim: {claim}\n\nCompare every factual part with the passages before deciding.")
            })
            .collect();
    }
    let correction = fixture_correction(request);
    if is_claim_check(user) || system.is_empty() {
        return vec![format!("{user}{correction}")];
    }
    vec![format!("{system}\n\n{user}{correction}")]
}

/// The correction a request asks again with, as the text that follows its task
/// prompt: nothing for a first ask.
fn fixture_correction(request: &CompletionRequest) -> String {
    request
        .input
        .iter()
        .rev()
        .find_map(|item| match item {
            lattice_core::application::ports::llm_port::CompletionInput::Message {
                role,
                content,
            } if role == "user" && content.starts_with(CORRECTION_LEAD) => {
                Some(format!("\n\n{content}"))
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// What a fixture model answers a lesson's check against its course plan
/// with, unless a test routes that call itself (`teaching::contract_tests`).
pub(crate) const CONTRACT_NOTHING: &str =
    "{\"planned\":[],\"usedBeforeTaught\":[],\"contradictions\":[],\"unseen\":[]}";

/// What a fixture model answers a confirming read of the lesson's check with:
/// not taught, and an item belongs in no part in particular.
pub(crate) const CONFIRM_PLANNED_NOTHING: &str =
    "{\"taught\":false,\"quote\":\"\",\"belongs\":\"\"}";
pub(crate) const CONFIRM_USE_NOTHING: &str =
    "{\"asked\":true,\"taught\":false,\"where\":\"thisLesson\",\"quote\":\"\"}";
pub(crate) const CONFIRM_CLASH_NOTHING: &str = "{\"case\":\"\",\"contradict\":false}";

/// The fixture's replies to [`fixture_prompts`], answered in the typed
/// protocol: one batched reply, a check's verdict as its last line, or the
/// reply text unchanged.
pub(crate) fn fixture_completion(
    request: &CompletionRequest,
    replies: Vec<String>,
) -> CompletionResponse {
    if crate::lessons::structured_call::is_draft_request(request) {
        return CompletionResponse::from_text(crate::lessons::structured_call::FIXTURE_DRAFT);
    }
    let (system, user) = fixture_messages(request);
    if system.starts_with(crate::lessons::teaching::contract::confirm::SYSTEM) {
        return CompletionResponse::from_text(
            if user.contains(crate::lessons::teaching::contract::confirm::PLANNED_TASK) {
                CONFIRM_PLANNED_NOTHING
            } else if user.contains(crate::lessons::teaching::contract::confirm::CLASH_TASK) {
                CONFIRM_CLASH_NOTHING
            } else {
                CONFIRM_USE_NOTHING
            },
        );
    }
    if system.starts_with(crate::lessons::teaching::contract::SYSTEM) {
        let text = replies.concat();
        return CompletionResponse::from_text(if text.is_empty() {
            CONTRACT_NOTHING
        } else {
            text.as_str()
        });
    }
    let text = if let Some(batch) = fixture_batch(user) {
        let checks: Vec<Value> = batch
            .iter()
            .enumerate()
            .zip(replies)
            .map(|((index, (_, _, evidence_ids)), reply)| {
                json!({"id": json!(format!("claim-{index}")), "response": verdict_last(&reply, evidence_ids)})
            })
            .collect();
        json!({ "checks": checks }).to_string()
    } else if is_claim_check(user) && system.contains("End with a third line: Verdict") {
        let local: Vec<String> = (0..64).map(|index| format!("passage-{index}")).collect();
        replies
            .into_iter()
            .next()
            .map(|reply| verdict_last(&reply, &local))
            .unwrap_or_default()
    } else if is_claim_check(user)
        && system == lattice_core::application::services::claim_verification::BRIDGE_SYSTEM
    {
        replies
            .into_iter()
            .next()
            .map(|reply| part_wise_reply(&reply, user))
            .unwrap_or_default()
    } else {
        fixture_repair_edits(request, user, replies.concat())
    };
    CompletionResponse::from_text(text)
}

/// A scripted repair answer made to obey the schema it was asked under: when
/// the schema asks for an entry for each finding, the changes are put in the
/// entries of the findings they name or, naming none, the finding sent for
/// their part; otherwise the names are taken away. Any other reply is
/// returned as written.
fn fixture_repair_edits(request: &CompletionRequest, user: &str, text: String) -> String {
    let Ok(Value::Object(mut answer)) = serde_json::from_str::<Value>(&text) else {
        return text;
    };
    if !["edits", "citations", "uncite"]
        .iter()
        .any(|key| answer.contains_key(*key))
    {
        return text;
    }
    let entries = request
        .json_schema
        .as_ref()
        .and_then(|schema| schema.pointer("/properties/answers/items"));
    let sent: Vec<Value> = serde_json::from_str::<Value>(user)
        .ok()
        .and_then(|prompt| prompt.get("issuesToFix").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let finding_for = |path: &str| -> Value {
        let mut segments = path.split('/').skip(1);
        let part = match (segments.next(), segments.next()) {
            (Some(list @ ("blocks" | "questions")), Some(index)) => format!("/{list}/{index}"),
            _ => String::new(),
        };
        let entry = sent
            .iter()
            .find(|entry| {
                entry["part"].as_str() == Some(part.as_str())
                    || entry["also"].as_str() == Some(part.as_str())
            })
            .or_else(|| sent.iter().find(|entry| entry["part"].is_null()))
            .or_else(|| sent.first());
        entry.map_or(Value::Null, |entry| entry["id"].clone())
    };
    if let Some(entry) = entries {
        return answered_by_finding(&answer, &sent, entry, &finding_for);
    }
    for key in ["edits", "citations"] {
        let Some(Value::Array(changes)) = answer.get_mut(key) else {
            continue;
        };
        for change in changes {
            let Some(fields) = change.as_object_mut() else {
                continue;
            };
            fields.remove("finding");
        }
    }
    if let Some(Value::Array(unciting)) = answer.get_mut("uncite") {
        for entry in unciting.iter_mut() {
            if let Some(path) = entry.get("path").and_then(Value::as_str).map(str::to_owned) {
                *entry = json!(path);
            }
        }
    }
    Value::Object(answer).to_string()
}

/// What a fixture writes as the plan of an entry it makes.
pub(crate) const FIXTURE_PLAN: &str = "Make the change the finding asks for.";

/// A flat scripted repair answer as the schema's `answers` ask for it: an
/// entry for each finding sent, holding the changes that name it or fall in
/// its part, and no change when none does.
fn answered_by_finding(
    answer: &serde_json::Map<String, Value>,
    sent: &[Value],
    entry: &Value,
    finding_for: &dyn Fn(&str) -> Value,
) -> String {
    let offered: Vec<&str> = entry
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|key| ["edits", "citations", "uncite"].contains(key))
        .collect();
    let blank = |id: &str| {
        let mut fields = serde_json::Map::new();
        fields.insert("finding".into(), json!(id));
        fields.insert("plan".into(), json!(FIXTURE_PLAN));
        for key in &offered {
            fields.insert((*key).into(), json!([]));
        }
        fields
    };
    let mut entries: Vec<serde_json::Map<String, Value>> = Vec::new();
    for id in sent.iter().filter_map(|sent| sent["id"].as_str()) {
        if !entries.iter().any(|known| known["finding"] == id) {
            entries.push(blank(id));
        }
    }
    for key in ["edits", "citations", "uncite"] {
        let Some(Value::Array(changes)) = answer.get(key) else {
            continue;
        };
        for change in changes {
            let mut fields = match change {
                Value::Object(fields) => fields.clone(),
                Value::String(path) => std::iter::once(("path".to_owned(), json!(path))).collect(),
                _ => continue,
            };
            let path = fields
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let finding = match fields.remove("finding") {
                Some(Value::String(named)) => named,
                _ => finding_for(&path).as_str().unwrap_or("").to_owned(),
            };
            let at = match entries
                .iter()
                .position(|known| known["finding"] == finding.as_str())
            {
                Some(at) => at,
                None => {
                    entries.push(blank(&finding));
                    entries.len() - 1
                }
            };
            if let Some(Value::Array(list)) = entries[at].get_mut(key) {
                list.push(Value::Object(fields));
            }
        }
    }
    json!({ "answers": entries }).to_string()
}

/// A fixture's label-first verdict ("supported\nReason: …\nSource passage:
/// passage-0") as the part-wise judge answers it: the claim as one part,
/// stated by the passage the verdict names, contradicted by it, or not given.
/// Anything else is returned as written.
fn part_wise_reply(reply: &str, user: &str) -> String {
    let mut lines = reply.lines().map(str::trim).filter(|line| !line.is_empty());
    let Some(label) = lines
        .next()
        .map(str::to_ascii_lowercase)
        .filter(|label| ["supported", "contradicted", "unsupported"].contains(&label.as_str()))
    else {
        return reply.to_owned();
    };
    let (mut reason, mut passage) = (String::new(), Value::Null);
    for line in lines {
        if let Some(value) = line.strip_prefix("Reason:") {
            reason = value.trim().to_owned();
        } else if let Some(value) = line.strip_prefix("Source passage:") {
            let value = value.trim();
            if value.starts_with("passage-") {
                passage = json!(value);
            }
        }
    }
    let claim = user
        .split("\n\nClaim: ")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .unwrap_or_default();
    let (kind, passage) = match label.as_str() {
        "supported" => ("stated", passage),
        "contradicted" => ("contradicted", passage),
        _ => ("specific_missing", Value::Null),
    };
    json!({
        "parts": [{"part": claim, "kind": kind, "passage": passage}],
        "verdict": label,
        "reason": reason,
    })
    .to_string()
}

/// Every schema-constrained learning request a fixture answers: its grammar
/// survives llama.cpp's repetition limit, so the server holds the shape, and
/// none of its messages also asks for the format in words.
fn assert_schema_request_holds_the_shape(request: &CompletionRequest, schema: &Value) {
    use lattice_core::application::ports::llm_port::{
        largest_repetition_bound, GRAMMAR_REPETITION_LIMIT,
    };
    let (bound, path) = largest_repetition_bound(schema);
    assert!(
        bound <= GRAMMAR_REPETITION_LIMIT as u64,
        "schema bound {bound} at {path} would make llama.cpp drop the grammar"
    );
    // The batched claim check is the shared claim-verification service's
    // prompt (application/services/claim_verification/batch.rs), not
    // learning's; it still says "Return JSON with checks" and is left to its
    // owner.
    let shared_claim_check = request.input.iter().any(|item| {
        matches!(
            item,
            lattice_core::application::ports::llm_port::CompletionInput::Message { role, content }
                if role == "system" && content.contains("Check the supplied claims independently.")
        )
    });
    if shared_claim_check {
        return;
    }
    for item in &request.input {
        if let lattice_core::application::ports::llm_port::CompletionInput::Message {
            content,
            ..
        } = item
        {
            for demand in [
                "Return only JSON",
                "Return JSON",
                "strict JSON",
                "JSON only",
                "matching the schema",
                "supplied schema",
                "original schema",
                "requested JSON",
            ] {
                assert!(
                    !content.contains(demand),
                    "a schema request still asks for the format in words: {demand}"
                );
            }
        }
    }
}

/// The task's own system and user text, so the answer step of a two-step call
/// reads as the one-step request it replaces.
fn fixture_messages(request: &CompletionRequest) -> (&str, &str) {
    crate::lessons::structured_call::task_messages(request)
}

fn is_claim_check(user: &str) -> bool {
    user.starts_with("Source passages:") || user.starts_with("Audit claim fidelity.")
}

/// One claim of a batched check: its text, its permitted passages and their
/// bank identifiers.
type FixtureClaim = (String, Vec<String>, Vec<String>);

fn fixture_batch(user: &str) -> Option<Vec<FixtureClaim>> {
    let batch: Value = serde_json::from_str(user).ok()?;
    let bank = batch["sourcePassages"].as_array()?;
    let text_of = |id: &str| {
        bank.iter()
            .find(|passage| passage["id"] == id)
            .and_then(|passage| passage["text"].as_str())
            .unwrap_or_default()
            .to_owned()
    };
    batch["claims"].as_array().map(|claims| {
        claims
            .iter()
            .map(|claim| {
                let ids: Vec<String> = claim["evidenceIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|id| id.as_str().map(str::to_owned))
                    .collect();
                (
                    claim["claim"].as_str().unwrap_or_default().to_owned(),
                    ids.iter().map(|id| text_of(id)).collect(),
                    ids,
                )
            })
            .collect()
    })
}

/// A label-first fixture verdict ("supported\nReason: …\nSource passage:
/// passage-0") rewritten to state the comparison first and the verdict last,
/// with the passage named by `ids` (the claim's own numbering mapped to the
/// identifiers the request used). Anything else is returned as written.
fn verdict_last(reply: &str, ids: &[String]) -> String {
    let mut lines = reply.lines().map(str::trim).filter(|line| !line.is_empty());
    let Some(label) = lines.next().filter(|line| {
        ["supported", "contradicted", "unsupported"].contains(&line.to_ascii_lowercase().as_str())
    }) else {
        return reply.to_owned();
    };
    let mut reason = None;
    let mut passage = "none".to_owned();
    for line in lines {
        if let Some(value) = line.strip_prefix("Reason:") {
            reason = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("Source passage:") {
            let value = value.trim();
            passage = value
                .strip_prefix("passage-")
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| ids.get(index).cloned())
                .unwrap_or_else(|| value.to_owned());
        }
    }
    match reason {
        Some(reason) => format!("Reason: {reason}\nSource passage: {passage}\nVerdict: {label}"),
        None => reply.to_owned(),
    }
}

/// The default answers to the two calls that review a question as a quiz
/// item: a blind solver that picks the first option, which the fixtures' keys
/// are, and an item judge that finds nothing in any item.
pub(crate) fn question_review_response(prompt: &str) -> Option<String> {
    if prompt.starts_with("Solve quiz questions independently") {
        let answers: Vec<Value> = fixture_task(prompt).unwrap()["questions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|question| json!({"index":question["index"],"reason":"The fixture's key.","correctIndices":[0],"answer":"","blankAnswers":[],"matches":[],"orderedValues":[]}))
            .collect();
        return Some(json!({"answers":answers}).to_string());
    }
    if prompt.starts_with("Review quiz questions one at a time") {
        let checks: Vec<Value> = fixture_task(prompt).unwrap()["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| json!({"id":item["id"],"finding":"","hasDefect":false,"repeats":[]}))
            .collect();
        return Some(json!({"checks":checks}).to_string());
    }
    None
}

/// Shared with course fixtures so they exercise the additional protocol calls.
pub(crate) fn fixture_response(prompt: &str) -> Option<String> {
    if let Some(reply) = question_review_response(prompt) {
        return Some(reply);
    }
    if let Some(plan) = crate::lessons::course_plan::fixture_response(prompt) {
        return Some(plan);
    }
    if prompt.starts_with("You sort statements extracted from a lesson") {
        // The fixtures' statements are the ones the strict check was written
        // for.
        let Some(Value::Object(statements)) = fixture_task(prompt) else {
            panic!("a JSON task");
        };
        return Some(
            Value::Object(
                statements
                    .keys()
                    .map(|id| (id.clone(), json!("sourced")))
                    .collect(),
            )
            .to_string(),
        );
    }
    if prompt.starts_with("Audit inventory fidelity.") {
        return Some(json!({"missingAssertions":[],"reason":"The fixture inventory faithfully represents the original assertions."}).to_string());
    }
    if prompt.starts_with("You decide whether one statement is standard knowledge of its field") {
        return Some(json!({"verdict":"needs_source","reason":"The fixture's statements are not standard knowledge."}).to_string());
    }
    if prompt.starts_with("Sentences were taken out of this lesson section") {
        return Some(json!({"edits":[]}).to_string());
    }
    if prompt.starts_with("Extract lesson claims.") {
        let data = fixture_task(prompt).unwrap();
        let units = data["units"].as_array().unwrap();
        return Some(inventory::fixture_response(units.iter().map(|v| {
            let passages = v["passages"].as_array().unwrap();
            let passage = passages.iter().find(|p|p["field"]=="/body" || p["field"]=="/explanation").unwrap();
            let quote = passage_text(passage);
            let index = v["index"].as_u64().unwrap() as usize;
            let statement = if quote.contains(BAD) { BAD } else if quote.contains(GOOD) { GOOD } else if quote.contains(FIXED) { FIXED } else { quote.as_str() };
            json!({"index":index,"claims":[{"sentenceId":sentence_id(passage, statement),"statement":statement}],"nonFactualReason":""})
        }).collect::<Vec<_>>()).to_string());
    }
    if prompt.starts_with("Audit claim coverage independently.") {
        let data = fixture_task(prompt).unwrap();
        let mut checks = Vec::new();
        for unit in data["units"].as_array().unwrap() {
            let ids: Vec<_> = unit["claims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| claim["id"].clone())
                .collect();
            for passage in unit["passages"].as_array().unwrap() {
                checks.push(json!({"passageId":passage["id"],"claimIds":ids,"nonFactualReason":if ids.is_empty() {"The fixture contains only a stipulated instruction."} else {""},"missingClaims":[]}));
            }
        }
        return Some(json!({"passages":checks}).to_string());
    }
    None
}
#[async_trait::async_trait]
impl LLMPort for Model {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let prompts = fixture_prompts(request);
        if self.batch_unavailable && prompts.len() > 1 {
            return Err(AppError::ServiceNotAvailable(
                "fixture server cannot finish a group".into(),
            ));
        }
        let mut replies = Vec::new();
        for prompt in prompts {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        self.name
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
impl Model {
    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Extract lesson claims.") {
            self.extracted_sections
                .lock()
                .unwrap()
                .push(serde_json::from_value(
                    fixture_task(prompt).unwrap()["requestedIndices"].clone(),
                )?);
        }
        if prompt.starts_with("Audit claim coverage independently.") {
            let batch: Vec<usize> = fixture_task(prompt).unwrap()["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|unit| unit["index"].as_u64().unwrap() as usize)
                .collect();
            if self.fail_coverage_at.is_some() && self.fail_coverage_at == batch.first().copied() {
                return Err(invalid("fixture interrupted during coverage"));
            }
            self.audited_sections.lock().unwrap().push(batch);
        }
        if prompt.starts_with("Audit claim coverage independently.") && self.incomplete_coverage {
            return Ok(json!({"units":[]}).to_string());
        }
        if let Some(index) = self.coverage_gap_in {
            if prompt.starts_with("Audit inventory fidelity.") {
                let data = fixture_task(prompt).unwrap();
                if data["targetPassage"]["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with(&format!("unit-{index}-")))
                    && passage_text(&data["targetPassage"]) == GOOD
                {
                    return Ok(json!({"missingAssertions":[{"sentenceId":data["targetPassage"]["sentences"][0]["id"],"statement":"An additional original assertion is missing from this fixture inventory."}],"reason":"The fixture names one omission."}).to_string());
                }
            }
            if prompt.starts_with("Audit claim coverage independently.") {
                let mut response: Value = serde_json::from_str(&fixture_response(prompt).unwrap())?;
                for unit in fixture_task(prompt).unwrap()["units"].as_array().unwrap() {
                    if unit["index"].as_u64() == Some(index as u64) {
                        let id = unit["passages"][0]["id"].as_str().unwrap();
                        let entry = response["passages"]
                            .as_array_mut()
                            .unwrap()
                            .iter_mut()
                            .find(|entry| entry["passageId"].as_str() == Some(id))
                            .unwrap();
                        entry["missingClaims"] = json!(["An assertion is missing."]);
                    }
                }
                return Ok(response.to_string());
            }
        }
        if let Some(response) = fixture_response(prompt) {
            return Ok(response);
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            assert!(prompt.contains(GOOD));
            *self.repaired.lock().unwrap() = true;
            return Ok(repair_body(
                prompt,
                candidate(FIXED)["blocks"][0]["body"].clone(),
            ));
        }
        if prompt.starts_with("Review instructional quality.") {
            let data = fixture_task(prompt).unwrap();
            let asked = data["sectionIndicesToReview"].as_array().cloned();
            let checks = data["candidate"]["blocks"].as_array().unwrap().iter().enumerate().filter(|(index,_)|asked.as_ref().is_none_or(|asked|asked.contains(&json!(index)))).map(|(index,block)|json!({"index":index,"passageId":block["bodyPassages"][0]["id"],"finding":"The corrected field-name statement agrees with the concrete example.","hasDefect":false})).collect::<Vec<_>>();
            let mut answer =
                json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(checks)});
            if let Some(open) = data["openFindings"].as_array() {
                answer["openFindings"] = open
                    .iter()
                    .map(|finding| json!({"id":finding["id"],"quote":"","verdict":"resolved"}))
                    .collect();
            }
            return Ok(answer.to_string());
        }
        if prompt.starts_with("Source passages:") {
            if !prompt.contains(CORRECTION_LEAD) {
                *self.judge_calls.lock().unwrap() += 1;
            }
            if self.failing_judge {
                return Ok("No verdict.".into());
            }
            let bad = prompt.split("\n\nClaim: ").nth(1).unwrap().starts_with(BAD);
            if bad && self.hold_bad_claim {
                std::future::pending::<()>().await;
            }
            return Ok(format!("{}\nReason: The captured reference and runtime preserve the header spaces.\nSource quote: {GOOD}\nSource passage: passage-0", if bad { "contradicted" } else { "supported" }));
        }
        if let Some(declined) = declined_teaching_edits(prompt) {
            return Ok(declined);
        }
        if prompt.starts_with("Plan focused reference searches") {
            return Ok("No plan.".into());
        }
        Err(invalid(format!(
            "Unexpected fixture prompt: {} ... {}",
            prompt.chars().take(100).collect::<String>(),
            prompt
                .chars()
                .rev()
                .take(400)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>()
        )))
    }
}

/// The answer of a model that finds nothing to change in a teaching repair's
/// request for edits: none.
pub(crate) fn declined_teaching_edits(prompt: &str) -> Option<String> {
    prompt
        .starts_with(
            "You are editing a rejected draft. Correct only the established teaching defects.",
        )
        .then(|| json!({"edits": []}).to_string())
}

fn candidate(statement: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"CSV header names","body":format!("{} ", statement).repeat(24),"rubric":[]}],"questions":[]})
}
fn repair_body(prompt: &str, after: Value) -> String {
    json!({"edits":[{"path":"/blocks/0/body","before":fixture_task(prompt).unwrap()["candidate"]["blocks"][0]["body"],"after":after}]}).to_string()
}
/// The checks of a job whose earlier pass measured what an answer costs:
/// its first request is not set aside to measure one (course evidence index
/// §5.3), so every claim's comparisons are packed from the start.
fn measured() -> ClaimChecks {
    let checks = ClaimChecks::default();
    *checks.answers.lock().unwrap() = evidence_checks::AnswerSize::of(130, 1);
    checks
}

/// The CSV reference, then an appendix on another subject. A collection
/// whose every passage holds a claim's words gives the claim no subject to
/// read the course by (course evidence index §5.4); the appendix is a
/// passage of its own, short enough that a passage widened around the CSV
/// text still holds the whole source.
fn source() -> LearningSourceDto {
    LearningSourceDto {
        id: uuid::Uuid::new_v4().to_string(),
        title: "CSV reference fixture".into(),
        url: Some("https://docs.python.org/3.14/library/csv.html".into()),
        excerpt: format!(
            "{SOURCE}\n\n{}",
            "Appendix: kiln logs tally glaze batches, cone ratings and cooling ramps across pottery firings. "
                .repeat(16)
        ),
        acquired_at: 0,
    }
}

/// One pass over `candidate` as the loop makes it before any remedy: what it
/// extracts, the statements waiting for a reading read along their graphs, and
/// the report of what the latest readings found. For tests that need a report
/// without the loop.
async fn verify_with(
    llm: &dyn LLMPort,
    candidate: &Value,
    references: &ReferenceCollection<'_>,
    checks: &mut ClaimChecks,
) -> Result<LessonVerificationReport> {
    let extraction = passes::extract(llm, &BuiltinRuntimes::for_tests(), candidate, checks).await?;
    verified(llm, candidate, references, extraction, checks).await
}

/// [`verify_with`] from an extraction already made.
async fn verified(
    llm: &dyn LLMPort,
    candidate: &Value,
    references: &ReferenceCollection<'_>,
    extraction: passes::Extraction,
    checks: &mut ClaimChecks,
) -> Result<LessonVerificationReport> {
    let policy = StandardPolicy::new();
    let mut ledger = claim_ledger::ClaimLedger::open(llm, &candidate.to_string()).await?;
    let pass = passes::identify(
        llm,
        &policy,
        &mut ledger,
        (candidate, references),
        extraction,
        checks,
    )
    .await?;
    let due = ledger.due(&policy);
    let readings = passes::read_steps(
        llm,
        &policy,
        &readings::Readers::standard(),
        &ledger,
        checks,
        (candidate, references),
        (&pass, &due),
    )
    .await?;
    passes::record_readings(&policy, &mut ledger, checks, readings).await?;
    let findings = passes::latest_findings(&ledger, checks, &pass).await?;
    Ok(passes::report(
        llm, references, candidate, &pass, findings, checks,
    ))
}

/// [`verify_with`] over lexical references and no saved choices.
async fn verify(
    llm: &dyn LLMPort,
    candidate: &Value,
    sources: &[LearningSourceDto],
) -> Result<LessonVerificationReport> {
    let references = ReferenceCollection::lexical(sources)?;
    verify_with(llm, candidate, &references, &mut ClaimChecks::default()).await
}

#[path = "checkpoint_tests.rs"]
mod checkpoint_tests;

#[path = "batch_overlap_tests.rs"]
mod batch_overlap_tests;
#[path = "coverage_resume_tests.rs"]
mod coverage_resume_tests;

#[path = "coverage_gaps_tests.rs"]
mod coverage_gaps_tests;

#[path = "claim_ledger_tests.rs"]
mod claim_ledger_tests;

#[path = "put_back_published_tests.rs"]
mod put_back_published_tests;
#[path = "report_tests.rs"]
mod report_tests;
#[path = "settle_published_tests.rs"]
mod settle_published_tests;

#[path = "edit_section_tests.rs"]
mod edit_section_tests;

#[path = "edit_question_tests.rs"]
mod edit_question_tests;

#[path = "claim_position_tests.rs"]
mod claim_position_tests;

#[path = "acceptance_switch_tests.rs"]
pub(in crate::lessons::content_verification) mod acceptance_switch_tests;

#[path = "user_hint_tests.rs"]
mod user_hint_tests;

#[path = "claim_identity_tests.rs"]
mod claim_identity_tests;

#[path = "class_checks_tests.rs"]
mod class_checks_tests;

#[path = "call_ceiling_tests.rs"]
mod call_ceiling_tests;

#[path = "teaching_release_tests.rs"]
mod teaching_release_tests;

#[path = "evidence_paging_tests.rs"]
mod evidence_paging_tests;

#[path = "research_provenance_tests.rs"]
mod research_provenance_tests;

#[path = "recite_tests.rs"]
mod recite_tests;

#[path = "review_resume_tests.rs"]
mod review_resume_tests;

#[path = "claim_review_flow_tests.rs"]
mod claim_review_flow_tests;

#[path = "resume_upgrade_tests.rs"]
mod resume_upgrade_tests;

#[path = "audit_flow_tests.rs"]
mod audit_flow_tests;

#[path = "rounds_tests.rs"]
mod rounds_tests;

#[path = "bridge_tests.rs"]
mod bridge_tests;

#[path = "knowledge_case_tests.rs"]
mod knowledge_case_tests;

#[path = "readings_tests.rs"]
pub(super) mod readings_tests;

#[path = "reading_driver_tests.rs"]
mod reading_driver_tests;

#[path = "reading_record_tests.rs"]
mod reading_record_tests;

#[path = "repair_request_tests.rs"]
mod repair_request_tests;

#[path = "born_of_repair_tests.rs"]
mod born_of_repair_tests;

#[path = "apparatus_tests.rs"]
mod apparatus_tests;

#[path = "right_choice_tests.rs"]
mod right_choice_tests;

#[path = "sentence_naming_tests.rs"]
mod sentence_naming_tests;

#[path = "invariants.rs"]
pub(in crate::lessons::content_verification) mod invariants;

#[tokio::test]
async fn unchanged_evidence_reuses_judgments_but_new_evidence_and_failed_calls_do_not() -> Result<()>
{
    use crate::lesson_drafts;
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let mut model = Model::new();
    let mut sources = vec![source()];
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        let references = ReferenceCollection::lexical(&sources)?;
        let world = world_of(&references, Citations::default());
        let first = read_one(Reading::Page, &model, &world, GOOD).await?;
        assert_eq!(first.outcome(), Outcome::Stands);
        assert!(first.finding().publishable());
        assert_eq!(*model.judge_calls.lock().unwrap(), 1);

        let mut unrelated = source();
        unrelated.excerpt = "Galaxies orbit stars.".into();
        sources.push(unrelated);
        let references = ReferenceCollection::lexical(&sources)?;
        let world = world_of(&references, Citations::default());
        let expanded = read_one(Reading::Page, &model, &world, GOOD).await?;
        assert!(expanded.reused());
        assert!(expanded.finding().publishable());
        assert_eq!(*model.judge_calls.lock().unwrap(), 1, "Adding an unrelated source should still retrieve current evidence without repeating an identical judgment");

        let mut changed = source();
        changed.excerpt = BAD.into();
        sources.push(changed);
        model.failing_judge = true;
        let references = ReferenceCollection::lexical(&sources)?;
        let world = world_of(&references, Citations::default());
        let failed = read_one(Reading::Page, &model, &world, GOOD).await?;
        assert_eq!(failed.outcome(), Outcome::Unjudged);
        assert_eq!(failed.finding().verdict, ClaimVerdict::Unverified);
        assert!(
            !failed.finding().publishable(),
            "New evidence cannot inherit an earlier approval"
        );
        assert_eq!(*model.judge_calls.lock().unwrap(), 2);
        model.failing_judge = false;
        read_one(Reading::Page, &model, &world, GOOD).await?;
        assert_eq!(
            *model.judge_calls.lock().unwrap(),
            3,
            "Failed checks must be attempted again"
        );

        let edited = read_one(Reading::Page, &model, &world, BAD).await?;
        assert_eq!(edited.finding().verdict, ClaimVerdict::Contradicted);
        assert_eq!(*model.judge_calls.lock().unwrap(), 4);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_section_that_goes_is_not_checked_and_its_statements_are_not_in_the_report() -> Result<()>
{
    let model = Model::new();
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let mut lesson = candidate(GOOD);
    lesson["blocks"]
        .as_array_mut()
        .unwrap()
        .push(candidate(BAD)["blocks"][0].clone());
    let mut checks = ClaimChecks {
        removed: sections::Going::new([1]),
        ..ClaimChecks::default()
    };

    let report = verify_with(&model, &lesson, &references, &mut checks).await?;

    // The section that goes holds a statement the references contradict; it
    // is neither judged nor an issue, and the report still accounts for the
    // unit.
    assert_eq!(*model.judge_calls.lock().unwrap(), 1);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].unit, 0);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(report.coverage.len(), 2);
    assert!(report.coverage[1].claims.is_empty());
    assert_eq!(report.coverage_audit.len(), 2);
    Ok(())
}

#[tokio::test]
async fn repairing_one_section_preserves_only_unchanged_claim_comparisons() -> Result<()> {
    use crate::lesson_drafts;
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Model::new();
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let world = world_of(&references, Citations::default());
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        let first = read_all(
            Reading::Page,
            &model,
            &world,
            &[(0, claim(GOOD)), (1, claim(BAD))],
        )
        .await?;
        assert_eq!(*model.judge_calls.lock().unwrap(), 2);
        assert_eq!(first[1].finding().verdict, ClaimVerdict::Contradicted);
        // The second section is repaired.
        let repaired = read_all(
            Reading::Page,
            &model,
            &world,
            &[(0, claim(GOOD)), (1, claim(GOOD))],
        )
        .await?;
        assert!(repaired.iter().all(|read| read.finding().publishable()));
        assert_eq!(
            *model.judge_calls.lock().unwrap(),
            3,
            "Repairing one section must not repeat the other section's identical comparison"
        );
        // Changed neighboring wording still needs coverage review, but an
        // identical standalone factual statement keeps its completed evidence
        // comparison.
        let reworded = Claim {
            quote: format!("{GOOD} Additional wording."),
            statement: GOOD.into(),
        };
        let again = read_all(
            Reading::Page,
            &model,
            &world,
            &[(0, reworded), (1, claim(GOOD))],
        )
        .await?;
        assert!(again.iter().all(|read| read.reused()));
        assert_eq!(*model.judge_calls.lock().unwrap(), 3);
        Ok(())
    })
    .await
}

#[test]
fn reused_comparisons_keep_current_retrieval_metadata_and_exact_source_identity() {
    let claim = Claim {
        quote: GOOD.into(),
        statement: GOOD.into(),
    };
    let evidence = vec![EvidencePassage {
        source_id: "source-1".into(),
        text: SOURCE.into(),
        start_byte: 0,
        end_byte: SOURCE.len(),
        retrieval_kind: "lexical".into(),
        score: 1.0,
    }];
    let key = ClaimChecks::key(0, &claim, &evidence);
    // A score is retrieval metadata, no input of the judge: the comparison is
    // the same, and its receipt restores with the passages retrieved now.
    let mut rescored = evidence.clone();
    rescored[0].score += 1.0;
    assert_eq!(key, ClaimChecks::key(0, &claim, &rescored));
    let receipt = ClaimReceipt {
        verdict: ClaimVerdict::Supported,
        reason: "The passage states it.".into(),
        supporting_quote: Some(GOOD.into()),
        shown: None,
        parts: vec![],
    };
    let restored = receipt.finding(0, &claim, rescored.clone()).unwrap();
    assert_eq!(restored.evidence[0].score, rescored[0].score);
    // Another version of the source, other text and another section are
    // other comparisons.
    let mut renamed = evidence.clone();
    renamed[0].source_id.push_str("-new-version");
    assert_ne!(key, ClaimChecks::key(0, &claim, &renamed));
    let mut edited = evidence.clone();
    edited[0].text.push(' ');
    assert_ne!(key, ClaimChecks::key(0, &claim, &edited));
    assert_ne!(key, ClaimChecks::key(1, &claim, &evidence));
}

#[test]
fn evidence_order_is_stable_without_discarding_conflicting_or_changed_passages() {
    let passage = |source: &str, text: &str| EvidencePassage {
        source_id: source.into(),
        text: text.into(),
        start_byte: 0,
        end_byte: text.len(),
        retrieval_kind: "lexical".into(),
        score: 1.0,
    };
    let claim = Claim {
        quote: GOOD.into(),
        statement: GOOD.into(),
    };
    let mut evidence = vec![passage("b", GOOD), passage("a", BAD)];
    canonicalize_evidence(&mut evidence);
    let original_key = ClaimChecks::key(0, &claim, &evidence);
    let original_input: Vec<_> = evidence.iter().map(|p| p.text.clone()).collect();
    evidence.reverse();
    evidence[0].score = 42.0;
    canonicalize_evidence(&mut evidence);
    assert_eq!(ClaimChecks::key(0, &claim, &evidence), original_key);
    assert_eq!(
        evidence.iter().map(|p| p.text.clone()).collect::<Vec<_>>(),
        original_input
    );
    assert!(evidence.iter().any(|p| p.text == BAD));
    evidence.push(passage("c", "Additional conditions apply."));
    canonicalize_evidence(&mut evidence);
    assert_ne!(ClaimChecks::key(0, &claim, &evidence), original_key);
    evidence.pop();
    evidence[0].source_id.push_str("-new-version");
    canonicalize_evidence(&mut evidence);
    assert_ne!(ClaimChecks::key(0, &claim, &evidence), original_key);
}

#[test]
fn pending_repairs_require_identical_content_sources_and_instructions() -> Result<()> {
    let model = Model::new();
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let original = candidate(BAD);
    let schema = json!({"type":"object"});
    let pending = PendingRepair {
        fingerprint: repair_fingerprint(&model, "instructions", &schema, &original, &references),
        defects: 1,
        context: json!({"candidate":original,"failedClaims":[{"statement":BAD}]}),
    };
    let restored: PendingRepair = serde_json::from_str(&serde_json::to_string(&pending)?)?;
    assert!(restored.matches(&model, "instructions", &schema, &original, &references));
    assert!(!restored.matches(
        &model,
        "changed instructions",
        &schema,
        &original,
        &references
    ));
    assert!(!restored.matches(
        &model,
        "instructions",
        &schema,
        &candidate(GOOD),
        &references
    ));
    assert!(!restored.matches(
        &model,
        "instructions",
        &json!({"type":"array"}),
        &original,
        &references
    ));
    let mut changed = sources.clone();
    changed[0].excerpt.push_str(" Changed evidence.");
    assert!(!restored.matches(
        &model,
        "instructions",
        &schema,
        &original,
        &ReferenceCollection::lexical(&changed)?
    ));
    let mut expanded = sources.clone();
    expanded.push(source());
    assert!(!restored.matches(
        &model,
        "instructions",
        &schema,
        &original,
        &ReferenceCollection::lexical(&expanded)?
    ));
    let mut approved = restored;
    approved.defects = 0;
    assert!(!approved.matches(&model, "instructions", &schema, &original, &references));
    Ok(())
}

#[tokio::test]
async fn failed_rewrite_resumes_its_checkpoint_and_checks_the_new_candidate() -> Result<()> {
    use crate::{
        curriculum::LearningGenerationJobKind,
        curriculum_repository::LearningCurriculumRepository,
        dto::AcceptLearningProgramRequestDto,
        lesson_drafts,
        plan_dto::{LearningGenerationJobActionRequestDto, StartLearningGenerationJobRequestDto},
        repository::LearningRepository,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct InterruptedRepair {
        model: Model,
        repairs: AtomicUsize,
        bad_checks: AtomicUsize,
        good_checks: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl LLMPort for InterruptedRepair {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            let mut replies = Vec::new();
            for prompt in fixture_prompts(request) {
                replies.push(self.respond(&prompt).await?);
            }
            Ok(fixture_completion(request, replies))
        }
        fn model_name(&self) -> &str {
            self.model.model_name()
        }
        fn count_tokens(&self, text: &str) -> usize {
            self.model.count_tokens(text)
        }
        fn max_context_tokens(&self) -> usize {
            self.model.max_context_tokens()
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    impl InterruptedRepair {
        async fn respond(&self, prompt: &str) -> Result<String> {
            if prompt.starts_with("Repair verified lesson defects.") {
                if self.repairs.fetch_add(1, Ordering::Relaxed) == 1 {
                    return Err(AppError::Network(
                        "fixture server failure during the second section repair".into(),
                    ));
                }
                let mut patch = fixture_task(prompt).unwrap()["candidate"].clone();
                for block in patch["blocks"].as_array_mut().unwrap() {
                    let good = if block["title"].as_str().unwrap().contains("second") {
                        format!("Note that {FIXED}")
                    } else {
                        FIXED.to_owned()
                    };
                    block["body"] = json!(block["body"].as_str().unwrap().replace(BAD, &good));
                }
                return Ok(repair_body(prompt, patch["blocks"][0]["body"].clone()));
            }
            if prompt.starts_with("Source passages:") {
                if prompt.split("\n\nClaim: ").nth(1).unwrap().starts_with(BAD) {
                    self.bad_checks.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.good_checks.fetch_add(1, Ordering::Relaxed);
                }
            }
            self.model.respond(prompt).await
        }
    }

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repository = LearningRepository::new(db.clone());
    let program = crate::tests::fixture();
    repository.create(&program).await?;
    repository
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program = repository.get(&program.summary.id).await?;
    let jobs = LearningCurriculumRepository::new(db.clone());
    jobs.plan(&program.summary.id).await?;
    let target = &program.modules[0].lessons[0].id;
    let job = jobs
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: json!({"lessonIds":[target]}).to_string(),
            progress_total: 1,
        })
        .await?;
    JobStore::new(db.clone()).claim(&job.id).await?;
    let model = InterruptedRepair {
        model: Model::new(),
        repairs: AtomicUsize::new(0),
        bad_checks: AtomicUsize::new(0),
        good_checks: AtomicUsize::new(0),
    };
    let sources = vec![source()];
    let mut original = candidate(BAD);
    let mut second = original["blocks"][0].clone();
    second["title"] = json!("A second section needing repair");
    original["blocks"].as_array_mut().unwrap().push(second);
    let raw = original.to_string();
    let first = lesson_drafts::run(&db, &jobs, &job.id, target, async {
        lesson_drafts::resume("same-inputs".into()).await?;
        lesson_drafts::save(&raw).await?;
        verify_and_repair(&model, "{}", &json!({}), raw.clone(), &sources, 4000).await
    })
    .await;
    let error = first.unwrap_err();
    JobStore::new(db.clone())
        .fail(&job.id, "generation_failed", &error.to_string())
        .await?;
    let partial = jobs.lesson_draft(&job.id, target).await?.unwrap();
    assert!(partial.pending_repair.is_some());
    let partial: Value = serde_json::from_str(&partial.candidate)?;
    assert!(partial["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains(FIXED));
    assert!(partial["blocks"][1]["body"].as_str().unwrap().contains(BAD));
    let retry = jobs
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            job_id: job.id,
            expected_revision: program.summary.revision,
        })
        .await?;
    JobStore::new(db.clone()).claim(&retry.id).await?;
    let (_, report) = lesson_drafts::run(&db, &jobs, &retry.id, target, async {
        let saved = lesson_drafts::resume("same-inputs".into()).await?.unwrap();
        verify_and_repair(&model, "{}", &json!({}), saved, &sources, 4000).await
    })
    .await?;
    assert_eq!(model.repairs.load(Ordering::Relaxed), 3);
    assert_eq!(
        model.bad_checks.load(Ordering::Relaxed),
        2,
        "Retry must resume the rewrite, not repeat its old diagnosis"
    );
    assert_eq!(
        model.good_checks.load(Ordering::Relaxed),
        2,
        "Rewritten content must undergo fresh evidence checking"
    );
    assert!(report.issues.is_empty());
    assert!(report
        .findings
        .iter()
        .all(|f| f.verdict == ClaimVerdict::Supported));
    Ok(())
}

#[tokio::test]
async fn a_contradicted_claim_gets_a_correction_then_is_taken_out() -> Result<()> {
    const SECOND: &str = "DictReader removes all whitespace from CSV values by default.";
    struct GradualRepair {
        inner: Model,
        /// The context of each repair request.
        asked: Mutex<Vec<Value>>,
        /// Why each section was asked for after a repair's edits could not
        /// be applied.
        sections: Mutex<Vec<String>>,
    }
    #[async_trait::async_trait]
    impl LLMPort for GradualRepair {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            let mut replies = Vec::new();
            for prompt in fixture_prompts(request) {
                replies.push(self.respond(&prompt).await?);
            }
            Ok(fixture_completion(request, replies))
        }
        fn model_name(&self) -> &str {
            "gradual-repair-fixture"
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
    impl GradualRepair {
        async fn respond(&self, prompt: &str) -> Result<String> {
            if prompt.starts_with("Extract lesson claims.") {
                let data = fixture_task(prompt).unwrap();
                let unit = &data["units"][0];
                let passage = unit["passages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["field"] == "/body")
                    .unwrap();
                let text = passage_text(passage);
                let claims: Vec<_> = [BAD, SECOND, GOOD]
                    .into_iter()
                    .filter(|s| text.contains(s))
                    .map(|statement| json!({"sentenceId":sentence_id(passage, statement),"statement":statement}))
                    .collect();
                return Ok(inventory::fixture_response(vec![
                    json!({"index":0,"claims":claims,"nonFactualReason":""}),
                ])
                .to_string());
            }
            if prompt.starts_with("Repair verified lesson defects.") {
                let mut asked = self.asked.lock().unwrap();
                // The section, asked for in the conversation of the request
                // whose edits could not be applied, comes back as it was.
                let task = prompt.rsplit("\n\n").next().unwrap();
                if serde_json::from_str::<Value>(task).is_err() {
                    self.sections.lock().unwrap().push(task.to_owned());
                    return Ok(asked.last().unwrap()["candidate"].to_string());
                }
                asked.push(fixture_task(prompt).unwrap());
                assert!(asked.len() <= 2, "A claim's last attempt is no request");
                assert!(!prompt.contains("lastAttempt"));
                // The correction drops the first sentence and leaves the
                // second as it was.
                let body = fixture_task(prompt).unwrap()["candidate"]["blocks"][0]["body"]
                    .as_str()
                    .unwrap()
                    .replace(&format!("{BAD} "), "");
                return Ok(repair_body(prompt, json!(body)));
            }
            if prompt.starts_with("Source passages:")
                && prompt
                    .split("\n\nClaim: ")
                    .nth(1)
                    .unwrap()
                    .starts_with(SECOND)
            {
                return Ok("contradicted\nReason: The fixture reference preserves whitespace rather than removing it.\nSource passage: passage-0".into());
            }
            self.inner.respond(prompt).await
        }
    }

    // The first repair corrects both claims and SECOND survives it, so it is
    // asked for once more, as the claim it is alone; it survives that too, and
    // the cut that follows is made in code: its sentence is taken out, the
    // lesson keeps what was verified and publishes.
    let body = format!("{}{BAD} {SECOND}", format!("{GOOD} ").repeat(12));
    let lesson = json!({"blocks":[{"kind":"explanation","title":"CSV header names","body":body,"rubric":[]}],"questions":[]});
    let model = GradualRepair {
        inner: Model::new(),
        asked: Default::default(),
        sections: Default::default(),
    };
    let (raw, report) = verify_and_repair(
        &model,
        "{}",
        &json!({}),
        lesson.to_string(),
        &[source()],
        3000,
    )
    .await?;
    let asked = model.asked.lock().unwrap();
    let named = |request: &Value| -> Vec<String> {
        request["failedClaims"]
            .as_array()
            .unwrap()
            .iter()
            .map(|claim| claim["statement"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(asked.len(), 2, "two requests, and the cut makes none");
    let first = named(&asked[0]);
    assert!(
        first.iter().any(|statement| statement == BAD)
            && first.iter().any(|statement| statement == SECOND),
        "{first:?}"
    );
    assert_eq!(named(&asked[1]), [SECOND]);
    let reason = asked[1]["failedClaims"][0]["reason"].as_str().unwrap();
    assert!(reason.contains("preserves whitespace"), "{reason}");
    assert!(!asked[1].to_string().contains("lastAttempt"));
    // The second correction has no first sentence left to drop, so its edit
    // changes nothing and the section is asked for once.
    let sections = model.sections.lock().unwrap();
    assert_eq!(sections.len(), 1, "{sections:?}");
    assert!(sections[0].contains("changed nothing"), "{}", sections[0]);
    assert!(!raw.contains(BAD) && !raw.contains(SECOND), "{raw}");
    assert!(raw.contains(GOOD));
    assert!(report.issues.is_empty());
    assert!(report
        .findings
        .iter()
        .all(|finding| finding.verdict == ClaimVerdict::Supported));
    Ok(())
}

#[tokio::test]
async fn contradictory_claim_is_repaired_and_rechecked_against_references() -> Result<()> {
    let model = Model::new();
    let bad = verify(&model, &candidate(BAD), &[source()]).await?;
    assert!(!bad.issues.is_empty());
    assert_eq!(bad.findings[0].verdict, ClaimVerdict::Contradicted);
    assert!(bad.executions.is_empty());
    let (raw, report) = verify_and_repair(
        &model,
        "{}",
        &json!({"type":"object"}),
        candidate(BAD).to_string(),
        &[source()],
        3000,
    )
    .await?;
    assert!(*model.repaired.lock().unwrap());
    assert!(raw.contains(FIXED));
    assert!(report.issues.is_empty());
    assert!(report
        .findings
        .iter()
        .all(|f| f.verdict == ClaimVerdict::Supported));
    Ok(())
}

#[tokio::test]
async fn repair_shares_exact_passages_without_dropping_conflicting_evidence() -> Result<()> {
    let model = Model::new();
    let candidate = candidate(BAD);
    let reference = source();
    let mut report = verify(&model, &candidate, std::slice::from_ref(&reference)).await?;
    let mut finding = report.findings[0].clone();
    let original = finding.evidence[0].clone();
    let mut conflicting = original.clone();
    conflicting.source_id = "independent-conflicting-source".into();
    conflicting.text = "Keep conflicting evidence too.\r\n  Preserve whitespace.".into();
    finding.evidence = vec![original.clone(), conflicting.clone()];
    report.findings = (0..107)
        .map(|index| {
            let mut copy = finding.clone();
            copy.statement = format!("Claim {index}: {BAD}");
            copy
        })
        .collect();
    let references = ReferenceCollection::lexical(std::slice::from_ref(&reference))?;
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    claim_ledger_tests::identify_findings(&mut ledger, &candidate, &report).await?;
    let targets = ledger.finding_claims().to_vec();
    let payload = repair_context(
        "{}",
        &candidate,
        &report,
        (&ledger, &StandardPolicy::new()),
        &targets,
        &references,
    )?;
    assert_eq!(payload["failedClaims"].as_array().unwrap().len(), 107);
    assert_eq!(payload["evidence"].as_array().unwrap().len(), 2);
    assert_eq!(payload["evidence"][0]["text"], original.text);
    assert_eq!(payload["evidence"][1]["text"], conflicting.text);
    assert_eq!(payload["evidence"][1]["sourceId"], conflicting.source_id);
    assert_eq!(payload["evidence"][0]["sourceTitle"], reference.title);
    for claim in payload["failedClaims"].as_array().unwrap() {
        assert_eq!(claim["evidence"], json!(["evidence-0", "evidence-1"]));
    }
    assert!(!payload["failedClaims"][0]
        .as_object()
        .unwrap()
        .contains_key("quote"));
    Ok(())
}

#[tokio::test]
async fn a_claim_is_checked_against_every_passage_one_check_can_hold() -> Result<()> {
    // Twelve passages on the claim's subject, and others on another, so the
    // claim's words tell its passages apart.
    let sources: Vec<_> = (0..16)
        .map(|i| LearningSourceDto {
            id: format!("source-{i}"),
            title: format!("Reference {i}"),
            url: None,
            excerpt: if i < 12 {
                format!("DictReader whitespace handling is described in note {i}.")
            } else {
                format!("Kiln firing schedules are logged in appendix {i}.")
            },
            acquired_at: 1,
        })
        .collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let evidence = evidence_for(
        &Model::new(),
        "DictReader whitespace handling",
        0,
        &references,
        &[],
    )
    .await?;
    assert_eq!(evidence.len(), 12, "not the former eight");
    Ok(())
}

#[test]
fn saved_coverage_rejects_foreign_duplicate_or_contradictory_locations() {
    let content = units(&candidate(GOOD)).unwrap();
    let gap = |passage_id: &str| MissingAssertion {
        passage_id: passage_id.into(),
        sentence_id: None,
        statement: "Missing condition".into(),
    };
    let valid = CoverageUnit {
        index: 0,
        complete: false,
        reason: "Missing condition".into(),
        missing_assertions: vec![gap("unit-0-passage-0")],
    };
    assert!(valid_saved_coverage(
        &Coverage {
            units: vec![valid.clone()]
        },
        &content
    ));
    assert!(!valid_saved_coverage(
        &Coverage {
            units: vec![valid.clone(), valid.clone()]
        },
        &content
    ));
    for changed in [
        CoverageUnit {
            index: 1,
            ..valid.clone()
        },
        CoverageUnit {
            complete: true,
            ..valid.clone()
        },
        CoverageUnit {
            complete: false,
            missing_assertions: Vec::new(),
            ..valid.clone()
        },
        CoverageUnit {
            missing_assertions: vec![gap("unit-1-passage-0")],
            ..valid.clone()
        },
        CoverageUnit {
            missing_assertions: vec![MissingAssertion {
                statement: "   ".into(),
                ..gap("unit-0-passage-0")
            }],
            ..valid
        },
    ] {
        assert!(!valid_saved_coverage(
            &Coverage {
                units: vec![changed]
            },
            &content
        ));
    }
}

#[test]
fn saved_coverage_with_a_long_missing_assertion_is_reused() {
    let content = units(&candidate(GOOD)).unwrap();
    let statement = format!(
        "A missing condition {}holds.",
        "that the passage states in full ".repeat(40)
    );
    let unit = CoverageUnit::from_missing(
        0,
        vec![MissingAssertion {
            passage_id: "unit-0-passage-0".into(),
            sentence_id: None,
            statement,
        }],
        "",
    );
    assert!(valid_saved_coverage(
        &Coverage { units: vec![unit] },
        &content
    ));
}

#[tokio::test]
async fn missing_evidence_failed_judge_and_incomplete_coverage_never_approve() -> Result<()> {
    let model = Model::new();
    // Without evidence a claim stating a figure stays an issue.
    let no_evidence = verify(&model, &candidate("A short definition from 2019."), &[]).await?;
    assert_eq!(no_evidence.findings[0].verdict, ClaimVerdict::Unsupported);
    assert!(!no_evidence.issues.is_empty());
    let offline = Model {
        failing_judge: true,
        ..Model::new()
    };
    let report = verify(&offline, &candidate(GOOD), &[source()]).await?;
    assert_eq!(report.findings[0].verdict, ClaimVerdict::Unverified);
    // A claim the checker could not answer for is no failure of the claim:
    // it is asked again once, and when the answer still cannot be read the
    // lesson publishes with the statement not verified, never taken for
    // verified (claim classes design §3.2; publish-unverified design, the
    // `checker_unreadable` case, reason `unchecked`).
    let (published, report) = verify_and_repair(
        &offline,
        "{}",
        &json!({}),
        candidate(GOOD).to_string(),
        &[source()],
        3000,
    )
    .await
    .expect("the lesson publishes, its statement not verified");
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(published.contains(GOOD), "the sentence stands: {published}");
    assert_ne!(report.findings[0].verdict, ClaimVerdict::Supported);
    assert!(
        matches!(
            &report.findings[0].resolution,
            Some(resolution::Resolution::Unverified {
                reason: lattice_core::domain::learning::claim_review::UnverifiedReason::Unchecked
            })
        ),
        "{:?}",
        report.findings[0].resolution
    );
    assert!(!*offline.repaired.lock().unwrap());
    let incomplete = Model {
        incomplete_coverage: true,
        ..Model::new()
    };
    assert!(verify(&incomplete, &candidate(GOOD), &[source()])
        .await
        .is_err());
    Ok(())
}

#[test]
fn omitted_duplicate_and_fabricated_inventory_units_are_rejected() {
    let content = units(&candidate(GOOD)).unwrap();
    let valid = UnitClaims {
        index: 0,
        claims: vec![Claim {
            quote: GOOD.into(),
            statement: GOOD.into(),
        }],
        non_factual_reason: String::new(),
    };
    assert!(validate_inventory(
        &Inventory {
            units: vec![valid.clone()]
        },
        &content
    )
    .is_ok());
    assert!(validate_inventory(&Inventory { units: vec![] }, &content).is_err());
    assert!(validate_inventory(
        &Inventory {
            units: vec![valid.clone(), valid.clone()]
        },
        &content
    )
    .is_err());
    let mut fabricated = valid;
    fabricated.claims[0].quote = "invented".into();
    assert!(validate_inventory(
        &Inventory {
            units: vec![fabricated]
        },
        &content
    )
    .is_err());
}

#[test]
fn a_claim_quoting_only_a_citation_quote_is_not_in_its_section() {
    let content = vec![json!({
        "kind": "explanation",
        "body": "Something else entirely.",
        "citations": [{"quote": GOOD}],
    })];
    let inventory = Inventory {
        units: vec![UnitClaims {
            index: 0,
            claims: vec![Claim {
                quote: GOOD.into(),
                statement: GOOD.into(),
            }],
            non_factual_reason: String::new(),
        }],
    };
    assert!(validate_inventory(&inventory, &content).is_err());
}

/// Persistence-only tests use an explicit synthetic attestation. This is
/// compiled only for tests and does not bypass production verification.
pub(crate) fn attest(lesson_id: &str, lesson: &PreparedLearningLesson) -> LessonVerificationReport {
    // Persistence fixtures model a complete report shape; live factual checks
    // are exercised separately and this helper is never compiled in production.
    let coverage: Vec<_> = lesson
        .blocks
        .iter()
        .map(|block| &block.body)
        .chain(lesson.questions.iter().map(|question| &question.prompt))
        .enumerate()
        .map(|(index, text)| UnitClaims {
            index,
            claims: vec![Claim {
                quote: text.clone(),
                statement: text.clone(),
            }],
            non_factual_reason: String::new(),
        })
        .collect();
    let findings = coverage
        .iter()
        .flat_map(|unit| {
            unit.claims.iter().map(|claim| Finding {
                unit: unit.index,
                quote: claim.quote.clone(),
                statement: claim.statement.clone(),
                verdict: ClaimVerdict::Supported,
                reason: "Persistence fixture".into(),
                evidence: vec![],
                supporting_quote: None,
                resolution: None,
                claim_id: None,
                two_pass: Default::default(),
                label: None,
                class: None,
                trust: None,
            })
        })
        .collect();
    LessonVerificationReport {
        policy: POLICY.to_string(),
        lesson_id: lesson_id.into(),
        content_sha256: content_hash(lesson).unwrap(),
        checker_model: "persistence-fixture".into(),
        checked_at: 0,
        sources: vec![],
        coverage,
        coverage_audit: (0..lesson.blocks.len() + lesson.questions.len())
            .map(|index| CoverageUnit {
                index,
                complete: true,
                reason: "Persistence fixture".into(),
                missing_assertions: Vec::new(),
            })
            .collect(),
        findings,
        executions: vec![],
        confirmed_examples: None,
        issues: vec![],
        retrieval_mode: "lexical_fallback".into(),
        embedding_model: None,
        page_budget: None,
        unexecuted_languages: vec![],
        replaced_findings: Vec::new(),
    }
}

#[tokio::test]
async fn publication_requires_current_report_and_source_and_rolls_back_on_rejection() -> Result<()>
{
    use crate::{dto::*, repository::LearningRepository};
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    let reference = source();
    program.sources = vec![reference];
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    let program = repo.get(&program.summary.id).await?;
    let lesson_id = &program.modules[0].lessons[0].id;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let (raw, report) = verify_and_repair(
        &Model::new(),
        "{}",
        &json!({}),
        candidate(BAD).to_string(),
        &sources,
        3000,
    )
    .await?;
    let value: Value = serde_json::from_str(&raw)?;
    let mut prepared = PreparedLearningLesson {
        blocks: vec![LearningBlockDto::new(
            LearningBlockKind::Explanation,
            "CSV header names".into(),
            value["blocks"][0]["body"].as_str().unwrap().into(),
            vec![sources[0].id.clone()],
            vec![],
        )],
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    assert!(repo
        .prepare(
            &program.summary.id,
            lesson_id,
            program.summary.revision,
            &prepared
        )
        .await
        .is_err());
    prepared.verification = Some(report.bind(lesson_id, &prepared)?);
    let mut changed = prepared.clone();
    changed.blocks[0].body = BAD.into();
    assert!(repo
        .prepare(
            &program.summary.id,
            lesson_id,
            program.summary.revision,
            &changed
        )
        .await
        .is_err());
    assert_eq!(
        repo.get(&program.summary.id).await?.summary.revision,
        program.summary.revision
    );
    assert!(prepared
        .verification
        .as_ref()
        .unwrap()
        .validate("different-lesson", &prepared)
        .is_err());
    sqlx::query("UPDATE learning_source_library SET deleted_at=1 WHERE program_id=?")
        .bind(&program.summary.id)
        .execute(&pool)
        .await?;
    assert!(repo
        .prepare(
            &program.summary.id,
            lesson_id,
            program.summary.revision,
            &prepared
        )
        .await
        .is_err());
    sqlx::query("UPDATE learning_source_library SET deleted_at=NULL WHERE program_id=?")
        .bind(&program.summary.id)
        .execute(&pool)
        .await?;
    repo.prepare(
        &program.summary.id,
        lesson_id,
        program.summary.revision,
        &prepared,
    )
    .await?;
    let saved = repo.get(&program.summary.id).await?;
    assert_eq!(
        saved.modules[0].lessons[0].preparation,
        LearningPreparation::Ready
    );
    let report_json: String = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE lesson_id=?",
    )
    .bind(lesson_id)
    .fetch_one(&pool)
    .await?;
    assert!(report_json.contains("\"lexical\""));
    assert!(!serde_json::to_string(&saved)?.contains("checker_model"));
    Ok(())
}

#[tokio::test]
async fn exact_worked_code_is_executed_and_bad_output_cannot_hide_a_failed_run() -> Result<()> {
    let runtimes = BuiltinRuntimes::for_tests();
    let good = json!({"blocks":[{"kind":"worked_example","body":"```javascript\nconsole.log(2 + 3);\n```"}]});
    let observed = execution::observe(&runtimes, &good).await?;
    assert!(observed[0].passed());
    assert!(observed[0].evidence().contains('5'));
    let bad = json!({"blocks":[{"kind":"worked_example","body":"```javascript\nthrow new Error('broken example');\n```"}]});
    assert!(!execution::observe(&runtimes, &bad).await?[0].passed());
    Ok(())
}

#[tokio::test]
async fn an_open_fence_is_no_example_and_the_closed_ones_still_run() -> Result<()> {
    let runtimes = BuiltinRuntimes::for_tests();
    let body = "```javascript\nconsole.log(1);\n```\nThen:\n```javascript\nconsole.log(2);\n";
    let lesson = json!({"blocks":[{"kind":"worked_example","body":body}]});
    let observed = execution::observe(&runtimes, &lesson).await?;
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].id, "block-0-example-0");
    assert!(observed[0].passed());
    Ok(())
}

#[tokio::test]
async fn unlabeled_data_and_code_are_labeled_without_changing_their_bytes() -> Result<()> {
    let runtimes = BuiltinRuntimes::for_tests();
    use crate::lessons::course_generation_tests::ScriptedModel;
    let model=ScriptedModel {outputs:Mutex::new(vec![json!({"fences":[{"id":"block-0-fence-0","language":"text"},{"id":"block-0-fence-1","language":"javascript"}]}).to_string()].into()),prompts:Mutex::new(Vec::new())};
    let tree = "project/\n└── main.rs\n";
    let code = "console.log(' a  b ');\n";
    let body = format!("Directory tree:\n```\n{tree}```\nRunnable example:\n```\n{code}```\n");
    let lesson = json!({"blocks":[{"kind":"worked_example","title":"Tree and program","body":body}],"questions":[]});
    let corrected: Value =
        serde_json::from_str(&normalize_example_fences(&model, lesson.to_string()).await?)?;
    assert_eq!(
        corrected["blocks"][0]["body"],
        format!(
            "Directory tree:\n```text\n{tree}```\nRunnable example:\n```javascript\n{code}```\n"
        )
    );
    let observed = execution::observe(&runtimes, &corrected).await?;
    assert_eq!(observed.len(), 1);
    assert!(observed[0].passed());
    assert!(observed[0].evidence().contains(" a  b "));
    let titled = json!({"blocks":[{"kind":"worked_example","body":"```javascript title=example.js\nconsole.log(42);\n```"}]});
    assert!(execution::observe(&runtimes, &titled).await?[0].passed());
    Ok(())
}

#[tokio::test]
async fn an_unclosed_fence_is_left_as_written_and_a_closed_one_elsewhere_is_still_labeled(
) -> Result<()> {
    use crate::lessons::course_generation_tests::ScriptedModel;
    let model = ScriptedModel {
        outputs: Mutex::new(
            vec![json!({"fences":[{"id":"block-1-fence-0","language":"text"}]}).to_string()].into(),
        ),
        prompts: Mutex::new(Vec::new()),
    };
    let open = "Try it:\n```\nprint(1)\nand the example goes on";
    let tree = "project/\n";
    let lesson = json!({"blocks":[
        {"kind":"worked_example","title":"Open","body":open},
        {"kind":"worked_example","title":"Closed","body":format!("Layout:\n```\n{tree}```\n")},
    ],"questions":[]});

    let corrected: Value =
        serde_json::from_str(&normalize_example_fences(&model, lesson.to_string()).await?)?;

    assert_eq!(corrected["blocks"][0]["body"], open);
    assert_eq!(
        corrected["blocks"][1]["body"],
        format!("Layout:\n```text\n{tree}```\n")
    );
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn invalid_fence_classification_cannot_hide_an_unexecuted_program() {
    let runtimes = BuiltinRuntimes::for_tests();
    use crate::lessons::course_generation_tests::ScriptedModel;
    let lesson = json!({"blocks":[{"kind":"worked_example","body":"```\nconsole.log(42);\n```"}],"questions":[]});
    for response in [
        json!({"fences":[]}),
        json!({"fences":[{"id":"invented","language":"text"}]}),
        json!({"fences":[{"id":"block-0-fence-0","language":"invalid-label"}]}),
    ] {
        let model = ScriptedModel {
            outputs: Mutex::new(vec![response.to_string()].into()),
            prompts: Mutex::new(Vec::new()),
        };
        assert!(normalize_example_fences(&model, lesson.to_string())
            .await
            .is_err());
    }
    assert!(execution::observe(&runtimes, &lesson).await.is_err());
}

struct TypedModel {
    response: CompletionResponse,
}

#[tokio::test]
async fn durable_judge_recovers_truncated_output_without_using_it_as_approval() {
    struct TruncatedJudge(std::sync::atomic::AtomicUsize);
    #[async_trait::async_trait]
    impl LLMPort for TruncatedJudge {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            assert!(request.no_time_limit);
            assert!(request.max_output_tokens.unwrap() > 512);
            assert_eq!(request.reasoning_effort.as_deref(), Some("low"));
            assert!(!request.want_logprobs);
            let call = self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if call == 0 {
                return Ok(CompletionResponse {
                    text: "Reason: Incomplete and not trustworthy".into(),
                    finish_reason: "length".into(),
                    ..Default::default()
                });
            }
            let sent = serde_json::to_string(&request.input)?;
            assert!(sent.contains(CORRECTION_LEAD));
            assert!(sent.contains(CUT_OFF_REFUSAL));
            Ok(CompletionResponse {
                text: "Reason: The reference does not state the claimed date.\nSource passage: none\nVerdict: unsupported".into(),
                finish_reason: "stop".into(),
                ..Default::default()
            })
        }

        fn model_name(&self) -> &str {
            "truncated-judge"
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
    let model = TruncatedJudge(Default::default());
    let result = ClaimChecker::new(
        &model,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Strict,
    )
    .check_passages_without_deadline(
        "Rust was released on the claimed date.",
        &["Rust is a language.".into()],
    )
    .await;
    assert!(
        matches!(result,ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Unsupported)
    );
    assert_eq!(model.0.load(std::sync::atomic::Ordering::Relaxed), 2);
}

#[async_trait::async_trait]
impl LLMPort for TypedModel {
    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
        Ok(self.response.clone())
    }
    fn model_name(&self) -> &str {
        "typed-fixture"
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
#[tokio::test]
async fn located_evidence_preserves_source_bytes_and_rejects_invented_locations() {
    let source = "A wrapped factual\nstatement.\n```text\na  b\n```".to_string();
    for (citation, valid) in [
        ("passage-0", true),
        ("passage-9", false),
        ("invented", false),
        ("none", false),
    ] {
        let model=TypedModel {response:CompletionResponse {text:format!("Reason: The source contains the wrapped factual statement.\nSource passage: {citation}\nVerdict: supported"),finish_reason:"stop".into(),..Default::default()}};
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_passages_without_deadline(
            "A wrapped factual statement.",
            std::slice::from_ref(&source),
        )
        .await;
        if valid {
            assert!(
                matches!(result,ClaimJudgment::Judged(outcome) if outcome.quote.as_deref()==Some(source.as_str()))
            );
        } else {
            assert!(matches!(result, ClaimJudgment::Unusable));
        }
    }
    let misleading_first_token = TypedModel {
        response: CompletionResponse {
            text: "Reason: The source matches.\nSource passage: passage-0\nVerdict: supported"
                .into(),
            finish_reason: "stop".into(),
            first_token_logprobs: Some(vec![
                ("supported".into(), 0.5_f32.ln()),
                ("unsupported".into(), 0.5_f32.ln()),
            ]),
            ..Default::default()
        },
    };
    assert!(matches!(
        ClaimChecker::new(
            &misleading_first_token,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict
        )
        .check_passages_without_deadline("A wrapped factual statement.", &[source])
        .await,
        ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.confidence.is_none()
    ));
}

#[tokio::test]
async fn durable_judgment_requires_one_final_verdict_after_its_comparison() {
    for text in [
        "supported\nReason: The source instead says it is a script, not a binary.\nSource passage: passage-0",
        "Verdict: supported\nReason: The comparison is not finished.\nSource passage: passage-0",
        "Reason: The source agrees.\nSource passage: passage-0\nVerdict: supported\nVerdict: unsupported",
        "Reason: The source agrees. Verdict: unsupported\nSource passage: passage-0\nVerdict: supported",
        "Reason: The source agrees. Verdict: supported Additional text.\nSource passage: passage-0\nVerdict: supported",
        "Reason: The source agrees. Source passage: passage-0 Verdict: supported Verdict: unsupported",
        "Reason: First reason. Reason: Replacement reason. Source passage: passage-0 Verdict: supported",
        "Reason: First reason. Reason: Replacement reason.\nSource passage: passage-0\nVerdict: supported",
        "Reason: The source agrees. Source passage: passage-0 Source passage: passage-1 Verdict: supported",
        "Reason: The source agrees. Verdict: supported Source passage: passage-0",
        "Reason: The source agrees. Source passage: passage-0 Verdict: supported Additional text.",
    ] {
        let model = TypedModel {
            response: CompletionResponse { text: text.into(), finish_reason: "stop".into(), ..Default::default() },
        };
        let result = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, CheckPolicy::Strict)
            .check_passages_without_deadline("The command downloads a binary.", &["The command downloads a script.".into()]).await;
        assert!(matches!(result, ClaimJudgment::Unusable));
    }
}

#[tokio::test]
async fn durable_judgment_accepts_an_identical_decision_echo_in_the_reason() {
    let source = "The setup process attempts to configure the environment.".to_owned();
    for verdict in ["supported", "unsupported", "contradicted"] {
        let model = TypedModel {
            response: CompletionResponse {
                text: format!("Reason: The source was compared with every assertion. Verdict: {verdict}\nSource passage: passage-0\nVerdict: {verdict}"),
                finish_reason: "stop".into(),
                ..Default::default()
            },
        };
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_passages_without_deadline(
            "The setup configures the environment.",
            std::slice::from_ref(&source),
        )
        .await;
        let ClaimJudgment::Judged(outcome) = result else {
            panic!("An identical echo made a canonical result unusable");
        };
        assert_eq!(serde_json::to_value(outcome.verdict).unwrap(), verdict);
        if verdict != "unsupported" {
            assert_eq!(outcome.quote.as_deref(), Some(source.as_str()));
        }
    }
}

#[tokio::test]
async fn durable_judgment_accepts_inline_fields_without_changing_its_evidence() {
    let source = "cargo: the Rust dependency manager and build tool".to_owned();
    for text in [
        "Reason: Passage-0 explicitly describes cargo as the Rust dependency manager and build tool. Source passage: passage-0\nVerdict: supported",
        "Reason: Passage-0 explicitly describes cargo as the Rust dependency manager and build tool. Source passage: passage-0 Verdict: supported",
        "Reason: Passage-0 explicitly describes cargo as the Rust dependency\nmanager and build tool.\nSource passage: passage-0\nVerdict: supported",
    ] {
        let model = TypedModel {
            response: CompletionResponse { text: text.into(), finish_reason: "stop".into(), ..Default::default() },
        };
        let result = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, CheckPolicy::Strict)
            .check_passages_without_deadline("cargo is the Rust dependency manager and build tool.", std::slice::from_ref(&source)).await;
        assert!(matches!(result, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.quote.as_deref() == Some(source.as_str())));
    }
    for citation in ["passage-9", "none", "invented"] {
        let model = TypedModel {
            response: CompletionResponse {
                text: format!(
                    "Reason: The source agrees. Source passage: {citation} Verdict: supported"
                ),
                finish_reason: "stop".into(),
                ..Default::default()
            },
        };
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_passages_without_deadline(
            "cargo is the Rust dependency manager and build tool.",
            std::slice::from_ref(&source),
        )
        .await;
        assert!(matches!(result, ClaimJudgment::Unusable));
    }
}

#[tokio::test]
async fn source_passage_prose_does_not_create_a_second_protocol_field() {
    let source = "The measured value is 12.".to_owned();
    for text in [
        "Reason: The factual assertion maps directly to a source passage: the measured value is 12.\nSource passage: passage-0\nVerdict: supported",
        "Reason: One source passage: the measured value is 12. Another source passage: the measurement is recorded.\nSource passage: passage-0\nVerdict: supported",
    ] {
        let model = TypedModel { response: CompletionResponse { text: text.into(), finish_reason: "stop".into(), ..Default::default() } };
        let result = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, CheckPolicy::Fidelity)
            .check_passages_without_deadline("The measured value is 12.", std::slice::from_ref(&source)).await;
        assert!(matches!(result, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.quote.as_deref() == Some(source.as_str())));
    }
    let model = TypedModel { response: CompletionResponse { text: "Reason: The source agrees.\nSource passage: passage-0\nSource passage: passage-1\nVerdict: supported".into(), finish_reason: "stop".into(), ..Default::default() } };
    let result = ClaimChecker::new(
        &model,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Fidelity,
    )
    .check_passages_without_deadline("The measured value is 12.", &[source])
    .await;
    assert!(matches!(result, ClaimJudgment::Unusable));
}

#[tokio::test]
async fn malformed_verdict_gets_one_format_correction_without_approving_missing_evidence() {
    use crate::lessons::course_generation_tests::ScriptedModel;
    let malformed="The passages describe Rust tools but do not mention Visual Studio.\nReason: Visual Studio is absent.\nSource quote: none";
    for corrected in [
        "unsupported\nReason: Visual Studio is absent.\nSource quote: none",
        malformed,
    ] {
        let model = ScriptedModel {
            outputs: Mutex::new(vec![malformed.into(), corrected.into()].into()),
            prompts: Mutex::new(vec![]),
        };
        let evidence = ClaimEvidence {
            text: SOURCE.into(),
            quote: None,
        };
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check("Visual Studio includes a package manager.", &evidence)
        .await;
        if corrected.starts_with("unsupported") {
            assert!(
                matches!(result,ClaimJudgment::Judged(outcome) if outcome.verdict==ClaimVerdict::Unsupported)
            );
        } else {
            assert!(matches!(result, ClaimJudgment::Unusable));
        }
        let prompts = model.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 2);
        assert!(prompts[1].contains(SOURCE));
        assert!(prompts[1].contains(CORRECTION_LEAD));
    }
    use lattice_core::application::services::claim_verification::parse_verdict_word;
    assert_eq!(
        parse_verdict_word("Label: unsupported\nReason: Missing source."),
        Some(ClaimVerdict::Unsupported)
    );
    assert_eq!(
        parse_verdict_word("Verdict: supported\nReason: Matching source."),
        Some(ClaimVerdict::Supported)
    );
}

#[tokio::test]
async fn strict_checker_records_a_long_or_reflowed_quote_as_the_sources_exact_text() {
    let quote = format!(
        "{} The field contains two spaces: a  b.",
        "A factual reference sentence. ".repeat(20)
    );
    let evidence = ClaimEvidence {
        text: quote.clone(),
        quote: None,
    };
    for cited in [quote.clone(), quote.replace("a  b", "a b")] {
        let model=TypedModel {response:CompletionResponse {
            text:format!("supported\nReason: The exact reference contains the statement.\nSource quote: {cited}"),
            finish_reason:"stop".into(),..Default::default()
        }};
        let checker = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        );
        let result = checker
            .check("The field contains two spaces.", &evidence)
            .await;
        assert!(
            matches!(result,ClaimJudgment::Judged(outcome) if outcome.quote.as_deref()==Some(quote.as_str()))
        );
    }
}

#[tokio::test]
async fn strict_checker_rejects_missing_invented_quotes_and_truncated_replies() {
    let evidence = ClaimEvidence {
        text: SOURCE.into(),
        quote: Some(GOOD.into()),
    };
    for (text, finish) in [("supported","stop"),("supported\nReason: a match\nSource quote: fabricated", "stop"),("supported\nReason: a match\nSource quote: DictReader preserves field-name whitespace by default.","length")] {
        let model=TypedModel { response:CompletionResponse { text:text.into(),finish_reason:finish.into(), ..Default::default() } };
        let strict=ClaimChecker::new(&model,SamplingOverride::deterministic(),512,CheckPolicy::Strict).check(GOOD,&evidence).await;
        assert!(matches!(strict,ClaimJudgment::Unusable));
    }
    for alternatives in [
        vec![
            ("contradicted".into(), 0.9_f32.ln()),
            ("supported".into(), 0.1_f32.ln()),
        ],
        vec![
            ("supported".into(), 0.5_f32.ln()),
            ("contradicted".into(), 0.5_f32.ln()),
        ],
    ] {
        let model = TypedModel {
            response: CompletionResponse {
                text: format!(
                    "supported\nReason: The source preserves spaces.\nSource quote: {GOOD}"
                ),
                finish_reason: "stop".into(),
                first_token_logprobs: Some(alternatives),
                ..Default::default()
            },
        };
        let checker = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        );
        assert!(matches!(
            checker.check(GOOD, &evidence).await,
            ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Unsupported && outcome.quote.is_none()
        ));
    }
}

pub(crate) async fn install_source(db: &Database, program_id: &str, text: &str) -> Result<()> {
    use crate::{
        dto::{LearningSourceKind, LearningSourcePolicy},
        source_library::{CapturedLearningSource, LearningSourceLibraryRepository},
    };
    let id = || uuid::Uuid::new_v4().to_string();
    LearningSourceLibraryRepository::new(db.clone())
        .add(
            crate::source_library::NewLearningSource {
                operation_id: &id(),
                source_id: &id(),
                version_id: &id(),
                program_id,
                kind: LearningSourceKind::Pasted,
                operation_kind: "add_text",
                origin: "Test reference",
                requested_url: None,
                freshness: LearningSourcePolicy::Fixed,
            },
            CapturedLearningSource {
                title: "Test reference".into(),
                publisher: None,
                requested_url: None,
                resolved_url: None,
                text: text.into(),
                truncated: false,
                extraction_version: "test".into(),
                signals: crate::references::trust::capture::not_extracted("test fixture"),
            },
            "fixture".into(),
        )
        .await
}

#[tokio::test]
async fn unsupported_languages_are_disclosed_and_csv_fixture_is_test_only() -> Result<()> {
    let runtimes = BuiltinRuntimes::for_tests();
    let rust = json!({"blocks":[{"kind":"worked_example","body":"```rust\nfn main() {}\n```"}]});
    assert!(execution::observe(&runtimes, &rust).await?.is_empty());
    assert_eq!(execution::unexecuted_languages(&rust), vec!["rust"]);
    let csv = json!({"blocks":[{"kind":"worked_example","body":format!("```python\n{}\n```", include_str!("csv_reference.py"))}]});
    let observed = execution::observe(&runtimes, &csv).await?;
    assert!(observed[0].passed());
    assert!(observed[0].evidence().contains("header_whitespace"));
    Ok(())
}

#[tokio::test]
async fn evidence_view_hides_assessment_claims_and_marks_source_changes() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let program = crate::tests::fixture();
    let repo = crate::repository::LearningRepository::new(db.clone());
    repo.create(&program).await?;
    let lesson = &program.modules[0].lessons[0];
    // This fixture deliberately puts a private key in an assessment unit. The
    // public projection must omit it even when there are zero teaching blocks.
    let source = &program.sources[0];
    let saved = repo.verification_sources(&program.summary.id).await?;
    let report = json!({"policy":POLICY,"lesson_id":lesson.id,"content_sha256":"fixture","checked_at":1,"checker_model":"fixture","sources":[{"id":source.id,"sha256":sha256_hex(&saved[0].excerpt)}],"findings":[{"unit":0,"quote":"PRIVATE QUOTE","statement":"PRIVATE ANSWER KEY","verdict":"supported","reason":"PRIVATE RATIONALE","evidence":[],"supporting_quote":null}],"coverage":[],"coverage_audit":[],"executions":[],"issues":[],"retrieval_mode":"hybrid","embedding_model":null,"unexecuted_languages":[]});
    sqlx::query("INSERT INTO learning_lesson_verifications(lesson_id,program_id,policy,content_sha256,report_json,checked_at) VALUES(?,?,?,?,?,?)").bind(&lesson.id).bind(&program.summary.id).bind(POLICY.as_str()).bind("fixture").bind(report.to_string()).bind(1_i64).execute(&pool).await?;
    let view = crate::lesson_evidence::get(&db, &program.summary.id, &lesson.id)
        .await?
        .unwrap();
    assert!(view.teaching_claims.is_empty());
    assert!(!serde_json::to_string(&view)?.contains("PRIVATE"));
    assert!(view.sources_current);
    sqlx::query("UPDATE learning_source_library SET deleted_at=1 WHERE program_id=?")
        .bind(&program.summary.id)
        .execute(&pool)
        .await?;
    let view = crate::lesson_evidence::get(&db, &program.summary.id, &lesson.id)
        .await?
        .unwrap();
    assert!(!view.sources_current);

    // Missing unit identity must not silently become teaching section zero.
    let mut malformed = report.clone();
    malformed["findings"][0]
        .as_object_mut()
        .unwrap()
        .remove("unit");
    sqlx::query("UPDATE learning_lesson_verifications SET report_json=? WHERE lesson_id=?")
        .bind(malformed.to_string())
        .bind(&lesson.id)
        .execute(&pool)
        .await?;
    let error = crate::lesson_evidence::get(&db, &program.summary.id, &lesson.id)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("unavailable"));
    assert!(!error.contains("PRIVATE"));

    let mut foreign = report;
    foreign["lesson_id"] = json!("another-lesson");
    sqlx::query("UPDATE learning_lesson_verifications SET report_json=? WHERE lesson_id=?")
        .bind(foreign.to_string())
        .bind(&lesson.id)
        .execute(&pool)
        .await?;
    assert!(
        crate::lesson_evidence::get(&db, &program.summary.id, &lesson.id)
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn a_questions_reasons_travel_with_its_unit_and_a_question_without_them_is_unchanged() {
    let mut lesson = candidate(GOOD);
    lesson["questions"] = json!([
        {"prompt":"Which?","options":["a","b","c","d"],"correctIndex":0,"explanation":"A holds.",
            "rationales":["","B fails.","C fails.","D fails."]},
        {"prompt":"Why?","options":["a","b","c","d"],"correctIndex":1,"explanation":"B holds."}
    ]);

    let content = units(&lesson).unwrap();

    assert_eq!(
        content[1]["rationales"],
        json!(["", "B fails.", "C fails.", "D fails."])
    );
    assert!(content[2].get("rationales").is_none());
}
