//! Independent checks receive the public task, never a reference answer.
use super::{
    invalid, validate_files, FileJson, GeneratedPracticalFile, PracticalFileRole,
    PracticalRuntimeContext, MAX_RESPONSE_CHARS,
};
use lattice_core::{application::ports::LLMPort, shared::error::Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChecksJson {
    check_files: Vec<FileJson>,
}

fn schema(runtime: &PracticalRuntimeContext) -> Value {
    let path = if runtime.evaluator_paths.is_empty() {
        json!({"type":"string","minLength":1,"maxLength":240})
    } else {
        json!({"type":"string","enum":runtime.evaluator_paths})
    };
    json!({
        "type":"object","additionalProperties":false,"required":["checkFiles"],
        "properties":{"checkFiles":{"type":"array","minItems":1,"maxItems":16,"items":{
            "type":"object","additionalProperties":false,"required":["path","content"],
            "properties":{"path":path,"content":{"type":"string","minLength":1}}
        }}}
    })
}

pub(super) fn check_python_line_quotes(path: &str, content: &str) -> Result<()> {
    let mut triple: Option<&str> = None;
    for (idx, line) in content.lines().enumerate() {
        let mut chars = line.chars().peekable();
        let mut quote: Option<char> = None;
        while let Some(ch) = chars.next() {
            if let Some(delim) = triple {
                let first = delim.chars().next().unwrap_or('"');
                if ch == '\\' {
                    let _ = chars.next();
                } else if ch == first && chars.peek() == Some(&first) {
                    let mut clone = chars.clone();
                    let _ = clone.next();
                    if clone.next() == Some(first) {
                        let _ = chars.next();
                        let _ = chars.next();
                        triple = None;
                    }
                }
                continue;
            }
            if let Some(active) = quote {
                if ch == '\\' {
                    let _ = chars.next();
                } else if ch == active {
                    quote = None;
                }
                continue;
            }
            if ch == '#' {
                break;
            }
            if ch == '\'' || ch == '"' {
                if chars.peek() == Some(&ch) {
                    let mut clone = chars.clone();
                    let _ = clone.next();
                    if clone.next() == Some(ch) {
                        let _ = chars.next();
                        let _ = chars.next();
                        triple = Some(if ch == '\'' { "'''" } else { "\"\"\"" });
                        continue;
                    }
                }
                quote = Some(ch);
            }
        }
        if quote.is_some() && !line.trim_end().ends_with('\\') {
            return Err(invalid(format!(
                "Python file '{path}' has an unterminated string literal on line {} (`{}`): close the string literal with a matching quote, and use double-quoted strings with backslash-escaped quotes instead of single-quoted strings containing double quotes.",
                idx + 1,
                line.trim()
            )));
        }
    }
    if triple.is_some() {
        return Err(invalid(format!(
            "Python file '{path}' has an unterminated triple-quoted string."
        )));
    }
    Ok(())
}

pub(super) async fn generate(
    llm: &dyn LLMPort,
    brief: &str,
    files: &[GeneratedPracticalFile],
    runtime: &PracticalRuntimeContext,
) -> Result<Vec<GeneratedPracticalFile>> {
    if files.iter().all(|file| !file.editable) {
        return Err(invalid(
            "An executable practical activity needs at least one editable starter file.",
        ));
    }
    let public_files = files
        .iter()
        .map(|file| {
            json!({
                "path":file.path,"content":file.content,"editable":file.editable,
            })
        })
        .collect::<Vec<_>>();
    let existing = files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<HashSet<_>>();
    crate::features::learning::generation::complete_checked(
        llm,
        "Author only independent evaluator files for this public practical brief and exact executionEnvironment. evaluatorPaths are app-owned required filenames; use those paths exactly and include every listed file. Every publicFiles path belongs to the learner workspace: never emit, copy or replace it. Import the learner's supplied interface and assert the stated behavior and edge cases with 4 to 6 concise, syntactically valid test cases. In Python files, use double-quoted string literals with backslash-escaped quotes instead of single-quoted strings containing double quotes. Use only supported language versions and available dependencies. Assertions must fail nonzero; zero checks cannot count as success. Do not include a solution or reference implementation, derive expected output from hidden solution text, use networking, or execute a host shell.",
        json!({"brief":brief,"publicFiles":public_files,"executionEnvironment":runtime}).to_string(),
        schema(runtime), 2_500, None, None,
        |raw| {
            if raw.chars().count() > MAX_RESPONSE_CHARS { return Err(invalid("The generated evaluator is too large.")); }
            let checks: ChecksJson = serde_json::from_str(raw).map_err(|_| invalid("The model returned malformed evaluator JSON."))?;
            if !(1..=16).contains(&checks.check_files.len()) { return Err(invalid("Return 1–16 independent evaluator files.")); }
            let checks = validate_files(checks.check_files.into_iter().map(|file| (file, PracticalFileRole::Check)))?;
            if let Some(file) = checks.iter().find(|file| existing.contains(file.path.as_str())) {
                return Err(invalid(format!("Evaluator file '{}' cannot replace a public practical file; return only independent checks at the required evaluator paths.", file.path)));
            }
            if !runtime.evaluator_paths.is_empty() {
                if checks.iter().any(|file| !runtime.evaluator_paths.contains(&file.path)) {
                    return Err(invalid("Evaluator files must use the exact app-owned evaluatorPaths."));
                }
                if runtime.evaluator_paths.iter().any(|path| !checks.iter().any(|file| &file.path == path)) {
                    return Err(invalid("The evaluator must include every required evaluatorPaths file."));
                }
            }
            Ok(checks)
        },
    ).await
}
