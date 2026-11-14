#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

const CHECK_IDS: [&str; 7] = [
    "worked_example",
    "exercise",
    "answer_given_away",
    "check_answer",
    "near_miss",
    "promise",
    "later_lesson",
];

fn reading<'a>(
    candidate: &'a Value,
    shown: &'a Value,
    passages: &'a [Vec<String>],
    sections: &'a [usize],
    settle: bool,
) -> Reading<'a> {
    Reading {
        candidate,
        shown,
        passages,
        sections,
        asked: &[],
        remit: Remit::Lesson,
        settle,
    }
}

fn read_lesson(candidate: &Value, reply: &Value, settle: bool) -> Result<Judged> {
    let (shown, passages) = review_candidate(candidate);
    let sections: Vec<usize> = (0..passages.len()).collect();
    read(
        &reply.to_string(),
        &reading(candidate, &shown, &passages, &sections, settle),
    )
}

fn check(section: usize, has_defect: bool, concerns: &[&str]) -> Value {
    json!({"sectionId":format!("section-{section}"),"passageId":format!("section-{section}-passage-0"),"finding":"The section was checked.","hasDefect":has_defect,"concerns":concerns})
}

#[test]
fn a_lesson_review_asks_only_for_its_own_concerns() {
    let passages = vec![vec!["section-0-passage-0".to_owned()]];
    let lesson = schema(&[0], &passages, &[], Remit::Lesson);
    assert_eq!(
        lesson.pointer("/properties/blockChecks/items/properties/concerns"),
        Some(&json!({"type":"array","items":{"type":"string","enum":CHECK_IDS}}))
    );
    let required = lesson
        .pointer("/properties/blockChecks/items/required")
        .unwrap();
    assert_eq!(
        required,
        &json!(["sectionId", "passageId", "finding", "hasDefect", "concerns"])
    );
    assert_eq!(
        lesson.pointer("/properties/issues/items"),
        Some(
            &json!({"type":"object","additionalProperties":false,"required":["note","concern"],"properties":{"note":{"type":"string","minLength":1},"concern":{"type":"string","enum":["promise","later_lesson","workload"]}}})
        )
    );
    let asked = schema(&[0], &passages, &["reviewer/1"], Remit::Lesson);
    assert_eq!(
        asked.pointer("/properties/issues/items/required"),
        Some(&json!(["note", "concern", "continues"]))
    );
    let open = schema(&[0], &passages, &[], Remit::Open);
    assert_eq!(
        open.pointer("/properties/issues/items"),
        Some(&json!({"type":"string","minLength":1}))
    );
    let mut keys: Vec<&String> = open
        .pointer("/properties/blockChecks/items/properties")
        .and_then(Value::as_object)
        .unwrap()
        .keys()
        .collect();
    keys.sort();
    assert_eq!(keys, ["finding", "hasDefect", "passageId", "sectionId"]);
}

#[test]
fn a_lesson_section_check_raises_only_the_concerns_its_kind_takes() {
    let candidate = json!({"blocks":[
        {"kind":"explanation","body":"A plain idea is stated here."},
        {"kind":"worked_example","body":"Adding two and three gives five."}
    ]});
    let reply = json!({"issues":[],"blockChecks":[
        check(0, true, &["worked_example"]),
        check(1, true, &["worked_example"])
    ]});
    let error = read_lesson(&candidate, &reply, false)
        .expect_err("the first pass took a concern its section's kind does not take")
        .to_string();
    assert!(error.contains("section-0"), "{error}");
    assert!(error.contains("worked_example"), "{error}");
    let judged = read_lesson(&candidate, &reply, true).unwrap();
    assert_eq!(judged.reported.len(), 1);
    assert_eq!(judged.reported[0].site, Some(FindingSite::Section(1)));
}

#[test]
fn a_lesson_check_that_names_no_concern_raises_no_finding() {
    let candidate =
        json!({"blocks":[{"kind":"worked_example","body":"Adding two and three gives five."}]});
    let reply = json!({"issues":[],"blockChecks":[check(0, true, &[])]});
    assert!(read_lesson(&candidate, &reply, false).is_err());
    let judged = read_lesson(&candidate, &reply, true).unwrap();
    assert!(judged.reported.is_empty());
}

#[test]
fn a_lesson_issue_holds_only_a_concern_no_section_holds() {
    let candidate =
        json!({"blocks":[{"kind":"worked_example","body":"Adding two and three gives five."}]});
    let reply = json!({"issues":[
        {"note":"The lesson promises that its tutor runs the learner's code.","concern":"promise"},
        {"note":"The example's sum is wrong.","concern":"worked_example"}
    ],"blockChecks":[check(0, false, &[])]});
    let error = read_lesson(&candidate, &reply, false)
        .expect_err("the first pass took an issue whose concern a section holds")
        .to_string();
    assert!(error.contains("worked_example"), "{error}");
    let judged = read_lesson(&candidate, &reply, true).unwrap();
    assert_eq!(judged.reported.len(), 1);
    assert_eq!(
        judged.reported[0].note,
        "The lesson promises that its tutor runs the learner's code."
    );
}
