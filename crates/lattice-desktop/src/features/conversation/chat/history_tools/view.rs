//! Results of the history and saved-knowledge tools as the model reads them:
//! the text and who said it, and a handle for each message a later call may
//! read. No message id, sequence, byte offset or cursor is shown.

use serde_json::Value;

use crate::features::conversation::knowledge_dto::KnowledgeItemDto;
use crate::features::function_calling::handles::CallView;

use super::{
    TOOL_READ_CONVERSATION_HISTORY, TOOL_SEARCH_CONVERSATION_HISTORY, TOOL_SEARCH_SAVED_KNOWLEDGE,
};

#[cfg(test)]
mod tests;

/// The result of `tool` as text, when this module renders that tool.
pub fn render(tool: &str, data: &Value, call: &CallView<'_>) -> Option<String> {
    match tool {
        TOOL_SEARCH_CONVERSATION_HISTORY => Some(search(data, call)),
        TOOL_READ_CONVERSATION_HISTORY => Some(read(data, call)),
        TOOL_SEARCH_SAVED_KNOWLEDGE => Some(knowledge(data, call)),
        _ => None,
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn named(call: &CallView<'_>, message: &Value) -> String {
    call.handles()
        .message(text(message, "message_id"))
        .unwrap_or_default()
}

fn search(data: &Value, call: &CallView<'_>) -> String {
    if data.get("repeat_of_earlier_search") == Some(&Value::Bool(true)) {
        return "You already ran this exact search this turn; its excerpts are above. Read one of those messages with read_conversation_history, or search for something different.".to_owned();
    }
    let matches = data
        .get("matches")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut lines = Vec::new();
    for found in matches {
        let partial = found.get("excerpt_is_partial") == Some(&Value::Bool(true));
        lines.push(format!(
            "{} {}: {}{}",
            named(call, found),
            text(found, "role"),
            text(found, "excerpt"),
            if partial { " ..." } else { "" }
        ));
    }
    if let Some(note) = data
        .get("retrieval")
        .map(|retrieval| text(retrieval, "note"))
        .filter(|note| !note.is_empty())
    {
        lines.push(note.to_owned());
    }
    if let Some(next) = data.get("next_offset").and_then(Value::as_u64) {
        let name = call.read_on(&[("offset", Value::from(next))]);
        lines.push(format!(
            "[More matches follow. {}]",
            call.read_on_line(&name)
        ));
    }
    lines.join("\n")
}

fn read(data: &Value, call: &CallView<'_>) -> String {
    if data.get("already_delivered") == Some(&Value::Bool(true)) {
        return "You already have the full text of these messages earlier in this turn. Re-read it there, or read a different message.".to_owned();
    }
    let messages = data
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut lines = Vec::new();
    for message in messages {
        let whole = message.get("is_whole_message") == Some(&Value::Bool(true));
        let part = if whole { "" } else { " (part of the message)" };
        lines.push(format!(
            "{} {}{part}:\n{}",
            named(call, message),
            text(message, "role"),
            text(message, "text")
        ));
    }
    let unread = data
        .get("not_readable_message_ids")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if unread > 0 {
        lines.push(format!(
            "{unread} requested messages are outside this conversation or did not fit this response."
        ));
    }
    if data.get("truncated") == Some(&Value::Bool(true)) {
        let name = call.read_on(&[]);
        lines.push(format!("[Cut here. {}]", call.read_on_line(&name)));
    }
    lines.join("\n\n")
}

fn knowledge(data: &Value, call: &CallView<'_>) -> String {
    let items: Vec<KnowledgeItemDto> = data
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let mut blocks = Vec::new();
    for item in &items {
        blocks.push(item_text(item, call));
    }
    if blocks.is_empty() {
        blocks.push("No saved item matches.".to_owned());
    }
    if let Some(note) = data
        .get("note")
        .and_then(Value::as_str)
        .filter(|note| !note.is_empty())
    {
        blocks.push(note.to_owned());
    }
    blocks.join("\n\n")
}

fn item_text(item: &KnowledgeItemDto, call: &CallView<'_>) -> String {
    let mut facts = vec![
        item.kind.clone(),
        item.state.clone(),
        format!("{} scope", item.scope),
        format!("learned {}", item.learned_at),
    ];
    if let Some(from) = &item.valid_from {
        facts.push(format!("valid from {from}"));
    }
    if let Some(until) = &item.valid_until {
        facts.push(format!("valid until {until}"));
    }
    if let Some(verified) = &item.verified_at {
        facts.push(format!("verified {verified}"));
    }
    if item.forgotten {
        facts.push("the user asked to forget it".to_owned());
    }
    if item.superseded_by.is_some() {
        facts.push("replaced by a later item".to_owned());
    }
    let mut lines = vec![
        format!("{} ({})", item.label, facts.join("; ")),
        format!(
            "From the conversation \"{}\". {}",
            item.conversation_title, item.availability_reason
        ),
    ];
    for evidence in &item.evidence {
        let quoted = evidence
            .text
            .as_deref()
            .unwrap_or("(the message was edited or deleted)");
        let message = if call.handles().reads(&item.conversation_id) {
            call.handles().message(&evidence.message_id)
        } else {
            None
        };
        lines.push(match message {
            Some(name) => format!("  {name} {} ({}): {quoted}", evidence.role, evidence.purpose),
            None => format!("  {} ({}): {quoted}", evidence.role, evidence.purpose),
        });
    }
    lines.join("\n")
}
