//! Whether a sentence a repair writes is one the lesson already has. Two
//! sentences are the same when they are the same text ([`fold`]). Only the
//! sentences of a paragraph are compared: not headings, list items, table
//! rows, code, a sentence that is only code (a command shown on a line of its
//! own), a sentence too short to be quoted, or the strong label that opens a
//! paragraph, which lessons repeat on purpose.
use crate::generation::long_enough_quote;
use crate::lessons::same_text::fold;
use crate::review_findings::FindingSite;
use lattice_core::shared::text::segmentation::code_fences;
use lattice_core::shared::text::sentences;
use serde_json::Value;
use std::collections::HashMap;

/// How often each sentence occurs in some text.
pub(crate) type Counts = HashMap<String, usize>;

/// `sentence` as sentences are compared.
pub(crate) fn key(sentence: &str) -> String {
    fold(sentence)
}

/// Whether `sentence` has a letter outside its inline code spans: a span is
/// opened by a run of backticks and closed by the next run of as many.
fn has_prose_letter(sentence: &str) -> bool {
    let mut rest = sentence;
    while let Some(open) = rest.find('`') {
        if rest
            .get(..open)
            .is_some_and(|prose| prose.chars().any(char::is_alphabetic))
        {
            return true;
        }
        let after = rest.get(open..).unwrap_or_default();
        let ticks = after.chars().take_while(|c| *c == '`').count();
        let marker = "`".repeat(ticks);
        let body = after.get(ticks..).unwrap_or_default();
        match body.find(&marker) {
            Some(close) => rest = body.get(close + ticks..).unwrap_or_default(),
            None => return body.chars().any(char::is_alphabetic),
        }
    }
    rest.chars().any(char::is_alphabetic)
}

fn is_item(line: &str) -> bool {
    if let Some(rest) = line.strip_prefix(['-', '*', '+']) {
        return rest.starts_with(char::is_whitespace);
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    digits > 0
        && line
            .get(digits..)
            .and_then(|rest| rest.strip_prefix(['.', ')']))
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

/// How far `line` is indented; a tab counts as four.
fn indent(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// The paragraphs of `text`: its lines outside code fences, headings, table
/// rows and list items. An item holds the lines indented deeper than its
/// marker, fences and blank lines among them, and an unindented line that
/// follows its text directly; the list ends at the first other line.
fn paragraphs(text: &str) -> Vec<String> {
    let fences = code_fences(text);
    let mut found = Vec::new();
    let mut current = String::new();
    let mut item: Option<usize> = None;
    let mut lazy = false;
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let start = offset;
        offset += raw.len();
        let (line, at) = (raw.trim(), indent(raw));
        let fence = fences.iter().find(|fence| fence.whole().contains(&start));
        if fence.is_some_and(|fence| fence.opening.start != start) {
            continue;
        }
        let opens_fence = fence.is_some();
        if line.is_empty() {
            if !current.is_empty() {
                found.push(std::mem::take(&mut current));
            }
            lazy = false;
            continue;
        }
        let structural = line.starts_with(['#', '|']) || opens_fence;
        if let Some(marker) = item {
            if at > marker {
                lazy = !opens_fence;
                continue;
            }
            if is_item(line) && !opens_fence {
                item = Some(at);
                lazy = true;
                continue;
            }
            if lazy && !structural {
                continue;
            }
            item = None;
        }
        if !current.is_empty() && (structural || is_item(line)) {
            found.push(std::mem::take(&mut current));
        }
        if opens_fence {
            continue;
        }
        if is_item(line) {
            item = Some(at);
            lazy = true;
        } else if !structural {
            current.push_str(line);
            current.push('\n');
        }
    }
    if !current.is_empty() {
        found.push(current);
    }
    found
}

/// `paragraph` without a strong label at its very start.
fn without_label(paragraph: &str) -> &str {
    paragraph
        .strip_prefix("**")
        .and_then(|rest| rest.split_once("**"))
        .map_or(paragraph, |(_, rest)| rest)
}

/// The sentences of the paragraphs of `text`, in order, as written.
pub(crate) fn comparable(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for paragraph in paragraphs(text) {
        let prose = without_label(&paragraph);
        found.extend(
            sentences(prose)
                .into_iter()
                .filter_map(|(from, to)| prose.get(from..to))
                .map(|sentence| sentence.trim().to_owned())
                .filter(|sentence| has_prose_letter(sentence) && long_enough_quote(sentence)),
        );
    }
    found
}

/// The compared sentences of `text`, in order.
pub(crate) fn keys(text: &str) -> Vec<String> {
    comparable(text)
        .iter()
        .map(|sentence| key(sentence))
        .collect()
}

fn counts_of_text(text: &str) -> Counts {
    let mut counts = Counts::new();
    for sentence in keys(text) {
        *counts.entry(sentence).or_default() += 1;
    }
    counts
}

/// Whether `name` holds text a lesson teaches: not its kind, and not the
/// citations and quotes, which copy sentences of a source or of the part.
fn teaches(name: &str) -> bool {
    !["kind", "citations", "quote"].contains(&name)
}

/// Whether the text at `path`, a JSON pointer or a `/`-joined path, is
/// taught: no segment of it names a field [`teaches`] leaves out.
pub(crate) fn teaches_path(path: &str) -> bool {
    path.split('/').all(teaches)
}

/// The sentences a lesson may say once: its sections' bodies.
pub(crate) fn said(values: &[&Value]) -> Counts {
    let mut counts = Counts::new();
    for value in values {
        let bodies = value
            .get("blocks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|block| block.get("body").and_then(Value::as_str));
        for body in bodies {
            for (sentence, count) in counts_of_text(body) {
                *counts.entry(sentence).or_default() += count;
            }
        }
    }
    counts
}

/// Whether `path` names a section body.
pub(crate) fn says_at(path: &str) -> bool {
    FindingSite::of_pointer(path)
        .field_pointer("body")
        .as_deref()
        == Some(path)
}

/// The sentences `after` has more of than `before`, as `after` writes them.
pub(crate) fn added(before: &str, after: &str) -> Vec<String> {
    let (had, has) = (counts_of_text(before), counts_of_text(after));
    comparable(after)
        .into_iter()
        .filter(|sentence| has.get(&key(sentence)) > had.get(&key(sentence)))
        .collect()
}

/// The compared sentences of each text in `value` by its path, laid out as
/// a repair confines it.
pub(crate) fn fields_of(value: &Value) -> Vec<(String, Vec<String>)> {
    fn walk(value: &Value, path: &str, out: &mut Vec<(String, Vec<String>)>) {
        let child = |name: &str| {
            if path.is_empty() {
                name.to_owned()
            } else {
                format!("{path}/{name}")
            }
        };
        match value {
            Value::String(text) => out.push((path.to_owned(), keys(text))),
            Value::Object(fields) => {
                for (name, value) in fields.iter().filter(|(name, _)| teaches(name)) {
                    walk(value, &child(name), out);
                }
            }
            Value::Array(items) => {
                for (index, value) in items.iter().enumerate() {
                    walk(value, &child(&index.to_string()), out);
                }
            }
            _ => (),
        }
    }
    let mut out = Vec::new();
    walk(value, "", &mut out);
    out
}

#[cfg(test)]
#[path = "repeats_tests.rs"]
mod tests;
