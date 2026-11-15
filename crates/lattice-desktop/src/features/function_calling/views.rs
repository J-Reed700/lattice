//! Results of the library and attachment tools as the model reads them: what a
//! reader judges by, and a handle for each thing a later call may name. No id
//! or offset is shown.

use super::handles::CallView;
use lattice_core::application::contracts::tools::{ListAttachmentsOutput, ListDocumentsOutput};
use serde_json::{json, Value};

#[cfg(test)]
mod tests;

/// The result of `tool` as text, when this module renders that tool.
pub fn render(tool: &str, data: &Value, call: &CallView<'_>) -> Option<String> {
    match tool {
        "list_documents" => serde_json::from_value(data.clone())
            .ok()
            .map(|output| list_documents(&output, call)),
        "list_attachments" => serde_json::from_value(data.clone())
            .ok()
            .map(|output| list_attachments(&output, call)),
        _ => None,
    }
}

fn list_documents(output: &ListDocumentsOutput, call: &CallView<'_>) -> String {
    if output.documents.is_empty() {
        return "No documents match.".to_owned();
    }
    let mut lines = vec![format!(
        "{} documents match; {} shown.",
        output.total,
        output.documents.len()
    )];
    for document in &output.documents {
        let name = call
            .handles()
            .document(&document.document_id)
            .unwrap_or_default();
        let mut facts = vec![
            document.mime_type.clone(),
            format!("{} bytes", document.size_bytes),
            format!("modified {}", document.modified_at.format("%Y-%m-%d")),
        ];
        if !document.tags.is_empty() {
            facts.push(format!("tags: {}", document.tags.join(", ")));
        }
        if document.is_favorite {
            facts.push("a favorite".to_owned());
        }
        lines.push(format!(
            "{name} {} ({})",
            document.filename,
            facts.join("; ")
        ));
    }
    if output.has_more {
        let next = call.read_on(&[("offset", json!(output.offset + output.documents.len()))]);
        lines.push(format!("[More documents match. {}]", call.read_on_line(&next)));
    }
    lines.join("\n")
}

fn list_attachments(output: &ListAttachmentsOutput, call: &CallView<'_>) -> String {
    if output.attachments.is_empty() {
        return "No files are attached to this conversation.".to_owned();
    }
    let mut lines = vec![format!(
        "{} files are attached to this conversation, oldest first.",
        output.total
    )];
    for attachment in &output.attachments {
        let name = call
            .handles()
            .document(&attachment.document_id)
            .unwrap_or_default();
        lines.push(format!(
            "{name} {} ({} bytes, {} words, attached {})",
            attachment.filename,
            attachment.size_bytes,
            attachment.word_count,
            attachment.indexed_at.format("%Y-%m-%d")
        ));
    }
    lines.join("\n")
}
