use super::*;

#[test]
fn lines_inside_a_longer_fence_have_no_keys() {
    assert!(keys("````md\n```\nRepeat this line of text.\n````").is_empty());
}

#[test]
fn consecutive_repeats_catch_short_duplicates_and_stutters_in_prose_and_list_items() {
    let short_prose = consecutive_repeats("Check MSA-2. Check MSA-2.");
    assert_eq!(short_prose.len(), 1);
    assert!(!short_prose[0].prefix_stutter);
    assert_eq!(short_prose[0].second, "Check MSA-2.");

    let list_stutter = consecutive_repeats(
        "4. Send MSH-10.\n5. Write the acknowledgement you expect back. Write the acknowledgement you expect back, and inspect MSA-2.",
    );
    assert_eq!(list_stutter.len(), 1);
    assert!(list_stutter[0].prefix_stutter);
    assert_eq!(
        list_stutter[0].first,
        "Write the acknowledgement you expect back."
    );
    assert_eq!(
        list_stutter[0].second,
        "Write the acknowledgement you expect back, and inspect MSA-2."
    );

    let adjacent_items = consecutive_repeats("- Send the ACK.\n- Send the ACK, and record MSA-2.");
    assert_eq!(adjacent_items.len(), 1);
    assert!(adjacent_items[0].prefix_stutter);

    let em_dash =
        consecutive_repeats("Check MSA-2 \u{2014} now. Check MSA-2 \u{2014} now, and log it.");
    assert_eq!(em_dash.len(), 1);
    assert!(em_dash[0].prefix_stutter);

    assert!(consecutive_repeats("```text\nCheck MSA-2. Check MSA-2.\n```").is_empty());
    assert!(consecutive_repeats("It fails. It fails when exp expires.").is_empty());
    assert!(consecutive_repeats(
        "**Client assertion lifetimes**\n\nClient assertion lifetimes remain short."
    )
    .is_empty());
}
