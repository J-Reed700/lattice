//! Tolerant parsing of the model's JSON row.
//!
//! Three tiers, in order — this never returns `Err`, only a possibly-empty map:
//!
//! 1. strict whole-string parse,
//! 2. first-`{` … last-`}` slice (strips fences and chatty preambles),
//! 3. per-column salvage scan for a truncated or malformed object.
//!
//! Anything still unreadable is simply absent, which the table renders as
//! "not stated" — never as a guess, never as an error.

use std::collections::HashMap;

use serde::Deserialize;

use super::retrieval::truncate_chars;
use super::use_case::{MAX_EXCERPT_CHARS, MAX_VALUE_CHARS};

/// Column name -> (value, quote). Missing / unparseable columns are absent.
pub type ParsedRow = HashMap<String, (Option<String>, Option<String>)>;

#[derive(Debug, Deserialize)]
struct RawCell {
    #[serde(default)]
    value: Option<serde_json::Value>,
    #[serde(default)]
    quote: Option<serde_json::Value>,
}

/// Lowercased, whitespace-collapsed form used for column-name matching.
pub fn normalize_column(column: &str) -> String {
    column.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

const NULL_LIKE: [&str; 7] = [
    "",
    "null",
    "n/a",
    "none",
    "not stated",
    "not specified",
    "unknown",
];

/// Collapses a JSON value to a trimmed string, or `None` when it carries no
/// answer. Objects and arrays mean the model went off-script.
fn normalize_value(value: Option<serde_json::Value>, max_chars: usize) -> Option<String> {
    let raw = match value? {
        serde_json::Value::String(text) => text,
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Null => return None,
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => return None,
    };
    normalize_text(&raw, max_chars)
}

fn normalize_text(raw: &str, max_chars: usize) -> Option<String> {
    let trimmed = raw.trim();
    if NULL_LIKE
        .iter()
        .any(|candidate| trimmed.eq_ignore_ascii_case(candidate))
    {
        return None;
    }
    Some(truncate_chars(trimmed, max_chars))
}

fn absorb(
    parsed: &mut ParsedRow,
    lookup: &HashMap<String, String>,
    map: HashMap<String, RawCell>,
) {
    for (key, cell) in map {
        let Some(canonical) = lookup.get(&normalize_column(&key)) else {
            continue;
        };
        if parsed.contains_key(canonical) {
            continue;
        }
        let value = normalize_value(cell.value, MAX_VALUE_CHARS);
        let quote = normalize_value(cell.quote, MAX_EXCERPT_CHARS);
        if value.is_none() && quote.is_none() {
            // An explicit null answer is still an answer: record it so the
            // salvage pass does not invent one.
            parsed.insert(canonical.clone(), (None, None));
            continue;
        }
        parsed.insert(canonical.clone(), (value, quote));
    }
}

pub fn parse_compare_response(raw: &str, columns: &[String]) -> ParsedRow {
    let mut parsed: ParsedRow = HashMap::new();
    let lookup: HashMap<String, String> = columns
        .iter()
        .map(|column| (normalize_column(column), column.clone()))
        .collect();

    // Tier 1 — strict.
    if let Ok(map) = serde_json::from_str::<HashMap<String, RawCell>>(raw.trim()) {
        absorb(&mut parsed, &lookup, map);
    }

    // Tier 2 — sliced.
    if parsed.len() < columns.len() {
        if let (Some(start), Some(end)) = (raw.find('{'), raw.rfind('}')) {
            if end > start {
                let slice = &raw[start..=end];
                if let Ok(map) = serde_json::from_str::<HashMap<String, RawCell>>(slice) {
                    absorb(&mut parsed, &lookup, map);
                }
            }
        }
    }

    // Tier 3 — per-column salvage.
    if parsed.len() < columns.len() {
        for column in columns {
            if parsed.contains_key(column) {
                continue;
            }
            if let Some(cell) = salvage_column(raw, column) {
                parsed.insert(column.clone(), cell);
            }
        }
    }

    parsed
}

/// Finds `"<column>"` in the raw text, then the next `"value"` / `"quote"`
/// string after it. Regex-free and deliberately shallow: it recovers the
/// columns a truncated object already emitted, and nothing else.
fn salvage_column(raw: &str, column: &str) -> Option<(Option<String>, Option<String>)> {
    // ASCII-only lowercasing: it maps byte-for-byte, so offsets found in
    // `haystack` are valid offsets into `raw`. `str::to_lowercase` is Unicode
    // aware and can change the byte length (`İ` → `i̇` grows by one), which
    // would shift every later index and slice `raw` mid-character — a panic in
    // a parser whose whole contract is that it never panics.
    let haystack = raw.to_ascii_lowercase();
    let needle = normalize_column(column);
    let key_position = haystack.find(&needle)?;
    let tail = &raw[key_position + needle.len()..];
    let tail_lower = &haystack[key_position + needle.len()..];

    // Stop at the next column-looking boundary so we do not steal a later
    // column's value. `},` is the object separator the model emits.
    let bounded = match tail.find("},") {
        Some(end) => &tail[..end + 1],
        None => tail,
    };
    let bounded_lower = &tail_lower[..bounded.len()];

    let value = find_string_after(bounded, bounded_lower, "\"value\"")
        .and_then(|text| normalize_text(&text, MAX_VALUE_CHARS));
    let quote = find_string_after(bounded, bounded_lower, "\"quote\"")
        .and_then(|text| normalize_text(&text, MAX_EXCERPT_CHARS));

    if value.is_none() && quote.is_none() {
        return None;
    }
    Some((value, quote))
}

/// Reads the next JSON string literal that follows `marker`.
fn find_string_after(text: &str, lowered: &str, marker: &str) -> Option<String> {
    let marker_position = lowered.find(marker)?;
    let after = &text[marker_position + marker.len()..];
    let mut chars = after.char_indices();
    let mut start = None;
    for (offset, character) in chars.by_ref() {
        match character {
            ':' | ' ' | '\t' | '\n' | '\r' => continue,
            '"' => {
                start = Some(offset + 1);
                break;
            }
            // `null` (or anything unquoted) — no string to read.
            _ => return None,
        }
    }
    let start = start?;
    let rest = &after[start..];
    let mut collected = String::new();
    let mut escaped = false;
    for character in rest.chars() {
        if escaped {
            match character {
                'n' => collected.push('\n'),
                't' => collected.push('\t'),
                'r' => collected.push('\r'),
                other => collected.push(other),
            }
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => return Some(collected),
            other => collected.push(other),
        }
    }
    // Truncated mid-string: keep what we read rather than dropping the column.
    if collected.is_empty() {
        None
    } else {
        Some(collected)
    }
}
