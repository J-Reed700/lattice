//! Whether a sentence a repair writes is one the lesson already has. Two
//! sentences are the same when they are equal after trimming and collapsing
//! whitespace runs. Only the sentences of a paragraph are compared: not
//! headings, list items, table rows, code, or the strong label that opens a
//! paragraph, which lessons repeat on purpose.
use lattice_core::shared::text::sentences;
use serde_json::Value;
use std::collections::HashMap;

/// How often each sentence occurs in some text.
pub(crate) type Counts = HashMap<String, usize>;

/// `sentence` as sentences are compared.
pub(crate) fn key(sentence: &str) -> String {
    sentence.split_whitespace().collect::<Vec<_>>().join(" ")
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

/// The marker of the code fence `line` opens, if it does.
fn fence_marker(line: &str) -> Option<char> {
    ['`', '~']
        .into_iter()
        .find(|marker| line.starts_with(&marker.to_string().repeat(3)))
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
    let mut found = Vec::new();
    let mut current = String::new();
    let mut fence: Option<char> = None;
    let mut item: Option<usize> = None;
    let mut lazy = false;
    for raw in text.lines() {
        let (line, at) = (raw.trim(), indent(raw));
        if let Some(marker) = fence {
            if line.starts_with(&marker.to_string().repeat(3)) {
                fence = None;
            }
            continue;
        }
        if line.is_empty() {
            if !current.is_empty() {
                found.push(std::mem::take(&mut current));
            }
            lazy = false;
            continue;
        }
        let structural = line.starts_with(['#', '|']) || fence_marker(line).is_some();
        if let Some(marker) = item {
            if at > marker {
                fence = fence_marker(line);
                lazy = fence.is_none();
                continue;
            }
            if is_item(line) {
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
        if let Some(marker) = fence_marker(line) {
            fence = Some(marker);
        } else if is_item(line) {
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
                .filter(|sentence| sentence.chars().any(char::is_alphabetic)),
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

fn collect(value: &Value, counts: &mut Counts) {
    match value {
        Value::String(text) => {
            for (sentence, count) in counts_of_text(text) {
                *counts.entry(sentence).or_default() += count;
            }
        }
        Value::Object(fields) => {
            for value in fields
                .iter()
                .filter(|(name, _)| teaches(name))
                .map(|(_, value)| value)
            {
                collect(value, counts);
            }
        }
        Value::Array(items) => {
            for value in items {
                collect(value, counts);
            }
        }
        _ => (),
    }
}

/// The sentences of every text in `values`.
pub(crate) fn counts_of(values: &[&Value]) -> Counts {
    let mut counts = Counts::new();
    for value in values {
        collect(value, &mut counts);
    }
    counts
}

/// The first sentence the text `after` has more of than `before` that
/// `lesson` already has.
pub(crate) fn gained_repeat(before: &str, after: &str, lesson: &Counts) -> Option<String> {
    let (had, has) = (counts_of_text(before), counts_of_text(after));
    keys(after)
        .into_iter()
        .find(|sentence| has.get(sentence) > had.get(sentence) && lesson.contains_key(sentence))
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
