use super::*;

fn shown(
    before: &str,
    quote: &str,
    after: &str,
    clipped_start: bool,
    clipped_end: bool,
) -> ClaimExcerptDto {
    ClaimExcerptDto {
        before: before.into(),
        quote: quote.into(),
        after: after.into(),
        clipped_start,
        clipped_end,
    }
}

#[test]
fn the_sentence_either_side_of_the_quote_is_its_context() {
    assert_eq!(
        excerpt(
            "Rome grew. Carthage fell in 146 BC. Greece followed.",
            "Carthage fell in 146 BC."
        ),
        shown(
            "Rome grew. ",
            "Carthage fell in 146 BC.",
            " Greece followed.",
            false,
            false
        )
    );
}

#[test]
fn more_of_the_passage_beyond_the_context_is_marked_clipped() {
    assert_eq!(
        excerpt(
            "Rome grew. Carthage rose. Carthage fell in 146 BC. Greece followed. Rome ruled.",
            "Carthage fell in 146 BC."
        ),
        shown(
            "Carthage rose. ",
            "Carthage fell in 146 BC.",
            " Greece followed.",
            true,
            true
        )
    );
}

#[test]
fn a_quote_the_passage_does_not_hold_is_shown_alone() {
    assert_eq!(
        excerpt("Carthage fell in 146 BC.", "Rome burned"),
        shown("", "Rome burned", "", false, false)
    );
}

#[test]
fn a_quote_in_other_typography_is_shown_as_the_passage_words_it() {
    assert_eq!(
        excerpt(
            "Monarchs breed in the north. It\u{2019}s a long trip south. The return takes generations.",
            "It's a long trip south."
        ),
        shown(
            "Monarchs breed in the north. ",
            "It\u{2019}s a long trip south.",
            " The return takes generations.",
            false,
            false
        )
    );
}

#[test]
fn an_abbreviation_does_not_end_the_sentence_before() {
    assert_eq!(
        excerpt(
            "Dr. Smith tagged the monarchs. They flew south in October. Few returned.",
            "They flew south in October."
        ),
        shown(
            "Dr. Smith tagged the monarchs. ",
            "They flew south in October.",
            " Few returned.",
            false,
            false
        )
    );
}

#[test]
fn code_is_never_context() {
    assert_eq!(
        excerpt(
            "Run the tally.\n```\ncount = 3\n```\nThen record the result here.",
            "Then record the result here."
        ),
        shown("", "Then record the result here.", "", true, false)
    );
}

#[test]
fn a_fence_closes_only_on_its_own_marker() {
    assert_eq!(
        excerpt(
            "Show the fence itself:\n~~~\n```\n~~~\nRecord the result here. Then check it twice.",
            "Record the result here."
        ),
        shown(
            "",
            "Record the result here.",
            " Then check it twice.",
            true,
            false
        )
    );
}

#[test]
fn a_one_word_line_is_not_context() {
    assert_eq!(
        excerpt(
            "Summary\nThe tally ran twice. Then it stopped.",
            "The tally ran twice."
        ),
        shown("", "The tally ran twice.", " Then it stopped.", true, false)
    );
}

#[test]
fn a_rule_line_is_not_context() {
    assert_eq!(
        excerpt(
            "The tally ran twice.\n\n---\n\nIt found three.",
            "The tally ran twice."
        ),
        shown("", "The tally ran twice.", "", false, true)
    );
}

#[test]
fn prose_in_any_script_is_context() {
    assert_eq!(
        excerpt(
            "Монархи летят на юг. Они зимуют в Мексике. Весной они возвращаются.",
            "Они зимуют в Мексике."
        ),
        shown(
            "Монархи летят на юг. ",
            "Они зимуют в Мексике.",
            " Весной они возвращаются.",
            false,
            false
        )
    );
}

#[test]
fn a_quote_over_two_sentences_takes_both() {
    assert_eq!(
        excerpt(
            "One. Rome grew fast. Carthage fell. Greece followed. Two.",
            "Rome grew fast. Carthage fell."
        ),
        shown(
            "",
            "Rome grew fast. Carthage fell.",
            " Greece followed.",
            true,
            true
        )
    );
}

#[test]
fn a_passage_opens_with_its_first_sentence() {
    assert_eq!(
        opening(
            "Dr. Smith tagged monarchs that flew 2,500 miles. Some flew farther. Most did not."
        ),
        shown(
            "",
            "Dr. Smith tagged monarchs that flew 2,500 miles.",
            " Some flew farther.",
            false,
            true
        )
    );
}

#[test]
fn a_blank_passage_has_no_opening() {
    assert_eq!(opening("  \n "), ClaimExcerptDto::default());
}
