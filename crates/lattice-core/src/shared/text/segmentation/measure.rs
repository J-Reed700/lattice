//! How big a piece is: bytes, characters or words.

use crate::shared::text::floor_char_boundary;

/// How big a piece of text is.
pub trait Measure {
    /// The furthest char boundary `end >= start` whose piece `start..end`
    /// measures at most `max`, clamped to the text. A `start` off a char
    /// boundary is floored to one.
    fn reach(&self, text: &str, start: usize, max: usize) -> usize;
}

/// Pieces measured in UTF-8 bytes.
#[derive(Clone, Copy, Debug)]
pub struct Bytes;

/// Pieces measured in Unicode scalar values.
#[derive(Clone, Copy, Debug)]
pub struct Chars;

/// Pieces measured in words: maximal runs of non-whitespace, counted in the
/// piece where they start. A piece ends before the first byte of the word
/// after its last, so the whitespace after that word stays with it.
#[derive(Clone, Copy, Debug)]
pub struct Words;

impl Measure for Bytes {
    fn reach(&self, text: &str, start: usize, max: usize) -> usize {
        let start = floor_char_boundary(text, start);
        floor_char_boundary(text, start.saturating_add(max))
    }
}

impl Measure for Chars {
    fn reach(&self, text: &str, start: usize, max: usize) -> usize {
        let start = floor_char_boundary(text, start);
        let rest = text.get(start..).unwrap_or_default();
        if max >= rest.len() {
            return text.len();
        }
        rest.char_indices()
            .nth(max)
            .map_or(text.len(), |(offset, _)| start + offset)
    }
}

impl Measure for Words {
    fn reach(&self, text: &str, start: usize, max: usize) -> usize {
        let start = floor_char_boundary(text, start);
        let rest = text.get(start..).unwrap_or_default();
        let mut in_space = text
            .get(..start)
            .and_then(|head| head.chars().next_back())
            .is_none_or(char::is_whitespace);
        let mut seen = 0usize;
        for (offset, c) in rest.char_indices() {
            let space = c.is_whitespace();
            if in_space && !space {
                if seen == max {
                    return start + offset;
                }
                seen += 1;
            }
            in_space = space;
        }
        text.len()
    }
}
