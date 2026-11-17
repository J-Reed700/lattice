//! A quote in its passage as the review shows it (UI design §3.4).

use std::ops::Range;

use lattice_core::shared::text::same_text;
use lattice_core::shared::text::segmentation::{code_fences, CodeFence};
use lattice_core::shared::text::sentences;

use super::super::dto::ClaimExcerptDto;

/// `quote` in `text`: the sentences that hold it, with the sentence before
/// and after them when it is prose. The quote alone when `text` does not
/// hold it.
pub(super) fn excerpt(text: &str, quote: &str) -> ClaimExcerptDto {
    match same_text::find(text, quote) {
        Some(span) => around(text, span),
        None => ClaimExcerptDto {
            quote: quote.to_owned(),
            ..ClaimExcerptDto::default()
        },
    }
}

/// The first sentence of `text` in its excerpt; empty when `text` has none.
pub(super) fn opening(text: &str) -> ClaimExcerptDto {
    sentences(text)
        .first()
        .map_or_else(ClaimExcerptDto::default, |&span| around(text, span))
}

fn around(text: &str, (from, to): (usize, usize)) -> ClaimExcerptDto {
    let fences: Vec<Range<usize>> = code_fences(text).iter().map(CodeFence::whole).collect();
    let prose = |&(start, end): &(usize, usize)| {
        text.get(start..end).is_some_and(|piece| words(piece) >= 2)
            && !fences
                .iter()
                .any(|fence| start < fence.end && end > fence.start)
    };
    let bounds = sentences(text);
    let first = bounds.iter().position(|&(_, end)| end > from);
    let last = bounds.iter().position(|&(_, end)| end >= to);
    let start = first
        .map_or(from, |first| {
            first
                .checked_sub(1)
                .and_then(|previous| bounds.get(previous))
                .filter(|sentence| prose(*sentence))
                .or_else(|| bounds.get(first))
                .map_or(from, |&(start, _)| start)
        })
        .min(from);
    let end = last
        .map_or(to, |last| {
            bounds
                .get(last + 1)
                .filter(|sentence| prose(*sentence))
                .or_else(|| bounds.get(last))
                .map_or(to, |&(_, end)| end)
        })
        .max(to);
    let piece = |range: Range<usize>| text.get(range).unwrap_or_default().to_owned();
    ClaimExcerptDto {
        before: piece(start..from),
        quote: piece(from..to),
        after: piece(to..end),
        clipped_start: !text.get(..start).unwrap_or_default().trim().is_empty(),
        clipped_end: !text.get(end..).unwrap_or_default().trim().is_empty(),
    }
}

/// How many words `piece` has: runs of letters, digits and '_'.
fn words(piece: &str) -> usize {
    piece
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .count()
}

#[cfg(test)]
#[path = "excerpt_tests.rs"]
mod tests;
