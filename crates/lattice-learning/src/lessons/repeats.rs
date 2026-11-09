//! Whether a sentence a repair writes is one the lesson already has. Two
//! sentences are the same when they are equal after trimming and collapsing
//! whitespace runs.
use crate::generation::teaching_sentences;
use serde_json::Value;
use std::collections::HashMap;

/// How often each sentence occurs in some text.
pub(crate) type Counts = HashMap<String, usize>;

/// `sentence` as sentences are compared.
pub(crate) fn key(sentence: &str) -> String {
    sentence.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The compared teaching sentences of `text`, in order.
pub(crate) fn keys(text: &str) -> Vec<String> {
    teaching_sentences(text)
        .iter()
        .map(|sentence| key(sentence))
        .collect()
}

fn counts_of_text(text: &str) -> Counts {
    let mut counts = Counts::new();
    for sentence in keys(text) {
        *counts.entry(sentence).or_default() += 1;
    }
    counts
}

/// Whether `name` holds text a lesson teaches: not its kind, and not the
/// citations, which quote sentences the part holds.
fn teaches(name: &str) -> bool {
    name != "kind" && name != "citations"
}

fn collect(value: &Value, counts: &mut Counts) {
    match value {
        Value::String(text) => {
            for (sentence, count) in counts_of_text(text) {
                *counts.entry(sentence).or_default() += count;
            }
        }
        Value::Object(fields) => {
            for value in fields
                .iter()
                .filter(|(name, _)| teaches(name))
                .map(|(_, value)| value)
            {
                collect(value, counts);
            }
        }
        Value::Array(items) => {
            for value in items {
                collect(value, counts);
            }
        }
        _ => (),
    }
}

/// The sentences of every text in `values`.
pub(crate) fn counts_of(values: &[&Value]) -> Counts {
    let mut counts = Counts::new();
    for value in values {
        collect(value, &mut counts);
    }
    counts
}

/// The first sentence the text `after` has more of than `before` that
/// `lesson` already has.
pub(crate) fn gained_repeat(before: &str, after: &str, lesson: &Counts) -> Option<String> {
    let (had, has) = (counts_of_text(before), counts_of_text(after));
    keys(after)
        .into_iter()
        .find(|sentence| has.get(sentence) > had.get(sentence) && lesson.contains_key(sentence))
}

/// The compared sentences of each text in `value` by its path, laid out as
/// a repair confines it.
pub(crate) fn fields_of(value: &Value) -> Vec<(String, Vec<String>)> {
    fn walk(value: &Value, path: &str, out: &mut Vec<(String, Vec<String>)>) {
        let child = |name: &str| {
            if path.is_empty() {
                name.to_owned()
            } else {
                format!("{path}/{name}")
            }
        };
        match value {
            Value::String(text) => out.push((path.to_owned(), keys(text))),
            Value::Object(fields) => {
                for (name, value) in fields.iter().filter(|(name, _)| teaches(name)) {
                    walk(value, &child(name), out);
                }
            }
            Value::Array(items) => {
                for (index, value) in items.iter().enumerate() {
                    walk(value, &child(&index.to_string()), out);
                }
            }
            _ => (),
        }
    }
    let mut out = Vec::new();
    walk(value, "", &mut out);
    out
}
