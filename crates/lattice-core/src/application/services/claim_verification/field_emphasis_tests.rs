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
