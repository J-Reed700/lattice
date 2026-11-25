//! Which parts a repair's change may touch, and what the repair is told of a finding's part.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::review_findings::PlanCheck;

const PLANNED: &str = "plan/planned/action-1";

fn planned_at_section_1() -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Planned,
        "action-1",
        FindingSite::Section(1),
        "Alpha holds the line.",
    )
}

fn used_at_section_1() -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Used,
        "beta-idea",
        FindingSite::Section(1),
        "Beta holds the line.",
    )
}

fn parts(of: &[&str]) -> BTreeSet<String> {
    of.iter().map(|part| (*part).to_owned()).collect()
}

fn refuses(reach: &Reach, part: &str) -> Option<String> {
    reach.refuses(&parts(&[part]))
}

#[test]
fn a_planned_item_may_be_added_in_another_part() {
    let reach = Reach::of(&[&planned_at_section_1()]);
    assert_eq!(reach, Reach::Anywhere);
    for part in ["/blocks/3", "/blocks/1", "/questions/0"] {
        assert_eq!(refuses(&reach, part), None, "{part}");
    }
}

#[test]
fn a_use_before_teaching_is_still_mended_in_its_own_part_only() {
    let reach = Reach::of(&[&used_at_section_1()]);
    assert_eq!(reach, Reach::Parts(vec!["/blocks/1".into()]));
    assert_eq!(refuses(&reach, "/blocks/1"), None);
    assert!(refuses(&reach, "/blocks/3").is_some());
}

#[test]
fn a_change_may_touch_its_findings_part_and_its_second_part_only() {
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "n")
        .with_also(FindingSite::Section(2));
    let reach = Reach::of(&[&finding]);
    assert_eq!(refuses(&reach, "/blocks/0"), None);
    assert_eq!(refuses(&reach, "/blocks/2"), None);
    let refused = refuses(&reach, "/blocks/1").unwrap();
    assert!(
        refused.contains("/blocks/1") && refused.contains("(/blocks/0 and /blocks/2)"),
        "{refused}"
    );
    let outside = refuses(&reach, "").unwrap();
    assert!(
        outside.contains("a text outside every block and question"),
        "{outside}"
    );
}

#[test]
fn a_finding_with_one_part_names_only_that_part_when_it_refuses() {
    let finding = ReviewFinding::reviewer(3, Some(FindingSite::Section(1)), "n");
    let refused = refuses(&Reach::of(&[&finding]), "/blocks/0").unwrap();
    assert!(refused.contains("(/blocks/1)"), "{refused}");
    assert!(!refused.contains('`'), "{refused}");
}

#[test]
fn a_finding_about_the_whole_material_may_change_any_part() {
    let finding = ReviewFinding::reviewer(0, None, "n");
    let reach = Reach::of(&[&finding]);
    for part in ["/blocks/0", "/questions/1", ""] {
        assert_eq!(refuses(&reach, part), None, "{part}");
    }
}

#[test]
fn findings_that_share_an_id_each_open_their_own_part() {
    let first = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "one");
    let second = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "two");
    let reach = Reach::of(&[&first, &second]);
    assert_eq!(refuses(&reach, "/blocks/0"), None);
    assert!(refuses(&reach, "/blocks/1").is_some());
    let sibling = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "three")
        .with_also(FindingSite::Section(1));
    let reach = Reach::of(&[&first, &sibling]);
    assert_eq!(refuses(&reach, "/blocks/1"), None);
    assert!(refuses(&reach, "/blocks/2").is_some());
}

#[test]
fn a_planned_item_finding_offers_the_repair_no_part() {
    let finding = planned_at_section_1();
    let entry = Issue {
        finding: &finding,
        last_attempt: None,
    }
    .entry();
    assert!(entry["part"].is_null());
    assert_eq!(entry["id"], PLANNED);
}

#[test]
fn a_finding_with_a_part_still_offers_it() {
    let finding = used_at_section_1();
    let entry = Issue {
        finding: &finding,
        last_attempt: None,
    }
    .entry();
    assert_eq!(entry["part"], "/blocks/1");
}

#[test]
fn only_a_finding_about_the_whole_material_or_a_missing_item_mends_anywhere() {
    assert!(planned_at_section_1().mends_anywhere());
    assert!(ReviewFinding::reviewer(1, None, "Gamma holds the line.").mends_anywhere());
    assert!(
        ReviewFinding::reviewer(2, Some(FindingSite::Lesson), "Gamma holds the line.")
            .mends_anywhere()
    );
    assert!(!used_at_section_1().mends_anywhere());
    assert!(
        !ReviewFinding::reviewer(3, Some(FindingSite::Section(1)), "Gamma holds the line.")
            .mends_anywhere()
    );
}

const KEPT: [&str; 2] = [
    "Added or changed text uses a term, or asks the learner to do or use something, only where the lesson has taught it by that point; otherwise it says there what the term means or how to do it.",
    "It never says again what the sentence before or after it already says.",
];

fn asked<'a>(schema: &'a Value, wording: &'a Wording) -> Request<'a> {
    Request {
        system: "",
        schema,
        authoring: schema,
        candidate: schema,
        quote_checks: &[],
        issues: Vec::new(),
        wording,
        keeps: Keeps::Anything,
        output_tokens: 0,
    }
}

#[test]
fn the_edit_instructions_hold_what_a_repair_adds_to_what_the_lesson_is_held_to() {
    for cites in [true, false] {
        let system = system(cites);
        for sentence in KEPT {
            assert!(system.contains(sentence), "{sentence}");
        }
    }
}

#[test]
fn the_edit_instructions_ask_for_an_entry_for_every_finding() {
    let told = "Answer every finding once in entries: the finding (of), what you will change for it and where (plan), then the edits that make that change (edits). An entry's edits change only sentences and citations of its finding's part or its also part; those of a finding whose part is null may change any part.";
    assert!(system(false).contains(told));
}

#[test]
fn the_rewrite_holds_what_a_repair_adds_to_what_the_lesson_is_held_to() {
    let (schema, wording) = (Value::Null, Wording::default());
    let prompt = rewrite_prompt(&asked(&schema, &wording), Value::Null);
    let instruction = prompt["instruction"].as_str().unwrap();
    for sentence in KEPT {
        assert!(instruction.contains(sentence), "{sentence}");
    }
}

#[test]
fn the_rewrite_keeps_its_prompt_keys_and_its_instructions() {
    let (schema, wording) = (Value::Null, Wording::default());
    let sent = asked(&schema, &wording);
    let prompt = rewrite_prompt(&sent, Value::Null);
    let keys: Vec<&String> = prompt.keys().collect();
    assert_eq!(
        keys,
        [
            "task",
            "originalRequirements",
            "candidate",
            "sourceQuoteChecks",
            "issuesToFix",
            "instruction"
        ]
    );
    let instruction = prompt["instruction"].as_str().unwrap();
    assert!(instruction.starts_with("Give the complete corrected material. Change every defective passage, exercise, key or explanation identified above. An unchanged draft is not a repair. Preserve valid content. Do not mention this internal review, the course plan or the sources' excerpts in the learner-facing material."));
    assert!(rewrite_system(&sent).ends_with(
        "\nYou are editing a rejected draft. Apply every listed correction to the actual content. Do not reproduce the draft unchanged."
    ));
}

#[test]
fn the_rewrite_lists_one_entry_for_each_finding_sent() {
    let named = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "note one")
        .with_passage("Passage one.");
    let mut sentenced = ReviewFinding::reviewer(4, Some(FindingSite::Question(1)), "note two");
    sentenced.sentence = Some("Whole sentence.".to_owned());
    let both = ReviewFinding::reviewer(3, Some(FindingSite::Section(1)), "note three")
        .with_also(FindingSite::Section(3));
    let whole = ReviewFinding::reviewer(0, None, "note four");
    let (schema, wording) = (Value::Null, Wording::default());
    let mut sent = asked(&schema, &wording);
    sent.issues = vec![
        Issue {
            finding: &named,
            last_attempt: Some("it named no finding".to_owned()),
        },
        Issue {
            finding: &sentenced,
            last_attempt: None,
        },
        Issue {
            finding: &both,
            last_attempt: None,
        },
        Issue {
            finding: &whole,
            last_attempt: None,
        },
    ];
    assert_eq!(
        rewrite_prompt(&sent, Value::Null)["issuesToFix"],
        json!([
            {"id":"reviewer/1","part":"/blocks/0","also":null,"passage":"Passage one.","note":"note one","lastAttempt":"it named no finding"},
            {"id":"reviewer/4","part":"/questions/1","also":null,"passage":"Whole sentence.","note":"note two","lastAttempt":null},
            {"id":"reviewer/3","part":"/blocks/1","also":"/blocks/3","passage":null,"note":"note three","lastAttempt":null},
            {"id":"reviewer/0","part":null,"also":null,"passage":null,"note":"note four","lastAttempt":null}
        ])
    );
}
