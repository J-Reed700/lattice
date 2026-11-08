//! The grammar bound llama.cpp puts on a schema.

/// The largest `maxLength` / `maxItems` a schema handed to llama.cpp may carry.
/// Its GBNF conversion throws on any repetition bound above 2000, and the
/// server then samples with no grammar at all, dropping every constraint.
pub const GRAMMAR_REPETITION_LIMIT: usize = 2000;

/// The largest `maxLength`, `maxItems`, `minLength` or `minItems` anywhere in
/// `schema`, with its JSON pointer: what a test compares with
/// [`GRAMMAR_REPETITION_LIMIT`] for every schema a model call sends.
#[cfg(any(test, feature = "test-support"))]
pub fn largest_repetition_bound(schema: &serde_json::Value) -> (u64, String) {
    fn walk(value: &serde_json::Value, path: &str, worst: &mut (u64, String)) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    let at = format!("{path}/{key}");
                    if matches!(
                        key.as_str(),
                        "maxLength" | "maxItems" | "minLength" | "minItems"
                    ) {
                        let bound = child.as_u64().unwrap_or(0);
                        if bound > worst.0 {
                            *worst = (bound, at.clone());
                        }
                    }
                    walk(child, &at, worst);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    walk(child, &format!("{path}/{index}"), worst);
                }
            }
            _ => {}
        }
    }
    let mut worst = (0, String::new());
    walk(schema, "", &mut worst);
    worst
}
