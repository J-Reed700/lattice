//! Scripted answers to a section repair, written as flat edits and read as the entries its request asks for.
use super::{labelled, part_of, pointer_of, teaching_repair_answer, unmarked};
use crate::lessons::teaching::repair_script::sentence_label;
use serde_json::{json, Value};

/// The request's task, when `prompt` is a section repair's.
pub(crate) fn task_of(prompt: &str) -> Option<Value> {
    prompt.rsplit("\n\n").find_map(|chunk| {
        serde_json::from_str::<Value>(chunk).ok().filter(|task| {
            task.get("failedClaims").is_some() && task.get("repairHistory").is_some()
        })
    })
}

/// The entries of the failed claims sent, each once, in order.
fn entry_handles(task: &Value) -> Vec<String> {
    let mut handles: Vec<String> = Vec::new();
    for row in task["failedClaims"].as_array().into_iter().flatten() {
        if let Some(handle) = row["entry"].as_str() {
            if !handles.iter().any(|known| known == handle) {
                handles.push(handle.to_owned());
            }
        }
    }
    handles
}

/// The place, among `handles`, of the entry whose sentence carries `label`.
fn entry_of(task: &Value, handles: &[String], label: &str) -> Option<usize> {
    let row = task["failedClaims"].as_array()?.iter().find(|row| {
        row["sentence"]
            .as_array()
            .is_some_and(|labels| labels.iter().any(|held| held.as_str() == Some(label)))
    })?;
    handles
        .iter()
        .position(|handle| Some(handle.as_str()) == row["entry"].as_str())
}

/// A section repair's answer, in the form its request asks for, making the
/// changes of a flat answer (`{"edits":[{path,before,after}],"citations":[..],"uncite":[..],"recites":[..]}`):
/// an entry for each failed sentence sent, each change in the entry whose
/// sentence it touches, else in the entry its `"finding"` place names, else in the first.
/// An answer already in the form of the request (`{"entries":[..]}`) is returned as written.
pub(crate) fn answer(task: &Value, flat: &Value) -> String {
    if flat.get("entries").is_some() {
        return flat.to_string();
    }
    let handles = entry_handles(task);
    let part = if task["candidate"]["questions"][0].is_object() {
        "/questions/0"
    } else {
        "/blocks/0"
    };
    let issues: Vec<Value> = handles
        .iter()
        .map(|handle| json!({"finding": handle, "part": part, "also": null}))
        .collect();
    let excerpts: Vec<&str> = task["evidence"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|passage| passage["text"].as_str())
        .collect();
    let mut sources: Vec<Value> = Vec::new();
    for key in ["citations", "recites"] {
        for named in flat[key].as_array().into_iter().flatten() {
            let Some(index) = named["sourceIndex"].as_u64() else {
                continue;
            };
            if !sources.iter().any(|source| source["sourceIndex"] == index) {
                sources.push(json!({"sourceIndex": index, "excerpt": excerpts.join("\n")}));
            }
        }
    }
    let pseudo = json!({
        "candidate": task["candidate"],
        "issuesToFix": issues,
        "originalRequirements": {"sources": sources},
    });
    let mut routed = flat.clone();
    for edit in routed["edits"].as_array_mut().into_iter().flatten() {
        if edit.get("finding").is_some() {
            continue;
        }
        let path = edit["path"].as_str().unwrap_or_default().to_owned();
        let Some(marked) = pseudo["candidate"]
            .pointer(&pointer_of(&path))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let (lead, pieces) = labelled(marked);
        let plain = unmarked(marked);
        let Some(start) = edit["before"]
            .as_str()
            .and_then(|before| plain.find(before))
        else {
            continue;
        };
        let mut at = lead.len();
        let mut place = None;
        for (label, text) in &pieces {
            let stops = at + text.trim_end().len();
            if place.is_none() && start >= at && start < stops {
                place = entry_of(task, &handles, label);
            }
            at += text.len();
        }
        if let (Some(place), Some(fields)) = (place, edit.as_object_mut()) {
            fields.insert("finding".into(), json!(place));
        }
    }
    for citation in routed["citations"].as_array_mut().into_iter().flatten() {
        if citation.get("finding").is_some() {
            continue;
        }
        let path = citation["path"].as_str().unwrap_or_default().to_owned();
        let part = part_of(&pointer_of(&path));
        let Some(sentence) = citation["sentence"].as_str().map(str::to_owned) else {
            continue;
        };
        let label = ["body", "explanation", "reasons"]
            .iter()
            .find_map(|field| sentence_label(&pseudo, &format!("{part}/{field}"), &sentence));
        let place = label.and_then(|label| entry_of(task, &handles, &label));
        if let (Some(place), Some(fields)) = (place, citation.as_object_mut()) {
            fields.insert("finding".into(), json!(place));
        }
    }
    let mut made: Value =
        serde_json::from_str(&teaching_repair_answer(&pseudo, &routed)).unwrap_or(Value::Null);
    for entry in made["entries"].as_array_mut().into_iter().flatten() {
        if let Some(fields) = entry.as_object_mut() {
            fields.remove("plan");
        }
    }
    made.to_string()
}
