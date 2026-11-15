//! Scripted teaching reviews for the fixture model: a script written as
//! `{"issues":[…],"sectionDefects":[…],"openFindings":[…]}` is answered in the
//! form the request asks for, with the handles the request shows.
use super::{ANSWER_FORM, CONCERN_FORM};
use crate::lessons::content_verification::tests::fixture_task;
use serde_json::{json, Value};

/// The reply to the review request `prompt` that the legacy `script` calls
/// for, or `None` when the prompt is not a review or the script is already in
/// the layer's form.
pub(crate) fn reply(prompt: &str, script: &str) -> Option<String> {
    if !prompt.contains(ANSWER_FORM) {
        return None;
    }
    let wanted: Value = serde_json::from_str(script).ok()?;
    let legacy = ["issues", "sectionDefects", "openFindings"]
        .iter()
        .any(|key| wanted.get(key).is_some());
    if !legacy {
        return None;
    }
    let task = fixture_task(prompt)?;
    let lesson = prompt.contains(CONCERN_FORM.trim());
    let open: Vec<Value> = task
        .get("openFindings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    let mut shown = |legacy: &str| -> Value {
        if let Some(entry) = open.iter().find(|entry| entry["id"] == legacy) {
            return entry["id"].clone();
        }
        // A legacy hash id is no longer shown: the nth distinct id is the nth open finding shown.
        let at = names.iter().position(|name| name == legacy).unwrap_or_else(|| {
            names.push(legacy.to_owned());
            names.len() - 1
        });
        open.get(at).map_or(json!("f999"), |entry| entry["id"].clone())
    };
    let mut items = Vec::new();
    let mut resolved = Vec::new();
    for verdict in wanted["openFindings"].as_array().into_iter().flatten() {
        let handle = shown(verdict["id"].as_str().unwrap_or_default());
        if verdict["verdict"] == "resolved" {
            resolved.push(json!({"entry":{"kind":"resolved","openFinding":handle}}));
        }
    }
    for issue in wanted["issues"].as_array().into_iter().flatten() {
        let (note, continues, concern) = match issue {
            Value::String(note) => (json!(note), None, None),
            other => (
                other["note"].clone(),
                other["continues"].as_str().map(str::to_owned),
                other["concern"].as_str().map(str::to_owned),
            ),
        };
        let continues = continues.map_or(json!("none"), |id| shown(&id));
        let mut entry = json!({"kind":"issue","note":note,"continues":continues});
        if lesson {
            entry["concern"] = json!(concern.unwrap_or_else(|| "promise".into()));
        }
        items.push(json!({"entry":entry}));
    }
    for defect in wanted["sectionDefects"].as_array().into_iter().flatten() {
        let place = defect["passageId"]
            .as_str()
            .and_then(passage_place)
            .and_then(|(section, number)| {
                task.pointer(&format!("/candidate/blocks/{section}/bodyPassages/{number}/id"))
            })
            .cloned()
            .unwrap_or(json!("p999"));
        let continues = defect["continues"]
            .as_str()
            .map_or(json!("none"), &mut shown);
        let mut entry = json!({"kind":"sectionDefect","passageId":place,"finding":defect["finding"],"continues":continues});
        if lesson {
            let concerns: Vec<Value> = defect["concerns"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|concern| json!({"concern":concern}))
                .collect();
            entry["concerns"] = json!(concerns);
        }
        items.push(json!({"entry":entry}));
    }
    items.extend(resolved);
    Some(json!({"items":items}).to_string())
}

/// The section and passage number of a legacy `section-N-passage-M` name.
fn passage_place(name: &str) -> Option<(usize, usize)> {
    let (section, number) = name.strip_prefix("section-")?.split_once("-passage-")?;
    Some((section.parse().ok()?, number.parse().ok()?))
}
