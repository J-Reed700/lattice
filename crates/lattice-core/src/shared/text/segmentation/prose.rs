//! `Prose`: where running text may be cut. UAX #29 sentence boundaries with
//! CLDR's English suppressions and the Markdown tailorings of the design
//! (§2b), and lines, paragraphs, sections, clauses and words around them.

use std::collections::HashSet;

use once_cell::sync::Lazy;
use unicode_segmentation::UnicodeSegmentation;

use super::suppressions::{CLDR_EN, HONORIFICS};
use super::{Boundaries, Break, Span, Strength};
use crate::shared::markdown_math::inline_math_spans;

/// Running text, Markdown included: sentences by UAX #29, CLDR's
/// abbreviations and the Markdown rules, then lines, paragraphs and sections
/// by its line breaks, clauses by its punctuation and words by its spaces.
pub struct Prose;

impl Boundaries for Prose {
    fn breaks(&self, text: &str) -> Vec<Break> {
        let layout = Layout::of(text);
        let mut found: Vec<(usize, Strength)> = text
            .split_sentence_bound_indices()
            .filter_map(|(at, _)| layout.sentence_break(text, at))
            .map(|at| (at, Strength::Sentence))
            .collect();
        let mut before = None;
        let mut in_space = false;
        for (at, c) in text.char_indices() {
            if c.is_whitespace() {
                in_space = true;
                continue;
            }
            if in_space {
                found.push((at, Strength::Word));
            }
            let clause = if in_space {
                matches!(before, Some(',' | ';' | ':' | '—'))
            } else {
                before == Some('—')
            };
            if clause && !layout.holds(at) {
                found.push((at, Strength::Clause));
            }
            in_space = false;
            before = Some(c);
        }
        found.sort_unstable();
        let mut breaks: Vec<Break> = Vec::with_capacity(found.len());
        for (at, strength) in found {
            match breaks.last_mut() {
                Some(last) if last.at == at => last.strength = last.strength.max(strength),
                _ => breaks.push(Break { at, strength }),
            }
        }
        for cut in &mut breaks {
            cut.strength = layout.strength(text, cut.at, cut.strength);
        }
        breaks
    }
}

/// `Prose` for hard-wrapped text, where a line break inside a paragraph is a cut only where it ends a sentence or opens
/// a block.
pub struct Flowed;

impl Boundaries for Flowed {
    fn breaks(&self, text: &str) -> Vec<Break> {
        let layout = Layout::of(text);
        let mut breaks = Prose.breaks(text);
        for cut in &mut breaks {
            if cut.strength == Strength::Line {
                cut.strength = flowed_strength(text, &layout, cut.at);
            }
        }
        breaks
    }
}

/// The strength of the Line break at `at`, which follows a newline and the next line's indentation.
fn flowed_strength(text: &str, layout: &Layout, at: usize) -> Strength {
    let before = text.get(..at).unwrap_or_default().trim_end();
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    let previous = before.get(line_start..).unwrap_or_default();
    let next = text
        .get(at..)
        .unwrap_or_default()
        .split('\n')
        .next()
        .unwrap_or_default()
        .trim_end();
    let opens = is_heading(previous)
        || is_rule(previous)
        || is_list_item(next)
        || is_heading(next)
        || is_rule(next)
        || (next.starts_with('>') && !previous.trim_start().starts_with('>'));
    if opens || layout.in_code(line_start) || layout.in_code(at) {
        return Strength::Line;
    }
    let joined = format!("{previous} {next}");
    let join = previous.len() + 1;
    match Prose.breaks(&joined).into_iter().find(|cut| cut.at == join) {
        Some(cut) if cut.strength >= Strength::Sentence => Strength::Line,
        Some(cut) => cut.strength,
        None => Strength::Word,
    }
}

/// Whether `line` is a Markdown heading: one to six `#` and a space.
fn is_heading(line: &str) -> bool {
    let line = line.trim_start();
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ')
}

/// Whether `line` opens a list item: a bullet or an ordered marker, and whitespace after it.
fn is_list_item(line: &str) -> bool {
    let line = line.trim_start();
    let bullet = line.strip_prefix(['-', '*', '+', '•', '◦', '▪', '‣', '–']);
    bullet.is_some_and(|rest| rest.starts_with([' ', '\t'])) || list_marker_end(line).is_some()
}

/// Whether `line` is a rule: three or more of one of `-`, `*`, `_` or `=`, with spaces between them.
fn is_rule(line: &str) -> bool {
    let mut marks = line.chars().filter(|c| !c.is_whitespace());
    let Some(first) = marks.next().filter(|c| matches!(c, '-' | '*' | '_' | '=')) else {
        return false;
    };
    let rest = marks.collect::<Vec<_>>();
    rest.len() >= 2 && rest.iter().all(|&c| c == first)
}

/// What a text's lines hold that limits where it may be cut.
struct Layout {
    /// Fenced code blocks and tables, as whole lines.
    code: Vec<Span>,
    /// Inline code, inline math and link destinations, merged.
    protected: Vec<Span>,
    /// Where ordered-list markers, and the whitespace after them, end.
    markers: Vec<usize>,
}

impl Layout {
    fn of(text: &str) -> Self {
        let mut layout = Self {
            code: Vec::new(),
            protected: Vec::new(),
            markers: Vec::new(),
        };
        let mut fence: Option<char> = None;
        let mut start = 0;
        for line in text.split_inclusive('\n') {
            let end = start + line.len();
            let trimmed = line.trim_start();
            let marker = if trimmed.starts_with("```") {
                Some('`')
            } else if trimmed.starts_with("~~~") {
                Some('~')
            } else {
                None
            };
            let code = match (fence, marker) {
                (Some(open), found) => {
                    if found == Some(open) {
                        fence = None;
                    }
                    true
                }
                (None, Some(open)) => {
                    fence = Some(open);
                    true
                }
                (None, None) => trimmed.starts_with('|'),
            };
            if code {
                match layout.code.last_mut() {
                    Some(last) if last.end == start => last.end = end,
                    _ => layout.code.push(start..end),
                }
            } else {
                for span in inline_math_spans(line)
                    .into_iter()
                    .chain(code_spans(line))
                    .chain(link_destinations(line))
                {
                    layout.protected.push(start + span.start..start + span.end);
                }
                layout
                    .markers
                    .extend(list_marker_end(line).map(|at| start + at));
            }
            start = end;
        }
        layout.protected.sort_unstable_by_key(|span| span.start);
        let mut merged: Vec<Span> = Vec::with_capacity(layout.protected.len());
        for span in layout.protected {
            match merged.last_mut() {
                Some(last) if span.start < last.end => last.end = last.end.max(span.end),
                _ => merged.push(span),
            }
        }
        layout.protected = merged;
        layout
    }

    /// Where the break UAX #29 put at `uax` lands, or `None` when a rule takes
    /// it away. A break after a line break is left to `strength`: the
    /// whitespace before it makes it a Line, Paragraph or Section break.
    fn sentence_break(&self, text: &str, uax: usize) -> Option<usize> {
        if uax == 0 {
            return None;
        }
        let at = settle(text, uax)?;
        let dropped = newlines_before(text, at) > 0
            || suppressed(text, uax)
            || continues(text, at)
            || self.markers.binary_search(&at).is_ok()
            || self.holds(at);
        (!dropped).then_some(at)
    }

    /// Whether a sentence or clause break at `at` is out of place: inside
    /// inline code, math or a link destination, or inside a line of a fenced
    /// code block or a table.
    fn holds(&self, at: usize) -> bool {
        self.in_code(at) || self.in_protected(at)
    }

    /// Whether the line holding `at` is in a fenced code block or a table.
    fn in_code(&self, at: usize) -> bool {
        let after = self.code.partition_point(|span| span.start <= at);
        after
            .checked_sub(1)
            .and_then(|last| self.code.get(last))
            .is_some_and(|span| at < span.end)
    }

    /// Whether `at` is strictly inside a protected span.
    fn in_protected(&self, at: usize) -> bool {
        let after = self.protected.partition_point(|span| span.start < at);
        after
            .checked_sub(1)
            .and_then(|last| self.protected.get(last))
            .is_some_and(|span| at < span.end)
    }

    /// The strength of the break at `at` that was found as `kind`: the
    /// whitespace before it decides a line break, a paragraph break, or a
    /// section break before a Markdown heading, and else it stays `kind`.
    fn strength(&self, text: &str, at: usize, kind: Strength) -> Strength {
        match newlines_before(text, at) {
            0 => kind,
            _ if self.opens_section(text, at) => Strength::Section,
            1 => Strength::Line,
            _ => Strength::Paragraph,
        }
    }

    /// Whether a Markdown heading line starts at `at`, outside code.
    fn opens_section(&self, text: &str, at: usize) -> bool {
        let heading = text.get(at..).is_some_and(|rest| {
            let hashes = rest.bytes().take_while(|&b| b == b'#').count();
            (1..=6).contains(&hashes) && rest.as_bytes().get(hashes) == Some(&b' ')
        });
        heading && text.get(..at).is_some_and(|head| head.ends_with('\n')) && !self.in_code(at)
    }
}

/// Where the break UAX #29 put at `at` lands: after the whitespace it leads
/// into. A break right after its terminator and before a run of `*`, `_`, `~`
/// or backticks moves past the run, the quotes and brackets after it, and the
/// whitespace after them, or is dropped (`None`) when text follows directly.
fn settle(text: &str, at: usize) -> Option<usize> {
    let mut end = at;
    if !text.get(..at)?.ends_with(char::is_whitespace) {
        let run = past(text, at, is_mark);
        if run > at {
            end = past(text, run, is_close);
            if !text.get(end..)?.starts_with(char::is_whitespace) {
                return None;
            }
        }
    }
    let end = past(text, end, char::is_whitespace);
    (end < text.len()).then_some(end)
}

/// The offset after the run of chars from `from` that satisfy `keep`.
fn past(text: &str, from: usize, keep: impl Fn(char) -> bool) -> usize {
    text.get(from..).map_or(from, |rest| {
        from + rest
            .chars()
            .take_while(|&c| keep(c))
            .map(char::len_utf8)
            .sum::<usize>()
    })
}

/// An emphasis or code mark that can close a sentence's last word.
fn is_mark(c: char) -> bool {
    matches!(c, '*' | '_' | '~' | '`')
}

/// UAX #29's `Close`, as far as prose uses it: quotes and brackets.
fn is_close(c: char) -> bool {
    matches!(
        c,
        '"' | '\''
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '«'
            | '»'
            | '\u{2018}'..='\u{201f}'
            | '\u{2039}'
            | '\u{203a}'
            | '\u{3008}'..='\u{3011}'
            | '\u{3014}'..='\u{301b}'
            | '\u{301d}'..='\u{301f}'
            | '\u{ff08}'
            | '\u{ff09}'
            | '\u{ff3b}'
            | '\u{ff3d}'
            | '\u{ff5b}'
            | '\u{ff5d}'
            | '\u{ff62}'
            | '\u{ff63}'
    )
}

fn is_closer(c: char) -> bool {
    is_mark(c) || is_close(c)
}

/// How many line breaks the whitespace just before `at` holds, up to two.
fn newlines_before(text: &str, at: usize) -> usize {
    text.get(..at).map_or(0, |head| {
        head.chars()
            .rev()
            .take_while(|c| c.is_whitespace())
            .filter(|&c| c == '\n')
            .take(2)
            .count()
    })
}

/// CLDR's abbreviations and the honorifics, and the byte length of the
/// longest.
static ABBREVIATIONS: Lazy<(HashSet<&'static str>, usize)> = Lazy::new(|| {
    let all: HashSet<&'static str> = CLDR_EN.iter().chain(&HONORIFICS).copied().collect();
    let longest = all.iter().map(|entry| entry.len()).max().unwrap_or(0);
    (all, longest)
});

/// Whether the text before `at`, without its trailing whitespace, ends in an
/// abbreviation of CLDR's list or the honorifics that starts a word.
fn suppressed(text: &str, at: usize) -> bool {
    let Some(before) = text.get(..at).map(str::trim_end) else {
        return false;
    };
    let (abbreviations, longest) = &*ABBREVIATIONS;
    before
        .char_indices()
        .rev()
        .take_while(|&(start, _)| before.len() - start <= *longest)
        .any(|(start, _)| {
            before
                .get(start..)
                .is_some_and(|tail| abbreviations.contains(tail))
                && before
                    .get(..start)
                    .and_then(|head| head.chars().next_back())
                    .is_none_or(|c| !c.is_alphanumeric() && c != '.')
        })
}

/// Whether the break at `at` follows `!` or `?`, any closers and the
/// whitespace, and the next char is lowercase: a sentence goes on. UAX #29
/// reads a lowercase letter that way after `.` only.
fn continues(text: &str, at: usize) -> bool {
    text.get(at..)
        .and_then(|rest| rest.chars().next())
        .is_some_and(char::is_lowercase)
        && text.get(..at).is_some_and(|before| {
            before
                .trim_end()
                .trim_end_matches(is_closer)
                .ends_with(['!', '?'])
        })
}

/// Where the ordered-list marker opening `line` (`1.`, `2)`, `1.)`) and the
/// whitespace after it end, if one does.
fn list_marker_end(line: &str) -> Option<usize> {
    let rest = line.trim_start_matches([' ', '\t']);
    let indent = line.len() - rest.len();
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let after_digits = rest.get(digits..)?;
    let stops = after_digits
        .bytes()
        .take_while(|b| matches!(b, b'.' | b')'))
        .count();
    let after_marker = after_digits.get(stops..)?;
    let space = after_marker.len() - after_marker.trim_start().len();
    (digits > 0 && stops > 0 && space > 0).then_some(indent + digits + stops + space)
}

/// The inline code spans of `line`: a backtick run to the next run of the
/// same length.
pub fn code_spans(line: &str) -> Vec<Span> {
    let bytes = line.as_bytes();
    let run = |from: usize| {
        bytes.get(from..).map_or(0, |rest| {
            rest.iter().take_while(|&&byte| byte == b'`').count()
        })
    };
    let tick = |from: usize| {
        bytes
            .get(from..)
            .and_then(|rest| rest.iter().position(|&byte| byte == b'`'))
            .map(|offset| from + offset)
    };
    let mut spans = Vec::new();
    let mut at = 0;
    while let Some(open) = tick(at) {
        let width = run(open);
        let mut next = open + width;
        at = next;
        while let Some(found) = tick(next) {
            let length = run(found);
            if length == width {
                spans.push(open..found + width);
                at = found + width;
                break;
            }
            next = found + length;
        }
    }
    spans
}

/// The link destinations of `line`: each `](` through the `)` that closes it.
fn link_destinations(line: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut at = 0;
    while let Some(open) = line
        .get(at..)
        .and_then(|rest| rest.find("]("))
        .map(|offset| at + offset)
    {
        let mut depth = 0usize;
        let end = line
            .bytes()
            .enumerate()
            .skip(open + 1)
            .find_map(|(index, byte)| {
                match byte {
                    b'(' => depth += 1,
                    b')' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return Some(index + 1);
                        }
                    }
                    _ => {}
                }
                None
            });
        match end {
            Some(end) => {
                spans.push(open..end);
                at = end;
            }
            None => at = open + 2,
        }
    }
    spans
}
