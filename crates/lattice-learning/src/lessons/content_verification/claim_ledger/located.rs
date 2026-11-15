//! A stored quote read as the claim's own sentence (claim sources design
//! §18.3). Claims located by older passes, or where the locator declined,
//! hold a whole passage; the review screen and the sync points narrow it
//! the way the next pass will, so they agree with the pass before it runs.

use lattice_core::domain::learning::claim_review::ClaimLocation;
use lattice_core::shared::text::{claim_sentence, sentences};

/// `location.quote` narrowed to the claim's sentence in `field_text` when the
/// stored quote is wider than one; the quote itself otherwise.
pub(crate) fn located_quote(location: &ClaimLocation, statement: &str, field_text: &str) -> String {
    located(location, statement, field_text).0
}

/// [`located_quote`] with its span in `field_text`: the sentence's own place
/// when narrowed, the stored span otherwise.
pub(crate) fn located(
    location: &ClaimLocation,
    statement: &str,
    field_text: &str,
) -> (String, Option<(usize, usize)>) {
    narrow(&location.quote, location.span, statement, field_text)
}

/// [`located`] for a `quote` that stands in `field_text` at `span`, when that
/// is known.
pub(crate) fn narrow(
    quote: &str,
    span: Option<(usize, usize)>,
    statement: &str,
    field_text: &str,
) -> (String, Option<(usize, usize)>) {
    let kept = || (quote.to_owned(), span);
    if sentences(quote).len() < 2 {
        return kept();
    }
    let within = span
        .filter(|&(start, end)| field_text.get(start..end) == Some(quote))
        .or_else(|| field_text.find(quote).map(|at| (at, at + quote.len())));
    let Some(within) = within else {
        return kept();
    };
    claim_sentence(field_text, within, statement)
        .and_then(|(start, end)| {
            field_text
                .get(start..end)
                .map(|sentence| (sentence.to_owned(), Some((start, end))))
        })
        .unwrap_or_else(kept)
}
