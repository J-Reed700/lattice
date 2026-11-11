//! The sentences a passage is shown as, and the id a producer of claims names
//! one by. The extractor and the coverage audit share this: a claim's quote is
//! the exact text of the sentence it names, split the way protection and
//! confinement split a field, so the quote is one of the sentences a repair is
//! confined by.
use lattice_core::shared::text::sentences;
use serde_json::{json, Value};

/// The sentences of `passage`, in order, each as its id and its exact text.
/// An id is its passage's id and the sentence's place in it.
fn of(passage: &Value) -> Vec<(String, String)> {
    let (Some(id), Some(text)) = (passage["id"].as_str(), passage["text"].as_str()) else {
        return Vec::new();
    };
    sentences(text)
        .into_iter()
        .enumerate()
        .filter_map(|(place, (start, end))| {
            text.get(start..end)
                .map(|sentence| (format!("{id}-sentence-{place}"), sentence.to_owned()))
        })
        .collect()
}

/// `passage` as it is shown to a model: its sentences, each with its id, in
/// place of its text.
pub(super) fn listed(passage: &Value) -> Value {
    let mut shown = passage.clone();
    if let Some(fields) = shown.as_object_mut() {
        fields.remove("text");
        let sentences: Vec<Value> = of(passage)
            .into_iter()
            .map(|(id, text)| json!({"id":id,"text":text}))
            .collect();
        fields.insert("sentences".into(), Value::Array(sentences));
    }
    shown
}

/// `unit` with its passages listed.
pub(super) fn shown(unit: &Value) -> Value {
    let mut shown = unit.clone();
    if let Some(passages) = shown.get_mut("passages").and_then(Value::as_array_mut) {
        *passages = passages.iter().map(listed).collect();
    }
    shown
}

/// The id of every sentence of `passages`.
pub(super) fn ids(passages: &[Value]) -> Vec<String> {
    passages.iter().flat_map(of).map(|(id, _)| id).collect()
}

/// The passage that holds the sentence `sentence_id` of `passages`, and the
/// sentence's exact text.
pub(super) fn quote(passages: &[Value], sentence_id: &str) -> Option<(String, String)> {
    passages.iter().find_map(|passage| {
        of(passage)
            .into_iter()
            .find(|(id, _)| id == sentence_id)
            .and_then(|(_, text)| Some((passage["id"].as_str()?.to_owned(), text)))
    })
}
