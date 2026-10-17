//! The model-facing side of the tool vocabulary: the registry's tools as the
//! model sees them, the OpenAI `tools` shape, and the marker for a call the
//! model wrote but that cannot be run.

use super::ToolDefinition;
use crate::application::contracts::tools;
use serde_json::{json, Value};

impl From<tools::ToolDefinition> for ToolDefinition {
    fn from(tool: tools::ToolDefinition) -> Self {
        Self {
            name: tool.name,
            description: tool.description,
            parameters: tool.input_schema,
        }
    }
}

/// Tool definitions in the OpenAI `tools` shape llama-server renders through
/// the model's chat template.
pub fn tool_specs(tools: &[ToolDefinition]) -> Value {
    json!(tools
        .iter()
        .map(|t| json!({"type":"function",
            "function":{"name":t.name,"description":t.description,"parameters":t.parameters}}))
        .collect::<Vec<_>>())
}

/// The arguments of a tool call the model wrote but that cannot be run.
///
/// Kept as a JSON object so the replayed history stays parseable, carrying the
/// model's raw text so the error it is shown quotes what it actually wrote.
pub fn invalid_tool_arguments(raw: &str, problem: &str) -> Value {
    json!({"invalid": true, "raw": raw, "error": problem})
}

/// Why a call cannot be run, when its arguments are the marker
/// [`invalid_tool_arguments`] wrote.
pub fn invalid_tool_call_problem(arguments: &Value) -> Option<String> {
    let object = arguments.as_object()?;
    if object.len() != 3 || object.get("invalid") != Some(&Value::Bool(true)) {
        return None;
    }
    let raw = object.get("raw")?.as_str()?;
    let problem = object.get("error")?.as_str()?;
    Some(format!(
        "Tool call not run: {problem}. You wrote: {}. Call the tool again with a complete JSON object of arguments, or answer from what you have.",
        crate::shared::text::safe_truncate(raw, 400)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_tool() -> tools::ToolDefinition {
        tools::ToolDefinition {
            name: "semantic_search".into(),
            description: "Search documents".into(),
            input_schema: json!({"type": "object", "properties": {"query": {"type": "string"}}}),
        }
    }

    #[test]
    fn the_two_tool_definitions_keep_their_own_json_shapes() {
        let registry = registry_tool();
        assert_eq!(
            serde_json::to_value(&registry).unwrap(),
            json!({
                "name": "semantic_search",
                "description": "Search documents",
                "input_schema": {"type": "object", "properties": {"query": {"type": "string"}}}
            }),
            "the frontend lists the registry's tools with `input_schema`"
        );
        let model: ToolDefinition = registry.into();
        assert_eq!(
            serde_json::to_value(&model).unwrap(),
            json!({
                "name": "semantic_search",
                "description": "Search documents",
                "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}
            }),
            "research jobs persist the model's tools with `parameters`"
        );
    }

    #[test]
    fn tool_specs_wrap_each_tool_as_an_openai_function() {
        let specs = tool_specs(&[registry_tool().into()]);
        assert_eq!(
            specs,
            json!([{
                "type": "function",
                "function": {
                    "name": "semantic_search",
                    "description": "Search documents",
                    "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}
                }
            }])
        );
    }

    #[test]
    fn only_the_exact_marker_reads_as_an_invalid_call() {
        let marker = invalid_tool_arguments("{\"query\": ", "unterminated JSON");
        let problem = invalid_tool_call_problem(&marker).unwrap();
        assert!(problem.contains("unterminated JSON"));
        assert!(problem.contains("{\"query\": "));
        assert!(invalid_tool_call_problem(&json!({"invalid": true, "query": "x"})).is_none());
        assert!(invalid_tool_call_problem(&json!({"q": "x"})).is_none());
    }
}
