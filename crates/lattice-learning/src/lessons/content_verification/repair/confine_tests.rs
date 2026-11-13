use super::*;
use serde_json::json;

fn sentence(field: &str, quote: &str) -> Protected {
    Protected {
        unit: 0,
        field: field.into(),
        quote: quote.into(),
        released: false,
    }
}

const VERIFIED: &str = "Water boils at 100 °C at sea level.";
const FAILED: &str = "Salt lowers the boiling point.";
const OTHER: &str = "Stir the water while it heats.";

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
    .unwrap()
    .section;
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
    .unwrap()
    .section;
    assert_eq!(confined["title"], "Boiling");
    assert_eq!(confined["body"], regenerated["body"]);
    // A section rewritten whole shares one change between the failed sentence
    // and the passed one: it stands as it was, and the failed sentence is put
    // back.
    let rewritten_whole = section("Boiling happens at 100 °C. Salt barely raises it.");
    let confined = confine(
        &before,
        &rewritten_whole,
        &[sentence("body", FAILED)],
        &[sentence("body", VERIFIED)],
    )
    .unwrap();
    assert_eq!(confined.section, before);
    assert_eq!(confined.put_back.len(), 1);
    assert_eq!(confined.put_back[0].target, sentence("body", FAILED));
}

/// What a correction is told when it was put back for a change it shared with
/// the sentences `passed`.
fn put_back_for(passed: &str) -> String {
    format!("The correction also changed a sentence it was not asked to change (\"{passed}\"), so it was put back; change only this sentence.")
}

#[test]
fn an_edit_that_merges_a_failed_and_a_protected_sentence_is_put_back_and_its_target_is_listed_with_why(
) {
    let before = section(&format!("{VERIFIED} {FAILED}"));
    let merged = section("Water boils at 100 °C, a little higher with salt.");
    let failed = sentence("body", FAILED);

    let confined = confine(
        &before,
        &merged,
        std::slice::from_ref(&failed),
        &[sentence("body", VERIFIED)],
    )
    .unwrap();

    // The merged change is put back whole, and the section stands as it was.
    assert_eq!(confined.section, before);
    assert_eq!(
        confined.put_back,
        vec![PutBack {
            target: failed,
            reason: put_back_for(VERIFIED),
        }]
    );
}

#[test]
fn only_the_targets_a_put_back_change_touched_are_listed_and_the_others_corrections_stand() {
    const SECOND_FAILED: &str = "Stirring makes the water boil sooner.";
    const FIXED: &str = "Salt raises the boiling point slightly.";
    let before = section(&format!("{FAILED} {OTHER} {VERIFIED} {SECOND_FAILED}"));
    // The first failed sentence is corrected on its own; the second is merged
    // with the passed sentence before it.
    let after = section(&format!(
        "{FIXED} {OTHER} Water boils at 100 °C at sea level whether or not it is stirred."
    ));
    let (first, second) = (sentence("body", FAILED), sentence("body", SECOND_FAILED));

    let confined = confine(
        &before,
        &after,
        &[first, second.clone()],
        &[sentence("body", VERIFIED)],
    )
    .unwrap();

    assert_eq!(
        confined.section["body"],
        format!("{FIXED} {OTHER} {VERIFIED} {SECOND_FAILED}")
    );
    assert_eq!(
        confined.put_back,
        vec![PutBack {
            target: second,
            reason: put_back_for(VERIFIED),
        }]
    );
}

#[test]
fn every_passed_sentence_a_put_back_change_rewrote_is_named_in_its_reason() {
    let before = section(&format!("{VERIFIED} {FAILED} {OTHER}"));
    let merged = section("Water boils at 100 °C, and salt changes that as you stir.");

    let confined = confine(
        &before,
        &merged,
        &[sentence("body", FAILED)],
        &[sentence("body", VERIFIED), sentence("body", OTHER)],
    )
    .unwrap();

    assert_eq!(confined.section, before);
    assert_eq!(
        confined.put_back[0].reason,
        put_back_for(&format!("{VERIFIED}\", \"{OTHER}"))
    );
}

#[test]
fn a_removed_failed_sentence_and_an_inserted_qualifier_beside_it_are_kept() {
    let before = section(&format!("{VERIFIED} {FAILED} {OTHER}"));
    let removed = section(&format!("{VERIFIED} {OTHER}"));
    let confined = confine(&before, &removed, &[sentence("body", FAILED)], &[])
        .unwrap()
        .section;
    assert_eq!(confined["body"], format!("{VERIFIED} {OTHER}"));

    let qualified = section(&format!(
        "{VERIFIED} {FAILED} In most kitchens it does not matter. {OTHER}"
    ));
    let confined = confine(&before, &qualified, &[sentence("body", FAILED)], &[])
        .unwrap()
        .section;
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
    .unwrap()
    .section;
    assert_eq!(confined["body"], format!("{OTHER} {VERIFIED}"));

    // A repeat inside a protected claim's quote stays as it was.
    let confined = confine(
        &before,
        &removed,
        &[sentence("body", &quote)],
        &[sentence("body", &format!("{FAILED} {VERIFIED}"))],
    )
    .unwrap()
    .section;
    assert_eq!(confined["body"], format!("{OTHER} {FAILED} {VERIFIED}"));
}

#[test]
fn a_failed_sentence_is_repaired_beside_verified_ones_in_the_same_passage() {
    // The case that broke (claim sources design §18.1): one biology
    // paragraph, the extractor's one passage, holds a failed claim between
    // two verified ones. Each is located through `fields::locate` from the
    // passage and its statement.
    use crate::lessons::content_verification::locate;
    const PASSAGE: &str = "Mitochondria produce most of a cell's ATP through oxidative phosphorylation. They carry their own circular DNA, passed down from the mother. A typical human cell holds exactly twelve mitochondria. Their inner membrane folds into cristae that enlarge its surface.";
    const TWELVE: &str = "A typical human cell holds exactly twelve mitochondria.";
    let before = section(PASSAGE);
    let lesson = json!({ "blocks": [before.clone()], "questions": [] });
    let located = |statement: &str| {
        let location = locate(&lesson, 0, PASSAGE, Some(statement));
        sentence(&location.field, &location.quote)
    };
    let v1 = located("Mitochondria make most of a cell's ATP by oxidative phosphorylation.");
    let failed = located("A typical human cell contains exactly twelve mitochondria.");
    let v2 =
        located("The inner mitochondrial membrane folds into cristae that enlarge its surface.");
    assert_eq!(failed, sentence("body", TWELVE));
    assert!(v1.quote.starts_with("Mitochondria produce") && v2.quote.starts_with("Their inner"));

    let fixed = PASSAGE.replace(
        TWELVE,
        "A human cell holds hundreds to thousands of mitochondria.",
    );
    let after = section(&fixed);
    let confined = confine(
        &before,
        &after,
        std::slice::from_ref(&failed),
        &[v1.clone(), v2.clone()],
    )
    .unwrap();
    assert_eq!(confined.section["body"], fixed, "the repair is accepted");
    assert!(confined.put_back.is_empty());
    for verified in [&v1, &v2] {
        assert!(confined.section["body"]
            .as_str()
            .unwrap()
            .contains(&verified.quote));
    }

    // A verified claim the locator could not narrow keeps its passage, which
    // holds the failed sentence too: the repair of that sentence is put back,
    // and the failed sentence is listed with the passage it shared the change
    // with.
    let unlocated = sentence("body", PASSAGE);
    let confined = confine(
        &before,
        &after,
        std::slice::from_ref(&failed),
        &[unlocated, v2],
    )
    .unwrap();
    assert_eq!(confined.section, before);
    let [back] = confined.put_back.as_slice() else {
        panic!("the failed sentence is put back: {:?}", confined.put_back);
    };
    assert_eq!(back.target, failed);
    assert_eq!(back.reason, put_back_for(PASSAGE));
}

#[test]
fn a_new_sentence_equal_to_one_the_section_already_has_is_not_kept() {
    let before = section(&format!("{VERIFIED} {FAILED} {OTHER}"));
    let repeated = section(&format!("{VERIFIED} {FAILED} {VERIFIED} {OTHER}"));
    let confined = confine(&before, &repeated, &[sentence("body", FAILED)], &[])
        .unwrap()
        .section;
    assert_eq!(confined["body"], format!("{VERIFIED} {FAILED} {OTHER}"));

    let with_note = |body: &str| json!({"kind":"explanation","body":body,"note":OTHER});
    let before = with_note(&format!("{VERIFIED} {FAILED}"));
    let repeated = with_note(&format!("{VERIFIED} {FAILED} {OTHER}"));
    let confined = confine(&before, &repeated, &[sentence("body", FAILED)], &[])
        .unwrap()
        .section;
    assert_eq!(confined["body"], format!("{VERIFIED} {FAILED}"));
}

const KETTLE: &str = "The kettle sits on the top shelf by the window.";
const ROPE: &str = "A long rope hangs on the hook beside the kettle.";
const BEADS: &str = "Glass beads fill the jar on the lowest shelf.";

fn cited(first: &str, second: &str) -> Value {
    json!({
        "kind": "explanation",
        "body": format!("{KETTLE} {ROPE} {BEADS}"),
        "citations": [
            {"sentence": first, "sourceIndex": 0, "quote": "a line about it"},
            {"sentence": second, "sourceIndex": 0, "quote": "another line about it"},
        ],
    })
}

/// `value` and everything under it as targets, a string by its whole text.
fn whole(value: &Value, path: &str, targets: &mut Vec<Protected>) {
    targets.push(sentence(path, value.as_str().unwrap_or_default()));
    match value {
        Value::Object(fields) => fields
            .iter()
            .for_each(|(key, value)| whole(value, &format!("{path}/{key}"), targets)),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .for_each(|(at, value)| whole(value, &format!("{path}/{at}"), targets)),
        _ => {}
    }
}

fn citation_targets(section: &Value) -> Vec<Protected> {
    let mut targets = Vec::new();
    if let Some(citations) = section.get("citations") {
        whole(citations, "citations", &mut targets);
    }
    targets
}

#[test]
fn citations_that_name_other_sentences_of_the_body_are_kept_when_a_repair_shifts_them() {
    let before = cited(KETTLE, ROPE);
    let repaired = cited(ROPE, BEADS);
    let confined = confine(&before, &repaired, &citation_targets(&before), &[])
        .unwrap()
        .section;
    assert_eq!(confined["citations"], repaired["citations"]);
    assert_eq!(confined["citations"][0]["sentence"], ROPE);
    assert_eq!(confined["citations"][1]["sentence"], BEADS);
}

#[test]
fn citations_no_target_names_stand_as_they_were() {
    let before = cited(KETTLE, ROPE);
    let repaired = cited(ROPE, BEADS);
    let confined = confine(&before, &repaired, &[], &[]).unwrap().section;
    assert_eq!(confined["citations"], before["citations"]);
}

#[test]
fn a_citation_that_names_another_sentence_of_the_body_in_place_is_kept() {
    let before = json!({
        "kind": "explanation",
        "body": format!("{KETTLE} {ROPE}"),
        "citations": [{"sentence": KETTLE, "sourceIndex": 0, "quote": "a line about it"}],
    });
    let mut repaired = before.clone();
    repaired["citations"][0]["sentence"] = json!(ROPE);
    let confined = confine(&before, &repaired, &citation_targets(&before), &[])
        .unwrap()
        .section;
    assert_eq!(confined["citations"][0]["sentence"], ROPE);
}

#[test]
fn confinement_lists_each_change_it_reverted_and_returns_the_same_section() {
    let before = section(&format!("{VERIFIED} {FAILED} {OTHER}"));
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
        confined.section,
        json!({
            "kind": "explanation",
            "title": "Boiling",
            "body": format!("{VERIFIED} Salt raises the boiling point slightly. {OTHER}"),
        })
    );
    assert!(confined
        .undone
        .contains(&Undone::Protected(VERIFIED.to_owned())));
    assert!(confined.undone.contains(&Undone::Outside(OTHER.to_owned())));
    assert!(confined.reasons().contains(&format!(
        "it changes \"{VERIFIED}\", which states a checked claim and must stay as it is"
    )));
    assert!(confined.reasons().contains(&format!(
        "it changes \"{OTHER}\", which is outside what the correction was asked to change"
    )));
}

#[test]
fn confinement_lists_a_new_sentence_the_section_already_has() {
    let before = section(&format!("{VERIFIED} {FAILED} {OTHER}"));
    let repeated = section(&format!("{VERIFIED} {FAILED} {VERIFIED} {OTHER}"));
    let confined = confine(&before, &repeated, &[sentence("body", FAILED)], &[]).unwrap();
    assert_eq!(confined.undone, vec![Undone::Repeated(VERIFIED.to_owned())]);
    assert_eq!(
        confined.reasons(),
        vec![format!(
            "it adds \"{VERIFIED}\", which the lesson already says"
        )]
    );
}

#[test]
fn confinement_lists_nothing_when_nothing_was_reverted() {
    let before = section(&format!("{VERIFIED} {FAILED} {OTHER}"));
    let fixed = section(&format!(
        "{VERIFIED} Salt raises the boiling point. {OTHER}"
    ));
    let confined = confine(
        &before,
        &fixed,
        &[sentence("body", FAILED)],
        &[sentence("body", VERIFIED)],
    )
    .unwrap();
    assert_eq!(confined.section, fixed);
    assert!(confined.undone.is_empty());
}
