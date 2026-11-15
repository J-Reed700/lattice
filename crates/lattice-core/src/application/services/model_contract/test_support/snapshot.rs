//! Prompt snapshots: what a request says, as a committed text file.

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::application::ports::llm_port::{CompletionInput, CompletionRequest, ContentPart};

const UPDATE_VARIABLE: &str = "LATTICE_UPDATE_SNAPSHOTS";

fn section(out: &mut String, what: &str) {
    if !out.is_empty() {
        out.push('\n');
    }
    let _ = writeln!(out, "=== {what} ===");
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// What `request` says, as text: stable, complete, and readable in a diff.
pub fn rendered(request: &CompletionRequest) -> String {
    let mut out = String::new();
    for (index, input) in request.input().iter().enumerate() {
        let number = index + 1;
        match input {
            CompletionInput::Message { role, content } => {
                section(&mut out, &format!("input {number}: message ({role})"));
                out.push_str(content);
                out.push('\n');
            }
            CompletionInput::Multimodal { role, parts } => {
                section(&mut out, &format!("input {number}: multimodal ({role})"));
                for part in parts {
                    match part {
                        ContentPart::Text { text } => {
                            out.push_str(text);
                            out.push('\n');
                        }
                        ContentPart::Image(image) => {
                            let _ = writeln!(out, "[image {}x{}]", image.width, image.height);
                        }
                    }
                }
            }
            CompletionInput::ToolCall { name, arguments, .. } => {
                section(&mut out, &format!("input {number}: tool call {name}"));
                out.push_str(&pretty(arguments));
                out.push('\n');
            }
            CompletionInput::ToolResult { output, .. } => {
                section(&mut out, &format!("input {number}: tool result"));
                out.push_str(output);
                out.push('\n');
            }
            CompletionInput::Native { .. } => {
                section(&mut out, &format!("input {number}: native"));
                out.push_str("[native provider state]\n");
            }
        }
    }
    for tool in request.tools() {
        section(&mut out, &format!("tool {}", tool.name));
        let _ = writeln!(out, "{}", tool.description);
        out.push_str(&pretty(&tool.parameters));
        out.push('\n');
    }
    if let Some(schema) = request.schema() {
        section(&mut out, "schema");
        out.push_str(&pretty(schema.as_json()));
        out.push('\n');
    }
    section(&mut out, "sent with");
    out.push_str(&pretty(&sent_with(request)));
    out.push('\n');
    out
}

/// Every serialised field of `request` that is not rendered above it, so a
/// field added to a request shows in every snapshot.
#[allow(clippy::panic)]
fn sent_with(request: &CompletionRequest) -> serde_json::Value {
    let mut fields = match serde_json::to_value(request) {
        Ok(serde_json::Value::Object(fields)) => fields,
        Ok(other) => panic!("a request serialised as {other}, not as an object"),
        Err(error) => panic!("a request could not be serialised: {error}"),
    };
    for rendered in ["input", "tools", "json_schema"] {
        fields.shift_remove(rendered);
    }
    serde_json::Value::Object(fields)
}

fn joined(requests: &[CompletionRequest]) -> String {
    let total = requests.len();
    let mut out = String::new();
    for (index, request) in requests.iter().enumerate() {
        let _ = writeln!(out, "######## request {} of {total} ########", index + 1);
        out.push_str(&rendered(request));
    }
    out
}

fn normalised(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn path_of(manifest_dir: &str, name: &str) -> PathBuf {
    PathBuf::from(manifest_dir)
        .join("tests")
        .join("prompt-snapshots")
        .join(format!("{name}.txt"))
}

/// Compares what `requests` say with the committed file
/// `<manifest_dir>/tests/prompt-snapshots/<name>.txt`. With `LATTICE_UPDATE_SNAPSHOTS=1` in the
/// environment it writes the file instead. Fails the test when the file is missing or differs.
#[allow(clippy::panic)]
pub fn assert_requests(manifest_dir: &str, name: &str, requests: &[CompletionRequest]) {
    assert!(
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "a snapshot name holds only ASCII letters, digits, '-' and '_': {name:?}"
    );
    assert!(!requests.is_empty(), "no request was sent for the snapshot {name}");
    let path = path_of(manifest_dir, name);
    let actual = normalised(&joined(requests));
    if std::env::var(UPDATE_VARIABLE).is_ok_and(|value| value == "1") {
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                panic!("could not create {}: {error}", parent.display());
            }
        }
        if let Err(error) = std::fs::write(&path, actual) {
            panic!("could not write {}: {error}", path.display());
        }
        return;
    }
    let committed = match std::fs::read_to_string(&path) {
        Ok(text) => normalised(&text),
        Err(error) => panic!(
            "the snapshot {} cannot be read ({error}); write it with {UPDATE_VARIABLE}=1",
            path.display()
        ),
    };
    if committed == actual {
        return;
    }
    let mut expected_lines = committed.lines();
    let mut actual_lines = actual.lines();
    let mut line = 1usize;
    loop {
        let (expected, got) = (expected_lines.next(), actual_lines.next());
        if expected != got {
            panic!(
                "the snapshot {} differs at line {line}:\n  committed: {}\n  rendered:  {}\nupdate it with {UPDATE_VARIABLE}=1 and review the diff",
                path.display(),
                expected.unwrap_or("<end of file>"),
                got.unwrap_or("<end of file>"),
            );
        }
        if expected.is_none() {
            panic!(
                "the snapshot {} differs only in its final line ending; update it with {UPDATE_VARIABLE}=1",
                path.display()
            );
        }
        line += 1;
    }
}
