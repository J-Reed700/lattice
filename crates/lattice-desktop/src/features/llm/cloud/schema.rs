//! The JSON Schema subset Claude's structured outputs compile: what a
//! learning schema must lose or move before it reaches Anthropic, when a
//! schema is too large to send as a format at all, and the retry in words when
//! Claude refuses one anyway.

use std::time::Instant;

use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

use super::CloudLlm;

/// The rejection that stands for Claude refusing a request's response schema.
const REFUSAL: &str = "Claude could not compile this request's response schema.";

impl CloudLlm {
    /// Sends `body`, and once more with its schema asked for in words when
    /// Claude refuses to compile it. How large a grammar a schema compiles to
    /// cannot be told from its shape ("The compiled grammar is too large").
    pub(super) async fn send_with_schema(
        &self,
        body: Value,
        deadline: Option<Instant>,
        bound_body: bool,
    ) -> Result<reqwest::Response> {
        match self.send(&body, deadline, bound_body).await {
            Err(AppError::ModelRejected(message)) if message == REFUSAL => {
                tracing::info!("Claude refused the response schema; asking for it in words");
                self.send(&in_words(body), deadline, bound_body).await
            }
            sent => sent,
        }
    }
}

/// Whether `body` asks Claude for a structured response format.
pub(super) fn asks_for_format(body: &Value) -> bool {
    body.pointer("/output_config/format").is_some()
}

/// `error`, or the schema refusal when that is what the 400 says. The reason
/// is read only to tell the two apart; it is never logged or shown, since a
/// provider's error can quote what was sent.
pub(super) async fn refusal_or(response: reqwest::Response, error: AppError) -> AppError {
    let Ok(reply) = response.json::<Value>().await else {
        return error;
    };
    let reason = reply
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if reason.contains("output_config.format")
        || reason.contains("grammar")
        || reason.contains("Schemas contains")
    {
        AppError::ModelRejected(REFUSAL.into())
    } else {
        error
    }
}

/// `body` with its response schema asked for in the system prompt instead of
/// as a format. The caller's parser checks the reply as it would any other.
pub(super) fn in_words(mut body: Value) -> Value {
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
pub(super) fn anthropic_schema(schema: &Value) -> Value {
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

/// Whether Claude will compile `schema` into a grammar. It refuses one with
/// more than 24 optional parameters or 16 union-typed ones, counted across
/// every object in it.
pub(super) fn anthropic_schema_compiles(schema: &Value) -> bool {
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
