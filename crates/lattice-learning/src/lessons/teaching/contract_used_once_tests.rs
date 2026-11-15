//! A thing the lesson uses before it teaches it is one finding, at the first
//! place the learner is asked to use it.
use super::*;
use crate::review_findings::{FindingSite, ReviewFinding};

const FLAT: &str = "Press the fold flat";
const SHARP: &str = "Crease the fold sharply";
const KNOT: &str = "Tie the knot twice";
const DEFINED: &str = "A fold is a crease made by bending the sheet back on itself";

fn entry(what: &str, uses: &[&str], later: &str) -> Value {
    json!({"what":what,"uses":uses,"taughtLater":later})
}

fn read_as(asked: bool, taught: bool, quote: &str) -> Value {
    json!({"asked":asked,"taught":taught,"where":"thisLesson","quote":quote})
}

fn words(quote: &str) -> String {
    format!("\"words\":\"{quote}\"")
}

async fn found(
    model: &Routed<'_>,
    candidate: &Value,
    protected: &[Protected],
) -> Vec<ReviewFinding> {
    let context = json!({"priorKnowledge":"The learner has folded paper once."});
    contract::check(
        model,
        &context,
        candidate,
        protected,
        &mut contract::Receipts::default(),
        &[],
    )
    .await
    .unwrap()
    .findings
}

fn model_answering<'a>(inner: &'a ScriptedModel, used: Vec<Value>) -> Routed<'a> {
    Routed::new(
        inner,
        vec![answer(json!([]), json!(used), json!([]), json!([]))],
    )
}

#[tokio::test]
async fn a_thing_used_in_three_parts_is_one_finding_at_the_first_in_lesson_order() {
    let inner = scripted(vec![]);
    let model = model_answering(
        &inner,
        vec![entry(
            "the fold",
            &[
                "Press the fold again, harder",
                "Fold the sheet in half",
                FLAT,
            ],
            "",
        )],
    );
    let candidate = lesson([
        "Fold the sheet in half.",
        "Press the fold flat.",
        "Press the fold again, harder.",
    ]);
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.site, Some(FindingSite::Section(0)));
    assert_eq!(finding.passage.as_deref(), Some("Fold the sheet in half"));
    assert!(finding.also.is_none());
    assert!(
        finding.note.ends_with(
            " Section 2 and Section 3 use it too: this step is the one place that teaches it."
        ),
        "{}",
        finding.note
    );
    assert_eq!(model.confirmed().len(), 1);
}

#[tokio::test]
async fn a_thing_used_in_two_parts_names_the_other_one_in_the_singular() {
    let inner = scripted(vec![]);
    let model = model_answering(
        &inner,
        vec![entry(
            "the fold",
            &[FLAT, "Press the fold again, harder"],
            "",
        )],
    );
    let candidate = lesson([
        "Hold the sheet.",
        "Press the fold flat.",
        "Press the fold again, harder.",
    ]);
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0]
            .note
            .ends_with(" Section 3 uses it too: this step is the one place that teaches it."),
        "{}",
        findings[0].note
    );
}

#[tokio::test]
async fn two_different_things_used_in_one_part_are_two_findings() {
    let inner = scripted(vec![]);
    let model = model_answering(
        &inner,
        vec![
            entry("the fold", &[FLAT], ""),
            entry("the knot", &[KNOT], ""),
        ],
    );
    let candidate = lesson([
        "Hold the sheet.",
        "Press the fold flat. Tie the knot twice.",
        "Done.",
    ]);
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(findings
        .iter()
        .all(|finding| finding.site == Some(FindingSite::Section(1))));
    let passages: Vec<_> = findings
        .iter()
        .map(|finding| finding.passage.clone())
        .collect();
    assert_eq!(passages, [Some(FLAT.to_owned()), Some(KNOT.to_owned())]);
    assert!(findings[0].note.contains("the fold"));
    assert!(findings[1].note.contains("the knot"));
    assert_eq!(model.confirmed().len(), 2);
}

#[tokio::test]
async fn a_thing_taught_after_its_first_use_is_told_to_move_its_teaching_to_the_use() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, vec![entry("the fold", &[FLAT], DEFINED)]);
    let candidate = lesson([
        "Press the fold flat to begin.",
        "Hold the sheet.",
        "A fold is a crease made by bending the sheet back on itself.",
    ]);
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.site, Some(FindingSite::Section(0)));
    assert_eq!(finding.also, Some(FindingSite::Section(2)));
    let expected = format!(
        "The lesson teaches it only afterwards, in section 3 (\"{DEFINED}\"). Move that teaching to this step, where it is first used ({EACH_SHOWN}), so that the lesson teaches it once."
    );
    assert!(finding.note.ends_with(&expected), "{}", finding.note);
}

#[tokio::test]
async fn a_thing_taught_after_its_use_in_the_same_part_has_no_second_part() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, vec![entry("the fold", &[FLAT], DEFINED)]);
    let candidate = lesson([
        "Hold the sheet.",
        "Press the fold flat. A fold is a crease made by bending the sheet back on itself.",
        "Done.",
    ]);
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].site, Some(FindingSite::Section(1)));
    assert!(findings[0].also.is_none());
    let expected = format!(
        "This step teaches it only after it is used (\"{DEFINED}\"). Move that teaching before the words that use it ({EACH_SHOWN}), so that the lesson teaches it once."
    );
    assert!(
        findings[0].note.ends_with(&expected),
        "{}",
        findings[0].note
    );
}

#[tokio::test]
async fn a_guarded_use_taught_later_keeps_its_words_and_takes_the_teaching_beside_them() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, vec![entry("the fold", &[FLAT], DEFINED)]);
    let candidate = lesson([
        "Press the fold flat to begin.",
        "Hold the sheet.",
        "A fold is a crease made by bending the sheet back on itself.",
    ]);
    let guarded = [protected_sentence(0, "Press the fold flat to begin")];
    let findings = found(&model, &candidate, &guarded).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.also, Some(FindingSite::Section(2)));
    let expected = format!(
        "The lesson teaches it only afterwards, in section 3 (\"{DEFINED}\"). Move that teaching to this step, where it is first used ({EACH_SHOWN}), so that the lesson teaches it once. The words that use it stay exactly as they are."
    );
    assert!(finding.note.ends_with(&expected), "{}", finding.note);
}

#[tokio::test]
async fn a_later_teaching_in_guarded_words_gives_the_teach_here_note_and_no_second_part() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, vec![entry("the fold", &[FLAT], DEFINED)]);
    let candidate = lesson([
        "Press the fold flat to begin.",
        "Hold the sheet.",
        "A fold is a crease made by bending the sheet back on itself.",
    ]);
    let guarded = [protected_sentence(2, DEFINED)];
    let findings = found(&model, &candidate, &guarded).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert!(finding.also.is_none());
    assert!(!finding.note.contains("afterwards"), "{}", finding.note);
    assert!(
        finding.note.contains(&format!(
            "Teach it in this step, where it is first used ({EACH_SHOWN}), or change the step"
        )),
        "{}",
        finding.note
    );
}

#[tokio::test]
async fn the_confirming_read_goes_use_by_use_and_the_second_use_carries_the_finding() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, vec![entry("the fold", &[FLAT, SHARP], "")])
        .confirming(&words(FLAT), read_as(false, false, ""))
        .confirming(&words(SHARP), read_as(true, false, ""));
    let candidate = lesson(["Press the fold flat.", "Crease the fold sharply.", "Done."]);
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].site, Some(FindingSite::Section(1)));
    assert_eq!(findings[0].passage.as_deref(), Some(SHARP));
    assert_eq!(model.confirmed().len(), 2);
}

#[tokio::test]
async fn a_use_read_as_taught_ends_the_thing_and_the_later_uses_are_not_read() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, vec![entry("the fold", &[FLAT, SHARP], "")])
        .confirming(&words(FLAT), read_as(true, true, FLAT));
    let candidate = lesson(["Press the fold flat.", "Crease the fold sharply.", "Done."]);
    let findings = found(&model, &candidate, &[]).await;
    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(model.confirmed().len(), 1);
}

#[tokio::test]
async fn an_entry_none_of_whose_quotes_is_in_the_lesson_is_neither_read_nor_a_finding() {
    let inner = scripted(vec![]);
    let model = model_answering(
        &inner,
        vec![entry("the knot", &["Knot the cord thrice"], "")],
    );
    let candidate = lesson(["Press the fold flat.", "Crease the fold sharply.", "Done."]);
    let findings = found(&model, &candidate, &[]).await;
    assert!(findings.is_empty(), "{findings:?}");
    assert!(model.confirmed().is_empty());
}

const HANG: &str = "Hang the kettle on the hook";
const TIE_ROPE: &str = "Coil the rope around the shelf post";

fn of_kinds(parts: &[(&str, &str)], questions: Value) -> Value {
    let blocks: Vec<Value> = parts
        .iter()
        .map(|(kind, body)| {
            json!({"kind":kind,"title":"Part","body":body,"rubric":[],"sourceIndex":0,"quote":EXCERPT})
        })
        .collect();
    json!({"blocks":blocks,"questions":questions})
}

fn never_taught(quotes: &[&str]) -> Vec<Value> {
    vec![entry("the hook", quotes, "")]
}

#[tokio::test]
async fn a_use_in_a_practice_step_is_taught_in_the_last_teaching_part_before_it() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, never_taught(&[HANG]));
    let candidate = of_kinds(
        &[
            ("explanation", "A kettle keeps water hot."),
            ("worked_example", "Beads sit on a string."),
            ("guided_practice", "Hang the kettle on the hook."),
        ],
        json!([]),
    );
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.site, Some(FindingSite::Section(2)));
    assert_eq!(finding.also, Some(FindingSite::Section(1)));
    assert!(
        finding
            .note
            .contains("Teach it in Section 2, the last part before this one that teaches"),
        "{}",
        finding.note
    );
    assert!(
        finding.note.contains("do not teach it inside this step"),
        "{}",
        finding.note
    );
    assert!(
        !finding.note.contains("Teach it in this step"),
        "{}",
        finding.note
    );
}

#[tokio::test]
async fn a_use_in_a_question_is_taught_in_the_last_teaching_block_of_the_lesson() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, never_taught(&[HANG]));
    let candidate = of_kinds(
        &[
            ("explanation", "A kettle keeps water hot."),
            ("check", "Name a thing on a shelf."),
            ("worked_example", "Beads sit on a string."),
            ("recap", "Water, beads and rope."),
        ],
        json!([{"kind":"multiple_choice","prompt":"Which way do you hang the kettle on the hook?","options":["Spout out","Spout in"],"correctIndex":0,"rationales":["Right","Wrong"],"explanation":"The spout points out."}]),
    );
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.site, Some(FindingSite::Question(0)));
    assert_eq!(finding.also, Some(FindingSite::Section(2)));
    assert!(
        finding
            .note
            .contains("Teach it in Section 3, the last part before this one that teaches"),
        "{}",
        finding.note
    );
}

#[tokio::test]
async fn a_use_in_a_teaching_block_is_taught_in_that_step_as_before() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, never_taught(&[HANG]));
    let candidate = of_kinds(
        &[
            ("explanation", "A kettle keeps water hot."),
            ("explanation", "Hang the kettle on the hook."),
            ("recap", "Water and beads."),
        ],
        json!([]),
    );
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].also.is_none());
    assert!(
        findings[0]
            .note
            .contains("Teach it in this step, where it is first used"),
        "{}",
        findings[0].note
    );
}

#[tokio::test]
async fn a_use_in_a_task_block_no_teaching_block_precedes_keeps_todays_note() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, never_taught(&[HANG]));
    let candidate = of_kinds(
        &[
            ("check", "Hang the kettle on the hook."),
            ("explanation", "A kettle keeps water hot."),
            ("recap", "Water and beads."),
        ],
        json!([]),
    );
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].also.is_none());
    assert!(
        findings[0]
            .note
            .contains("Teach it in this step, where it is first used"),
        "{}",
        findings[0].note
    );
    assert!(
        !findings[0]
            .note
            .contains("do not teach it inside this step"),
        "{}",
        findings[0].note
    );
}

#[tokio::test]
async fn a_use_after_a_contrast_is_taught_in_the_section_before_the_contrast() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, never_taught(&[HANG]));
    let candidate = of_kinds(
        &[
            ("explanation", "A kettle keeps water hot."),
            ("contrast", "A rope is not a string."),
            ("guided_practice", "Hang the kettle on the hook."),
        ],
        json!([]),
    );
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    let note = &findings[0].note;
    assert_eq!(findings[0].also, Some(FindingSite::Section(0)));
    assert!(note.contains("Teach it in Section 1"), "{note}");
    assert!(!note.contains("Section 2"), "{note}");
}

#[tokio::test]
async fn a_guarded_use_in_a_task_block_takes_the_teaching_in_the_earlier_part() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, never_taught(&[HANG]));
    let candidate = of_kinds(
        &[
            ("explanation", "A kettle keeps water hot."),
            ("worked_example", "A rope is not a string."),
            ("guided_practice", "Hang the kettle on the hook."),
        ],
        json!([]),
    );
    let guarded = [protected_sentence(2, "Hang the kettle on the hook")];
    let findings = found(&model, &candidate, &guarded).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    let note = &findings[0].note;
    assert_eq!(findings[0].also, Some(FindingSite::Section(1)));
    assert!(note.contains("Teach it in Section 2"), "{note}");
    assert!(
        note.contains("the quoted words stay exactly as they are"),
        "{note}"
    );
    assert!(!note.contains("do not teach it inside this step"), "{note}");
}

#[tokio::test]
async fn a_thing_used_in_two_task_blocks_names_the_teaching_part_as_the_one_place() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, never_taught(&[HANG, TIE_ROPE]));
    let candidate = of_kinds(
        &[
            ("explanation", "A kettle keeps water hot."),
            ("worked_example", "Beads sit on a string."),
            ("guided_practice", "Hang the kettle on the hook."),
            ("check", "Coil the rope around the shelf post."),
        ],
        json!([]),
    );
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].site, Some(FindingSite::Section(2)));
    assert!(
        findings[0]
            .note
            .ends_with(" Section 4 uses it too: Section 2 is the one place that teaches it."),
        "{}",
        findings[0].note
    );
}

#[tokio::test]
async fn a_use_in_a_task_block_taught_later_elsewhere_still_moves_that_teaching_to_the_use() {
    let inner = scripted(vec![]);
    let model = model_answering(&inner, vec![entry("the hook", &[HANG], DEFINED)]);
    let candidate = of_kinds(
        &[
            ("explanation", "A kettle keeps water hot."),
            ("guided_practice", "Hang the kettle on the hook."),
            (
                "explanation",
                "A fold is a crease made by bending the sheet back on itself.",
            ),
        ],
        json!([]),
    );
    let findings = found(&model, &candidate, &[]).await;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].also, Some(FindingSite::Section(2)));
    assert!(
        findings[0].note.contains("Move that teaching to this step"),
        "{}",
        findings[0].note
    );
    assert!(
        !findings[0]
            .note
            .contains("do not teach it inside this step"),
        "{}",
        findings[0].note
    );
}
