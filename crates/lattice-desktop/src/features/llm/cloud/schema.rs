//! The JSON Schema subset Claude's structured outputs compile: what a
//! learning schema must lose or move before it reaches Anthropic, and when a
//! schema is too large to send as a format at all.

use serde_json::{json, Value};

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
