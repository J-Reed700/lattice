//! The one reader of a JSON answer that may be cut off at any byte: it keeps
//! what closed and never completes, repairs or guesses anything.

use serde_json::{Map, Value};

/// A model's JSON answer as far as it was written.
#[derive(Debug, Clone, Default)]
pub(super) struct Document {
    /// The fields of the top-level object whose values closed, in written order.
    pub fields: Map<String, Value>,
    /// The list field that was still open when the text ended: its name and the elements that closed.
    pub open_list: Option<(String, Vec<Value>)>,
    /// Whether the top-level object closed.
    pub closed: bool,
}

/// The first JSON object in `text`, as far as it was written. `None` when no
/// JSON object begins in it.
pub(super) fn read(text: &str) -> Option<Document> {
    text.bytes()
        .enumerate()
        .filter(|(_, byte)| *byte == b'{')
        .find_map(|(at, _)| object_at(text, at))
}

/// Where a value ends.
enum End {
    /// The value closed; the offset is one past its last byte.
    Closed(usize),
    /// The text ended inside the value.
    Open,
    /// What is there cannot begin or end a value.
    Bad,
}

/// What a list holds when the scan of it stopped.
enum Listing {
    Closed(Vec<Value>, usize),
    /// The text ended, or stopped being JSON, before the list closed.
    Cut(Vec<Value>),
}

fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at = at.saturating_add(1);
    }
    at
}

/// The end of the string that starts with the quote at `start`.
fn string_end(bytes: &[u8], start: usize) -> End {
    let mut at = start.saturating_add(1);
    while let Some(byte) = bytes.get(at) {
        match byte {
            b'\\' => at = at.saturating_add(2),
            b'"' => return End::Closed(at.saturating_add(1)),
            _ => at = at.saturating_add(1),
        }
    }
    End::Open
}

/// The end of the object or list that starts at `start`.
fn nested_end(bytes: &[u8], start: usize) -> End {
    let mut depth = 0usize;
    let mut at = start;
    while let Some(byte) = bytes.get(at) {
        match byte {
            b'"' => match string_end(bytes, at) {
                End::Closed(end) => {
                    at = end;
                    continue;
                }
                other => return other,
            },
            b'{' | b'[' => depth = depth.saturating_add(1),
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return End::Closed(at.saturating_add(1));
                }
            }
            _ => {}
        }
        at = at.saturating_add(1);
    }
    End::Open
}

/// The end of a number, `true`, `false` or `null`. One that reaches the end
/// of the text may have been cut short, so it has not closed.
fn literal_end(bytes: &[u8], start: usize) -> End {
    let mut at = start;
    while let Some(byte) = bytes.get(at) {
        if byte.is_ascii_whitespace() || matches!(byte, b',' | b'}' | b']') {
            return if at == start {
                End::Bad
            } else {
                End::Closed(at)
            };
        }
        at = at.saturating_add(1);
    }
    End::Open
}

fn value_end(bytes: &[u8], start: usize) -> End {
    match bytes.get(start) {
        None => End::Open,
        Some(b'"') => string_end(bytes, start),
        Some(b'{' | b'[') => nested_end(bytes, start),
        Some(_) => literal_end(bytes, start),
    }
}

fn parse(text: &str, start: usize, end: usize) -> Option<Value> {
    serde_json::from_str(text.get(start..end)?).ok()
}

/// The elements that closed in the list that starts at `open`.
fn list_at(text: &str, open: usize) -> Listing {
    let bytes = text.as_bytes();
    let mut items = Vec::new();
    let mut at = open.saturating_add(1);
    let mut need_comma = false;
    loop {
        at = skip_space(bytes, at);
        match bytes.get(at) {
            None => return Listing::Cut(items),
            Some(b']') => return Listing::Closed(items, at.saturating_add(1)),
            Some(b',') if need_comma => {
                need_comma = false;
                at = at.saturating_add(1);
            }
            Some(b',') => return Listing::Cut(items),
            Some(_) if need_comma => return Listing::Cut(items),
            Some(_) => match value_end(bytes, at) {
                End::Closed(end) => match parse(text, at, end) {
                    Some(value) => {
                        items.push(value);
                        at = end;
                        need_comma = true;
                    }
                    None => return Listing::Cut(items),
                },
                End::Open | End::Bad => return Listing::Cut(items),
            },
        }
    }
}

/// The object whose `{` is at `open`, or `None` when no object begins there.
fn object_at(text: &str, open: usize) -> Option<Document> {
    let bytes = text.as_bytes();
    let first = skip_space(bytes, open.saturating_add(1));
    if !matches!(bytes.get(first), Some(b'}' | b'"')) {
        return None;
    }
    let mut document = Document::default();
    let mut at = first;
    loop {
        // At a key, or at the brace that closes the object.
        at = skip_space(bytes, at);
        match bytes.get(at) {
            Some(b'}') => {
                document.closed = true;
                return Some(document);
            }
            Some(b'"') => {}
            _ => return Some(document),
        }
        let End::Closed(key_end) = string_end(bytes, at) else {
            return Some(document);
        };
        let Some(Value::String(key)) = parse(text, at, key_end) else {
            return Some(document);
        };
        at = skip_space(bytes, key_end);
        if bytes.get(at) != Some(&b':') {
            return Some(document);
        }
        at = skip_space(bytes, at.saturating_add(1));
        if bytes.get(at) == Some(&b'[') {
            match list_at(text, at) {
                Listing::Closed(items, end) => {
                    document.fields.insert(key, Value::Array(items));
                    at = end;
                }
                Listing::Cut(items) => {
                    document.open_list = Some((key, items));
                    return Some(document);
                }
            }
        } else {
            let End::Closed(end) = value_end(bytes, at) else {
                return Some(document);
            };
            let Some(value) = parse(text, at, end) else {
                return Some(document);
            };
            document.fields.insert(key, value);
            at = end;
        }
        // After a value: a comma, or the brace that closes the object.
        at = skip_space(bytes, at);
        match bytes.get(at) {
            Some(b',') => at = at.saturating_add(1),
            Some(b'}') => {
                document.closed = true;
                return Some(document);
            }
            _ => return Some(document),
        }
    }
}
