//! The JSON Schema subset Claude's structured outputs compile: what a
//! learning schema must lose or move before it reaches Anthropic, how a part
//! that repeats is written once, when a schema is too large to send as a
//! format at all, and the retry in words when Claude refuses one anyway.

use lattice_core::shared::error::AppError;
use serde_json::{json, Value};

/// The rejection that stands for Claude refusing a request's response schema.
const REFUSAL: &str = "Claude could not compile this request's response schema.";

/// Whether `error` is the rejection that stands for a refused response schema.
pub(super) fn is_refusal(error: &AppError) -> bool {
    matches!(error, AppError::ModelRejected(message) if message == REFUSAL)
}

/// Whether `body` asks Claude for a structured response format.
pub(super) fn asks_for_format(body: &Value) -> bool {
    body.pointer("/output_config/format").is_some()
}

/// The schema refusal, when that is what a 400's `reason` says.
pub(super) fn refusal(reason: &str) -> Option<AppError> {
    (reason.contains("output_config.format")
        || reason.contains("grammar")
        || reason.contains("Schemas contains")
        || reason.contains("Schema is too complex"))
    .then(|| AppError::ModelRejected(REFUSAL.into()))
}

/// Logs Claude's reason for refusing the response schema of `body`, with the
/// fields of the schema it refused.
pub(super) fn log_refusal(body: &Value, reason: &str) {
    if !asks_for_format(body) || refusal(reason).is_none() {
        return;
    }
    let fields = body
        .pointer("/output_config/format/schema/properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect::<Vec<_>>().join(","))
        .unwrap_or_default();
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default();
    tracing::warn!(reason, model, schema_fields = %fields, "Claude refused the response schema");
}

/// `body` with its response schema asked for in the system prompt instead of
/// as a format. The caller's parser checks the reply as it would any other.
pub(in crate::features::llm::cloud) fn in_words(mut body: Value) -> Value {
    let Some(format) = body
        .get_mut("output_config")
        .and_then(Value::as_object_mut)
        .and_then(|config| config.remove("format"))
    else {
        return body;
    };
    if body
        .get("output_config")
        .and_then(Value::as_object)
        .is_some_and(serde_json::Map::is_empty)
    {
        if let Some(fields) = body.as_object_mut() {
            fields.remove("output_config");
        }
    }
    let ask = format!(
        "Reply with only a JSON value that matches this JSON Schema, and no other text:\n{}",
        format.get("schema").unwrap_or(&Value::Null)
    );
    // A cached system prompt is a block, and keeps its breakpoint.
    let block = body
        .get_mut("system")
        .and_then(Value::as_array_mut)
        .and_then(|blocks| blocks.last_mut())
        .and_then(|block| block.get_mut("text"));
    if let Some(Value::String(text)) = block {
        *text = format!("{text}\n\n{ask}");
        return body;
    }
    let system = match body.get("system").and_then(Value::as_str) {
        Some(system) if !system.is_empty() => format!("{system}\n\n{ask}"),
        _ => ask,
    };
    if let Some(fields) = body.as_object_mut() {
        fields.insert("system".into(), json!(system));
    }
    body
}

/// `schema` as Claude's structured outputs accept it. The API answers
/// `maxItems`, `uniqueItems`, numeric bounds and a `minItems` above 1 with a
/// 400, so those move into the description, where the model still reads them;
/// callers check what comes back either way. A keyword is only ever a number or
/// a boolean, so a property that happens to be named `maximum` is kept. Every
/// object must also say `additionalProperties: false` outright.
pub(in crate::features::llm) fn anthropic_schema(schema: &Value) -> Value {
    const MOVED: [&str; 7] = [
        "maxItems",
        "uniqueItems",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
    ];
    match schema {
        Value::Object(map) => {
            let mut kept = serde_json::Map::new();
            let mut moved = Vec::new();
            for (key, value) in map {
                let keyword = value.is_number() || value.is_boolean();
                if keyword && MOVED.contains(&key.as_str()) {
                    moved.push(format!("{key}: {value}"));
                } else if keyword && key == "minItems" && value.as_u64().is_some_and(|n| n > 1) {
                    moved.push(format!("{key}: {value}"));
                    kept.insert(key.clone(), json!(1));
                } else {
                    kept.insert(key.clone(), anthropic_schema(value));
                }
            }
            if !moved.is_empty() {
                let moved = moved.join(", ");
                let description = match kept.get("description").and_then(Value::as_str) {
                    Some(description) => format!("{description} ({moved})"),
                    None => moved,
                };
                kept.insert("description".into(), json!(description));
            }
            if kept.get("type").and_then(Value::as_str) == Some("object")
                || kept.get("properties").is_some_and(Value::is_object)
            {
                kept.insert("additionalProperties".into(), json!(false));
            }
            Value::Object(kept)
        }
        Value::Array(items) => Value::Array(items.iter().map(anthropic_schema).collect()),
        other => other.clone(),
    }
}

/// `schema` with each object or `enum` that occurs more than once defined
/// once under `$defs` and referred to from its places. The answers it admits
/// are the same. Written out in every place, a schema with many places for one
/// large part is refused as too large a grammar. Two parts are one only when
/// they are written the same, the order of their fields included: a model
/// writes an object's fields in the order its schema lists them.
pub(in crate::features::llm) fn shared(schema: &Value) -> Value {
    let mut sent = schema.clone();
    if sent.get("$defs").is_some() {
        return sent;
    }
    let mut seen = Vec::new();
    count(&mut sent, &mut seen);
    let repeated: Vec<String> = seen
        .into_iter()
        .filter(|(_, times)| *times > 1)
        .map(|(part, _)| part)
        .collect();
    if repeated.is_empty() {
        return sent;
    }
    let mut defined = serde_json::Map::new();
    refer(&mut sent, &repeated, &mut defined);
    if let Value::Object(fields) = &mut sent {
        fields.insert("$defs".into(), Value::Object(defined));
    }
    sent
}

/// The schemas directly inside `schema`: its properties, its items and its alternatives.
fn parts(schema: &mut Value) -> Vec<&mut Value> {
    let Value::Object(fields) = schema else {
        return Vec::new();
    };
    let mut inside = Vec::new();
    for (key, value) in fields.iter_mut() {
        match (key.as_str(), value) {
            ("properties", Value::Object(properties)) => inside.extend(properties.values_mut()),
            ("anyOf", Value::Array(alternatives)) => inside.extend(alternatives.iter_mut()),
            ("items", items) if items.is_object() => inside.push(items),
            _ => {}
        }
    }
    inside
}

/// An object or an `enum` as it is written: what `shared` defines once when it repeats.
fn definable(part: &Value) -> Option<String> {
    (part.get("properties").is_some() || part.get("enum").is_some()).then(|| part.to_string())
}

/// How often each object and `enum` inside `schema` occurs. The inside of a
/// part met again is not counted again: it is sent where the part is defined.
fn count(schema: &mut Value, seen: &mut Vec<(String, usize)>) {
    for part in parts(schema) {
        if let Some(written) = definable(part) {
            if let Some((_, times)) = seen.iter_mut().find(|(other, _)| *other == written) {
                *times += 1;
                continue;
            }
            seen.push((written, 1));
        }
        count(part, seen);
    }
}

/// Puts a reference in each place of a `repeated` part inside `schema`, and
/// the part itself, with the same done inside it, in `defined`.
fn refer(schema: &mut Value, repeated: &[String], defined: &mut serde_json::Map<String, Value>) {
    for part in parts(schema) {
        let place =
            definable(part).and_then(|written| repeated.iter().position(|other| *other == written));
        let Some(index) = place else {
            refer(part, repeated, defined);
            continue;
        };
        let name = format!("d{}", index + 1);
        let mut written = std::mem::replace(part, json!({"$ref": format!("#/$defs/{name}")}));
        if !defined.contains_key(&name) {
            refer(&mut written, repeated, defined);
            defined.insert(name, written);
        }
    }
}

/// Whether Claude will compile `schema` into a grammar. It refuses one with
/// more than 24 optional parameters or 16 union-typed ones, counted across
/// every object in it.
pub(in crate::features::llm) fn anthropic_schema_compiles(schema: &Value) -> bool {
    fn count(schema: &Value, optional: &mut usize, unions: &mut usize) {
        match schema {
            Value::Object(map) => {
                if let Some(Value::Object(properties)) = map.get("properties") {
                    let required = map.get("required").and_then(Value::as_array);
                    for (name, property) in properties {
                        let listed = required
                            .is_some_and(|required| required.iter().any(|item| item == name));
                        if !listed {
                            *optional += 1;
                        }
                        if property.get("anyOf").is_some()
                            || property.get("type").is_some_and(Value::is_array)
                        {
                            *unions += 1;
                        }
                    }
                }
                for value in map.values() {
                    count(value, optional, unions);
                }
            }
            Value::Array(items) => {
                for item in items {
                    count(item, optional, unions);
                }
            }
            _ => {}
        }
    }
    let (mut optional, mut unions) = (0, 0);
    count(schema, &mut optional, &mut unions);
    optional <= 24 && unions <= 16
}
