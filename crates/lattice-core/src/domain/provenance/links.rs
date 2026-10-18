//! Outbound links in a source's text, with their byte offsets (§4, §8).
//! One reader for every kind of source: the web feature's reference pages,
//! library documents and pasted text all call [`markdown_links`].
//!
//! The reference reader writes links as `[anchor](url)` and escapes every
//! bracket, backslash and backtick that came from the page's own text, so a
//! page cannot write a link into its prose. This is a scanner for that
//! grammar, not a pattern over prose: backslash escapes, code spans and
//! fenced code blocks are honoured, brackets are matched, and only absolute
//! http(s) destinations count.

use url::Url;

use super::signals::OutboundLink;

const ESCAPE: u8 = b'\\';
const CODE: u8 = b'`';
const OPEN_LABEL: u8 = b'[';
const CLOSE_LABEL: u8 = b']';
const OPEN_DESTINATION: u8 = b'(';
const CLOSE_DESTINATION: u8 = b')';
const IMAGE: u8 = b'!';

/// A fence opens with at least this many backticks or tildes.
const FENCE_MIN: usize = 3;

/// Every `[anchor](http(s) url)` in `text`, in order, outside code. The
/// offsets cover the whole link, `[` through `)`.
pub fn markdown_links(text: &str) -> Vec<OutboundLink> {
    let mut links = Vec::new();
    let mut line_start = 0;
    let mut fence: Option<(u8, usize)> = None;
    while line_start < text.len() {
        let line_end = text
            .get(line_start..)
            .and_then(|rest| rest.find('\n'))
            .map_or(text.len(), |offset| line_start + offset + 1);
        let line = text.get(line_start..line_end).unwrap_or_default();
        match (fence, fence_marker(line)) {
            (None, Some(opening)) => fence = Some(opening),
            (Some((mark, length)), Some((closing, closing_length)))
                if closing == mark && closing_length >= length && is_bare_fence(line) =>
            {
                fence = None;
            }
            (Some(_), _) => {}
            (None, None) => scan_line(text, line_start, line_end, &mut links),
        }
        line_start = line_end;
    }
    links
}

/// The fence character and run length when `line` opens or closes a fence.
fn fence_marker(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start();
    let mark = *trimmed.as_bytes().first()?;
    if mark != CODE && mark != b'~' {
        return None;
    }
    let run = trimmed.bytes().take_while(|&byte| byte == mark).count();
    // A backtick fence's info string has no backtick (CommonMark); a line
    // like ```` ```x``` ```` is an inline code span.
    let info_has_backtick = trimmed.get(run..).is_some_and(|info| info.contains('`'));
    (run >= FENCE_MIN && !(mark == CODE && info_has_backtick)).then_some((mark, run))
}

/// A closing fence carries nothing after its run.
fn is_bare_fence(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.bytes().all(|byte| byte == CODE) || trimmed.bytes().all(|byte| byte == b'~')
}

fn scan_line(text: &str, start: usize, end: usize, links: &mut Vec<OutboundLink>) {
    let bytes = text.as_bytes();
    let mut index = start;
    while index < end {
        match bytes.get(index).copied() {
            Some(ESCAPE) => index += 2,
            Some(CODE) => index = skip_code_span(bytes, index, end),
            Some(OPEN_LABEL) => {
                let is_image = index > start && bytes.get(index - 1) == Some(&IMAGE);
                match link_at(text, index, end) {
                    Some((link, link_end)) => {
                        if !is_image {
                            links.push(link);
                        }
                        index = link_end;
                    }
                    None => index += 1,
                }
            }
            _ => index += 1,
        }
    }
}

/// Past a code span opened at `start`, or one byte on when the run of
/// backticks is never closed (then it is literal text).
fn skip_code_span(bytes: &[u8], start: usize, end: usize) -> usize {
    let run = bytes
        .get(start..end)
        .map_or(0, |rest| rest.iter().take_while(|&&b| b == CODE).count());
    let mut index = start + run;
    while index < end {
        if bytes.get(index) == Some(&CODE) {
            let closing = bytes
                .get(index..end)
                .map_or(0, |rest| rest.iter().take_while(|&&b| b == CODE).count());
            if closing == run {
                return index + closing;
            }
            index += closing;
        } else {
            index += 1;
        }
    }
    start + run
}

/// The link whose label opens at `open`, and the offset just past it.
fn link_at(text: &str, open: usize, end: usize) -> Option<(OutboundLink, usize)> {
    let bytes = text.as_bytes();
    let close = matching_label_end(bytes, open, end)?;
    if bytes.get(close + 1) != Some(&OPEN_DESTINATION) {
        return None;
    }
    let destination_start = close + 2;
    let destination_len = bytes
        .get(destination_start..end)?
        .iter()
        .position(|&byte| {
            byte == CLOSE_DESTINATION || byte == OPEN_DESTINATION || byte.is_ascii_whitespace()
        })?;
    let destination_end = destination_start + destination_len;
    if bytes.get(destination_end) != Some(&CLOSE_DESTINATION) {
        return None;
    }
    let destination = text.get(destination_start..destination_end)?;
    let url = Url::parse(destination).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let anchor = plain_anchor(text.get(open + 1..close)?);
    let link_end = destination_end + 1;
    Some((
        OutboundLink {
            url: destination.to_string(),
            anchor,
            start_byte: open,
            end_byte: link_end,
        },
        link_end,
    ))
}

/// The `]` matching the `[` at `open`, across nested brackets, escapes and
/// code spans.
fn matching_label_end(bytes: &[u8], open: usize, end: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open;
    while index < end {
        match bytes.get(index).copied()? {
            ESCAPE => index += 2,
            CODE => index = skip_code_span(bytes, index, end),
            OPEN_LABEL => {
                depth += 1;
                index += 1;
            }
            CLOSE_LABEL => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    None
}

/// The anchor as a reader sees it: escapes resolved, the reader's entity
/// escapes decoded, emphasis, strikethrough and code markers dropped. The
/// reader escapes a literal `*` or backtick, so an unescaped one is markup.
fn plain_anchor(label: &str) -> String {
    let mut plain = String::with_capacity(label.len());
    let mut characters = label.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                if let Some(escaped) = characters.next() {
                    plain.push(escaped);
                }
            }
            '*' | '`' => {}
            _ => plain.push(character),
        }
    }
    plain
        .replace("~~", "")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
