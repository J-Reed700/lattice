//! `Code`: where a source file may be cut, by its lines.

use super::{Boundaries, Break, Strength};

/// Source files and other line-oriented text. A line that opens a top-level
/// item (and whose predecessor does not) is a Section break, a line after a
/// blank one a Paragraph break, every other line start a Line break, and the
/// end of each whitespace run inside a line a Word break. Blank lines stay
/// with the piece before them.
pub struct Code;

impl Boundaries for Code {
    fn breaks(&self, text: &str) -> Vec<Break> {
        let mut breaks = Vec::new();
        let mut previous: Option<&str> = None;
        let mut start = 0;
        for raw in text.split_inclusive('\n') {
            let line = raw.trim_end_matches(['\n', '\r']);
            if let Some(strength) = previous
                .filter(|_| !is_blank(line))
                .map(|previous| strength_before(previous, line))
            {
                breaks.push(Break {
                    at: start,
                    strength,
                });
            }
            breaks.extend(word_starts(line).map(|at| Break {
                at: start + at,
                strength: Strength::Word,
            }));
            previous = Some(line);
            start += raw.len();
        }
        breaks
    }
}

/// How good a place the start of `line` is to cut, after `previous`.
fn strength_before(previous: &str, line: &str) -> Strength {
    if opens_item(line) && !opens_item(previous) {
        Strength::Section
    } else if is_blank(previous) {
        Strength::Paragraph
    } else {
        Strength::Line
    }
}

/// Where each whitespace run of `line` ends.
fn word_starts(line: &str) -> impl Iterator<Item = usize> + '_ {
    let mut in_space = false;
    line.char_indices().filter_map(move |(at, c)| {
        let space = c.is_whitespace();
        let starts = in_space && !space;
        in_space = space;
        starts.then_some(at)
    })
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// A line at indentation 0 that opens a top-level item, or leads into one
/// (an attribute, a doc comment, a decorator).
fn opens_item(line: &str) -> bool {
    const OPENERS: [&str; 26] = [
        "fn ",
        "pub ",
        "pub(",
        "impl ",
        "impl<",
        "struct ",
        "enum ",
        "trait ",
        "mod ",
        "unsafe ",
        "async ",
        "class ",
        "def ",
        "function ",
        "export ",
        "const ",
        "interface ",
        "type ",
        "func ",
        "module ",
        "#[",
        "///",
        "//!",
        "/**",
        "@",
        "#!",
    ];
    if line.starts_with(char::is_whitespace) {
        return false;
    }
    if OPENERS.iter().any(|opener| line.starts_with(opener)) {
        return true;
    }
    // A Markdown heading: one to six `#` and a space.
    let hashes = line.chars().take_while(|&c| c == '#').count();
    (1..=6).contains(&hashes) && line.chars().nth(hashes) == Some(' ')
}
