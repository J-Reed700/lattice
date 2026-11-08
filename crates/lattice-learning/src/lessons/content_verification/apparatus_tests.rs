//! A lesson's text never holds the schema's citation apparatus: a source
//! mentioned by the schema's index, a list of bare quotations, and the
//! heading over such a list.
use super::super::strip_citation_apparatus;

const LIST: &str = "**Citations:**\n*   \"Energy refers to a combination of loudness and density.\" (SourceIndex: 0)\n*   \"Segments with high energy are loud and dense.\" (SourceIndex: 0)";

#[test]
fn a_trailing_citations_list_and_its_heading_go_and_the_text_above_stays() {
    let body = "Tension builds when a section holds back.\n\nA release comes with full intensity.";
    assert_eq!(strip_citation_apparatus(&format!("{body}\n\n{LIST}")), body);
}

#[test]
fn an_inline_mention_goes_with_its_space_and_the_rest_stays() {
    assert_eq!(
        strip_citation_apparatus(
            "Energy is loudness and density (SourceIndex: 0). It builds tension."
        ),
        "Energy is loudness and density. It builds tension."
    );
    assert_eq!(
        strip_citation_apparatus(
            "Keep  two  spaces.\n\n\nAnd\ttabs stay (SourceIndex: 2)\nnext line."
        ),
        "Keep  two  spaces.\n\n\nAnd\ttabs stay\nnext line."
    );
}

#[test]
fn every_spelling_of_the_mention_is_removed() {
    for mention in [
        "(SourceIndex: 0)",
        "(sourceIndex 2)",
        "(Source Index: 1)",
        "[SourceIndex: 3]",
        "(SOURCEINDEX:4)",
        "(sourceindex: 12)",
        "[source index 5]",
        "(SourceIndex:7)",
    ] {
        assert_eq!(
            strip_citation_apparatus(&format!("A claim {mention} holds.")),
            "A claim holds.",
            "{mention}"
        );
    }
}

#[test]
fn quotation_items_go_whole_in_any_quote_style() {
    let text = "Teaching text.\n\n**Sources:**\n- “A curly quotation.” (SourceIndex: 1)\n- \"…an ellipsis inside…\" [SourceIndex: 2]\n- \"And one more.\" (sourceIndex 0)";
    assert_eq!(strip_citation_apparatus(text), "Teaching text.");
    assert_eq!(
        strip_citation_apparatus(
            "Before.\n\n- Keep this point.\n- \"A quotation.\" (SourceIndex: 0)\n- Keep this too.\n\nAfter."
        ),
        "Before.\n\n- Keep this point.\n- Keep this too.\n\nAfter."
    );
}

#[test]
fn each_heading_form_goes_with_its_removed_list() {
    for heading in [
        "**Citations:**",
        "Citations:",
        "### Citations",
        "**Sources:**",
        "References:",
    ] {
        assert_eq!(
            strip_citation_apparatus(&format!(
                "Teaching text.\n\n{heading}\n\n- \"A quotation.\" (SourceIndex: 0)\n\n- \"Another.\" (SourceIndex: 1)"
            )),
            "Teaching text.",
            "{heading}"
        );
    }
}

#[test]
fn a_list_that_mixes_a_quotation_with_a_teaching_item_keeps_the_teaching_item_and_its_heading() {
    let text = "Contrast sharpens a drop.\n\n**Sources:**\n*   \"Segments with high energy are loud.\" (SourceIndex: 0)\n*   **Purpose:** To heighten contrast.";
    assert_eq!(
        strip_citation_apparatus(text),
        "Contrast sharpens a drop.\n\n**Sources:**\n*   **Purpose:** To heighten contrast."
    );
}

#[test]
fn prose_about_sources_and_indexes_without_a_number_is_unchanged() {
    for text in [
        "Add an index on the source column.",
        "The book's source index lists every cited work.",
        "See the source (index) and [source index] for the list.",
        "Each source has a number (see source 2, index 3).",
        "Citations: are covered in the next lesson.",
        "**Sources:**\n- Primary documents matter most.\n- Secondary summaries come after.",
    ] {
        assert_eq!(strip_citation_apparatus(text), text);
    }
}

#[test]
fn code_is_untouched_in_fences_and_inline_spans() {
    let text = "Read the field:\n\n```python\nrow = {\"SourceIndex\": 0}\nprint(\"SourceIndex: 0\")  # (SourceIndex: 0)\n```\n\n```text\n**Citations:**\n*   \"A quotation.\" (SourceIndex: 0)\n```\n\nCall `rec(SourceIndex: 0)` or `[SourceIndex: 1]` to set it.";
    assert_eq!(strip_citation_apparatus(text), text);
    assert_eq!(
        strip_citation_apparatus(&format!("{text} Also this (SourceIndex: 3).")),
        format!("{text} Also this.")
    );
}

#[test]
fn crlf_text_loses_the_same_and_every_remaining_line_keeps_its_crlf() {
    assert_eq!(
        strip_citation_apparatus(
            "First line (SourceIndex: 0).\r\nSecond line.\r\n\r\n**Citations:**\r\n*   \"Quote one.\" (SourceIndex: 0)\r\n*   \"Quote two.\" (SourceIndex: 0)"
        ),
        "First line.\r\nSecond line."
    );
    assert_eq!(
        strip_citation_apparatus(
            "Intro (SourceIndex: 1).\r\nMiddle line.\r\n\r\n- Keep this.\r\n- \"A quotation.\" (SourceIndex: 0)\r\n\r\nLast words."
        ),
        "Intro.\r\nMiddle line.\r\n\r\n- Keep this.\r\n\r\nLast words."
    );
}

#[test]
fn a_text_that_would_become_empty_is_left_as_written() {
    assert_eq!(strip_citation_apparatus(LIST), LIST);
    assert_eq!(
        strip_citation_apparatus("(SourceIndex: 0)"),
        "(SourceIndex: 0)"
    );
}

#[test]
fn a_text_without_apparatus_is_unchanged() {
    let text =
        "Plain teaching.\r\n\r\n  - An indented item\n\n\n> A quoted line \"with quotes\" stays.\n";
    assert_eq!(strip_citation_apparatus(text), text);
}

#[test]
fn a_quotation_item_that_names_no_source_stays() {
    let text = "Sayings to know:\n\n- \"Measure twice, cut once.\"\n- \"A stitch in time.\" (the old saying)\n";
    assert_eq!(strip_citation_apparatus(text), text);
    assert_eq!(
        strip_citation_apparatus(
            "Sayings:\n\n- \"Measure twice.\"\n- \"Cut once.\" (SourceIndex: 0)"
        ),
        "Sayings:\n\n- \"Measure twice.\""
    );
}
