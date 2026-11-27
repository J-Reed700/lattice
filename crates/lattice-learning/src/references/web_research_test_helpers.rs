use super::Room;
use serde_json::{json, Value};

/// A planned search as the model writes it to the layer, from the fields a test
/// names: the others are left empty and a gap given as its index is written as
/// its handle.
pub(crate) fn written_need(need: &Value) -> Value {
    let mut need = need.as_object().cloned().unwrap_or_default();
    for field in ["subject", "identifier", "site", "after", "asOf"] {
        need.entry(field).or_insert(json!(""));
    }
    need.entry("ecosystem").or_insert(json!("none"));
    let rewordings = need.remove("alternatives").unwrap_or_else(|| json!([]));
    let rewordings: Vec<Value> = rewordings
        .as_array()
        .into_iter()
        .flatten()
        .map(|text| json!({ "text": text }))
        .collect();
    need.insert("alternatives".into(), json!(rewordings));
    if let Some(gap) = need.get("gap").and_then(Value::as_u64) {
        need.insert("gap".into(), json!(format!("g{}", gap + 1)));
    }
    Value::Object(need)
}

/// The answer of a search planner to the layer: `needs` as a list of searches.
pub(crate) fn needs_answer(needs: Value) -> Value {
    let items: Vec<Value> = needs
        .as_array()
        .into_iter()
        .flatten()
        .map(written_need)
        .collect();
    json!({ "items": items })
}

impl Room<'static> {
    /// Room for every result, for tests about something else.
    pub(crate) fn for_all() -> Self {
        fn one(_: &super::Candidate) -> usize {
            1
        }
        Self {
            tokens: usize::MAX,
            cost: &one,
        }
    }
}
