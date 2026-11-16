//! Every answer schema of the layer's samples, through every backend's schema path, enforced as written.
//!
//! Anthropic rewrites a schema, so it must return the schema unchanged and say it compiles. OpenAI
//! (`openai.rs`, strict) and Google (`google.rs`, strict) send `schema.as_json()` as it is, so a
//! schema is accepted when it is in the strict subset: closed objects, every property required,
//! no keyword outside the dialect. llama.cpp (`llama_cpp/request.rs`) and Ollama
//! (`ollama_client.rs`, the `format` field) also send it as it is and build a grammar from it, so a
//! schema is accepted when no repetition bound exceeds what that conversion takes.

use lattice_core::application::ports::llm_port::{
    largest_repetition_bound, GRAMMAR_REPETITION_LIMIT,
};
use lattice_core::application::services::model_contract::test_support::samples;
use serde_json::Value;

use super::cloud::protocol::anthropic::schema::{
    anthropic_schema, anthropic_schema_compiles, shared,
};

/// Keywords the dialect never writes and a strict subset does not take.
const OUTSIDE_THE_DIALECT: [&str; 7] = [
    "$ref",
    "oneOf",
    "allOf",
    "not",
    "patternProperties",
    "minimum",
    "maximum",
];

/// What the strict subset of OpenAI and Google requires, walked over the whole schema.
fn strict_problem(schema: &Value, at: &str) -> Option<String> {
    match schema {
        Value::Object(map) => {
            if let Some(keyword) = OUTSIDE_THE_DIALECT
                .iter()
                .find(|key| map.contains_key(**key))
            {
                return Some(format!("{at}: `{keyword}` is outside the dialect"));
            }
            if map.get("type").is_some_and(Value::is_array) {
                return Some(format!(
                    "{at}: a type that is a list (nullable) is outside the dialect"
                ));
            }
            if let Some(Value::Object(properties)) = map.get("properties") {
                if map.get("additionalProperties") != Some(&Value::Bool(false)) {
                    return Some(format!("{at}: an object that is not closed"));
                }
                let required: Vec<&str> = map
                    .get("required")
                    .and_then(Value::as_array)
                    .map(|names| names.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                if let Some(name) = properties
                    .keys()
                    .find(|name| !required.contains(&name.as_str()))
                {
                    return Some(format!("{at}: property `{name}` is not required"));
                }
            }
            map.iter()
                .find_map(|(key, value)| strict_problem(value, &format!("{at}/{key}")))
        }
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(index, value)| strict_problem(value, &format!("{at}/{index}"))),
        _ => None,
    }
}

#[test]
fn the_samples_cover_every_structured_shape_both_ways() {
    let names: Vec<&str> = samples::schemas().iter().map(|(name, _)| *name).collect();
    assert!(names.len() >= 16, "{names:?}");
    assert!(names.contains(&"sample-each/enum") && names.contains(&"sample-each/plain"));
}

#[test]
fn anthropic_enforces_every_schema_as_written() {
    for (name, schema) in samples::schemas() {
        let written = schema.as_json();
        assert_eq!(
            &anthropic_schema(written),
            written,
            "{name} is changed for Anthropic"
        );
        assert!(
            anthropic_schema_compiles(written),
            "{name} does not compile for Anthropic"
        );
    }
}

/// `schema` with each reference replaced by the part it names.
fn written_out(schema: &Value) -> Value {
    fn resolved(part: &Value, defined: &Value) -> Value {
        match part {
            Value::Object(fields) => match fields.get("$ref").and_then(Value::as_str) {
                Some(name) => resolved(&defined[name.trim_start_matches("#/$defs/")], defined),
                None => Value::Object(
                    fields
                        .iter()
                        .filter(|(key, _)| key.as_str() != "$defs")
                        .map(|(key, value)| (key.clone(), resolved(value, defined)))
                        .collect(),
                ),
            },
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| resolved(item, defined)).collect())
            }
            other => other.clone(),
        }
    }
    resolved(schema, &schema["$defs"])
}

#[test]
fn anthropic_sends_every_schema_with_the_meaning_it_was_written_with() {
    let mut defining = 0;
    for (name, schema) in samples::schemas() {
        let written = schema.as_json();
        let sent = shared(written);
        assert_eq!(
            written_out(&sent).to_string(),
            written.to_string(),
            "{name}"
        );
        if sent.get("$defs").is_some() {
            defining += 1;
        }
    }
    assert!(defining > 0, "no sample repeats a part");
}

#[test]
fn parts_whose_fields_are_in_another_order_stay_apart() {
    let reason_first = serde_json::json!({"type":"object","properties":{
        "reason":{"type":"string"},"verdict":{"type":"string"}}});
    let verdict_first = serde_json::json!({"type":"object","properties":{
        "verdict":{"type":"string"},"reason":{"type":"string"}}});
    assert_eq!(reason_first, verdict_first, "equal as JSON values");
    let written = serde_json::json!({"type":"object",
        "properties":{"first":reason_first,"second":verdict_first}});
    assert_eq!(shared(&written).to_string(), written.to_string());
}

#[test]
fn a_schema_that_defines_parts_itself_is_sent_as_written() {
    let part = serde_json::json!({"type":"object","properties":{"a":{"type":"string"}}});
    let written = serde_json::json!({"type":"object","$defs":{"d1":{"type":"integer"}},
        "properties":{"first":part,"second":part,"third":{"$ref":"#/$defs/d1"}}});
    assert_eq!(shared(&written), written);
}

#[test]
fn openai_and_google_strict_modes_accept_every_schema_as_written() {
    for (name, schema) in samples::schemas() {
        if let Some(problem) = strict_problem(schema.as_json(), "") {
            panic!("{name} is not in the strict subset: {problem}");
        }
    }
}

#[test]
fn llama_cpp_and_ollama_build_a_grammar_from_every_schema_as_written() {
    for (name, schema) in samples::schemas() {
        let (bound, at) = largest_repetition_bound(schema.as_json());
        assert!(
            bound <= u64::try_from(GRAMMAR_REPETITION_LIMIT).unwrap_or(u64::MAX),
            "{name} holds a repetition bound of {bound} at {at}"
        );
    }
}
