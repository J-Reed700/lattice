//! What a cached judgement is keyed by and what a call cost (§12, §16).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// A judgement's cache key: the policy (its prompt version), the model, and
/// the sha256 of what it judged. A model or policy change makes a new row;
/// old rows stay for audit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JudgementKey {
    pub policy: String,
    pub model: String,
    pub input_sha256: String,
}

/// A judgement's value with what the call cost. A fallback made without a
/// call costs nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Judged<T> {
    pub value: T,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// The canonical JSON of `value`: object keys sorted at every depth, no
/// whitespace. Two equal values always hash the same, whatever order their
/// fields were written in.
pub fn canonical_json(value: &Value) -> String {
    sorted(value).to_string()
}

/// Lower-case hex sha256 of `text`.
pub fn sha256_hex(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<(&String, &Value)> = object.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), sorted(value)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}
