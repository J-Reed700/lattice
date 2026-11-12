//! One layer for cutting text: where it may be cut and how good each place is
//! ([`Boundaries`]), how big a piece is ([`Measure`]), and how pieces are
//! chosen under a budget ([`partition`], [`windows`]).
//! Design: docs/design/2026-11-02-text-segmentation.md.

mod align;
mod code;
mod fences;
mod measure;
mod pack;
mod prose;
mod suppressions;

#[cfg(test)]
mod align_tests;
#[cfg(test)]
mod fences_tests;
#[cfg(test)]
mod golden;
#[cfg(test)]
mod tests;

pub use align::{align, common_subsequence, Alignment, Fate};
pub use code::{is_blank, opens_item, Code};
pub(crate) use fences::block_content;
pub use fences::{code_fences, in_code_fence, CodeFence};
pub use measure::{Bytes, Chars, Measure, Words};
pub use pack::{partition, widen, windows, Overflow, Plan, Rung};
pub use prose::{code_spans, item_marker_end, Flowed, Prose};

/// A byte range of a text, both ends on char boundaries.
pub type Span = std::ops::Range<usize>;

/// How good a place a break is to cut, weakest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Strength {
    Char,
    Word,
    Clause,
    Sentence,
    Line,
    Paragraph,
    Section,
}

/// A place a text may be cut: between the bytes before `at` and the bytes
/// from `at`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Break {
    pub at: usize,
    pub strength: Strength,
}

/// Where a text may be cut. Every break is a char boundary strictly inside
/// the text, ascending, one per offset (its strongest kind). Char breaks are
/// implied at every char boundary and never listed.
pub trait Boundaries {
    fn breaks(&self, text: &str) -> Vec<Break>;
}

/// The pieces of `text` between its breaks of at least `at_least`. They tile
/// the text; the empty text has none.
pub fn units<B: Boundaries + ?Sized>(text: &str, boundaries: &B, at_least: Strength) -> Vec<Span> {
    if text.is_empty() {
        return Vec::new();
    }
    if at_least <= Strength::Char {
        return text
            .char_indices()
            .map(|(at, c)| at..at + c.len_utf8())
            .collect();
    }
    let mut spans = Vec::new();
    let mut start = 0;
    for cut in boundaries.breaks(text) {
        if cut.strength >= at_least {
            spans.push(start..cut.at);
            start = cut.at;
        }
    }
    spans.push(start..text.len());
    spans
}

/// The byte spans of `text`'s sentences, trimmed of surrounding whitespace,
/// in order and without the empty ones.
pub fn sentences(text: &str) -> Vec<(usize, usize)> {
    trimmed_spans(text, &Prose)
}

/// [`sentences`] of hard-wrapped text: a line break inside a paragraph cuts
/// only where it ends a sentence or opens a block ([`Flowed`]).
pub fn flowed_sentences(text: &str) -> Vec<(usize, usize)> {
    trimmed_spans(text, &Flowed)
}

fn trimmed_spans<B: Boundaries>(text: &str, boundaries: &B) -> Vec<(usize, usize)> {
    units(text, boundaries, Strength::Sentence)
        .into_iter()
        .filter_map(|span| {
            let piece = text.get(span.clone())?;
            let trimmed = piece.trim();
            let start = span.start + piece.len() - piece.trim_start().len();
            (!trimmed.is_empty()).then_some((start, start + trimmed.len()))
        })
        .collect()
}
