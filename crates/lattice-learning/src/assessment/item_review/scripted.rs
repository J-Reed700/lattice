//! Scripted replies to the item judge, for tests that run a quiz review.
use serde_json::{json, Value};

/// The number in a handle such as `q3`, from one.
fn number(handle: &Value) -> usize {
    handle
        .as_str()
        .and_then(|written| written.get(1..))
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(0)
}

/// A review of every item of `task`. `tell` gives an item its one defect (none when empty) and the
/// questions it repeats: `have-N` is the learner's question N and `new-N` the Nth item of the set,
/// both from zero.
pub(crate) fn judged(task: &Value, tell: impl Fn(&Value) -> (String, Vec<String>)) -> Value {
    let entries: Vec<Value> = task["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            let (finding, repeats) = tell(item);
            let me = number(&item["id"]).saturating_sub(1);
            let mut held = Vec::new();
            let mut new = Vec::new();
            for repeat in &repeats {
                let named = |prefix: &str| {
                    repeat
                        .strip_prefix(prefix)
                        .and_then(|n| n.parse::<usize>().ok())
                };
                if let Some(n) = named("have-") {
                    held.push(format!("h{}", n + 1));
                } else if let Some(n) = named("new-") {
                    match n.cmp(&me) {
                        std::cmp::Ordering::Less => new.push(format!("n{}", n + 1)),
                        std::cmp::Ordering::Greater => new.push(format!("n{n}")),
                        std::cmp::Ordering::Equal => {}
                    }
                }
            }
            let defects: Vec<Value> = if finding.is_empty() {
                Vec::new()
            } else {
                vec![json!({"defect": finding})]
            };
            json!({
                "of": item["id"],
                "defects": defects,
                "repeatsHeld": held,
                "repeatsNew": new
            })
        })
        .collect();
    json!({"entries": entries})
}

/// A review that finds nothing in any item of `task`.
pub(crate) fn sound(task: &Value) -> Value {
    judged(task, |_| (String::new(), Vec::new()))
}

/// A review of items `q1` to `q{items}` that finds the defect `fault` gives in the item it names (from zero) and nothing else.
pub(crate) fn judged_static(items: usize, fault: Option<(usize, &str)>) -> Value {
    let entries: Vec<Value> = (0..items)
        .map(|item| {
            let defects: Vec<Value> = fault
                .filter(|(at, _)| *at == item)
                .map(|(_, text)| json!({"defect": text}))
                .into_iter()
                .collect();
            json!({
                "of": format!("q{}", item + 1),
                "defects": defects,
                "repeatsHeld": [],
                "repeatsNew": []
            })
        })
        .collect();
    json!({"entries": entries})
}

/// A review that finds nothing in items `q1` to `q{items}`.
pub(crate) fn sound_static(items: usize) -> Value {
    judged_static(items, None)
}
