//! Explorer tool vocabulary: the folder tools' names, the step label each call
//! shows, and the readings of their arguments that the labels and the tools
//! share.
//!
//! The tools themselves run in the explorer feature; the chat names and labels
//! them from free functions with no folder in scope, so the names live here.

use serde_json::Value;

pub const TOOL_LIST_DIRECTORY: &str = "list_directory";
pub const TOOL_READ_FILE: &str = "read_file";
pub const TOOL_SEARCH_FILES: &str = "search_files";
pub const TOOL_FIND_FILES: &str = "find_files";
pub const TOOL_SEARCH_FOLDER: &str = "search_folder";

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

pub fn arg_str<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// `arg_str` that also reads "/", "." and "./" as absent: how `search_folder`
/// reads its arguments.
pub fn index_arg_str<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arg_str(arguments, key).filter(|value| !matches!(*value, "/" | "." | "./"))
}

/// Small local models write numbers as strings often enough to be worth
/// reading both.
pub fn arg_u32(arguments: &Value, key: &str) -> Option<u32> {
    match arguments.get(key)? {
        Value::Number(number) => number.as_u64().and_then(|n| u32::try_from(n).ok()),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// A model asking for "/" or "." means the folder itself. That is the one
/// absolute-looking path accepted, and only here: the scope still refuses
/// every other absolute path.
pub fn folder_path(arguments: &Value, key: &str) -> String {
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
            match arg_u32(arguments, "end_line") {
                Some(end) => format!("Read {path} {start}–{}", end.max(start)),
                None => format!("Read {path} from line {start}"),
            }
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
        TOOL_SEARCH_FOLDER => search_folder_label(arguments),
        other => format!("Running {other}"),
    }
}

/// The step label: `Searched the folder for "…"`.
fn search_folder_label(arguments: &Value) -> String {
    let query = index_arg_str(arguments, "query").unwrap_or_default();
    match index_arg_str(arguments, "path") {
        Some(path) => format!("Searched the folder for \"{query}\" in {path}"),
        None => format!("Searched the folder for \"{query}\""),
    }
}
