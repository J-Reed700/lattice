//! Whether a sentence a repair writes is one the lesson already has. Two
//! sentences are the same when they are the same text ([`fold`]). Only the
//! sentences of a paragraph are compared: not headings, list items, table
//! rows, code, a sentence that is only code (a command shown on a line of its
//! own), a sentence too short to be quoted, or the strong label that opens a
//! paragraph, which lessons repeat on purpose.
use crate::review_findings::FindingSite;
use lattice_core::shared::text::same_text::{fold, long_enough_quote};
use lattice_core::shared::text::segmentation::code_fences;
use lattice_core::shared::text::{item_marker_end, sentences};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Two consecutive sentences in the same block where the second repeats the
/// first verbatim or opens by stuttering the first sentence's wording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConsecutiveRepeat {
    pub(crate) first: String,
    pub(crate) second: String,
    pub(crate) prefix_stutter: bool,
}

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
                lazy = !opens_fence && !ends_terminal_sentence(line);
                continue;
            }
            if is_item(line) && !opens_fence {
                item = Some(at);
                lazy = !ends_terminal_sentence(line);
                continue;
            }
            if lazy && !structural {
                continue;
            }
            item = None;
            lazy = false;
        }
        if !current.is_empty() && (structural || is_item(line)) {
            found.push(std::mem::take(&mut current));
        }
        if opens_fence {
            continue;
        }
        if is_item(line) {
            item = Some(at);
            lazy = !ends_terminal_sentence(line);
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

fn is_horizontal_rule(line: &str) -> bool {
    let mut marks = line.chars().filter(|c| !c.is_whitespace());
    let Some(first) = marks.next().filter(|c| matches!(c, '-' | '*' | '_' | '=')) else {
        return false;
    };
    let rest: Vec<_> = marks.collect();
    rest.len() >= 2 && rest.iter().all(|&c| c == first)
}

fn is_bold_label_only(unit: &str) -> bool {
    let trimmed = unit.trim();
    trimmed
        .strip_prefix("**")
        .and_then(|rest| rest.strip_suffix("**"))
        .is_some_and(|inner| !inner.contains("**"))
}

fn sentence_words(sentence: &str) -> Vec<String> {
    fold(sentence)
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'')
                .to_owned()
        })
        .filter(|word| !word.is_empty() && word.chars().any(char::is_alphanumeric))
        .collect()
}

fn has_clause_after_prefix(next: &str, prefix_len: usize, allow_conjunction: bool) -> bool {
    let folded = fold(next);
    let tokens: Vec<&str> = folded.split_whitespace().collect();
    let word_indices: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, w)| w.chars().any(char::is_alphanumeric))
        .map(|(idx, _)| idx)
        .collect();
    let Some(&last_idx) = prefix_len
        .checked_sub(1)
        .and_then(|idx| word_indices.get(idx))
    else {
        return false;
    };
    let last = tokens.get(last_idx).copied().unwrap_or_default();
    let punct_break = last.ends_with([',', ';', ':', '-', '(']);
    let next_token = tokens.get(last_idx + 1).copied().unwrap_or_default();
    let trimmed = next_token.trim_matches(|c: char| !c.is_alphanumeric() && c != '(');
    let starts_clause = next_token.starts_with(['(', '-'])
        || (allow_conjunction
            && matches!(
                trimmed,
                "and" | "or" | "but" | "then" | "so" | "because" | "while" | "which" | "when"
            ));
    punct_break || starts_clause
}

fn classify_consecutive(prev: &str, next: &str) -> Option<ConsecutiveRepeat> {
    let prev_words = sentence_words(prev);
    let next_words = sentence_words(next);
    if prev_words.is_empty() || next_words.is_empty() {
        return None;
    }
    if prev_words == next_words {
        return Some(ConsecutiveRepeat {
            first: prev.to_owned(),
            second: next.to_owned(),
            prefix_stutter: false,
        });
    }
    if next_words.len() > prev_words.len() && next_words.starts_with(&prev_words) {
        let long_stutter = prev_words.len() >= 4;
        let clause_stutter =
            prev_words.len() >= 3 && has_clause_after_prefix(next, prev_words.len(), true);
        let punct_pair = prev_words.len() >= 2
            && !matches!(
                prev_words.first().map(String::as_str),
                Some("it" | "this" | "that" | "these" | "those" | "they" | "there")
            )
            && has_clause_after_prefix(next, prev_words.len(), false);
        if long_stutter || clause_stutter || punct_pair {
            return Some(ConsecutiveRepeat {
                first: prev.to_owned(),
                second: next.to_owned(),
                prefix_stutter: true,
            });
        }
    }
    if prev_words.len() == next_words.len() && prev_words.len() >= 7 {
        let mut diff: Option<(&str, &str)> = None;
        let mut multiple = false;
        for (left, right) in prev_words.iter().zip(&next_words) {
            if left != right {
                if diff.is_some() {
                    multiple = true;
                    break;
                }
                diff = Some((left.as_str(), right.as_str()));
            }
        }
        if !multiple {
            if let Some((left, right)) = diff {
                let code_or_protocol_token = |word: &str| {
                    word.bytes().any(|b| {
                        b.is_ascii_digit() || matches!(b, b'/' | b'.' | b':' | b'-' | b'_')
                    })
                };
                if prev_words.len() >= 13
                    || code_or_protocol_token(left)
                    || code_or_protocol_token(right)
                {
                    return Some(ConsecutiveRepeat {
                        first: prev.to_owned(),
                        second: next.to_owned(),
                        prefix_stutter: true,
                    });
                }
            }
        }
    }
    if prev_words.len() >= 8
        && next_words.len() >= 8
        && prev_words.get(..2) == next_words.get(..2)
        && high_similarity_prefix_stutter(&prev_words, &next_words)
    {
        return Some(ConsecutiveRepeat {
            first: prev.to_owned(),
            second: next.to_owned(),
            prefix_stutter: true,
        });
    }
    if has_contiguous_clause_overlap(&prev_words, &next_words) {
        return Some(ConsecutiveRepeat {
            first: prev.to_owned(),
            second: next.to_owned(),
            prefix_stutter: true,
        });
    }
    None
}

fn has_contiguous_clause_overlap(prev_words: &[String], next_words: &[String]) -> bool {
    const CONTRAST_OPENERS: &[&str] = &[
        "when",
        "if",
        "while",
        "where",
        "whereas",
        "although",
        "however",
        "instead",
        "otherwise",
        "unless",
    ];
    if prev_words
        .first()
        .is_some_and(|w| CONTRAST_OPENERS.contains(&w.as_str()))
        || next_words
            .first()
            .is_some_and(|w| CONTRAST_OPENERS.contains(&w.as_str()))
    {
        return false;
    }
    let aligned_diffs = prev_words
        .iter()
        .zip(next_words)
        .filter(|(left, right)| left != right)
        .count()
        + prev_words.len().abs_diff(next_words.len());
    if aligned_diffs < 2 {
        return false;
    }
    let prev_codes = code_tokens(prev_words);
    let next_codes = code_tokens(next_words);
    if !next_codes.iter().all(|tok| prev_codes.contains(tok))
        || (prev_words.len() <= next_words.len() && prev_codes != next_codes)
    {
        return false;
    }
    let has_six_word_run = next_words.len() >= 6
        && next_words
            .windows(6)
            .any(|win| prev_words.windows(6).any(|p| p == win));
    if !has_six_word_run {
        return false;
    }
    let prev_unique: HashSet<&str> = prev_words.iter().map(String::as_str).collect();
    let next_unique: HashSet<&str> = next_words.iter().map(String::as_str).collect();
    let shared = prev_unique.intersection(&next_unique).count();
    shared >= 7 && shared * 5 >= next_words.len() * 3
}

fn ends_terminal_sentence(line: &str) -> bool {
    let trimmed = line
        .trim_end()
        .trim_end_matches(['"', '\'', '\u{201d}', '\u{2019}', ')', ']', '`', '*']);
    trimmed.ends_with(['.', '?', '!'])
}

fn high_similarity_prefix_stutter(prev_words: &[String], next_words: &[String]) -> bool {
    const CONTRAST_OPENERS: &[&str] = &["when", "if", "while", "where", "whereas", "although"];
    if prev_words
        .first()
        .is_some_and(|w| CONTRAST_OPENERS.contains(&w.as_str()))
    {
        return false;
    }
    let aligned_diffs = prev_words
        .iter()
        .zip(next_words)
        .filter(|(left, right)| left != right)
        .count()
        + prev_words.len().abs_diff(next_words.len());
    if aligned_diffs < 2 {
        return false;
    }
    if code_tokens(prev_words) != code_tokens(next_words) {
        return false;
    }
    let prev_unique: HashSet<&str> = prev_words.iter().map(String::as_str).collect();
    let next_unique: HashSet<&str> = next_words.iter().map(String::as_str).collect();
    let shared: Vec<&str> = prev_unique.intersection(&next_unique).copied().collect();
    let long_shared = shared.iter().filter(|word| word.len() >= 5).count();
    let min_len = prev_words.len().min(next_words.len());
    shared.len() >= 4 && long_shared >= 2 && shared.len() * 11 >= min_len * 5
}

fn code_tokens(words: &[String]) -> Vec<&str> {
    words
        .iter()
        .filter(|w| {
            w.bytes()
                .any(|b| b.is_ascii_digit() || matches!(b, b'/' | b'.' | b':' | b'-' | b'_'))
        })
        .map(String::as_str)
        .collect()
}

fn flush_consecutive_unit(
    unit: &mut String,
    prev: &mut Option<String>,
    history: &mut Vec<(String, Vec<String>)>,
    out: &mut Vec<ConsecutiveRepeat>,
) {
    let body = std::mem::take(unit);
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }
    if is_bold_label_only(trimmed) {
        *prev = None;
        history.clear();
        return;
    }
    for (from, to) in sentences(trimmed) {
        let Some(piece) = trimmed.get(from..to).map(str::trim) else {
            continue;
        };
        if !has_prose_letter(piece) {
            continue;
        }
        let words = sentence_words(piece);
        if let Some(earlier) = prev.as_deref() {
            if let Some(found) = classify_consecutive(earlier, piece) {
                out.push(found);
            }
        }
        if !words.is_empty() {
            let matched = match history.as_slice() {
                [.., s0, s1, s2, s3, s4]
                    if s0.1 == s3.1
                        && s1.1 == s4.1
                        && s2.1 == words
                        && !(s0.1 == s1.1 && s1.1 == s2.1) =>
                {
                    Some((s0.0.clone(), s3.0.clone()))
                }
                [.., s0, s1, s2] if s0.1 == s2.1 && s1.1 == words && s0.1 != s1.1 => {
                    Some((s0.0.clone(), s2.0.clone()))
                }
                _ => None,
            };
            if let Some((first, second)) = matched {
                out.push(ConsecutiveRepeat {
                    first,
                    second,
                    prefix_stutter: false,
                });
            }
        }
        history.push((piece.to_owned(), words));
        if history.len() > 6 {
            history.remove(0);
        }
        *prev = Some(piece.to_owned());
    }
}

/// Consecutive identical sentences and prefix-stutters (`"X. X, and..."`) across
/// prose paragraphs and list items, with no minimum length floor.
pub(crate) fn consecutive_repeats(text: &str) -> Vec<ConsecutiveRepeat> {
    let fences = code_fences(text);
    let mut found = Vec::new();
    let mut current = String::new();
    let mut prev: Option<String> = None;
    let mut history: Vec<(String, Vec<String>)> = Vec::new();
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let start = offset;
        offset += raw.len();
        if fences.iter().any(|fence| fence.whole().contains(&start)) {
            flush_consecutive_unit(&mut current, &mut prev, &mut history, &mut found);
            prev = None;
            history.clear();
            continue;
        }
        let line = raw.trim();
        if line.is_empty() {
            flush_consecutive_unit(&mut current, &mut prev, &mut history, &mut found);
            continue;
        }
        if line.starts_with(['#', '|', '>']) || is_horizontal_rule(line) {
            flush_consecutive_unit(&mut current, &mut prev, &mut history, &mut found);
            prev = None;
            history.clear();
            continue;
        }
        if let Some(marker_end) = item_marker_end(raw) {
            flush_consecutive_unit(&mut current, &mut prev, &mut history, &mut found);
            let content = raw.get(marker_end..).unwrap_or_default().trim();
            if !content.is_empty() {
                current.push_str(content);
                current.push(' ');
            }
            continue;
        }
        current.push_str(line);
        current.push(' ');
    }
    flush_consecutive_unit(&mut current, &mut prev, &mut history, &mut found);
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
