//! A teaching repair's answer: one entry for each finding sent, with its plan
//! and its own edits, read as the flat patch the edits apply from.
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// An answer read: the flat patch, each change naming the finding of its
/// entry, and the plan of each finding's entry.
pub(super) struct Answered {
    pub patch: String,
    pub plans: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    answers: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    finding: String,
    plan: String,
    #[serde(default)]
    edits: Vec<Value>,
    #[serde(default)]
    citations: Vec<Value>,
    #[serde(default)]
    uncite: Vec<Value>,
}

fn entries(raw: &str) -> Result<Vec<Entry>> {
    crate::generation::parse_json::<Answer>(raw).map(|answer| answer.answers)
}

/// Why the entries are not one with a plan for each of the `sent` findings,
/// naming each finding at fault.
fn refusal(entries: &[Entry], sent: &[String]) -> Option<String> {
    let count = |id: &str| entries.iter().filter(|entry| entry.finding == id).count();
    let ids = |faulty: &dyn Fn(&str) -> bool| {
        sent.iter()
            .map(String::as_str)
            .filter(|id| faulty(id))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let left_out = ids(&|id| count(id) == 0);
    let repeated = ids(&|id| count(id) > 1);
    let blank = ids(&|id| {
        entries
            .iter()
            .any(|entry| entry.finding == id && entry.plan.trim().is_empty())
    });
    let problems: Vec<String> = [
        (left_out, "it gives no entry for"),
        (repeated, "it gives more than one entry for"),
        (blank, "it gives no plan for"),
    ]
    .into_iter()
    .filter(|(faulty, _)| !faulty.is_empty())
    .map(|(faulty, what)| format!("{what} {faulty}."))
    .collect();
    (!problems.is_empty()).then(|| {
        format!(
            "The repair's answer must hold one entry, with its plan, for each finding sent: {}",
            problems.join(" ")
        )
    })
}

/// `change` for the finding of its entry.
fn named(change: Value, finding: &str) -> Value {
    match change {
        Value::Object(mut fields) => {
            fields.insert("finding".into(), json!(finding));
            Value::Object(fields)
        }
        Value::String(path) => json!({"finding":finding,"path":path}),
        other => other,
    }
}

/// The flat patch of `entries`, their changes in the order of their entries.
fn flatten(entries: Vec<Entry>) -> Answered {
    let (mut edits, mut citations, mut uncite) = (Vec::new(), Vec::new(), Vec::new());
    let mut plans = BTreeMap::new();
    for entry in entries {
        let finding = entry.finding.as_str();
        edits.extend(entry.edits.into_iter().map(|change| named(change, finding)));
        citations.extend(
            entry
                .citations
                .into_iter()
                .map(|change| named(change, finding)),
        );
        uncite.extend(
            entry
                .uncite
                .into_iter()
                .map(|change| named(change, finding)),
        );
        plans.insert(entry.finding, entry.plan);
    }
    Answered {
        patch: json!({"edits":edits,"citations":citations,"uncite":uncite}).to_string(),
        plans,
    }
}

/// The answer in `raw` to the `sent` findings, refused when it does not hold
/// one entry with a plan for each of them.
pub(super) fn read(raw: &str, sent: &[String]) -> Result<Answered> {
    let entries = entries(raw)?;
    match refusal(&entries, sent) {
        Some(reason) => Err(AppError::InvalidInput(reason)),
        None => Ok(flatten(entries)),
    }
}

/// A saved answer, which was read when it was given.
pub(super) fn reread(raw: &str) -> Result<Answered> {
    entries(raw).map(flatten)
}
