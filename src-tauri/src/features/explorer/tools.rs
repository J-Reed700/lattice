//! The model's read-only view of an Explorer folder.
//!
//! Four tools, offered only on turns of a conversation bound to a folder and
//! only to a model that can call tools. They are not in the process-wide
//! function registry: the folder is the conversation's, not a global setting,
//! so the scope is handed in by the turn and never read from the arguments.
//!
//! Results are plain text, cut to the size the tool loop allows this call, and
//! each one says how to get the rest when it is cut.

use super::fs::{self, SearchRequest};
use super::index::tool::{self as folder_index_tool, TOOL_SEARCH_FOLDER};
use super::scope::Scope;
use crate::application::ports::ToolDefinition;
use crate::features::function_calling::domain::{FunctionCall, FunctionResult};
use crate::shared::{AppError, Result};
use serde_json::Value;

pub const TOOL_LIST_DIRECTORY: &str = "list_directory";
pub const TOOL_READ_FILE: &str = "read_file";
pub const TOOL_SEARCH_FILES: &str = "search_files";
pub const TOOL_FIND_FILES: &str = "find_files";

/// Lines one `read_file` call returns at most.
pub const READ_WINDOW_LINES: u32 = 400;
const SEARCH_RESULTS: usize = 50;
const FIND_RESULTS: usize = 100;
/// A minified bundle can put a whole file on one line; past this a line is
/// clipped so one line cannot spend the whole allowance.
const MAX_LINE_CHARS: usize = 500;
/// Room kept for the header and the "how to read on" footer.
const FRAMING_CHARS: usize = 300;

pub fn is_explorer_tool(name: &str) -> bool {
    matches!(
        name,
        TOOL_LIST_DIRECTORY
            | TOOL_READ_FILE
            | TOOL_SEARCH_FILES
            | TOOL_FIND_FILES
            | TOOL_SEARCH_FOLDER
    )
}

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: TOOL_LIST_DIRECTORY.to_string(),
            description: "List one level of the folder the user has open beside this chat: \
                 directories first (ending in /), then files with their sizes. Paths are \
                 relative to the folder root. Entries ignored by .gitignore are left out."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory to list, relative to the folder root. Omit for the root."
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: TOOL_READ_FILE.to_string(),
            description: format!(
                "Read a text file from the user's folder, with line numbers. Returns at most \
                 {READ_WINDOW_LINES} lines per call and says how to read the next window. Use \
                 these line numbers when you point at lines in your answer."
            ),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to the folder root, e.g. src/main.rs"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "First line to read (1-based). Defaults to 1.",
                        "minimum": 1
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Last line to read, inclusive.",
                        "minimum": 1
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: TOOL_SEARCH_FILES.to_string(),
            description: format!(
                "Search the text of the files in the user's folder. Returns up to \
                 {SEARCH_RESULTS} matching lines as path:line with a preview. A lowercase query \
                 ignores case. Files ignored by .gitignore and binary files are not searched."
            ),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Text to find, e.g. an identifier or a phrase"
                    },
                    "regex": {
                        "type": "boolean",
                        "description": "Treat the query as a regular expression",
                        "default": false
                    },
                    "path": {
                        "type": "string",
                        "description": "Only search inside this file or directory, relative to the root"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: TOOL_FIND_FILES.to_string(),
            description: format!(
                "Find files in the user's folder by name or path: a substring such as \
                 \"router\", or a glob such as \"*.rs\" or \"src/**/test_*.py\". Returns up to \
                 {FIND_RESULTS} paths relative to the folder root."
            ),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Substring or glob to match against file names and paths"
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
        },
    ]
}

/// Adds the folder tools to a turn's tool list, once each; `search_folder`
/// only when the folder's index has something in it.
pub fn add_to_turn(tools: &mut Vec<ToolDefinition>, folder_index: bool) {
    let index_tool = folder_index.then(folder_index_tool::definition);
    for definition in definitions().into_iter().chain(index_tool) {
        if !tools.iter().any(|tool| tool.name == definition.name) {
            tools.push(definition);
        }
    }
}

fn arg_str<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Small local models write numbers as strings often enough to be worth
/// reading both.
fn arg_u32(arguments: &Value, key: &str) -> Option<u32> {
    match arguments.get(key)? {
        Value::Number(number) => number.as_u64().and_then(|n| u32::try_from(n).ok()),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn arg_bool(arguments: &Value, key: &str) -> bool {
    match arguments.get(key) {
        Some(Value::Bool(value)) => *value,
        Some(Value::String(text)) => text.trim().eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// A model asking for "/" or "." means the folder itself. That is the one
/// absolute-looking path accepted, and only here: the scope still refuses
/// every other absolute path.
fn folder_path(arguments: &Value, key: &str) -> String {
    match arg_str(arguments, key) {
        None | Some("/") | Some(".") | Some("./") => String::new(),
        Some(path) => path.to_string(),
    }
}

/// The step label shown while the call runs and kept on the turn record.
pub fn activity_label(tool: &str, arguments: &Value) -> String {
    match tool {
        TOOL_LIST_DIRECTORY => match folder_path(arguments, "path").as_str() {
            "" => "Listed the folder".to_string(),
            path => format!("Listed {}/", path.trim_end_matches('/')),
        },
        TOOL_READ_FILE => {
            let path = arg_str(arguments, "path").unwrap_or("a file");
            let start = arg_u32(arguments, "start_line").unwrap_or(1).max(1);
            let last = start + READ_WINDOW_LINES - 1;
            let end = arg_u32(arguments, "end_line").map_or(last, |end| end.clamp(start, last));
            format!("Read {path} {start}–{end}")
        }
        TOOL_SEARCH_FILES => {
            let query = arg_str(arguments, "query").unwrap_or_default();
            match arg_str(arguments, "path") {
                Some(path) => format!("Searched for \"{query}\" in {path}"),
                None => format!("Searched for \"{query}\""),
            }
        }
        TOOL_FIND_FILES => format!(
            "Looked for files matching \"{}\"",
            arg_str(arguments, "pattern").unwrap_or_default()
        ),
        TOOL_SEARCH_FOLDER => folder_index_tool::activity_label(arguments),
        other => format!("Running {other}"),
    }
}

/// Runs one folder tool call. A refusal or a missing file is a failed result
/// the model reads and can act on, not an error that ends the turn.
pub async fn execute(
    scope: &Scope,
    call: FunctionCall,
    max_chars: usize,
) -> Result<FunctionResult> {
    let scope = scope.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        run(
            &scope,
            &call.name,
            &call.arguments,
            max_chars.max(FRAMING_CHARS * 2),
        )
    })
    .await;
    Ok(match outcome {
        Ok(Ok(text)) => FunctionResult::success(Value::String(text)),
        Ok(Err(error)) => FunctionResult::error("explorer_error", describe(&error)),
        Err(error) => {
            FunctionResult::error("explorer_error", format!("The folder tool failed: {error}"))
        }
    })
}

pub(super) fn describe(error: &AppError) -> String {
    match error {
        AppError::InvalidInput(message)
        | AppError::NotFound(message)
        | AppError::PermissionDenied(message) => message.clone(),
        AppError::FileRead { path, reason } => format!("Could not read {path}: {reason}"),
        other => other.to_string(),
    }
}

/// The text one call returns. Public for tests and for callers that already
/// run on a blocking thread.
pub fn run(scope: &Scope, tool: &str, arguments: &Value, max_chars: usize) -> Result<String> {
    match tool {
        TOOL_LIST_DIRECTORY => list_directory(scope, &folder_path(arguments, "path"), max_chars),
        TOOL_READ_FILE => {
            let path = arg_str(arguments, "path")
                .ok_or_else(|| AppError::InvalidInput("read_file needs a path.".to_string()))?;
            read_window(
                scope,
                path,
                arg_u32(arguments, "start_line"),
                arg_u32(arguments, "end_line"),
                max_chars,
            )
        }
        TOOL_SEARCH_FILES => {
            let query = arg_str(arguments, "query")
                .ok_or_else(|| AppError::InvalidInput("search_files needs a query.".to_string()))?;
            let path = folder_path(arguments, "path");
            search_files(scope, query, arg_bool(arguments, "regex"), &path, max_chars)
        }
        TOOL_FIND_FILES => {
            let pattern = arg_str(arguments, "pattern")
                .ok_or_else(|| AppError::InvalidInput("find_files needs a pattern.".to_string()))?;
            find_files(scope, pattern, max_chars)
        }
        other => Err(AppError::InvalidInput(format!(
            "Unknown folder tool: {other}"
        ))),
    }
}

/// Appends lines until the next would pass `budget`, and says how many did
/// not fit.
fn push_lines(out: &mut String, lines: impl Iterator<Item = String>, total: usize, budget: usize) {
    let mut shown = 0usize;
    for line in lines {
        if shown > 0 && out.len() + line.len() + 1 > budget {
            break;
        }
        out.push_str(&line);
        out.push('\n');
        shown += 1;
    }
    if shown < total {
        out.push_str(&format!("[{} more not shown.]\n", total - shown));
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS.get(unit).copied().unwrap_or("GB"))
    }
}

pub(super) fn list_directory(scope: &Scope, path: &str, max_chars: usize) -> Result<String> {
    let listing = fs::list_dir(scope, path)?;
    let ignored = listing.entries.iter().filter(|entry| entry.ignored).count();
    let visible: Vec<String> = listing
        .entries
        .iter()
        .filter(|entry| !entry.ignored)
        .map(|entry| match entry.size {
            None => format!("{}/", entry.path),
            Some(size) => format!("{}  ({})", entry.path, human_size(size)),
        })
        .collect();
    let place = if listing.path.is_empty() {
        format!("The folder root ({})", scope.name())
    } else {
        format!("{}/", listing.path)
    };
    let mut out = format!("{place}: {} entries\n", visible.len());
    let total = visible.len();
    push_lines(
        &mut out,
        visible.into_iter(),
        total,
        max_chars.saturating_sub(FRAMING_CHARS),
    );
    if ignored > 0 {
        out.push_str(&format!(
            "[{ignored} entries ignored by .gitignore left out.]\n"
        ));
    }
    Ok(out)
}

fn clip_line(line: &str) -> String {
    if line.chars().count() <= MAX_LINE_CHARS {
        return line.to_string();
    }
    let kept: String = line.chars().take(MAX_LINE_CHARS).collect();
    format!("{kept} … [line clipped]")
}

/// One numbered line in the shape `  12│ text`, the shape the prompt also
/// uses, so a model reads line numbers the same way wherever they come from.
pub(super) fn number_line(number: u32, text: &str, width: usize) -> String {
    format!("{number:>width$}│ {}", clip_line(text))
}

pub(super) fn number_width(last_line: u32) -> usize {
    last_line.to_string().len().max(4)
}

pub(super) fn read_window(
    scope: &Scope,
    path: &str,
    start_line: Option<u32>,
    end_line: Option<u32>,
    max_chars: usize,
) -> Result<String> {
    let file = fs::read_file(scope, path)?;
    if file.binary {
        return Err(AppError::InvalidInput(format!(
            "{} is a binary file ({}); it has no text to read.",
            file.path,
            human_size(file.size_bytes)
        )));
    }
    if file.too_large {
        return Err(AppError::InvalidInput(format!(
            "{} is {}, too large to open here. Use search_files with path \"{}\" to find the lines you need.",
            file.path,
            human_size(file.size_bytes),
            file.path
        )));
    }
    let text = file.text.unwrap_or_default();
    let total = file.line_count;
    if total == 0 {
        return Ok(format!("{} is empty.", file.path));
    }
    let start = start_line.unwrap_or(1).max(1);
    if start > total {
        return Err(AppError::InvalidInput(format!(
            "{} has {total} lines; start_line {start} is past the end.",
            file.path
        )));
    }
    let window_end = start + READ_WINDOW_LINES - 1;
    let end = end_line.unwrap_or(window_end).min(window_end).min(total);
    if end < start {
        return Err(AppError::InvalidInput(format!(
            "end_line {end} comes before start_line {start}."
        )));
    }

    let budget = max_chars.saturating_sub(FRAMING_CHARS);
    let width = number_width(end);
    let mut body = String::new();
    let mut last = start - 1;
    let numbered = text
        .lines()
        .enumerate()
        .skip(start as usize - 1)
        .take((end - start + 1) as usize)
        .map(|(index, line)| number_line(index as u32 + 1, line, width));
    for line in numbered {
        if last >= start && body.len() + line.len() + 1 > budget {
            break;
        }
        body.push_str(&line);
        body.push('\n');
        last += 1;
    }

    let mut out = format!("{} · lines {start}–{last} of {total}\n{body}", file.path);
    if last < total {
        out.push_str(&format!(
            "[Lines {}–{total} not shown. To read on, call read_file with path \"{}\" and start_line {}.]",
            last + 1,
            file.path,
            last + 1
        ));
    } else {
        out.push_str("[End of file.]");
    }
    Ok(out)
}

fn search_files(
    scope: &Scope,
    query: &str,
    regex: bool,
    path: &str,
    max_chars: usize,
) -> Result<String> {
    let result = fs::search(
        scope,
        &SearchRequest {
            query,
            regex,
            path_prefix: (!path.is_empty()).then_some(path),
            max_results: SEARCH_RESULTS,
        },
    )?;
    let place = if path.is_empty() {
        String::new()
    } else {
        format!(" in {path}")
    };
    if result.matches.is_empty() {
        let mut out = format!(
            "No matches for \"{query}\"{place} ({} files searched; files ignored by .gitignore and binary files are skipped).",
            result.files_scanned
        );
        if result.truncated {
            out.push_str(
                " The search stopped at a limit before covering every file; narrow it with path.",
            );
        }
        return Ok(out);
    }
    let count = result.matches.len();
    let mut out = format!(
        "{count} match{} for \"{query}\"{place}:\n",
        if count == 1 { "" } else { "es" }
    );
    let lines = result
        .matches
        .iter()
        .map(|found| format!("{}:{}: {}", found.path, found.line, found.preview));
    push_lines(
        &mut out,
        lines,
        count,
        max_chars.saturating_sub(FRAMING_CHARS),
    );
    if result.truncated {
        out.push_str("[The search stopped at a limit, so there may be more. Narrow it with path or a more specific query.]\n");
    }
    Ok(out)
}

fn find_files(scope: &Scope, pattern: &str, max_chars: usize) -> Result<String> {
    let found = fs::find_files(scope, pattern, FIND_RESULTS)?;
    if found.paths.is_empty() {
        return Ok(format!(
            "No files match \"{pattern}\" (files ignored by .gitignore are skipped)."
        ));
    }
    let count = found.paths.len();
    let mut out = format!(
        "{count} file{} matching \"{pattern}\":\n",
        if count == 1 { "" } else { "s" }
    );
    push_lines(
        &mut out,
        found.paths.into_iter(),
        count,
        max_chars.saturating_sub(FRAMING_CHARS),
    );
    if found.truncated {
        out.push_str("[Stopped at a limit, so there may be more. Use a more specific pattern.]\n");
    }
    Ok(out)
}
