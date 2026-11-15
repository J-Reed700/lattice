//! Scripted answers for tests that hold an outline in its stored form: the answers the
//! outline's two questions are asked for, made from the outline and the prompt.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

const DESCRIBE_TASK: &str = "Propose an editable, subject-neutral learning program outline";
const PREREQUISITES_TASK: &str = "Name the earlier modules each module of the outline builds on";

/// The positions each module builds on, by the titles of the modules up to it.
static BUILT_ON: LazyLock<Mutex<HashMap<String, Vec<usize>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn task_of(prompt: &str) -> Option<Value> {
    prompt.split("\n\n").find_map(|chunk| {
        serde_json::from_str::<Value>(chunk)
            .ok()
            .filter(|task| task.get("task").is_some())
    })
}

fn key_of(titles: &[&str]) -> String {
    titles.join("\u{1f}")
}

fn lines(texts: &Value) -> Value {
    json!(texts
        .as_array()
        .into_iter()
        .flatten()
        .map(|text| json!({ "text": text }))
        .collect::<Vec<_>>())
}

/// The answer to the outline request `prompt` that describes the stored outline `stored`: each
/// citation names the passage the request shows that holds its quote. `None` when `prompt` asks
/// for something else or `stored` is no outline.
pub(crate) fn description_answer(prompt: &str, stored: &str) -> Option<String> {
    let task = task_of(prompt).filter(|task| task["task"] == DESCRIBE_TASK)?;
    let outline: Value = serde_json::from_str(stored).ok()?;
    let modules = outline.get("modules")?.as_array()?;
    let passages: Vec<(String, String)> = task["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|source| source["passages"].as_array().into_iter().flatten())
        .filter_map(|passage| {
            Some((
                passage["id"].as_str()?.to_owned(),
                passage["text"].as_str()?.to_owned(),
            ))
        })
        .collect();
    let cite = |item: &Value| -> Value {
        let quote = item["quote"].as_str().unwrap_or_default().trim();
        if quote.is_empty() {
            return json!([]);
        }
        let whole = passages.iter().find(|(_, text)| text.contains(quote));
        let ids: Vec<&String> = match whole {
            Some((id, _)) => vec![id],
            None => passages
                .iter()
                .filter(|(_, text)| quote.contains(text.as_str()))
                .map(|(id, _)| id)
                .collect(),
        };
        json!(ids)
    };
    let titles: Vec<&str> = modules
        .iter()
        .map(|module| module["title"].as_str().unwrap_or_default())
        .collect();
    {
        let mut built = BUILT_ON.lock().unwrap_or_else(PoisonError::into_inner);
        for (at, module) in modules.iter().enumerate() {
            let positions = module["prerequisiteIndices"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|position| position.as_u64().and_then(|n| usize::try_from(n).ok()))
                .collect();
            built.insert(key_of(&titles[..=at]), positions);
        }
    }
    let written: Vec<Value> = modules
        .iter()
        .map(|module| {
            json!({
                "title": module["title"],
                "summary": {"text": module["summary"]["text"], "sources": cite(&module["summary"])},
                "outcomes": module["outcomes"].as_array().into_iter().flatten()
                    .map(|outcome| json!({"text": outcome["text"], "sources": cite(outcome)}))
                    .collect::<Vec<_>>(),
                "project": {
                    "title": module["project"]["title"],
                    "brief": module["project"]["brief"],
                    "deliverables": lines(&module["project"]["deliverables"]),
                    "successCriteria": lines(&module["project"]["successCriteria"]),
                },
                "lessons": module["lessons"].as_array().into_iter().flatten()
                    .map(|lesson| json!({
                        "title": lesson["title"],
                        "objective": lesson["objective"],
                        "estimatedMinutes": lesson["estimatedMinutes"].as_i64().unwrap_or(0).max(0),
                        "sources": cite(lesson),
                    }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    Some(json!({ "modules": written }).to_string())
}

/// The answer to the prerequisites question `prompt`: each module builds on the earlier modules the
/// outline last described for it said it builds on. `None` when `prompt` asks for something else.
pub(crate) fn prerequisites_answer(prompt: &str) -> Option<String> {
    let task = task_of(prompt).filter(|task| task["task"] == PREREQUISITES_TASK)?;
    let built = BUILT_ON.lock().unwrap_or_else(PoisonError::into_inner);
    let entries: Vec<Value> = task["modules"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|module| {
            let earlier = module["earlierModules"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut titles: Vec<&str> = earlier
                .iter()
                .map(|before| before["title"].as_str().unwrap_or_default())
                .collect();
            titles.push(module["title"].as_str().unwrap_or_default());
            let named: Vec<Value> = built
                .get(&key_of(&titles))
                .into_iter()
                .flatten()
                .filter_map(|position| earlier.get(*position))
                .map(|before| before["module"].clone())
                .collect();
            json!({"of": module["module"], "prerequisites": named})
        })
        .collect();
    Some(json!({ "entries": entries }).to_string())
}
