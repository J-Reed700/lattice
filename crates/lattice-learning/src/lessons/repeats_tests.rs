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

    let dup_fallback = consecutive_repeats(
        "If the EHR rejects system/Binary.c, fall back to standard documentation guidance. If the EHR rejects system/Binary.w, fall back to standard documentation guidance.",
    );
    assert_eq!(dup_fallback.len(), 1);
    assert!(dup_fallback[0].prefix_stutter);

    let similar_stutter = consecutive_repeats(
        "This prevents multiple connectors from retrying at the exact same moment. This prevents thundering herds if multiple connectors hit the limit simultaneously.",
    );
    assert_eq!(similar_stutter.len(), 1);
    assert!(similar_stutter[0].prefix_stutter);

    assert!(consecutive_repeats(
        "A long explanation of the idea aa, with no figures in it. A long explanation of the idea ab, with no figures in it."
    )
    .is_empty());

    let two_sentence_block = "6. Write unit tests verifying the retry behavior.\n\nTo implement this in NestJS, create an HTTP interceptor that wraps your outgoing requests. In your unit tests, mock the HTTP client to simulate 429 responses.\nTo implement this in NestJS, create an HTTP interceptor that wraps your outgoing requests. In your unit tests, mock the HTTP client to simulate 429 responses.";
    let period_two = consecutive_repeats(two_sentence_block);
    assert_eq!(period_two.len(), 1, "{period_two:?}");
    assert!(!period_two[0].prefix_stutter);

    let unindented_after_list =
        "6. Write unit tests verifying the retry behavior.\nTo implement this in NestJS, create an HTTP interceptor that wraps your outgoing requests.";
    let extracted = comparable(unindented_after_list);
    assert_eq!(
        extracted,
        vec![
            "To implement this in NestJS, create an HTTP interceptor that wraps your outgoing requests."
        ]
    );

    assert!(consecutive_repeats(
        "When an access token is missing or expired, return HTTP 401 Unauthorized. When a token lacks the required scope, return HTTP 403 Forbidden."
    )
    .is_empty());

    let three_sentence_cycle = "Validate the JWT signature first. Verify the token expiration timestamp next. Confirm the required FHIR scopes match.\nValidate the JWT signature first. Verify the token expiration timestamp next. Confirm the required FHIR scopes match.";
    let cycle_repeats = consecutive_repeats(three_sentence_cycle);
    assert_eq!(cycle_repeats.len(), 1, "{cycle_repeats:?}");
    assert!(!cycle_repeats[0].prefix_stutter);

    let trailing_clause = "athenahealth supports HL7 FHIR Standard for Trial Use Release 2.0 and CCDA 2.1, with user authorization supported by SMART on FHIR and OAuth 2.0. It also mentions that user authorization is supported by SMART on FHIR and OAuth 2.0.";
    let tail_repeats = consecutive_repeats(trailing_clause);
    assert_eq!(tail_repeats.len(), 1, "{tail_repeats:?}");
    assert!(tail_repeats[0].prefix_stutter);
}
