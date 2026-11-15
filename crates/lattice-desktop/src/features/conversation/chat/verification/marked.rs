//! The reply as the claim check reads it. The check cuts the text into
//! sentences, which no byte range of the reply survives, so each source
//! reference is written into the text as its number between two private-use
//! characters the app alone writes. The pieces below read those back; no code
//! here reads a marker out of the model's words.

use std::collections::HashSet;

use crate::features::conversation::chat::reply_text::ReplyText;

const OPEN: char = '\u{e000}';
const CLOSE: char = '\u{e001}';

/// `reply`'s text with each source reference as its number, any other
/// reference taken out, and any private-use character of the model's own
/// dropped so it cannot pass for one the app wrote.
pub(super) fn marked(reply: &ReplyText) -> String {
    let mut out = String::with_capacity(reply.text.len());
    let mut at = 0;
    for placed in &reply.references {
        push_plain(
            &mut out,
            reply.text.get(at..placed.range.start).unwrap_or_default(),
        );
        for number in reply.cited_in(placed.range.clone()) {
            out.push(OPEN);
            out.push_str(&number.to_string());
            out.push(CLOSE);
        }
        at = placed.range.end.max(at);
    }
    push_plain(&mut out, reply.text.get(at..).unwrap_or_default());
    out
}

fn push_plain(out: &mut String, text: &str) {
    out.extend(text.chars().filter(|ch| *ch != OPEN && *ch != CLOSE));
}

/// The numbers `text` cites, each once, in order.
pub(super) fn numbers(text: &str) -> Vec<u32> {
    let mut seen = HashSet::new();
    text.split(OPEN)
        .skip(1)
        .filter_map(|tail| tail.split_once(CLOSE))
        .filter_map(|(digits, _)| digits.parse::<u32>().ok())
        .filter(|number| seen.insert(*number))
        .collect()
}

/// `text` without its references.
pub(super) fn without(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for ch in text.chars() {
        match ch {
            OPEN => inside = true,
            CLOSE => inside = false,
            _ if !inside => out.push(ch),
            _ => {}
        }
    }
    out
}

/// The reference `text` starts with and what follows it.
pub(super) fn leading(text: &str) -> Option<(u32, &str)> {
    let (digits, tail) = text.strip_prefix(OPEN)?.split_once(CLOSE)?;
    Some((digits.parse().ok()?, tail))
}

/// What comes before the reference `text` ends with, and its number.
pub(super) fn trailing(text: &str) -> Option<(&str, u32)> {
    let (head, digits) = text.strip_suffix(CLOSE)?.rsplit_once(OPEN)?;
    Some((head, digits.parse().ok()?))
}
