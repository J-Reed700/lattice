use super::*;
use serde_json::json;

fn sentence(field: &str, quote: &str) -> Protected {
    Protected {
        unit: 0,
        field: field.into(),
        quote: quote.into(),
    }
}

const VERIFIED: &str = "Water boils at 100 °C at sea level.";
const FAILED: &str = "Salt lowers the boiling point.";
const OTHER: &str = "Stir while it heats.";

fn section(body: &str) -> Value {
    json!({ "kind": "explanation", "title": "Boiling", "body": body })
}

#[test]
fn confinement_reverts_changes_outside_the_failed_sentences_and_keeps_protected_bytes() {
    let before = section(&format!("{VERIFIED} {FAILED} {OTHER}"));
    // The edit fixes the failed sentence, and also rewords the verified
    // one, the unrelated one and the title.
    let after = json!({
        "kind": "explanation",
        "title": "Boiling water",
        "body": "Water boils at 100 degrees at sea level. Salt raises the boiling point slightly. Stir it as it heats.",
    });
    let confined = confine(
        &before,
        &after,
        &[sentence("body", FAILED)],
        &[sentence("body", VERIFIED)],
    )
    .unwrap();
    assert_eq!(
        confined["body"],
        format!("{VERIFIED} Salt raises the boiling point slightly. {OTHER}")
    );
    assert_eq!(confined["title"], "Boiling", "no claim's field changed");
    assert_eq!(confined["kind"], "explanation");

    // The regenerated-section fallback is confined the same way: a section
    // returned whole keeps everything but the failed sentence.
    let regenerated = json!({
        "kind": "explanation",
        "title": "A new title",
        "body": format!("{VERIFIED} Salt barely raises the boiling point. {OTHER}"),
    });
    let confined = confine(
        &before,
        &regenerated,
        &[sentence("body", FAILED)],
        &[sentence("body", VERIFIED)],
    )
    .unwrap();
    assert_eq!(confined["title"], "Boiling");
    assert_eq!(confined["body"], regenerated["body"]);
    let rewritten_whole = section("Boiling happens at 100 °C. Salt barely raises it.");
    assert!(confine(
        &before,
        &rewritten_whole,
        &[sentence("body", FAILED)],
        &[sentence("body", VERIFIED)],
    )
    .is_err());
}

#[test]
fn an_edit_that_merges_a_failed_and_a_protected_sentence_is_rejected_and_charged() {
    let before = section(&format!("{VERIFIED} {FAILED}"));
    let merged = section("Water boils at 100 °C, a little higher with salt.");
    let error = confine(
        &before,
        &merged,
        &[sentence("body", FAILED)],
        &[sentence("body", VERIFIED)],
    )
    .unwrap_err();
    // `repair_sections` charges a rejected repair to its claims and keeps the
    // section as it was.
    assert!(
        super::super::super::repair_work::rejected(&error),
        "{error}"
    );
}

#[test]
fn a_removed_failed_sentence_and_an_inserted_qualifier_beside_it_are_kept() {
    let before = section(&format!("{VERIFIED} {FAILED} {OTHER}"));
    let removed = section(&format!("{VERIFIED} {OTHER}"));
    let confined = confine(&before, &removed, &[sentence("body", FAILED)], &[]).unwrap();
    assert_eq!(confined["body"], format!("{VERIFIED} {OTHER}"));

    let qualified = section(&format!(
        "{VERIFIED} {FAILED} In most kitchens it does not matter. {OTHER}"
    ));
    let confined = confine(&before, &qualified, &[sentence("body", FAILED)], &[]).unwrap();
    assert_eq!(confined, qualified);
}

#[test]
fn a_failed_sentence_repeated_outside_its_quote_is_repaired_where_it_repeats() {
    // The claim's quote (its inventory passage) holds only the first copy of
    // the failed sentence; the repair takes out both.
    let before = section(&format!("{FAILED} {OTHER} {FAILED} {VERIFIED}"));
    let quote = format!("{FAILED} {OTHER}");
    let removed = section(&format!("{OTHER} {VERIFIED}"));
    let confined = confine(
        &before,
        &removed,
        &[sentence("body", &quote)],
        &[sentence("body", VERIFIED)],
    )
    .unwrap();
    assert_eq!(confined["body"], format!("{OTHER} {VERIFIED}"));

    // A repeat inside a protected claim's quote stays as it was.
    let confined = confine(
        &before,
        &removed,
        &[sentence("body", &quote)],
        &[sentence("body", &format!("{FAILED} {VERIFIED}"))],
    )
    .unwrap();
    assert_eq!(confined["body"], format!("{OTHER} {FAILED} {VERIFIED}"));
}
