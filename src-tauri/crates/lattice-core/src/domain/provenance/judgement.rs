//! What a cached judgement is keyed by and what a call cost (§12, §16).

use serde::{Deserialize, Serialize};

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
