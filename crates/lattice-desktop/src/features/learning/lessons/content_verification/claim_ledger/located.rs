//! A stored quote read as the claim's own sentence (claim sources design
//! §18.3). Claims located by older passes, or where the locator declined,
//! hold a whole passage; the review screen and the sync points narrow it
//! the way the next pass will, so they agree with the pass before it runs.

use lattice_core::domain::learning::claim_review::ClaimLocation;
use lattice_core::shared::text::{claim_sentence, sentences};

/// `location.quote` narrowed to the claim's sentence in `field_text` when the
/// stored quote is wider than one; the quote itself otherwise.
pub(in crate::features::learning) fn located_quote(
    location: &ClaimLocation,
    statement: &str,
    field_text: &str,
) -> String {
    located(location, statement, field_text).0
}

/// [`located_quote`] with its span in `field_text`: the sentence's own place
/// when narrowed, the stored span otherwise.
pub(in crate::features::learning) fn located(
    location: &ClaimLocation,
    statement: &str,
    field_text: &str,
) -> (String, Option<(usize, usize)>) {
    let quote = &location.quote;
    let kept = || (quote.clone(), location.span);
    if sentences(quote).len() < 2 {
        return kept();
    }
    let within = location
        .span
        .filter(|&(start, end)| field_text.get(start..end) == Some(quote.as_str()))
        .or_else(|| {
            field_text
                .find(quote.as_str())
                .map(|at| (at, at + quote.len()))
        });
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
