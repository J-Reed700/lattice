use crate::review_findings::FindingSite;
use serde_json::{json, Value};

/// The answer's schema: a list of decisions, each naming its issue among
/// `keys` and citing among `ids`.
pub(super) fn schema(keys: &[String], ids: &[String]) -> Value {
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
    let decision = json!({"type":"object","additionalProperties":false,
        "required":["issueId","verdict","basis","evidenceIds","requirementId","reason","partId"],"properties":{
        "issueId":{"type":"string","enum":keys},
        "verdict":{"type":"string","enum":["actionable","not_established"]},
        "basis":{"type":"string","enum":["external_fact","lesson_contract","internal_conflict"]},
        "evidenceIds":{"type":"array","minItems":0,"maxItems":8,"uniqueItems":true,"items":{"type":"string","enum":ids}},
        "requirementId":{"enum":["requirements",null]},
        "reason":{"type":"string","minLength":1},
        "partId":{"type":"string","enum":parts}}});
    json!({"type":"object","additionalProperties":false,"required":["decisions"],"properties":{
        "decisions":{"type":"array","items":decision}}})
}
