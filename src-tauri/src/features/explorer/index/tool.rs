//! `search_folder`: the model's search of the open folder by meaning.
//!
//! Offered beside the other folder tools when the folder's index holds at
//! least one passage. A result is passages, each headed by its line reference
//! and shown as numbered lines, so the model can cite exactly what it read.

use super::dto::FolderIndexState;
use super::manager::group_digits;
use super::search::{FolderHit, FolderSearch};
use crate::application::ports::ToolDefinition;
use crate::features::explorer::tools::{number_line, number_width};
use crate::shared::{AppError, Result};
use serde_json::Value;

pub const TOOL_SEARCH_FOLDER: &str = "search_folder";
/// Passages one call returns at most.
pub const TOOL_HITS: usize = 8;
/// Room kept for the header and the notes after the passages.
const FRAMING_CHARS: usize = 300;
/// Room `render_passages` keeps for its own closing notes.
const NOTE_CHARS: usize = 64;

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: TOOL_SEARCH_FOLDER.to_string(),
        description: format!(
            "Find code or text in the folder by meaning. Use it for where/how questions when \
             you don't know the exact words; use search_files for exact text. Returns up to \
             {TOOL_HITS} passages, each headed by its line reference, e.g. `src/main.rs:10-24`."
        ),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What to look for, in words, e.g. \"where retries are scheduled\""
                },
                "path": {
                    "type": "string",
                    "description": "Only search inside this directory or file, relative to the root"
                }
            },
            "required": ["query"],
            "additionalProperties": false
        }),
    }
}

fn arg_str<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && !matches!(*value, "/" | "." | "./"))
}

/// The step label: `Searched the folder for "…"`.
pub fn activity_label(arguments: &Value) -> String {
    let query = arg_str(arguments, "query").unwrap_or_default();
    match arg_str(arguments, "path") {
        Some(path) => format!("Searched the folder for \"{query}\" in {path}"),
        None => format!("Searched the folder for \"{query}\""),
    }
}

/// A line saying the index is still being built, when it is.
pub fn progress_note(search: &FolderSearch) -> Option<String> {
    let status = search.status();
    matches!(
        status.state,
        FolderIndexState::Scanning | FolderIndexState::Indexing
    )
    .then(|| {
        format!(
            "[The folder index is still being built ({}% of {} passages so far), so a passage may be missing.]",
            status.percent(),
            group_digits(status.passages_total as usize)
        )
    })
}

/// Passages as `` `path:a-b` `` headings over numbered lines, cut at a line
/// to fit `budget` characters. A cut passage says so; passages that did not
/// fit at all are counted.
pub fn render_passages(hits: &[FolderHit], budget: usize) -> String {
    let budget = budget.saturating_sub(NOTE_CHARS);
    let mut out = String::new();
    let mut shown = 0usize;
    for hit in hits {
        let heading = format!("`{}:{}-{}`\n", hit.path, hit.start_line, hit.end_line);
        if out.len() + heading.len() > budget {
            break;
        }
        out.push_str(&heading);
        shown += 1;
        let width = number_width(hit.end_line);
        let mut cut = false;
        for (offset, line) in hit.text.lines().enumerate() {
            let numbered = number_line(hit.start_line + offset as u32, line, width);
            if out.len() + numbered.len() + 1 > budget {
                cut = true;
                break;
            }
            out.push_str(&numbered);
            out.push('\n');
        }
        if cut {
            out.push_str("[…passage cut for length]\n");
            break;
        }
        out.push('\n');
    }
    if shown < hits.len() {
        out.push_str(&format!(
            "[{} more passage{} not shown.]\n",
            hits.len() - shown,
            if hits.len() - shown == 1 { "" } else { "s" }
        ));
    }
    out
}

/// The text one call returns.
pub async fn run(search: &FolderSearch, arguments: &Value, max_chars: usize) -> Result<String> {
    let query = arg_str(arguments, "query")
        .ok_or_else(|| AppError::InvalidInput("search_folder needs a query.".to_string()))?;
    let path = arg_str(arguments, "path");
    let hits = search.search(query, path, TOOL_HITS).await?;
    let place = path.map(|path| format!(" in {path}")).unwrap_or_default();
    let note = progress_note(search);
    if hits.is_empty() {
        let mut out = format!(
            "No passages for \"{query}\"{place}. search_files finds exact text, find_files finds names."
        );
        if let Some(note) = note {
            out.push(' ');
            out.push_str(&note);
        }
        return Ok(out);
    }
    let mut out = format!(
        "{} passage{} for \"{query}\"{place}, best first:\n\n",
        hits.len(),
        if hits.len() == 1 { "" } else { "s" }
    );
    let budget = max_chars
        .max(FRAMING_CHARS * 2)
        .saturating_sub(FRAMING_CHARS);
    let room = budget.saturating_sub(out.len());
    out.push_str(&render_passages(&hits, room));
    if let Some(note) = note {
        out.push_str(&note);
        out.push('\n');
    }
    Ok(out)
}
