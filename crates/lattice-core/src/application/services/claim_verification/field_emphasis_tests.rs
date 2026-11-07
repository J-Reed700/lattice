use super::*;

#[test]
fn bold_field_labels_are_read_as_fields() {
    let reply = "**Reason:** The passage gives 1887, not 1850.\n**Source passage:** passage-0\n**Verdict:** **contradicted**";
    let text = without_field_emphasis(reply);
    assert_eq!(final_verdict(&text), Some(ClaimVerdict::Contradicted));
    assert_eq!(
        labeled_value(&text, &["Reason"]).as_deref(),
        Some("The passage gives 1887, not 1850.")
    );
    assert_eq!(
        located_quote(&text, &["The mill closed in 1887.".to_string()]).as_deref(),
        Some("The mill closed in 1887.")
    );
}

#[test]
fn a_label_in_bold_before_its_colon_is_read_as_a_field() {
    let text = without_field_emphasis(
        "**Reason**: Not stated.\n**Source passage**: passage-0\n**Verdict**: unsupported",
    );
    assert_eq!(final_verdict(&text), Some(ClaimVerdict::Unsupported));
}

/// Haiku opened 22 of 331 replies this way, and each read as no verdict.
#[test]
fn a_bold_opening_label_is_read_as_the_verdict() {
    let text = without_field_emphasis(
        "**Label:** unsupported\n\nReason: Not stated.\n\nSource quote: none",
    );
    assert_eq!(parse_verdict_word(&text), Some(ClaimVerdict::Unsupported));
    let text = without_field_emphasis("**Label**: **supported**\nReason: It says so.");
    assert_eq!(parse_verdict_word(&text), Some(ClaimVerdict::Supported));
}

#[test]
fn a_verdict_with_a_closing_full_stop_is_read() {
    assert_eq!(
        final_verdict("Reason: Not stated.\nSource passage: passage-0\nVerdict: unsupported."),
        Some(ClaimVerdict::Unsupported)
    );
}

#[test]
fn emphasis_in_prose_is_left_as_written() {
    let reply = "Reason: The **key** date is 1887.\nNote: *not a field*";
    assert_eq!(without_field_emphasis(reply), reply);
}

#[test]
fn a_supported_page_stands_on_a_cited_passage_and_elsewhere_on_any_other() {
    assert_eq!(page_outcome(ClaimVerdict::Supported, true), Outcome::Stands);
    assert_eq!(
        page_outcome(ClaimVerdict::Supported, false),
        Outcome::StandsElsewhere
    );
}

#[test]
fn a_page_that_does_not_support_the_claim_reads_the_same_whatever_the_citation() {
    for on_cited in [true, false] {
        assert_eq!(
            page_outcome(ClaimVerdict::Unsupported, on_cited),
            Outcome::Unproven
        );
        assert_eq!(
            page_outcome(ClaimVerdict::Contradicted, on_cited),
            Outcome::Refuted
        );
        assert_eq!(
            page_outcome(ClaimVerdict::Unverified, on_cited),
            Outcome::Unjudged
        );
    }
}
