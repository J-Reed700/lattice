//! What the notes of the check's findings say.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::confirm::{Settled, Unmet};
use super::*;

fn part(index: usize) -> Part {
    Part {
        id: format!("section-{index}"),
        site: FindingSite::Section(index),
        fields: vec!["body".to_owned()],
        texts: vec!["Alpha holds the line.".to_owned()],
        shown: json!({"body": "Alpha holds the line."}),
    }
}

fn missing_note(belongs: Option<&Part>) -> String {
    let item = Item {
        id: "action-1".to_owned(),
        kind: "action",
        text: "Hold the line".to_owned(),
    };
    let settled = Settled {
        taught: Vec::new(),
        missing: vec![(&item, belongs)],
        uses: Vec::new(),
        clashes: Vec::new(),
    };
    let found = findings(&[], &settled, &[], &Value::Null, &[]);
    assert_eq!(found.len(), 1);
    found[0].note.clone()
}

#[test]
fn a_missing_item_s_note_does_not_name_the_part_it_was_placed_in() {
    let placed = part(1);
    let note = missing_note(Some(&placed));
    assert!(!note.contains("section 2"), "{note}");
}

#[test]
fn a_missing_item_with_no_part_is_not_told_one() {
    let note = missing_note(None);
    assert!(!note.contains("fits best"), "{note}");
}

fn used_note(protected: &[Protected]) -> String {
    let parts = [part(0)];
    let texts = Lesson::of(&parts, 1, |_| false).texts();
    let unit = texts
        .iter()
        .next()
        .and_then(|(_, units)| units.spans().first().map(|(unit, _)| *unit))
        .unwrap();
    let hit = hit_at(&parts, &texts, unit).unwrap();
    let settled = Settled {
        taught: Vec::new(),
        missing: Vec::new(),
        uses: vec![Unmet {
            hit,
            what: "the line",
            elsewhere: Vec::new(),
            later: None,
        }],
        clashes: Vec::new(),
    };
    let candidate = json!({"blocks":[{"body":"Alpha holds the line."}],"questions":[]});
    let found = findings(&[], &settled, &parts_of(&candidate), &candidate, protected);
    assert_eq!(found.len(), 1);
    found[0].note.clone()
}

#[test]
fn a_use_in_a_protected_passage_is_told_to_teach_beside_the_quoted_words() {
    let protected = [Protected {
        unit: 0,
        field: "body".into(),
        quote: "Alpha holds the line.".into(),
        released: false,
    }];
    let note = used_note(&protected);
    assert!(
        note.ends_with(concat!("Teach it in this step, where it is first used (", each_shown!(), "), in sentences added before or after the quoted words; the quoted words stay exactly as they are.")),
        "{note}"
    );
    assert!(!note.contains("change the step"), "{note}");
}

#[test]
fn a_use_in_a_passage_nothing_protects_keeps_its_note() {
    let note = used_note(&[]);
    assert!(
        note.ends_with(concat!("Teach it in this step, where it is first used (", each_shown!(), "), or change the step to use only what the lesson has taught (a record asked for in the learner's own notes needs nothing taught).")),
        "{note}"
    );
}

/// Where the finding for a missing `kind` item waits, and the sections it belongs in.
fn sited(
    kind: &'static str,
    text: &str,
    candidate: &Value,
) -> (Option<FindingSite>, Vec<FindingSite>) {
    let item = Item {
        id: format!("{kind}-0"),
        kind,
        text: text.to_owned(),
    };
    let settled = Settled {
        taught: Vec::new(),
        missing: vec![(&item, None)],
        uses: Vec::new(),
        clashes: Vec::new(),
    };
    let found = findings(&[], &settled, &parts_of(candidate), candidate, &[]);
    assert_eq!(found.len(), 1);
    (found[0].site, found[0].within.clone())
}

fn shaped(kinds: &[&str]) -> Value {
    let blocks: Vec<Value> = kinds
        .iter()
        .map(|kind| json!({"kind":kind,"title":"Parcels","body":"Parcels are sorted by weight."}))
        .collect();
    json!({"blocks":blocks,"questions":[]})
}

#[test]
fn a_planned_contrast_no_part_was_named_for_belongs_in_the_contrast_section() {
    let candidate = shaped(&["explanation", "contrast", "explanation"]);
    assert_eq!(
        sited("contrast", "Light and heavy", &candidate),
        (Some(FindingSite::Lesson), vec![FindingSite::Section(1)])
    );
}

#[test]
fn a_missing_item_no_part_was_named_for_waits_at_the_lesson_and_its_summary_names_every_section_it_belongs_in(
) {
    let candidate = shaped(&["contrast", "worked_example", "explanation"]);
    let item = Item {
        id: "term-0".to_owned(),
        kind: "term",
        text: "weight class".to_owned(),
    };
    let settled = Settled {
        taught: Vec::new(),
        missing: vec![(&item, None)],
        uses: Vec::new(),
        clashes: Vec::new(),
    };
    let found = findings(&[], &settled, &parts_of(&candidate), &candidate, &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].site, Some(FindingSite::Lesson));
    assert_eq!(
        found[0].within,
        [FindingSite::Section(1), FindingSite::Section(2)]
    );
    assert!(!found[0].note.contains("section 2"), "{}", found[0].note);
    assert!(
        found[0]
            .summary
            .ends_with(" It belongs in one of these sections: section 2, section 3."),
        "{}",
        found[0].summary
    );
}

#[test]
fn an_idea_is_taught_only_in_a_section_and_a_term_anywhere() {
    let question = Part {
        id: "question-0".to_owned(),
        site: FindingSite::Question(0),
        fields: vec!["prompt".to_owned()],
        texts: vec!["Which bin takes the heavy parcel?".to_owned()],
        shown: json!({"prompt": "Which bin takes the heavy parcel?"}),
    };
    let item = |kind: &'static str| Item {
        id: format!("{kind}-0"),
        kind,
        text: "weight class".to_owned(),
    };
    assert!(item("idea").counts_in(&part(0)));
    assert!(!item("idea").counts_in(&question));
    assert!(item("term").counts_in(&question));
    assert!(item("action").counts_in(&question));
}

#[test]
fn a_planned_contrast_in_a_lesson_without_a_contrast_section_is_sited_at_the_lesson() {
    let candidate = shaped(&["explanation", "worked_example"]);
    assert_eq!(
        sited("contrast", "Light and heavy", &candidate),
        (Some(FindingSite::Lesson), Vec::new())
    );
}
