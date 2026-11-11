//! Whether the lesson's own words bear out a quote the check gives.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

fn found(quote: &str) -> bool {
    let lesson = json!({"blocks":[{"kind":"explanation","title":"Opening",
        "body":"The tool prints the working folder when it runs."}],"questions":[]});
    locate(&parts_of(&lesson)[0], quote).is_some()
}

#[test]
fn a_quote_that_differs_only_in_the_case_of_a_letter_is_found_in_the_part() {
    assert!(found("Prints the working folder"));
    assert!(found("prints the working folder"));
}

#[test]
fn a_quote_in_other_words_is_still_not_found() {
    assert!(!found("Prints the current directory"));
}
