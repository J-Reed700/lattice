use super::super::lexical::lexical_pass;
use super::super::test_support::source;
use super::*;

const RUNBOOK: &str = "Hold the mitigation for 20 seconds. Once the hold expires with the band \
    still broken, page Marisol Quenby and quote TCK-1836. Never raise a second ticket for the \
    same incident while the first one is open.";

fn cited(content: &str, citation_id: u32) -> SourceDto {
    SourceDto {
        citation_id: Some(citation_id),
        ..source(content)
    }
}

fn flagged(answer: &str, sources: &[SourceDto]) -> Vec<String> {
    let claims = lexical_pass(answer, sources);
    let misquoted = misquoted_claims(answer, sources, &claims);
    let mut sentences: Vec<String> = misquoted
        .into_iter()
        .map(|index| claims[index].claim_text.clone())
        .collect();
    sentences.sort();
    sentences
}

/// The sentence that started this: right ticket, wrong person, in no document.
#[test]
fn a_made_up_quotation_is_found_out() {
    let answer = "The runbook says \"page Anneke Voskuil and quote TCK-1836\" [1].";
    assert_eq!(flagged(answer, &[cited(RUNBOOK, 1)]).len(), 1);
}

#[test]
fn the_words_of_the_passage_are_left_to_the_judge() {
    let answer = "The runbook says \"page Marisol Quenby and quote TCK-1836\" [1].";
    assert!(flagged(answer, &[cited(RUNBOOK, 1)]).is_empty());
}

/// The splitter cuts a claim at a full stop inside the marks, which leaves the
/// quotation's closing mark and its marker in a "sentence" of their own.
#[test]
fn a_full_stop_inside_the_marks_changes_nothing() {
    let answer = "The runbook says \"page Anneke Voskuil and quote TCK-1836.\" [1]";
    assert_eq!(flagged(answer, &[cited(RUNBOOK, 1)]).len(), 1);
}

/// Over two sentences, the true one is not marked for the false one's sake.
#[test]
fn only_the_sentence_that_is_wrong_is_marked() {
    let answer = "It reads: \"Page Anneke Voskuil and quote TCK-1836. Never raise a second \
                  ticket for the same incident while the first one is open.\" [1]";
    let marked = flagged(answer, &[cited(RUNBOOK, 1)]);
    assert_eq!(marked.len(), 1, "{marked:?}");
    assert!(marked[0].contains("Voskuil"), "{marked:?}");
}

/// A quotation in another passage is still not in the one a reader is sent to.
#[test]
fn the_passage_it_cites_is_the_one_that_counts() {
    let answer = "The runbook says \"page Marisol Quenby and quote TCK-1836\" [2].";
    let sources = [
        cited(RUNBOOK, 1),
        cited("Parental leave runs for a year.", 2),
    ];
    assert_eq!(flagged(answer, &sources).len(), 1);
}

/// `[1] "..." [2] "..." [3]`: a quotation labelled first has the next one's
/// label behind it, and is in the passage before it.
#[test]
fn a_marker_right_before_a_quotation_counts() {
    let answer = "[1] \"page Marisol Quenby and quote TCK-1836\" [2] \"Parental leave runs for \
                  a whole year.\" [3]";
    let sources = [
        cited(RUNBOOK, 1),
        cited("Parental leave runs for a whole year.", 2),
        cited("Something else entirely is written on this page.", 3),
    ];
    assert!(flagged(answer, &sources).is_empty());
}

/// A document's title in marks, cited at the end of its sentence, is in no
/// passage's text, and is not a quotation of one.
#[test]
fn a_marker_further_along_the_sentence_does_not_count() {
    let answer = "See \"Platform operations handbook, structured edition\" for the details [1].";
    assert!(flagged(answer, &[cited(RUNBOOK, 1)]).is_empty());
}

#[test]
fn a_line_break_a_hyphen_and_typography_are_not_different_words() {
    let passage = "The supplier\nshall main-\ntain the \u{201c}ﬁnal\u{201d} register at all sites.";
    let answer = "It says \u{201c}The supplier shall maintain the \"final\" register\u{201d} [1].";
    assert!(flagged(answer, &[cited(passage, 1)]).is_empty());
}

#[test]
fn an_ellipsis_and_a_marked_edit_leave_words_out() {
    let answer = "It says \"Hold the mitigation for 20 seconds... page Marisol Quenby and quote \
                  TCK-1836\" [1], and \"[t]he hold expires with the band [must] still broken, \
                  page Marisol Quenby\" [1].";
    let sources = [cited(
        format!("{RUNBOOK} The hold expires with the band ").as_str(),
        1,
    )];
    assert!(flagged(answer, &sources).is_empty());
}

#[test]
fn a_short_quotation_is_a_term() {
    let answer = "The runbook calls this a \"standing band breach\" [1].";
    assert!(flagged(answer, &[cited(RUNBOOK, 1)]).is_empty());
}

/// `6"` must not open a quotation, or the marks after it pair the wrong way
/// round and the words between two quotations read as one.
#[test]
fn an_inch_mark_does_not_pair_with_a_quotation() {
    let answer = "The 6\" pipe is covered by \"page Marisol Quenby and quote TCK-1836\" [1].";
    assert!(flagged(answer, &[cited(RUNBOOK, 1)]).is_empty());
}

/// Nothing can be missing from a page with no saved text, and a marker that
/// points at nothing is the citation check's to report.
#[test]
fn an_unreadable_passage_settles_nothing() {
    let answer = "The runbook says \"page Anneke Voskuil and quote TCK-1836\" [1].";
    assert!(flagged(answer, &[cited("", 1)]).is_empty());
    assert!(flagged(answer, &[cited(RUNBOOK, 2)]).is_empty());
}

#[test]
fn a_fenced_block_is_not_prose() {
    let answer = "```\nprint(\"page Anneke Voskuil and quote TCK-1836\") [1]\n```";
    assert!(flagged(answer, &[cited(RUNBOOK, 1)]).is_empty());
}
