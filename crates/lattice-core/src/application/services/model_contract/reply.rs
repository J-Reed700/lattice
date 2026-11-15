//! Replies: pointers written in prose.
//!
//! A streamed answer cannot be a JSON object, so a chat, tutor or Explorer
//! reply points from inside its prose. A marker is a bracketed handle: `[s12]`
//! for a source, `[i3]` on a line of its own for a picture, `[f2:10-20]` for
//! lines of a file. One scanner, here, outside code spans and fences. A
//! bracketed token is a marker only if its kind is one the reply may point at
//! and it resolves in the turn's scope; anything else is the reply's own text
//! and is left alone. The stream that leaves this module is already split
//! into text and references: nothing downstream parses prose.

use serde::{Deserialize, Serialize};

use super::shown::{Drawn, Kind, Scope};

#[cfg(test)]
mod tests;

/// A line range a marker names inside the thing it points at: `10-20` in `[f2:10-20]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lines {
    pub first: u32,
    pub last: u32,
}

/// A marker that resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    /// The prefix of the kind pointed at.
    pub prefix: &'static str,
    /// The handle's own number: what a person sees for a kind drawn as a number.
    pub number: u32,
    /// The kind's `Ref`, as a stored scope keeps it.
    pub reference: serde_json::Value,
    pub drawn: Drawn,
    pub lines: Option<Lines>,
}

/// One piece of a reply, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(String),
    Reference(Reference),
}

/// Where in a reply a marker of a kind counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Anywhere in the prose.
    Inline,
    /// Only alone on a line of its own.
    OwnLine,
}

#[derive(Debug, Clone, Copy)]
struct Pointable {
    prefix: &'static str,
    drawn: Drawn,
    place: Place,
    lines: bool,
}

/// The kinds a reply may point at. The feature that owns the reply declares them.
#[derive(Debug, Clone, Default)]
pub struct Markers {
    kinds: Vec<Pointable>,
}

impl Markers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Things of kind `K` may be pointed at, where `place` says.
    pub fn of<K: Kind>(mut self, place: Place) -> Self {
        self.kinds.push(Pointable {
            prefix: K::PREFIX,
            drawn: K::DRAWN,
            place,
            lines: false,
        });
        self
    }

    /// Things of kind `K` may be pointed at with a line range: `[f2:10-20]`.
    pub fn of_lines<K: Kind>(mut self, place: Place) -> Self {
        self.kinds.push(Pointable {
            prefix: K::PREFIX,
            drawn: K::DRAWN,
            place,
            lines: true,
        });
        self
    }
}

/// Splits a reply as it streams. A marker may arrive in parts: text that could
/// still become one is held back until it resolves or cannot.
pub struct Splitter<'s> {
    scope: &'s Scope,
    markers: Markers,
    /// Input not yet decided: it starts at the first character whose reading needs what follows.
    held: String,
    /// The open fenced block: its character and the length of its opening run.
    fence: Option<(char, usize)>,
    /// The length of the backtick run that opened the inline code span still open.
    span: Option<usize>,
    /// Only spaces and tabs so far on this line.
    line_blank: bool,
    /// Spaces so far on this line when nothing else came and a fence could still start; else `None`.
    indent: Option<usize>,
}

/// What the scanner does next with the undecided input.
enum Step {
    /// The next characters depend on input not yet seen.
    Wait,
    /// This many bytes are the reply's own text.
    Text(usize),
    /// This many bytes are a marker that resolved.
    Marker(usize, Reference),
}

enum Candidate {
    NeedMore,
    Text,
    Marker(usize, Reference),
}

enum Number {
    Done { value: u32, end: usize },
    /// The input ended inside the digits.
    Open,
    Bad,
}

enum Ranged {
    Done(Lines, usize),
    Open,
    Bad,
}

impl<'s> Splitter<'s> {
    pub fn new(scope: &'s Scope, markers: Markers) -> Self {
        Self {
            scope,
            markers,
            held: String::new(),
            fence: None,
            span: None,
            line_blank: true,
            indent: Some(0),
        }
    }

    /// The pieces `chunk` completes.
    pub fn push(&mut self, chunk: &str) -> Vec<Piece> {
        self.held.push_str(chunk);
        self.advance(false)
    }

    /// The pieces still held when the reply ends.
    pub fn finish(mut self) -> Vec<Piece> {
        self.advance(true)
    }

    /// Decides as much of the held input as it can; at the end of the reply, all of it.
    fn advance(&mut self, last: bool) -> Vec<Piece> {
        let buffer = std::mem::take(&mut self.held);
        let mut pieces = Vec::new();
        let mut text = String::new();
        let mut at = 0;
        while let Some(rest) = buffer.get(at..).filter(|rest| !rest.is_empty()) {
            match self.next(rest, last) {
                Step::Wait => break,
                Step::Text(len) => {
                    let Some(taken) = rest.get(..len) else { break };
                    for ch in taken.chars() {
                        self.note(ch);
                    }
                    text.push_str(taken);
                    at += len;
                }
                Step::Marker(len, reference) => {
                    if !text.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut text)));
                    }
                    pieces.push(Piece::Reference(reference));
                    self.line_blank = false;
                    self.indent = None;
                    at += len;
                }
            }
        }
        self.held = buffer.get(at..).unwrap_or_default().to_owned();
        if !text.is_empty() {
            pieces.push(Piece::Text(text));
        }
        pieces
    }

    /// Where `ch`, now part of the reply's text, leaves the line and the paragraph.
    fn note(&mut self, ch: char) {
        match ch {
            '\n' => {
                if self.line_blank {
                    self.span = None;
                }
                self.line_blank = true;
                self.indent = Some(0);
            }
            ' ' => self.indent = self.indent.map(|spaces| spaces + 1).filter(|spaces| *spaces <= 3),
            '\t' => self.indent = None,
            // Before a line feed it is part of the line's end.
            '\r' => {}
            _ => {
                self.line_blank = false;
                self.indent = None;
            }
        }
    }

    fn next(&mut self, rest: &str, last: bool) -> Step {
        let Some(ch) = rest.chars().next() else {
            return Step::Wait;
        };
        let one = Step::Text(ch.len_utf8());
        if let Some((fence, size)) = self.fence {
            if ch != fence || self.indent.is_none() {
                return one;
            }
            let (len, complete) = run(rest, ch);
            if !complete && !last {
                return Step::Wait;
            }
            if len >= size {
                self.fence = None;
            }
            return Step::Text(len);
        }
        match ch {
            '`' | '~' => {
                let fence_possible = self.indent.is_some();
                if ch == '~' && !fence_possible {
                    return one;
                }
                let (len, complete) = run(rest, ch);
                if !complete && !last {
                    return Step::Wait;
                }
                if fence_possible && len >= 3 {
                    self.fence = Some((ch, len));
                    self.span = None;
                } else if ch == '`' {
                    self.span = match self.span {
                        Some(open) if open == len => None,
                        None => Some(len),
                        open => open,
                    };
                }
                Step::Text(len)
            }
            '[' if self.span.is_none() => match self.candidate(rest, last) {
                Candidate::NeedMore => Step::Wait,
                Candidate::Text => one,
                Candidate::Marker(len, reference) => Step::Marker(len, reference),
            },
            _ => one,
        }
    }

    /// Reads a marker at the `[` that starts `rest`. Each character either continues
    /// the one form a marker can have or ends the candidate, so no more than that form is ever held.
    fn candidate(&self, rest: &str, last: bool) -> Candidate {
        let bytes = rest.as_bytes();
        let byte = |index: usize| bytes.get(index).copied();
        let unfinished = || {
            if last {
                Candidate::Text
            } else {
                Candidate::NeedMore
            }
        };
        let mut letters_end = 1;
        loop {
            match byte(letters_end) {
                Some(b) if b.is_ascii_alphabetic() => {
                    letters_end += 1;
                    let letters = rest.get(1..letters_end).unwrap_or_default();
                    if !self.markers.kinds.iter().any(|kind| kind.prefix.starts_with(letters)) {
                        return Candidate::Text;
                    }
                }
                Some(_) => break,
                None => return unfinished(),
            }
        }
        let prefix = rest.get(1..letters_end).unwrap_or_default();
        let Some(kind) = self.markers.kinds.iter().find(|kind| kind.prefix == prefix) else {
            return Candidate::Text;
        };
        let handle_end = match read_number(bytes, letters_end) {
            Number::Done { end, .. } => end,
            Number::Open => return unfinished(),
            Number::Bad => return Candidate::Text,
        };
        let (close, lines) = match byte(handle_end) {
            None => return unfinished(),
            Some(b']') => (handle_end, None),
            Some(b':') if kind.lines => match read_range(bytes, handle_end.saturating_add(1)) {
                Ranged::Done(lines, close) => (close, Some(lines)),
                Ranged::Open => return unfinished(),
                Ranged::Bad => return Candidate::Text,
            },
            Some(_) => return Candidate::Text,
        };
        let handle = rest.get(1..handle_end).unwrap_or_default();
        let Some((_, number, reference)) = self.scope.resolve_any(handle) else {
            return Candidate::Text;
        };
        let len = close.saturating_add(1);
        if kind.place == Place::OwnLine {
            if !self.line_blank {
                return Candidate::Text;
            }
            let mut after = len;
            loop {
                match byte(after) {
                    Some(b' ' | b'\t' | b'\r') => after += 1,
                    Some(b'\n') => break,
                    Some(_) => return Candidate::Text,
                    None if last => break,
                    None => return Candidate::NeedMore,
                }
            }
        }
        Candidate::Marker(
            len,
            Reference {
                prefix: kind.prefix,
                number,
                reference,
                drawn: kind.drawn,
                lines,
            },
        )
    }
}

/// The length of the run of `ch` (an ASCII character) at the start of `rest`, and whether
/// a different character ends it.
fn run(rest: &str, ch: char) -> (usize, bool) {
    let len = rest.len().saturating_sub(rest.trim_start_matches(ch).len());
    (len, len < rest.len())
}

/// A number with no leading zero that fits a `u32`, starting at `from`.
fn read_number(bytes: &[u8], from: usize) -> Number {
    let mut value: u32 = 0;
    let mut index = from;
    loop {
        match bytes.get(index).copied() {
            Some(b @ b'0'..=b'9') => {
                if index == from && b == b'0' {
                    return Number::Bad;
                }
                let digit = u32::from(b - b'0');
                match value.checked_mul(10).and_then(|tens| tens.checked_add(digit)) {
                    Some(next) => value = next,
                    None => return Number::Bad,
                }
                index += 1;
            }
            Some(_) if index == from => return Number::Bad,
            Some(_) => return Number::Done { value, end: index },
            None => return Number::Open,
        }
    }
}

/// `first]` or `first-last]` from `from`, with `first <= last`; the index of the `]`.
fn read_range(bytes: &[u8], from: usize) -> Ranged {
    let (first, after_first) = match read_number(bytes, from) {
        Number::Done { value, end } => (value, end),
        Number::Open => return Ranged::Open,
        Number::Bad => return Ranged::Bad,
    };
    match bytes.get(after_first).copied() {
        None => Ranged::Open,
        Some(b']') => Ranged::Done(
            Lines {
                first,
                last: first,
            },
            after_first,
        ),
        Some(b'-') => match read_number(bytes, after_first.saturating_add(1)) {
            Number::Done { value: last, end } => match bytes.get(end).copied() {
                None => Ranged::Open,
                Some(b']') if first <= last => Ranged::Done(Lines { first, last }, end),
                Some(_) => Ranged::Bad,
            },
            Number::Open => Ranged::Open,
            Number::Bad => Ranged::Bad,
        },
        Some(_) => Ranged::Bad,
    }
}

/// A whole reply split at once.
pub fn split(scope: &Scope, markers: Markers, reply: &str) -> Vec<Piece> {
    let mut splitter = Splitter::new(scope, markers);
    let mut pieces = splitter.push(reply);
    pieces.extend(splitter.finish());
    pieces
}

/// A message as a later prompt reads it: its text, each reference written as
/// its bracketed handle.
pub fn written(pieces: &[Piece]) -> String {
    let mut written = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(text) => written.push_str(text),
            Piece::Reference(reference) => {
                let (prefix, number) = (reference.prefix, reference.number);
                let marker = match reference.lines {
                    None => format!("[{prefix}{number}]"),
                    Some(Lines { first, last }) if first == last => {
                        format!("[{prefix}{number}:{first}]")
                    }
                    Some(Lines { first, last }) => format!("[{prefix}{number}:{first}-{last}]"),
                };
                written.push_str(&marker);
            }
        }
    }
    written
}
