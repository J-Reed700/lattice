//! Shapes what one turn left behind into the row the scorer reads.
//!
//! Field names follow `scripts/chat_rag_eval.py` where the meaning is the
//! same (`query_id`, `answer`, `ranked_ids`, `citation_map`, `latency_ms`,
//! `model`), so `review_rag_eval.py` and `compare_rag_eval.py` read a run of
//! a labelled dataset without translation. Everything the desktop path adds —
//! the evidence the model was handed, the grounding verdicts, the turn record,
//! each model call — sits beside those fields.

use crate::proxy::CallRecord;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};

pub const SCHEMA: &str = "lattice-shipped-path-answers/1";

/// Everything one turn produced.
pub struct TurnOutcome<'a> {
    pub index: usize,
    pub ask: &'a str,
    pub conversation_id: Option<&'a str>,
    pub answer: Option<&'a str>,
    pub error: Option<String>,
    /// From the request until the answer returned.
    pub answered_ms: u64,
    /// From the answer until the grounding check arrived.
    pub verification_wait_ms: Option<u64>,
    pub went_quiet: bool,
    pub sources: Value,
    pub assistant_metadata: Value,
    pub verification: Option<Value>,
    pub verified_turn_record: Option<Value>,
    pub flow_timing: Value,
    pub events: &'a [Value],
    pub calls: &'a [CallRecord],
    pub rate_limit_retries: usize,
}

/// Dataset id for a Lattice document id; web sources keep their own.
fn dataset_id(to_dataset: &HashMap<String, String>, lattice_id: &str) -> String {
    to_dataset
        .get(lattice_id)
        .cloned()
        .unwrap_or_else(|| lattice_id.to_owned())
}

/// The numbered evidence the model was told to cite, by citation number.
pub fn evidence(sources: &Value, to_dataset: &HashMap<String, String>) -> Vec<Value> {
    let mut entries: Vec<Value> = sources
        .as_array()
        .into_iter()
        .flatten()
        .map(|source| {
            let lattice_id = source
                .get("documentId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let excerpts: Vec<Value> = source
                .get("chunkExcerpts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|excerpt| {
                    json!({
                        "chunk_index": excerpt.get("chunkIndex"),
                        "section": excerpt.get("section"),
                        "score": excerpt.get("score"),
                        "text": excerpt.get("excerpt"),
                    })
                })
                .collect();
            json!({
                "citation_id": source.get("citationId"),
                "document_id": dataset_id(to_dataset, lattice_id),
                "lattice_document_id": lattice_id,
                "file_name": source.get("fileName"),
                "score": source.get("score"),
                "section": source.get("section"),
                "chunk_index": source.get("chunkIndex"),
                "content": source.get("content"),
                "excerpts": excerpts,
            })
        })
        .collect();
    entries.sort_by_key(|entry| {
        entry
            .get("citation_id")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX)
    });
    entries
}

/// `citation_map` (number → document) and `ranked_ids` (documents in
/// citation order, each once), as the answer harness names them.
pub fn citations(evidence: &[Value]) -> (BTreeMap<String, String>, Vec<String>) {
    let mut map = BTreeMap::new();
    let mut ranked: Vec<String> = Vec::new();
    for entry in evidence {
        let document = entry
            .get("document_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if let Some(number) = entry.get("citation_id").and_then(Value::as_u64) {
            map.insert(number.to_string(), document.clone());
        }
        if !ranked.contains(&document) {
            ranked.push(document);
        }
    }
    (map, ranked)
}

/// Every model name the run's replies said they were served by, sorted. One
/// name is one model measured; more than one is a run that is not one
/// model's result, and its manifest says so.
pub fn served_models(calls: &[CallRecord]) -> Vec<String> {
    let names: std::collections::BTreeSet<String> = calls
        .iter()
        .filter_map(|call| call.reply.as_ref()?.model.clone())
        .collect();
    names.into_iter().collect()
}

/// One model call, summarized; the full request lives in the calls file.
pub fn call_summary(call: &CallRecord, answer: Option<&str>) -> Value {
    let request = call.request.as_ref();
    let messages = request
        .and_then(|r| r.get("messages"))
        .and_then(Value::as_array);
    let system_head: Option<String> = messages
        .and_then(|m| m.first())
        .filter(|m| m.get("role").and_then(Value::as_str) == Some("system"))
        .and_then(|m| m.get("content").and_then(Value::as_str))
        .map(|text| text.chars().take(100).collect());
    let prompt_chars: usize = messages
        .into_iter()
        .flatten()
        .map(|m| m.to_string().chars().count())
        .sum();
    let tools: Vec<&str> = request
        .and_then(|r| r.get("tools"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| t.pointer("/function/name").and_then(Value::as_str))
        .collect();
    let structured = request.is_some_and(|r| r.get("response_format").is_some());
    let reply = call.reply.as_ref();
    let content = reply.map(|r| r.content.trim()).unwrap_or_default();
    let is_answer = answer.is_some_and(|a| !content.is_empty() && a.trim() == content);
    let purpose = if is_answer {
        "answer"
    } else if !tools.is_empty() {
        "tool_round"
    } else if structured {
        "structured"
    } else {
        "text"
    };
    let timing = |key: &str| {
        reply
            .and_then(|r| r.timings.as_ref())
            .and_then(|t| t.get(key))
            .cloned()
    };
    json!({
        "seq": call.seq,
        "purpose": purpose,
        "system_head": system_head,
        "started_ms": call.started_ms,
        "duration_ms": call.duration_ms,
        "first_byte_ms": call.first_byte_ms,
        "status": call.status,
        "error": call.error,
        "message_count": messages.map_or(0, Vec::len),
        "prompt_chars": prompt_chars,
        "tools_offered": tools,
        "structured": structured,
        "max_tokens": request.and_then(|r| r.get("max_tokens")),
        "temperature": request.and_then(|r| r.get("temperature")),
        "chat_template_kwargs": request.and_then(|r| r.get("chat_template_kwargs")),
        "thinking_switch_added": call.thinking_switch_added,
        "finish_reason": reply.and_then(|r| r.finish_reason.clone()),
        "content_chars": content.chars().count(),
        "reasoning_chars": reply.map_or(0, |r| r.reasoning_content.chars().count()),
        "tool_calls": reply.map(|r| r.tool_calls.clone()).unwrap_or_default(),
        // llama-server's split of the prompt: reused from the slot's cache
        // versus evaluated afresh. The answer to "did the prefix survive".
        "cached_prompt_tokens": timing("cache_n"),
        "evaluated_prompt_tokens": timing("prompt_n"),
        "prompt_ms": timing("prompt_ms"),
        "completion_tokens": timing("predicted_n"),
        "completion_ms": timing("predicted_ms"),
        "usage": reply.and_then(|r| r.usage.clone()),
    })
}

/// The turn as one JSON object.
pub fn turn(outcome: &TurnOutcome<'_>, to_dataset: &HashMap<String, String>) -> Value {
    let evidence = evidence(&outcome.sources, to_dataset);
    let (citation_map, ranked_ids) = citations(&evidence);
    let chat_calls: Vec<Value> = outcome
        .calls
        .iter()
        .filter(|call| call.path.ends_with("/chat/completions"))
        .map(|call| call_summary(call, outcome.answer))
        .collect();
    let mut other_calls: BTreeMap<String, usize> = BTreeMap::new();
    for call in outcome.calls {
        if !call.path.ends_with("/chat/completions") {
            *other_calls.entry(call.path.clone()).or_default() += 1;
        }
    }
    let total = |key: &str| -> u64 {
        chat_calls
            .iter()
            .filter_map(|c| c.get(key).and_then(Value::as_u64))
            .sum()
    };
    let statuses: Vec<String> = outcome
        .events
        .iter()
        .filter_map(|e| e.get("status").and_then(Value::as_str))
        .fold(Vec::new(), |mut seen, status| {
            if !seen.iter().any(|s: &String| s == status) {
                seen.push(status.to_owned());
            }
            seen
        });
    let metadata = &outcome.assistant_metadata;
    json!({
        "index": outcome.index,
        "question": outcome.ask,
        "conversation_id": outcome.conversation_id,
        "answer": outcome.answer,
        "error": outcome.error,
        "answered_ms": outcome.answered_ms,
        "verification_wait_ms": outcome.verification_wait_ms,
        "background_settled": outcome.went_quiet,
        "rate_limit_retries": outcome.rate_limit_retries,
        "evidence": evidence,
        "citation_map": citation_map,
        "ranked_ids": ranked_ids,
        "retrieval_trace": metadata.get("retrieval"),
        "memory": metadata.get("memory"),
        "turn_record": outcome.verified_turn_record.clone().or_else(|| metadata.get("turn").cloned()),
        "verification": outcome.verification,
        "flow_timing": outcome.flow_timing,
        "model_calls": chat_calls,
        "other_server_calls": other_calls,
        "prompt_tokens": {
            "cached": total("cached_prompt_tokens"),
            "evaluated": total("evaluated_prompt_tokens"),
            "completion": total("completion_tokens"),
        },
        "event_statuses": statuses,
    })
}

/// The item's row: its turns, with the last turn's answer and evidence
/// lifted to the top level under the answer harness's names.
pub fn item(
    id: &str,
    tags: &[String],
    turns: Vec<Value>,
    model: &str,
    error: Option<String>,
) -> Value {
    let last = turns.last().cloned().unwrap_or(Value::Null);
    let latency_ms: u64 = turns
        .iter()
        .filter_map(|t| t.get("answered_ms").and_then(Value::as_u64))
        .sum();
    let mut row = Map::new();
    row.insert("schema".into(), json!(SCHEMA));
    row.insert("query_id".into(), json!(id));
    row.insert("tags".into(), json!(tags));
    row.insert(
        "answer".into(),
        last.get("answer").cloned().unwrap_or(Value::Null),
    );
    row.insert(
        "ranked_ids".into(),
        last.get("ranked_ids").cloned().unwrap_or(json!([])),
    );
    row.insert(
        "citation_map".into(),
        last.get("citation_map").cloned().unwrap_or(json!({})),
    );
    row.insert("latency_ms".into(), json!(latency_ms));
    row.insert("model".into(), json!(model));
    row.insert("error".into(), json!(error));
    row.insert("turns".into(), Value::Array(turns));
    Value::Object(row)
}
