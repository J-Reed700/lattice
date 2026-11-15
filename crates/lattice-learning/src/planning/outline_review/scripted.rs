//! Scripted outline reviews for the fixture model: a script says what the
//! review judges (`{"verdicts":[{id,status,reason}],"newFindings":[{path,kind,claim,reason}]}`,
//! the n-th verdict for the n-th open finding shown) and the reply is written
//! in the form the request asks for, with the handles the request shows.
use super::contract::REVIEW_SYSTEM;
use crate::lessons::content_verification::tests::fixture_task;
use serde_json::{json, Value};

/// The reply to the review request `prompt` that `script` calls for, or
/// `None` when the prompt is not a review or the script is not a review.
pub(crate) fn reply(prompt: &str, script: &str) -> Option<String> {
    if !prompt.starts_with(REVIEW_SYSTEM) {
        return None;
    }
    let wanted: Value = serde_json::from_str(script).ok()?;
    wanted.get("newFindings")?;
    let task = fixture_task(prompt)?;
    let handles = |list: &str| -> Vec<Value> {
        task.get(list)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let open = handles("openFindings");
    let places = handles("allowedPaths");
    let mut items = Vec::new();
    for (at, verdict) in wanted["verdicts"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let handle = open
            .get(at)
            .map_or(json!("f999"), |shown| shown["id"].clone());
        items.push(json!({"entry":{"kind":"verdict","at":handle,"status":verdict["status"],"reason":verdict["reason"]}}));
    }
    for finding in wanted["newFindings"].as_array().into_iter().flatten() {
        let handle = places
            .iter()
            .find(|shown| shown["path"] == finding["path"])
            .map_or(json!("p999"), |shown| shown["id"].clone());
        items.push(json!({"entry":{"kind":"finding","place":handle,"category":finding["kind"],"reason":finding["reason"]}}));
    }
    Some(json!({"items":items}).to_string())
}
