#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::generation::CHECK_REVEAL;
use serde_json::json;

fn block(kind: &str, title: &str) -> Value {
    let body = if kind == "check" {
        format!("Case for {title}: what happens?\n\n{CHECK_REVEAL}\n\nAnswer for {title}: this.")
    } else {
        format!("Body of {title}.")
    };
    json!({"kind":kind,"title":title,"body":body})
}

fn question(kind: &str, prompt: &str) -> Value {
    json!({"kind":kind,"prompt":prompt})
}

/// Eleven sections (checks at units 3 and 6, worked examples at 1 and 5) and
/// six questions: the shape the writer is asked for.
fn lesson() -> Value {
    json!({"blocks":[
        block("explanation","First idea"), block("worked_example","First example"),
        block("contrast","Not to be confused with"), block("check","Your turn"),
        block("explanation","Second idea"), block("worked_example","Second example"),
        block("check","Your turn again"), block("guided_practice","Guided"),
        block("independent_practice","On your own"), block("reflection","From memory"),
        block("recap","Recap"),
    ],"questions":[
        question("practice","P1?"), question("practice","P2?"), question("quiz","Q1?"),
        question("quiz","Q2?"), question("test","T1?"), question("test","T2?"),
    ]})
}

fn units(units: impl IntoIterator<Item = usize>) -> BTreeSet<usize> {
    units.into_iter().collect()
}

fn ids(findings: &[ReviewFinding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| finding.place().id())
        .collect()
}

fn found(lesson: &Value, phase: Phase) -> Vec<ReviewFinding> {
    StandardShape.shape_findings(lesson, phase)
}

#[test]
fn the_draft_schema_asks_for_the_margin_and_the_kept_schema_for_the_floor() {
    let draft = policy().schema(Phase::Draft, 1);
    let kept = policy().schema(Phase::Kept, 1);
    let bound = |schema: &LessonSchema, list: &str, limit: &str| {
        schema
            .value()
            .pointer(&format!("/properties/{list}/{limit}"))
            .and_then(Value::as_u64)
    };
    assert_eq!(bound(&draft, "blocks", "minItems"), Some(11));
    assert_eq!(bound(&draft, "blocks", "maxItems"), Some(16));
    assert_eq!(bound(&draft, "questions", "minItems"), Some(6));
    assert_eq!(bound(&draft, "questions", "maxItems"), Some(6));
    assert_eq!(bound(&kept, "blocks", "minItems"), Some(1));
    assert_eq!(bound(&kept, "blocks", "maxItems"), Some(16));
    assert_eq!(bound(&kept, "questions", "minItems"), Some(3));
    assert_eq!(bound(&kept, "questions", "maxItems"), Some(6));
}

#[test]
fn both_schemas_are_one_definition_with_different_counts() {
    for sources in [0, 2] {
        let mut draft = policy().schema(Phase::Draft, sources).into_value();
        let mut kept = policy().schema(Phase::Kept, sources).into_value();
        for schema in [&mut draft, &mut kept] {
            for list in ["blocks", "questions"] {
                let fields = schema
                    .pointer_mut(&format!("/properties/{list}"))
                    .and_then(Value::as_object_mut)
                    .unwrap();
                fields.remove("minItems");
                fields.remove("maxItems");
            }
        }
        assert_eq!(draft, kept, "{sources} sources");
    }
}

#[test]
fn the_writers_lesson_schema_is_the_draft_schema() {
    assert_eq!(
        crate::generation::lesson_schema(2),
        policy().schema(Phase::Draft, 2).into_value()
    );
}

#[test]
fn a_draft_in_the_writers_shape_breaks_no_rule() {
    assert!(found(&lesson(), Phase::Draft).is_empty());
    assert!(found(&lesson(), Phase::Kept).is_empty());
}

#[test]
fn the_draft_is_held_to_its_margin() {
    let mut cut = lesson();
    cut["questions"].as_array_mut().unwrap().pop();
    let findings = found(&cut, Phase::Draft);
    assert_eq!(ids(&findings), ["structure:lesson"]);
    assert!(findings[0].note.contains("1 test"), "{}", findings[0].note);

    let mut fewer = lesson();
    fewer["blocks"].as_array_mut().unwrap().remove(2);
    let findings = found(&fewer, Phase::Draft);
    assert_eq!(ids(&findings), ["structure:lesson"]);
    assert!(findings[0].note.contains("0 contrast"));
}

#[test]
fn a_kept_lesson_is_held_to_the_floor_not_the_margin() {
    let mut cut = lesson();
    cut["questions"].as_array_mut().unwrap().pop();
    cut["blocks"].as_array_mut().unwrap().remove(2);
    assert!(found(&cut, Phase::Kept).is_empty());
}

#[test]
fn a_kept_lesson_needs_a_question_of_every_kind_whether_or_not_a_cut_happened() {
    let mut none_written = lesson();
    none_written["questions"]
        .as_array_mut()
        .unwrap()
        .retain(|question| question["kind"] != "test");
    let findings = found(&none_written, Phase::Kept);
    assert_eq!(ids(&findings), ["structure:lesson"]);
    assert!(findings[0].note.contains("no test question"));
    assert!(!findings[0].note.contains("quiz"));

    none_written["questions"]
        .as_array_mut()
        .unwrap()
        .retain(|question| question["kind"] == "practice");
    let findings = found(&none_written, Phase::Kept);
    assert!(findings[0].note.contains("no quiz and test question"));
}

#[test]
fn a_kept_lesson_needs_an_explanation() {
    let mut lesson = lesson();
    lesson["blocks"]
        .as_array_mut()
        .unwrap()
        .retain(|block| block["kind"] != "explanation");
    let findings = found(&lesson, Phase::Kept);
    assert_eq!(ids(&findings), ["structure:lesson"]);
    assert!(findings[0].note.contains("no explanation"));
}

#[test]
fn the_order_rules_hold_in_both_phases() {
    for phase in [Phase::Draft, Phase::Kept] {
        let mut lesson = lesson();
        let blocks = lesson["blocks"].as_array_mut().unwrap();
        blocks.swap(5, 6);
        blocks.swap(9, 10);
        let findings = found(&lesson, phase);
        assert_eq!(
            ids(&findings),
            ["structure:section-5", "structure:section-9"]
        );
        assert!(findings[0].note.contains("no worked example has shown"));
        assert!(findings[1].note.contains("from memory"));
    }
}

#[test]
fn a_cut_takes_the_check_that_loses_its_worked_example() {
    // The first example goes: the check at unit 3 follows no example now.
    assert_eq!(policy().dependants(&lesson(), &units([1])), units([3]));
    // The second example goes: the check at unit 6 is the one left bare.
    assert_eq!(policy().dependants(&lesson(), &units([5])), units([6]));
}

#[test]
fn a_cut_that_loses_nothing_a_check_depends_on_takes_no_check() {
    assert!(policy().dependants(&lesson(), &units([])).is_empty());
    assert!(policy().dependants(&lesson(), &units([2, 4, 7])).is_empty());
    // A check that goes with its example takes no other.
    assert!(policy().dependants(&lesson(), &units([1, 3])).is_empty());
}

#[test]
fn a_check_the_draft_already_placed_badly_is_not_a_cuts_to_take() {
    let mut lesson = lesson();
    lesson["blocks"].as_array_mut().unwrap().swap(1, 3);
    // The check at unit 1 follows no example in the draft itself, so it is no
    // cut's to take, whatever else goes.
    assert!(policy().dependants(&lesson, &units([])).is_empty());
    assert!(policy().dependants(&lesson, &units([3])).is_empty());
}

#[test]
fn a_check_keeps_its_place_while_any_worked_example_since_the_last_check_stays() {
    let lesson = json!({"blocks":[
        block("explanation","Idea"), block("worked_example","One"),
        block("worked_example","Two"), block("check","Apply"), block("recap","Recap"),
    ],"questions":[]});
    assert!(policy().dependants(&lesson, &units([1])).is_empty());
    assert_eq!(policy().dependants(&lesson, &units([1, 2])), units([3]));
}

#[test]
fn what_the_floor_keeps_is_the_first_explanation_a_third_of_the_sections_and_the_questions() {
    let lesson = lesson();
    assert!(policy().floor(&lesson, &units([])).is_none());
    assert!(policy().floor(&lesson, &units([1, 3, 5])).is_none());

    let first = policy().floor(&lesson, &units([0])).unwrap();
    assert!(first.contains("first explanation"), "{first}");
    // The second explanation is not the first.
    assert!(policy().floor(&lesson, &units([4])).is_none());

    let third = policy().floor(&lesson, &units([1, 2, 3, 5])).unwrap();
    assert!(third.contains("4 of its 11 sections"), "{third}");

    let all = policy()
        .floor(&lesson, &units([11, 12, 13, 14, 15, 16]))
        .unwrap();
    assert!(all.contains("all of its questions"), "{all}");

    let kind = policy().floor(&lesson, &units([15, 16])).unwrap();
    assert!(kind.contains("test question"), "{kind}");
    assert!(policy().floor(&lesson, &units([15])).is_none());
}

#[test]
fn a_third_of_the_drafted_sections_is_measured_against_the_draft_that_still_holds_them() {
    // The sections that go stay in the draft until the lesson is final, so the
    // count a third is taken of is the drafted count however many are out.
    let lesson = lesson();
    assert!(policy().floor(&lesson, &units([1, 2, 3])).is_none());
    assert!(policy().floor(&lesson, &units([1, 2, 3, 5])).is_some());
}

#[test]
fn a_section_that_took_a_cut_is_too_little_with_fewer_than_two_teaching_sentences() {
    assert!(policy().too_little("One.", false));
    assert!(policy().too_little("", false));
    assert!(!policy().too_little("One. Two.", false));
    assert!(policy().too_little("One. Two. Three.", true));
    // Headings and code are no teaching.
    assert!(policy().too_little("### Title\n```python\nprint(1)\n```\nOne.", false));
}

#[test]
fn the_sections_asked_for_are_the_mix_at_its_least_and_the_ideas_at_their_most() {
    assert_eq!(IDEAS, (2, 3));
    assert_eq!(SECTIONS, (11, 16));
    let schema = policy().schema(Phase::Draft, 1);
    let blocks = |limit: &str| {
        schema
            .value()
            .pointer(&format!("/properties/blocks/{limit}"))
            .and_then(Value::as_u64)
    };
    assert_eq!(blocks("minItems"), Some(11));
    assert_eq!(blocks("maxItems"), Some(16));
}

#[test]
fn a_draft_of_the_most_sections_the_writer_may_give_breaks_no_rule() {
    let mut blocks = Vec::new();
    for idea in 1..=3 {
        blocks.push(block("explanation", &format!("Idea {idea}")));
        blocks.push(block("worked_example", &format!("Example {idea}")));
        blocks.push(block("contrast", &format!("Contrast {idea}")));
        blocks.push(block("check", &format!("Check {idea}")));
    }
    blocks.extend([
        block("guided_practice", "Guided"),
        block("independent_practice", "On your own"),
        block("reflection", "From memory"),
        block("recap", "Recap"),
    ]);
    assert_eq!(blocks.len(), SECTIONS.1);
    let draft = json!({"blocks":blocks,"questions":lesson()["questions"]});
    assert!(found(&draft, Phase::Draft).is_empty());
}

#[test]
fn the_prose_of_the_counts_is_the_counts_in_words() {
    let mut fewer = lesson();
    fewer["blocks"].as_array_mut().unwrap().remove(2);
    fewer["questions"].as_array_mut().unwrap().pop();
    let findings = found(&fewer, Phase::Draft);
    assert_eq!(findings.len(), 2);
    assert_eq!(
        findings[0].summary,
        "The lesson should have at least two explanations, two worked examples, one contrast and two checks, and one each of guided practice, independent practice, reflection and recap. It has 2 explanations, 2 worked examples, 0 contrasts, 2 checks, 1 guided practice, 1 independent practice, 1 reflection, 1 recap."
    );
    assert!(findings[0].note.contains(
        "needs at least two explanations, two worked examples, one contrast and two checks, and exactly one each of guided practice"
    ));
    assert_eq!(
        findings[1].summary,
        "The lesson needs at least two questions each for practice, quiz and test. It has 2 practice questions, 2 quiz questions, 1 test question."
    );
    assert!(findings[1].note.starts_with(
        "The lesson needs at least two distinct questions each for practice, quiz and test;"
    ));
}

#[test]
fn numbering_more_sources_rebuilds_the_schema_of_the_same_phase() {
    // What the lesson's schema was widened to before the schema carried its
    // phase: every source index allowing the numbered count.
    fn widen(schema: &mut Value, count: usize) {
        match schema {
            Value::Object(fields) => {
                for (name, value) in fields.iter_mut() {
                    if name == "sourceIndex" {
                        if let Some(maximum) = value.get_mut("maximum") {
                            *maximum = json!(count - 1);
                        }
                    } else {
                        widen(value, count);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| widen(item, count)),
            _ => {}
        }
    }
    for phase in [Phase::Draft, Phase::Kept] {
        for writers in 1..=2 {
            for numbered in writers..=4 {
                let schema = policy().schema(phase, writers);
                let rebuilt = schema.numbered(policy(), numbered);
                let mut widened = schema.into_value();
                widen(&mut widened, numbered);
                assert_eq!(rebuilt.phase(), phase);
                assert_eq!(
                    rebuilt.value(),
                    &widened,
                    "{phase:?} {writers} to {numbered}"
                );
            }
        }
    }
}

#[test]
fn the_draft_s_words_carry_the_counts_the_draft_is_held_to() {
    let words = policy().asked(Phase::Draft);
    let [ideas, questions] = words.as_slice() else {
        panic!("the draft's words are two lines: {words:?}");
    };
    assert!(ideas.contains(&number(IDEAS.0)), "{ideas}");
    assert!(ideas.contains(&number(IDEAS.1)), "{ideas}");
    assert!(
        ideas.contains(&format!("{} to {} blocks", SECTIONS.0, SECTIONS.1)),
        "{ideas}"
    );
    assert!(
        questions.contains(&number(QUESTIONS_PER_KIND)),
        "{questions}"
    );
    for kind in LessonQuestionKind::ALL {
        assert!(questions.contains(kind.as_str()), "{questions}");
    }
}

#[test]
fn the_kept_words_ask_for_one_question_of_each_kind_and_no_count_of_the_draft_s() {
    let kept = policy().asked(Phase::Kept);
    let draft = policy().asked(Phase::Draft);
    let words = kept.join("\n");
    for kind in LessonQuestionKind::ALL {
        assert!(words.contains(kind.as_str()), "{words}");
    }
    assert!(
        words.contains("at least one question of each kind"),
        "{words}"
    );
    assert!(kept.iter().all(|line| !draft.contains(line)));

    let floor = json!({
        "blocks":[block("explanation","Idea")],
        "questions":[question("practice","P?"), question("quiz","Q?"), question("test","T?")]
    });
    let findings = found(&floor, Phase::Kept);
    assert!(findings.is_empty(), "{:?}", ids(&findings));
}
