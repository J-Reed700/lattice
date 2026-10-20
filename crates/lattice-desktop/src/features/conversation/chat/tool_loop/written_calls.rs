//! Tool calls a model wrote into its text instead of making them.
//!
//! A server reads a model's calls out of what it generates and returns them
//! apart from its text. A call written in a shape the server does not expect
//! comes back as text. Qwen3.6 on llama.cpp sometimes leaves out the
//! `<tool_call>` opener (llama.cpp issue #30118), and the whole call then
//! arrives as the reply's content, with no calls and a `stop` finish. Taken as
//! the answer, it ended the turn on a call nobody ran.
//!
//! On a round that may call tools, a call written in Qwen's XML form, with or
//! without its opener, or as Hermes JSON inside `<tool_call>`, is read back
//! into the call it was meant to be and run like any other. Every call has to
//! parse and name a tool the round offered, and nothing but calls may follow
//! the first one, so an answer that only mentions a call stays an answer.

use super::final_round::leaked_tool_call_start;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::ports::ToolDefinition;
use serde_json::Value;
use tracing::warn;
use uuid::Uuid;

const CALL_OPEN: &str = "<tool_call>";
const CALL_CLOSE: &str = "</tool_call>";
const FUNCTION_OPEN: &str = "<function=";
const FUNCTION_CLOSE: &str = "</function>";
const PARAMETER_OPEN: &str = "<parameter=";
const PARAMETER_CLOSE: &str = "</parameter>";

/// What a round does with a call the model writes into its text.
#[derive(Debug, Clone, Copy)]
pub(super) enum WrittenCalls<'t> {
    /// The round offers no tools, so its text is shown as written.
    Shown,
    /// The last round may not call: a written call is cut from the answer and
    /// never run.
    Cut,
    /// A round that may call these tools: a written call is kept out of the
    /// bubble and run, or shown after all when it does not read back as calls.
    Run(&'t [ToolDefinition]),
}

/// Calls read back out of a reply's text.
#[derive(Debug)]
pub(super) struct Recovered {
    /// What the reply said before its first call.
    pub(super) text: String,
    /// One `CompletionInput::ToolCall` per call, each with an id of its own.
    pub(super) calls: Vec<CompletionInput>,
}

/// A call as written: the tool it names and its arguments.
struct WrittenCall {
    name: String,
    arguments: Arguments,
}

enum Arguments {
    /// Hermes JSON carries its arguments already typed.
    Json(Value),
    /// The XML form writes every value as text, by parameter name.
    Text(Vec<(String, String)>),
}

/// Makes a reply that wrote its calls as text into one that made them, and
/// says whether it did. A reply with calls of its own is left as it is.
pub(super) fn run_written_calls(
    response: &mut CompletionResponse,
    tools: &[ToolDefinition],
) -> bool {
    if !response.tool_calls.is_empty() {
        return false;
    }
    let Some(recovered) = recover(&response.text, tools) else {
        return false;
    };
    let names = recovered
        .calls
        .iter()
        .filter_map(|call| match call {
            CompletionInput::ToolCall { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(", ");
    warn!(
        calls = recovered.calls.len(),
        tools = %names,
        finish_reason = %response.finish_reason,
        "Model wrote its tool calls as text; running them as calls"
    );
    // Replayed as what the model meant: its text, then its calls. The
    // provider's own replay holds the call as text, which the results that
    // follow could not answer.
    response.replay = (!recovered.text.is_empty())
        .then(|| CompletionInput::Message {
            role: "assistant".into(),
            content: recovered.text.clone(),
        })
        .into_iter()
        .chain(recovered.calls.iter().cloned())
        .collect();
    response.text = recovered.text;
    response.tool_calls = recovered.calls;
    true
}

/// The calls written into `text`, and what it said before them, when every
/// call parses and names one of `tools`.
pub(super) fn recover(text: &str, tools: &[ToolDefinition]) -> Option<Recovered> {
    let start = leaked_tool_call_start(text)?;
    let calls = read_calls(&text[start..])?
        .into_iter()
        .map(|call| {
            let tool = tools.iter().find(|tool| tool.name == call.name)?;
            Some(CompletionInput::ToolCall {
                id: format!("call_{}", Uuid::new_v4().simple()),
                arguments: call.arguments.typed_for(tool),
                name: call.name,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Recovered {
        text: text[..start].trim_end().to_string(),
        calls,
    })
}

/// Every call from the first marker to the end of the reply, or `None` when
/// anything there is not a call, its wrapper, or space between them.
fn read_calls(mut rest: &str) -> Option<Vec<WrittenCall>> {
    let mut calls = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return (!calls.is_empty()).then_some(calls);
        }
        if let Some(after) = rest
            .strip_prefix(CALL_OPEN)
            .or_else(|| rest.strip_prefix(CALL_CLOSE))
        {
            rest = after;
            continue;
        }
        let (call, after) = match rest.strip_prefix(FUNCTION_OPEN) {
            Some(after) => xml_call(after)?,
            None => json_call(rest)?,
        };
        calls.push(call);
        rest = after;
    }
}

/// One call in Qwen's XML form, read from just after `<function=`, and the
/// text after its `</function>`.
fn xml_call(text: &str) -> Option<(WrittenCall, &str)> {
    let (name, mut body) = text.split_once('>')?;
    let mut parameters = Vec::new();
    loop {
        body = body.trim_start();
        if let Some(rest) = body.strip_prefix(FUNCTION_CLOSE) {
            let call = WrittenCall {
                name: name.trim().to_string(),
                arguments: Arguments::Text(parameters),
            };
            return Some((call, rest));
        }
        let (key, rest) = body.strip_prefix(PARAMETER_OPEN)?.split_once('>')?;
        let (value, rest) = rest.split_once(PARAMETER_CLOSE)?;
        // A value that runs into another parameter, or past its call, lost
        // its closing tag.
        if value.contains(PARAMETER_OPEN) || value.contains(FUNCTION_CLOSE) {
            return None;
        }
        parameters.push((
            key.trim().to_string(),
            without_framing_newlines(value).to_string(),
        ));
        body = rest;
    }
}

/// One Hermes call, `{"name": …, "arguments": {…}}`, and the text after it.
fn json_call(text: &str) -> Option<(WrittenCall, &str)> {
    let mut values = serde_json::Deserializer::from_str(text).into_iter::<Value>();
    let value = values.next()?.ok()?;
    let rest = &text[values.byte_offset()..];
    let object = value.as_object()?;
    let name = object.get("name")?.as_str()?.trim().to_string();
    let arguments = match object
        .get("arguments")
        .or_else(|| object.get("parameters"))?
    {
        // Some models write the arguments as a string of JSON, the way
        // OpenAI's API carries them.
        Value::String(encoded) => serde_json::from_str(encoded).ok()?,
        arguments => arguments.clone(),
    };
    arguments.is_object().then_some((
        WrittenCall {
            name,
            arguments: Arguments::Json(arguments),
        },
        rest,
    ))
}

/// A value with the newline that opens it and the one that closes it taken
/// off: the XML form lays each value out on lines of its own.
fn without_framing_newlines(value: &str) -> &str {
    let value = value.strip_prefix('\n').unwrap_or(value);
    value.strip_suffix('\n').unwrap_or(value)
}

impl Arguments {
    /// The arguments as the tool's schema types them. As llama.cpp's own
    /// Qwen3-Coder parser does, a value written as text is read as JSON when
    /// its parameter is typed as anything but a string, and stays text when
    /// it is not JSON.
    fn typed_for(self, tool: &ToolDefinition) -> Value {
        match self {
            Self::Json(arguments) => arguments,
            Self::Text(parameters) => Value::Object(
                parameters
                    .into_iter()
                    .map(|(key, value)| {
                        let typed = if takes_text(&tool.parameters, &key) {
                            Value::String(value)
                        } else {
                            serde_json::from_str(&value).unwrap_or(Value::String(value))
                        };
                        (key, typed)
                    })
                    .collect(),
            ),
        }
    }
}

/// Whether the schema types parameter `key` as a string, or leaves it untyped.
fn takes_text(schema: &Value, key: &str) -> bool {
    match schema
        .get("properties")
        .and_then(|properties| properties.get(key))
        .and_then(|property| property.get("type"))
    {
        Some(Value::String(kind)) => kind == "string",
        Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind == "string"),
        _ => true,
    }
}

#[cfg(test)]
mod tests;
