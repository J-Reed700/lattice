use crate::review_findings::FindingSite;
use serde_json::{json, Value};

/// The answer's schema: a list of decisions, each naming its issue among
/// `keys` and citing among `ids`; with `recorded` findings or `checks` beside
/// the reviewer each also says which describes its defect or decides it, or
/// none.
pub(super) fn schema(
    keys: &[String],
    ids: &[String],
    recorded: &[String],
    checks: &[String],
) -> Value {
    // Keep the wire schema flat. In a constrained decoder, choosing an empty
    // evidence list must not commit an earlier anyOf branch and force a later
    // verdict to `not_established`. Validate the conditional evidence rules in
    // accepted(), where an inconsistent response can be corrected explicitly.
    let parts: Vec<&str> = ids
        .iter()
        .map(String::as_str)
        .filter(|id| FindingSite::of_part(id).is_some())
        .chain(["lesson"])
        .collect();
    let mut decision = json!({"type":"object","additionalProperties":false,
        "required":["issueId","verdict","basis","evidenceIds","requirementId","reason","partId"],"properties":{
        "issueId":{"type":"string","enum":keys},
        "verdict":{"type":"string","enum":["actionable","not_established"]},
        "basis":{"type":"string","enum":["external_fact","lesson_contract","internal_conflict"]},
        "evidenceIds":{"type":"array","minItems":0,"maxItems":8,"uniqueItems":true,"items":{"type":"string","enum":ids}},
        "requirementId":{"enum":["requirements",null]},
        "reason":{"type":"string","minLength":1},
        "partId":{"type":"string","enum":parts}}});
    if !recorded.is_empty() || !checks.is_empty() {
        let named: Vec<&str> = recorded
            .iter()
            .map(String::as_str)
            .chain(checks.iter().map(String::as_str))
            .chain(["none"])
            .collect();
        if let Some(required) = decision.get_mut("required").and_then(Value::as_array_mut) {
            required.push(json!("leftTo"));
        }
        if let Some(properties) = decision
            .get_mut("properties")
            .and_then(Value::as_object_mut)
        {
            properties.insert("leftTo".into(), json!({"type":"string","enum":named}));
        }
    }
    json!({"type":"object","additionalProperties":false,"required":["decisions"],"properties":{
        "decisions":{"type":"array","items":decision}}})
}
