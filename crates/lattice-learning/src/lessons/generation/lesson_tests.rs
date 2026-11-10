#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::lessons::course_generation_tests::length_bounds;
use serde_json::{json, Value};

const STEPS_IN_ORDER: &str = "The learner does the lesson's steps in order, with their own tools and materials, so each step starts from what the steps before it left: a later step uses what an earlier one made or set up, and never has the learner make or set it up again.";

const EXCERPT: &str =
    "Water boils at 99.97 degrees Celsius at one standard atmosphere of pressure.";

fn source() -> LearningSourceDto {
    LearningSourceDto {
        id: "source-a".into(),
        title: "Steam tables".into(),
        url: None,
        excerpt: EXCERPT.into(),
        acquired_at: 1,
    }
}

fn block(kind: &str, title: &str, body: &str, cited: bool) -> Value {
    json!({"kind":kind,"title":title,"body":body,"rubric":[],
        "sourceIndex":if cited { json!(0) } else { Value::Null },
        "quote":if cited { EXCERPT } else { "" },"citations":[]})
}

fn question(kind: &str, prompt: &str, cited: bool) -> Value {
    json!({"kind":kind,"prompt":prompt,"options":["One","Two","Three","Four"],"correctIndex":1,"rationales":["One fails here.","","Three fails here.","Four fails here."],"explanation":"Two follows.",
        "sourceIndex":if cited { json!(0) } else { Value::Null },
        "quote":if cited { EXCERPT } else { "" },"citations":[]})
}

/// Seven sections, two sharing a title, one with no body, no reflection; five
/// questions, one of them practice.
fn off_shape_value(cited: bool) -> Value {
    let long = |part: u8| -> String {
        (0..30u8)
            .map(|n| {
                format!(
                    "A substantive explanation {}{} of part {}. ",
                    char::from(b'a' + n / 26),
                    char::from(b'a' + n % 26),
                    char::from(b'a' + part)
                )
            })
            .collect()
    };
    let mut blocks: Vec<_> = [
        "explanation",
        "explanation",
        "worked_example",
        "worked_example",
        "guided_practice",
        "independent_practice",
    ]
    .iter()
    .enumerate()
    .map(|(i, kind)| {
        block(
            kind,
            &format!("Part {i}"),
            &long(u8::try_from(i).unwrap()),
            cited,
        )
    })
    .collect();
    blocks[1]["title"] = json!("Part 0");
    blocks.push(block("recap", "Recap", "", cited));
    let questions: Vec<_> = ["practice", "quiz", "quiz", "test", "test"]
        .iter()
        .enumerate()
        .map(|(i, kind)| question(kind, &format!("Question {i}?"), cited))
        .collect();
    json!({"blocks":blocks,"questions":questions})
}

fn off_shape(cited: bool) -> GeneratedLesson {
    serde_json::from_value(off_shape_value(cited)).unwrap()
}

#[test]
fn a_lesson_off_its_shape_is_saved_and_its_rules_are_findings_not_refusals() {
    let sources = [source()];
    let owned = HashSet::from(["source-a"]);
    let (prepared, citations) = assemble(off_shape(true), &sources, &owned).unwrap();
    assert_eq!((prepared.blocks.len(), prepared.questions.len()), (7, 5));
    assert_eq!(prepared.keys.len(), 5);
    assert!(citations.is_empty());
    // The rules come back as findings for the user, not as an error.
    let policy = crate::lessons::shape::policy();
    let schema = policy.schema(crate::lessons::shape::Phase::Draft, 1);
    let findings = crate::teaching::structural_findings(
        crate::teaching::ReviewSchema::Lesson(crate::lessons::shape::LessonShape::new(
            &schema, policy,
        )),
        &off_shape_value(true).to_string(),
        &Default::default(),
    )
    .unwrap();
    let keys: std::collections::BTreeSet<_> = findings.iter().map(|f| f.place().id()).collect();
    let expected = [
        "structure:lesson",
        "structure:section-1",
        "structure:section-4",
        "structure:section-5",
        "structure:section-6",
    ]
    .map(String::from);
    assert_eq!(keys, std::collections::BTreeSet::from(expected));
}

#[test]
fn the_publication_check_reports_a_question_kind_the_lesson_lacks() {
    let mut lesson = off_shape_value(true);
    lesson["questions"]
        .as_array_mut()
        .unwrap()
        .retain(|question| question["kind"] != "test");
    let policy = crate::lessons::shape::policy();
    let schema = policy.schema(crate::lessons::shape::Phase::Kept, 1);
    let findings = crate::teaching::structural_findings(
        crate::teaching::ReviewSchema::Lesson(crate::lessons::shape::LessonShape::new(
            &schema, policy,
        )),
        &lesson.to_string(),
        &Default::default(),
    )
    .unwrap();
    assert!(
        findings
            .iter()
            .any(|finding| finding.place().id() == "structure:lesson"
                && finding.note.contains("test")),
        "{findings:?}"
    );
}

#[test]
fn a_quotation_that_does_not_match_keeps_its_source_and_waits_for_the_user() {
    let mut lesson = off_shape(true);
    lesson.blocks[2].quote = "Water boils at exactly one hundred degrees in every kitchen.".into();
    let sources = [source()];
    let (prepared, citations) = assemble(lesson, &sources, &HashSet::from(["source-a"])).unwrap();
    assert_eq!(prepared.blocks[2].source_ids, ["source-a"]);
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].place().id(), "quotation:section-2");
    assert!(citations[0].note.contains("blocks[2]"));
    assert_eq!(
        citations[0].summary,
        "The quotation cited in section 3 could not be found in its source."
    );
    assert_eq!(
        citations[0].passage.as_deref(),
        Some("Water boils at exactly one hundred degrees in every kitchen.")
    );
    assert_eq!(citations[0].sentence, None);
}

#[test]
fn a_cited_sentence_adds_its_source_and_one_that_cannot_be_confirmed_is_a_finding() {
    const KETTLE: &str = "The kettle whistles once the water inside it has boiled away.";
    let kettle = LearningSourceDto {
        id: "source-b".into(),
        title: "Kettle notes".into(),
        url: None,
        excerpt: KETTLE.into(),
        acquired_at: 2,
    };
    let mut lesson = off_shape(true);
    lesson.blocks[2].citations = vec![
        GeneratedCitation {
            sentence: "A kettle whistles when it runs dry.".into(),
            source_index: Some(1),
            quote: KETTLE.into(),
        },
        GeneratedCitation {
            sentence: "Water boils at 50 degrees Celsius.".into(),
            source_index: Some(0),
            quote: "Water boils at exactly fifty degrees in every kitchen.".into(),
        },
    ];
    let owned = HashSet::from(["source-a", "source-b"]);

    let (prepared, findings) = assemble(lesson, &[source(), kettle], &owned).unwrap();

    assert_eq!(prepared.blocks[2].source_ids, ["source-a", "source-b"]);
    assert_eq!(prepared.blocks[3].source_ids, ["source-a"]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].place().id(), "quotation:section-2");
    assert!(findings[0].note.contains("Water boils at 50 degrees"));
    assert_eq!(
        findings[0].summary,
        "The quotation cited for this sentence could not be found in its source: \u{201c}Water boils at 50 degrees Celsius.\u{201d}"
    );
    assert_eq!(
        findings[0].passage.as_deref(),
        Some("Water boils at exactly fifty degrees in every kitchen.")
    );
    assert_eq!(
        findings[0].sentence.as_deref(),
        Some("Water boils at 50 degrees Celsius.")
    );
}

const CURLY: &str =
    "If you\u{2019}re ever unsure, the file reader will show you exactly which file is open.";
const STRAIGHT: &str =
    "If you're ever unsure, the file reader will show you exactly which file is open.";

fn curly_source() -> LearningSourceDto {
    LearningSourceDto {
        id: "source-b".into(),
        title: "Reader notes".into(),
        url: None,
        excerpt: CURLY.into(),
        acquired_at: 2,
    }
}

#[test]
fn a_sentence_citation_differing_from_its_source_only_in_typography_is_confirmed() {
    let mut lesson = off_shape(true);
    lesson.blocks[2].citations = vec![GeneratedCitation {
        sentence: "The file reader shows which file is open.".into(),
        source_index: Some(1),
        quote: STRAIGHT.into(),
    }];
    let owned = HashSet::from(["source-a", "source-b"]);

    let (prepared, findings) = assemble(lesson, &[source(), curly_source()], &owned).unwrap();

    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(prepared.blocks[2].source_ids, ["source-a", "source-b"]);
}

#[test]
fn a_part_quotation_differing_from_its_source_only_in_typography_is_confirmed() {
    let mut lesson = off_shape(true);
    lesson.blocks[2].source_index = Some(1);
    lesson.blocks[2].quote = STRAIGHT.into();
    lesson.questions[1].source_index = Some(1);
    lesson.questions[1].quote = STRAIGHT.into();
    let owned = HashSet::from(["source-a", "source-b"]);

    let (prepared, findings) = assemble(lesson, &[source(), curly_source()], &owned).unwrap();

    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(prepared.blocks[2].source_ids, ["source-b"]);
    assert_eq!(prepared.questions[1].source_ids, ["source-b"]);
}

#[test]
fn a_quotation_with_different_words_is_still_a_finding_beside_a_curly_source() {
    let mut lesson = off_shape(true);
    lesson.blocks[2].citations = vec![GeneratedCitation {
        sentence: "The file reader closes files.".into(),
        source_index: Some(1),
        quote: "If you're ever unsure, the file reader will close exactly which file is open."
            .into(),
    }];
    let owned = HashSet::from(["source-a", "source-b"]);

    let (prepared, findings) = assemble(lesson, &[source(), curly_source()], &owned).unwrap();

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].place().id(), "quotation:section-2");
    assert_eq!(prepared.blocks[2].source_ids, ["source-a"]);
}

const MARKED: &str =
    "Start by using the **F**ile **R**eader (FR) tool, then open the `config` file to check it.";
const PLAIN: &str = "using the File Reader (FR) tool, then open the config file to check it.";
const DASHED: &str = "The reader \u{2013} unlike the writer \u{2014} never changes a file on disk.";

fn source_b(excerpt: &str) -> LearningSourceDto {
    LearningSourceDto {
        id: "source-b".into(),
        title: "Reader notes".into(),
        url: None,
        excerpt: excerpt.into(),
        acquired_at: 2,
    }
}

fn confirmed_as_part_quote(excerpt: &str, quote: &str) {
    let mut lesson = off_shape(true);
    lesson.blocks[2].source_index = Some(1);
    lesson.blocks[2].quote = quote.into();
    let owned = HashSet::from(["source-a", "source-b"]);
    let (_, findings) = assemble(lesson, &[source(), source_b(excerpt)], &owned).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
}

fn confirmed_as_sentence_citation(excerpt: &str, quote: &str) {
    let mut lesson = off_shape(true);
    lesson.blocks[2].citations = vec![GeneratedCitation {
        sentence: "The file reader behaves as described.".into(),
        source_index: Some(1),
        quote: quote.into(),
    }];
    let owned = HashSet::from(["source-a", "source-b"]);
    let (prepared, findings) = assemble(lesson, &[source(), source_b(excerpt)], &owned).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(prepared.blocks[2].source_ids, ["source-a", "source-b"]);
}

#[test]
fn a_part_quotation_omitting_markdown_markers_of_its_source_is_confirmed() {
    confirmed_as_part_quote(MARKED, PLAIN);
}

#[test]
fn a_sentence_citation_omitting_markdown_markers_of_its_source_is_confirmed() {
    confirmed_as_sentence_citation(MARKED, PLAIN);
}

#[test]
fn a_quotation_writing_a_hyphen_for_a_source_dash_is_confirmed() {
    let hyphens = "The reader - unlike the writer - never changes a file on disk.";
    confirmed_as_part_quote(DASHED, hyphens);
    confirmed_as_sentence_citation(DASHED, hyphens);
}

#[test]
fn a_quotation_with_a_different_word_is_still_a_finding_beside_a_marked_source() {
    for (excerpt, quote) in [
        (MARKED, PLAIN.replace("open", "close")),
        (
            DASHED,
            "The reader - unlike the writer - always changes a file on disk.".to_owned(),
        ),
    ] {
        let mut lesson = off_shape(true);
        lesson.blocks[2].citations = vec![GeneratedCitation {
            sentence: "The file reader behaves as described.".into(),
            source_index: Some(1),
            quote,
        }];
        let owned = HashSet::from(["source-a", "source-b"]);
        let (prepared, findings) =
            assemble(lesson, &[source(), source_b(excerpt)], &owned).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(prepared.blocks[2].source_ids, ["source-a"]);
    }
}

#[test]
fn a_long_cited_sentence_that_cannot_be_confirmed_is_shown_whole() {
    const LONG: &str = "Water boils at 50 degrees Celsius in every kitchen on every day of the year, whatever the pressure of the air around the pan.";
    let mut lesson = off_shape(true);
    lesson.blocks[2].citations = vec![GeneratedCitation {
        sentence: LONG.into(),
        source_index: Some(0),
        quote: "Water boils at exactly fifty degrees in every kitchen.".into(),
    }];

    let (_, findings) = assemble(lesson, &[source()], &HashSet::from(["source-a"])).unwrap();

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].summary,
        format!("The quotation cited for this sentence could not be found in its source: \u{201c}{LONG}\u{201d}")
    );
}

#[test]
fn a_question_citing_a_numbered_source_the_program_does_not_own_is_refused() {
    const KETTLE: &str = "The kettle whistles once the water inside it has boiled away.";
    let kettle = LearningSourceDto {
        id: "source-b".into(),
        title: "Kettle notes".into(),
        url: None,
        excerpt: KETTLE.into(),
        acquired_at: 2,
    };
    let mut lesson = off_shape(true);
    lesson.questions[1].citations = vec![GeneratedCitation {
        sentence: "A kettle whistles when it runs dry.".into(),
        source_index: Some(1),
        quote: KETTLE.into(),
    }];

    let error = assemble(lesson, &[source(), kettle], &HashSet::from(["source-a"])).unwrap_err();

    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(message)
            if message == "A question referred to a source outside this program."),
        "{error:?}"
    );
}

#[test]
fn the_schema_asks_for_a_citation_list_per_part_and_none_without_sources() {
    let pointer = "/properties/blocks/items/properties/citations/maxItems";
    assert_eq!(lesson_schema(2).pointer(pointer), Some(&json!(24)));
    assert_eq!(lesson_schema(0).pointer(pointer), Some(&json!(0)));
    for part in ["blocks", "questions"] {
        let required = lesson_schema(2)
            .pointer(&format!("/properties/{part}/items/required"))
            .cloned()
            .unwrap();
        assert!(required.as_array().unwrap().contains(&json!("citations")));
    }
}

#[test]
fn an_invented_citation_in_a_course_without_sources_is_dropped_and_reported() {
    let mut lesson = off_shape(false);
    lesson.questions[0].source_index = Some(0);
    lesson.questions[0].quote = "An imaginary textbook says this is true.".into();
    let (prepared, citations) = assemble(lesson, &[], &HashSet::new()).unwrap();
    assert!(prepared.questions[0].source_ids.is_empty());
    assert_eq!(citations[0].place().id(), "quotation:question-0");
}

#[test]
fn a_sourced_part_that_names_no_source_cannot_be_saved_and_is_not_called_invalid_input() {
    let mut lesson = off_shape(true);
    lesson.blocks[0].source_index = Some(3);
    let error = assemble(lesson, &[source()], &HashSet::from(["source-a"])).unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
    assert!(!error.to_string().contains("check your input"));
}

#[test]
fn the_lesson_schema_bounds_no_text_but_a_blank() {
    for sources in [0, 2] {
        let bounds = length_bounds(&lesson_schema(sources));
        assert!(bounds.is_empty(), "{sources} sources: {bounds:?}");
    }
}

#[test]
fn a_lessons_text_of_any_length_is_no_schema_finding() {
    let policy = crate::lessons::shape::policy();
    let schema = policy.schema(crate::lessons::shape::Phase::Draft, 1);
    let kinds = [
        "explanation",
        "worked_example",
        "contrast",
        "check",
        "explanation",
        "worked_example",
        "check",
        "guided_practice",
        "independent_practice",
        "reflection",
        "recap",
    ];
    let blocks: Vec<_> = kinds
        .iter()
        .enumerate()
        .map(|(i, kind)| {
            let body = "A substantive explanation. ".repeat(30);
            block(kind, &format!("Part {i}"), &body, true)
        })
        .collect();
    let questions: Vec<_> = ["practice", "practice", "quiz", "quiz", "test", "test"]
        .iter()
        .enumerate()
        .map(|(i, kind)| question(kind, &format!("Question {i}?"), true))
        .collect();
    let mut lesson = json!({"blocks":blocks,"questions":questions});
    let schema_findings = |lesson: &Value| -> Vec<String> {
        crate::teaching::structural_findings(
            crate::teaching::ReviewSchema::Lesson(crate::lessons::shape::LessonShape::new(
                &schema, policy,
            )),
            &lesson.to_string(),
            &Default::default(),
        )
        .unwrap()
        .into_iter()
        .map(|finding| finding.note)
        .filter(|note| note.starts_with("Fix the schema violation"))
        .collect()
    };
    let found = schema_findings(&lesson);
    assert!(found.is_empty(), "the fixture is not a lesson: {found:?}");

    lesson["blocks"][0]["title"] = json!("A title that runs on and on. ".repeat(10));
    lesson["blocks"][1]["body"] = json!("Short.");
    lesson["blocks"][2]["quote"] = json!(EXCERPT.repeat(20));
    lesson["blocks"][3]["citations"] = json!([
        {"sentence":"A cited sentence that runs on. ".repeat(30),"sourceIndex":0,"quote":EXCERPT},
        {"sentence":"No","sourceIndex":0,"quote":EXCERPT}
    ]);
    lesson["questions"][0]["prompt"] = json!("Which choice follows from the premise? ".repeat(40));
    lesson["questions"][0]["options"][0] = json!("A long answer choice. ".repeat(40));
    lesson["questions"][1]["explanation"] =
        json!("Because the premise fixes the outcome. ".repeat(60));
    let found = schema_findings(&lesson);
    assert!(found.is_empty(), "{found:?}");
}

#[tokio::test]
async fn a_lesson_with_no_reference_material_asks_for_it_in_plain_words() {
    use crate::lessons::course_generation_tests::outline;
    let program = crate::tests::fixture();
    let lesson = &program.modules[0].lessons[0];
    let references = ReferenceCollection::lexical(&[]).unwrap();

    let error = prepare_lesson_with_references(
        &outline(1, 1),
        &BuiltinRuntimes::for_tests(),
        &program,
        lesson,
        &CoursePlan::blank(&program),
        &references,
    )
    .await
    .err()
    .map(|error| error.to_string())
    .unwrap_or_default();

    assert!(
        error.contains("Add reference material in Sources before preparing a lesson."),
        "{error}"
    );
    assert!(!error.contains("MVP"), "{error}");
}

fn writer_prompt(program: &LearningProgramDto, lesson: &LearningLessonDto) -> Value {
    use crate::lessons::course_generation_tests::outline;
    let references = ReferenceCollection::lexical(&[]).unwrap();
    let prompt = lesson_prompt(
        &outline(1, 1),
        program,
        lesson,
        &CoursePlan::blank(program),
        &references,
        Vec::new(),
        Vec::new(),
        true,
        crate::lessons::shape::policy(),
    )
    .unwrap();
    serde_json::from_str(&prompt).unwrap()
}

#[test]
fn the_writer_is_given_the_course_plan_and_no_recaps() {
    let program = crate::tests::fixture();
    let lesson = program.modules[0].lessons[0].clone();
    let prompt = writer_prompt(&program, &lesson);

    assert!(prompt["courseSequence"]["coursePlan"].is_array());
    assert!(prompt["courseSequence"].get("recentTeaching").is_none());
    let whole = prompt.to_string();
    assert!(!whole.contains("recentTeaching"), "{whole}");
    assert!(!whole.contains("earlier recaps"), "{whole}");
}

#[test]
fn the_writer_teaches_exactly_its_lessons_planned_ideas_and_never_a_later_lessons() {
    let requirements = writer_requirements();

    assert!(
        requirements.contains("Teach exactly that lesson's planned ideas, terms and actions"),
        "{requirements}"
    );
    assert!(
        requirements.contains("Never teach a later lesson's planned ideas"),
        "{requirements}"
    );
    assert!(
        requirements.contains("an earlier lesson's planned ideas in coursePlan"),
        "the last check mixes in an earlier lesson's planned ideas"
    );
    assert!(
        requirements.contains("an earlier lesson's planned actions in coursePlan"),
        "actions an earlier lesson plans are not taught again"
    );
    assert!(
        requirements.contains("the ideas coursePlan lists for earlier lessons"),
        "recall questions draw on earlier lessons' planned ideas"
    );
    assert!(
        requirements.contains("the next lesson's planned ideas in coursePlan"),
        "the recap bridges to the next lesson's plan"
    );
}

#[test]
fn the_writer_has_the_learner_prepare_what_an_action_needs_with_steps_it_teaches() {
    let requirements = writer_requirements();

    assert!(
        requirements.contains("When an action needs something prepared first"),
        "{requirements}"
    );
    assert!(
        requirements.contains("teaches them where they are first used"),
        "{requirements}"
    );
}

#[test]
fn the_writer_asks_for_what_an_action_needs_in_no_subjects_terms() {
    let text = writer_requirements();
    let rule = text
        .lines()
        .find(|line| line.contains("teach every action a practice or check asks"))
        .unwrap();
    for word in ["command", "option", "folder", "file", "path"] {
        assert!(
            !rule.contains(word),
            "the action rule mentions \"{word}\": {rule}"
        );
    }
    assert!(
        rule.contains("the lesson has the learner prepare it with steps the lesson teaches"),
        "{rule}"
    );
    let question = text
        .lines()
        .find(|line| line.contains("Every rationale and the explanation say what is true"))
        .unwrap();
    assert!(!question.contains("paths"), "{question}");
    assert!(
        question.contains("using the names and values its prompt gives"),
        "{question}"
    );
}

#[test]
fn the_writer_is_told_a_section_never_says_the_same_thing_twice() {
    let text = writer_requirements();
    assert!(
        text.contains("a section never says the same thing twice"),
        "{text}"
    );
}

fn writer_requirements() -> String {
    let program = crate::tests::fixture();
    let lesson = program.modules[0].lessons[0].clone();
    writer_prompt(&program, &lesson)["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_writer_writes_a_cases_own_values_plainly_and_is_not_asked_for_hypotheticals() {
    for text in [writer_requirements(), lesson_system(true)] {
        assert!(!text.contains("explicit hypotheticals"), "{text}");
        assert!(!text.contains("Suppose…"), "{text}");
        assert!(
            text.contains("own names, paths and values are made up for its case"),
            "{text}"
        );
        assert!(text.contains("write them plainly"), "{text}");
    }
}

#[test]
fn the_writer_is_not_told_how_practice_is_reviewed_but_may_not_promise_execution() {
    let text = writer_requirements();
    assert!(!text.contains("review submitted text"), "{text}");
    assert!(!text.contains("do not execute code"), "{text}");
    assert!(
        text.contains("Do not describe how practice is reviewed."),
        "{text}"
    );
    assert!(
        text.contains(
            "Never promise automatic code execution, external peer review or expert certification."
        ),
        "{text}"
    );
    assert!(
        text.contains("A learner may run code in a lab after starting a supported runtime."),
        "{text}"
    );
}

#[test]
fn the_writer_teaches_the_learners_setup_and_every_step_it_asks_for() {
    let text = writer_requirements();
    assert!(
        text.contains(
            "Teach for the learner's own situation, systems and tools as programGoal and priorKnowledge describe them."
        ),
        "{text}"
    );
    assert!(
        text.contains("name that system instead of presenting what it says as the learner's"),
        "{text}"
    );
    assert!(
        text.contains(
            "Within this lesson, teach every action a practice or check asks the learner to perform"
        ),
        "{text}"
    );
}

#[test]
fn the_writer_keeps_to_its_own_lesson_and_writes_steps_and_questions_well() {
    let text = writer_requirements();
    assert!(
        text.contains("Teach this lesson's objective. What another lesson's objective"),
        "{text}"
    );
    assert!(text.contains("at most a one-line pointer"), "{text}");
    assert!(
        text.contains(
            "Write a worked_example's steps as a numbered Markdown list, one step per item; where a step has the learner type or enter something"
        ),
        "{text}"
    );
    assert!(
        text.contains("Spread the questions across the lesson's ideas; no two test the same fact."),
        "{text}"
    );
}

#[test]
fn the_writers_lesson_sequence_gives_each_lessons_objective_and_marks_the_current_one() {
    let mut program = crate::tests::fixture();
    program.modules[0].lessons.truncate(1);
    let template = program.modules[0].lessons[0].clone();
    let mut other = template.clone();
    other.id = "other-lesson".into();
    other.title = "Paths and cd".into();
    other.objective = "Move between directories with cd.".into();
    program.modules[0].lessons.push(other);
    let lesson = program.modules[0].lessons[0].clone();

    let prompt = writer_prompt(&program, &lesson);
    let sequence = prompt["currentModule"]["lessonSequence"]
        .as_array()
        .unwrap();

    assert_eq!(sequence.len(), 2);
    assert_eq!(sequence[0]["title"], json!(template.title));
    assert_eq!(sequence[0]["objective"], json!(template.objective));
    assert_eq!(sequence[0]["current"], json!(true));
    assert_eq!(sequence[1]["title"], json!("Paths and cd"));
    assert_eq!(
        sequence[1]["objective"],
        json!("Move between directories with cd.")
    );
    assert_eq!(sequence[1]["current"], json!(false));
}

#[test]
fn the_lesson_writers_prompt_says_to_name_a_choice_by_its_words() {
    let text = writer_requirements();

    assert!(text.contains(
        "An explanation names each choice by its words, never by its number, letter or position, because the choices are shuffled before learners see them."
    ));
}

#[test]
fn the_schema_asks_for_one_reason_per_choice_after_the_right_choice() {
    let schema = lesson_schema(1);
    let question = schema.pointer("/properties/questions/items").unwrap();
    let required: Vec<_> = question["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(required.contains(&"rationales"), "{required:?}");
    let reasons = &question["properties"]["rationales"];
    assert_eq!(reasons["type"], "array");
    assert_eq!(reasons["minItems"], 4);
    assert_eq!(reasons["maxItems"], 4);
    assert_eq!(reasons["items"]["type"], "string");
    let keys: Vec<_> = question["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let at = |name: &str| keys.iter().position(|key| *key == name).unwrap();
    assert!(at("correctIndex") < at("rationales"), "{keys:?}");
}

#[test]
fn a_questions_reasons_are_trimmed_and_kept_with_its_key_in_the_writers_order() {
    let mut value = off_shape_value(true);
    value["questions"][0]["rationales"] = json!([
        "",
        "  Two is wrong because it reverses the order.  ",
        "Three confuses the cause with the effect.",
        "Four is the cause stated backwards."
    ]);
    value["questions"][1]
        .as_object_mut()
        .unwrap()
        .remove("rationales");
    let lesson: GeneratedLesson = serde_json::from_value(value).unwrap();

    let (prepared, _) = assemble(lesson, &[source()], &HashSet::from(["source-a"])).unwrap();

    assert_eq!(
        prepared.keys[0].rationales,
        [
            "",
            "Two is wrong because it reverses the order.",
            "Three confuses the cause with the effect.",
            "Four is the cause stated backwards."
        ]
    );
    assert!(prepared.keys[1].rationales.is_empty());
}

#[test]
fn the_writer_writes_only_what_the_learner_sees() {
    let text = writer_requirements();
    for expected in [
        "The learner reads only this lesson, never the sources' excerpts, the course plan (coursePlan) or these requirements.",
        "say what a source states in the lesson's own words",
        "never point at an excerpt, passage, tutorial, plan or requirement the learner cannot see",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in {text}");
    }
    assert!(!text.contains("When an excerpt describes"), "{text}");
}

#[test]
fn the_writer_asks_questions_only_about_what_the_learner_was_taught() {
    let text = writer_requirements();
    assert!(
        text.contains("Every question, its choices, its rationales and its explanation rest only on what this lesson teaches or on an earlier lesson's planned ideas, terms and actions in coursePlan; a learner who studied only those can answer it."),
        "{text}"
    );
}

#[test]
fn the_writer_is_told_question_reasons_say_what_is_true() {
    let text = writer_requirements();
    let sentence = "Every rationale and the explanation say what is true about this question's own case, using the names and values its prompt gives; never give what a source, an excerpt, a passage or a tutorial says or does not say as the reason.";
    assert!(text.contains(sentence), "missing {sentence:?} in {text}");
}

#[test]
fn the_writer_teaches_how_a_deliverable_is_recorded() {
    let text = writer_requirements();
    for expected in [
        "every step a deliverable needs, including how the learner records or saves their work",
        "a deliverable kept anywhere but the learner's own notes needs the way to record it there taught, here or in an earlier lesson's planned actions",
        "otherwise ask for the record in the learner's own notes",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in {text}");
    }
}

#[test]
fn the_writer_keeps_the_four_choices_close_in_length_and_in_their_own_words() {
    let text = writer_requirements();
    assert!(
        text.contains("The four choices are close in length, so the right one never stands out as the longest"),
        "{text}"
    );
    assert!(
        text.contains(
            "the right choice is worded in its own words rather than repeating the prompt's"
        ),
        "{text}"
    );
}

#[test]
fn the_writer_and_both_repairs_are_told_a_step_starts_from_what_the_steps_before_it_left() {
    for sources in [0, 2] {
        let schema = lesson_schema(sources);
        for (who, text) in [
            ("writer", writer_requirements()),
            (
                "teaching repair",
                crate::lessons::text_edits::instructions(&schema),
            ),
            (
                "claim repair",
                crate::lessons::text_edits::rewrite_instructions(&schema),
            ),
        ] {
            assert!(
                text.contains(STEPS_IN_ORDER),
                "{who} with {sources} sources: {text}"
            );
        }
    }
}

#[test]
fn the_writer_is_told_the_lesson_s_shape_under_its_own_key() {
    let program = crate::tests::fixture();
    let lesson = program.modules[0].lessons[0].clone();
    let prompt = writer_prompt(&program, &lesson);

    let lines = prompt["lessonShape"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(!lines.is_empty(), "the request has no lessonShape");
    let words: Vec<&str> = lines.iter().filter_map(Value::as_str).collect();
    let words = words.join("\n");
    assert!(
        words.contains("core ideas the objective depends on"),
        "{words}"
    );
    assert!(
        words.contains("multiple-choice items of each kind"),
        "{words}"
    );
    for requirement in prompt["requirements"].as_array().unwrap() {
        let requirement = requirement.as_str().unwrap_or_default();
        assert!(!requirement.contains("core ideas the objective depends on"));
        assert!(!requirement.contains("multiple-choice items of each kind"));
    }
}

#[test]
fn the_writer_s_shape_is_the_draft_s_in_the_policy_s_words() {
    use crate::lessons::shape::{self, Phase};
    let program = crate::tests::fixture();
    let lesson = program.modules[0].lessons[0].clone();
    assert_eq!(
        writer_prompt(&program, &lesson)[shape::WORDS],
        json!(shape::policy().asked(Phase::Draft))
    );
}

fn sweep_baseline() -> Value {
    let kinds = [
        "explanation",
        "worked_example",
        "contrast",
        "check",
        "explanation",
        "worked_example",
        "check",
        "guided_practice",
        "independent_practice",
        "reflection",
        "recap",
    ];
    let blocks: Vec<_> = kinds
        .iter()
        .enumerate()
        .map(|(i, kind)| {
            let title = format!("Part {i}");
            let mut section = block(kind, &title, &format!("Body of part {i}."), false);
            if kind.ends_with("practice") {
                section["rubric"] = json!([
                    {"title":"One","description":"Observable.","dimension":"recall"},
                    {"title":"Two","description":"Observable.","dimension":"application"}
                ]);
            }
            section
        })
        .collect();
    let questions: Vec<_> = ["practice", "practice", "quiz", "quiz", "test", "test"]
        .iter()
        .enumerate()
        .map(|(i, kind)| question(kind, &format!("Question {i}?"), false))
        .collect();
    json!({"blocks":blocks,"questions":questions})
}

fn kept_rules(lesson: &Value) -> Vec<Rule> {
    let policy = crate::lessons::shape::policy();
    let schema = policy.schema(crate::lessons::shape::Phase::Kept, 0);
    crate::teaching::structural_findings(
        crate::teaching::ReviewSchema::Lesson(crate::lessons::shape::LessonShape::new(
            &schema, policy,
        )),
        &lesson.to_string(),
        &Default::default(),
    )
    .unwrap()
    .into_iter()
    .filter_map(|finding| match finding.origin {
        crate::review_findings::Origin::Rule { rule, .. } => Some(rule),
        _ => None,
    })
    .collect()
}

/// One row for each rule a lesson can break: the defect and the rule that reports it.
#[test]
fn a_lesson_whose_findings_wait_is_still_saved_and_passes_the_save_gate() {
    type Mutation = fn(&mut Value);
    let rows: [(&str, Mutation, Rule); 17] = [
        (
            "blank block title",
            |l| l["blocks"][0]["title"] = json!("  "),
            Rule::TitleMissing,
        ),
        (
            "repeated block title",
            |l| l["blocks"][1]["title"] = json!("Part 0"),
            Rule::TitleRepeated,
        ),
        (
            "blank block body",
            |l| l["blocks"][0]["body"] = json!("  "),
            Rule::BlockBody,
        ),
        (
            "blank question prompt",
            |l| l["questions"][0]["prompt"] = json!("  "),
            Rule::PromptMissing,
        ),
        (
            "repeated question prompt",
            |l| l["questions"][1]["prompt"] = json!("Question 0?"),
            Rule::PromptRepeated,
        ),
        (
            "three choices",
            |l| l["questions"][0]["options"] = json!(["One", "Two", "Three"]),
            Rule::Choices,
        ),
        (
            "an identical choice twice",
            |l| l["questions"][0]["options"] = json!(["One", "Two", "Two", "Four"]),
            Rule::Choices,
        ),
        (
            "a blank choice",
            |l| l["questions"][0]["options"] = json!(["One", "Two", "  ", "Four"]),
            Rule::Choices,
        ),
        (
            "correctIndex out of range",
            |l| l["questions"][0]["correctIndex"] = json!(7),
            Rule::Schema,
        ),
        (
            "blank explanation",
            |l| l["questions"][0]["explanation"] = json!("  "),
            Rule::ExplanationMissing,
        ),
        (
            "reasons count differs from the choices",
            |l| {
                l["questions"][0]["rationales"] =
                    json!(["One fails here.", "", "Three fails here."])
            },
            Rule::RationaleCount,
        ),
        (
            "a wrong choice with no reason",
            |l| {
                l["questions"][0]["rationales"] =
                    json!(["", "", "Three fails here.", "Four fails here."])
            },
            Rule::RationaleMissing,
        ),
        (
            "a question kind missing",
            |l| {
                l["questions"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|q| q["kind"] != "test")
            },
            Rule::QuestionKinds,
        ),
        (
            "no questions at all",
            |l| l["questions"] = json!([]),
            Rule::QuestionKinds,
        ),
        (
            "no explanation",
            |l| {
                l["blocks"][0]["kind"] = json!("contrast");
                l["blocks"][4]["kind"] = json!("contrast");
            },
            Rule::NoExplanation,
        ),
        (
            "a rubric with a repeated criterion",
            |l| {
                let criterion = json!({"title":"Names the step","description":"Says which step comes next.","dimension":"recall"});
                l["blocks"][7]["rubric"] = json!([criterion.clone(), criterion]);
            },
            Rule::PracticeRubric,
        ),
        (
            "a rubric with five criteria",
            |l| {
                let criteria: Vec<Value> = (0..5).map(|n| json!({"title":format!("Criterion {n}"),"description":"Says which step comes next.","dimension":"recall"})).collect();
                l["blocks"][7]["rubric"] = json!(criteria);
            },
            Rule::PracticeRubric,
        ),
    ];
    let mut program = crate::test_support::fixture();
    program.sources.clear();
    for (name, mutate, rule) in rows {
        assert!(
            !kept_rules(&sweep_baseline()).contains(&rule),
            "{name}: the baseline already breaks {rule:?}"
        );
        let mut lesson = sweep_baseline();
        mutate(&mut lesson);
        assert!(
            kept_rules(&lesson).contains(&rule),
            "{name}: {rule:?} is not reported"
        );
        let generated: GeneratedLesson = serde_json::from_value(lesson).unwrap();
        let (prepared, _) = match assemble(generated, &[], &HashSet::new()) {
            Ok(assembled) => assembled,
            Err(error) => panic!("{name}: the lesson is not saved: {error:?}"),
        };
        let checked = crate::service::validate_generated(&prepared, &program);
        assert!(
            checked.is_ok(),
            "{name}: the save gate refuses it: {checked:?}"
        );
    }
}
