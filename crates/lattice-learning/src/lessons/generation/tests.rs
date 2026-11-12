use super::recall::{parse_recall_drafts, recall_draft_prompt, recall_draft_schema};
use super::*;
use crate::dto::LearningProgramDto;
use crate::lessons::course_generation_tests::length_bounds;
use crate::lessons::course_plan::CoursePlan;

fn source() -> LearningSourceDto {
    LearningSourceDto {
        id: "source-a".into(), title: "Neutral reference".into(), url: None,
        excerpt: "A process records each observation with a stable identifier. Later review compares the new observation with the earlier record.".into(),
        acquired_at: 1,
    }
}

#[test]
fn rejects_unknown_source_reference() {
    assert!(evidence_source(
        &[source()],
        1,
        "A process records each observation with a stable identifier."
    )
    .is_err());
}

#[test]
fn rejects_quote_not_present_in_source() {
    assert!(evidence_source(
        &[source()],
        0,
        "A different observation appears in an unrelated document."
    )
    .is_err());
}

#[test]
fn accepts_whitespace_normalized_quote() {
    assert!(evidence_source(
        &[source()],
        0,
        "A process records each observation\nwith a stable identifier."
    )
    .is_ok());
}

#[test]
fn rejects_malformed_or_truncated_json_and_trailing_text() {
    assert!(parse_json::<serde_json::Value>("{\"modules\":[").is_err());
    assert!(parse_json::<serde_json::Value>("{} trailing").is_err());
    assert!(parse_json::<serde_json::Value>("```json\n{}\n```").is_ok());
    assert!(parse_json::<serde_json::Value>("Here it is:\n```json\n{}\n```").is_err());
    assert!(parse_json::<serde_json::Value>("```json\n{}\n```\nDone.").is_err());
    assert!(parse_json::<serde_json::Value>("```json\n{}").is_err());
}

#[test]
fn accepts_fenced_examples_inside_the_material() {
    // A lesson on tool schemas teaches with fenced JSON. Schema-constrained
    // output is bare JSON, so the fence is inside a string, not around it.
    let body = r#"Define the tool:\n\n```json\n{\"name\": \"get_weather\"}\n```\n\nThen call it:\n\n```python\nclient.call()\n```"#;
    let bare = format!(r#"{{"body":"{body}"}}"#);
    let parsed: serde_json::Value = parse_json(&bare).expect("bare answer with fenced examples");
    assert!(parsed["body"]
        .as_str()
        .unwrap()
        .contains("```json\n{\"name\""));
    let fenced = format!("```json\n{bare}\n```");
    assert_eq!(parse_json::<serde_json::Value>(&fenced).unwrap(), parsed);
}

#[test]
fn outline_citations_are_optional_but_exact_when_present() {
    let sources = [source()];
    let exact = "A process records each observation with a stable identifier.";
    assert!(optional_citation(&sources, None, "").unwrap().is_none());
    assert!(optional_citation(&sources, Some(0), exact)
        .unwrap()
        .is_some());
    // A quote without a source, a source without a match, or any citation
    // when there are no sources is a fabrication.
    assert!(optional_citation(&sources, None, exact).is_err());
    assert!(optional_citation(
        &sources,
        Some(0),
        "Words that appear nowhere in the saved text."
    )
    .is_err());
    assert!(optional_citation(&sources, Some(1), exact).is_err());
    assert!(optional_citation(&[], Some(0), exact).is_err());
    assert!(optional_citation(&[], None, "").unwrap().is_none());
    let with_sources = outline_schema(None, 1).to_string();
    assert!(with_sources.contains("\"anyOf\":[{\"type\":\"null\"}"));
    let without = outline_schema(None, 0).to_string();
    assert!(!without.contains("anyOf"));
    assert!(without.contains("\"sourceIndex\":{\"type\":\"null\"}"));
}

#[test]
fn decode_outline_accepts_null_citations_with_sources_and_rejects_quotes_without_a_source() {
    let request = GenerateLearningProgramRequestDto {
        goal: "Record observations".into(),
        prior_knowledge: String::new(),
        minutes_per_session: 30,
        document_ids: vec![],
        source_urls: vec![],
        course_depth: None,
    };
    let raw = crate::lessons::course_generation_tests::outline(2, 2).0;
    let sources = [source()];
    let modules = decode_outline(&raw, &request, &sources).unwrap();
    assert_eq!(modules.len(), 2);
    let mut cited: serde_json::Value = serde_json::from_str(&raw).unwrap();
    cited["modules"][0]["summary"]["sourceIndex"] = json!(0);
    cited["modules"][0]["summary"]["quote"] =
        json!("A process records each observation with a stable identifier.");
    assert!(decode_outline(&cited.to_string(), &request, &sources).is_ok());
    let mut orphan: serde_json::Value = serde_json::from_str(&raw).unwrap();
    orphan["modules"][0]["outcomes"][0]["quote"] =
        json!("A process records each observation with a stable identifier.");
    assert!(decode_outline(&orphan.to_string(), &request, &sources).is_err());
    assert!(decode_outline(&cited.to_string(), &request, &[]).is_err());
}

#[test]
fn decode_outline_names_the_modules_and_lessons_whose_titles_repeat() {
    let request = GenerateLearningProgramRequestDto {
        goal: "Record observations".into(),
        prior_knowledge: String::new(),
        minutes_per_session: 30,
        document_ids: vec![],
        source_urls: vec![],
        course_depth: None,
    };
    let raw = crate::lessons::course_generation_tests::outline(2, 2).0;
    let outline: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let reason =
        |outline: &serde_json::Value| match decode_outline(&outline.to_string(), &request, &[]) {
            Err(AppError::InvalidInput(reason)) => reason,
            Err(other) => panic!("expected a refusal, got {other}"),
            Ok(_) => panic!("expected a refusal"),
        };
    // Titles that differ only in case and spacing are the same title.
    let mut modules = outline.clone();
    modules["modules"][0]["title"] = json!("experimental  DESIGN module 1");
    let refused = reason(&modules);
    assert!(
        refused.starts_with("Modules 1 and 2 share the title \"Experimental design module 1\""),
        "{refused}"
    );
    assert!(!refused.contains("Try again"), "{refused}");
    let mut lessons = outline;
    lessons["modules"][1]["lessons"][1]["title"] = json!("Experimental design 0.0");
    let refused = reason(&lessons);
    assert!(
        refused.starts_with("Lessons 1.1 and 2.2 share the title"),
        "{refused}"
    );
}

#[test]
fn generated_ids_are_not_accepted_from_model_schema() {
    let schema = outline_schema(None, 1);
    assert!(schema.to_string().contains("additionalProperties"));
    assert!(!schema.to_string().contains("\"id\""));
}

#[test]
fn goal_and_prior_knowledge_have_no_character_limit() {
    let mut request = GenerateLearningProgramRequestDto {
        goal: "Understand the material".into(),
        prior_knowledge: "".into(),
        minutes_per_session: 30,
        document_ids: vec![],
        source_urls: vec![],
        course_depth: None,
    };
    assert!(validate_outline_request(&request).is_ok());
    request.goal = "goal ".repeat(1_000);
    request.prior_knowledge = "prior knowledge ".repeat(1_000);
    assert!(validate_outline_request(&request).is_ok());
}

#[test]
fn rejects_duplicate_or_empty_program_source_ids() {
    let one = source();
    assert!(validate_source_snapshot_ids(&[one.clone(), one]).is_err());
    let mut empty = source();
    empty.id.clear();
    assert!(validate_source_snapshot_ids(&[empty]).is_err());
}

fn recall_response(cards: serde_json::Value) -> String {
    json!({"cards":cards}).to_string()
}

fn valid_recall_card(question: &str) -> serde_json::Value {
    recall_card(
        question,
        "A stable identifier links the observation to its record.",
        json!(["source-a"]),
    )
}

fn recall_card(question: &str, answer: &str, source_ids: serde_json::Value) -> serde_json::Value {
    json!({
        "question":question,
        "answer":answer,
        "explanation":"The excerpt says that each observation has a stable identifier.",
        "sourceIds":source_ids,
        "quote":"A process records each observation with a stable identifier."
    })
}

#[test]
fn recall_drafts_require_exact_count_and_lesson_source_ids() {
    let allowed = vec![source()];
    assert!(parse_recall_drafts(
        &recall_response(json!([valid_recall_card("What links the observation?")])),
        2,
        &allowed,
    )
    .is_err());
    assert!(parse_recall_drafts(
        &recall_response(json!([
            valid_recall_card("First question?"),
            recall_card(
                "What links the observation?",
                "A stable identifier.",
                json!(["source-outside-lesson"])
            )
        ])),
        2,
        &allowed,
    )
    .is_err());
}

#[test]
fn recall_drafts_reject_duplicate_questions_and_empty_fields() {
    let allowed = vec![source()];
    assert!(parse_recall_drafts(
        &recall_response(json!([
            valid_recall_card("What links the observation?"),
            valid_recall_card("  What links the   observation? ")
        ])),
        2,
        &allowed,
    )
    .is_err());
    let unsupported_quote = json!({
        "question":"Question with unsupported quote?",
        "answer":"A stable identifier.",
        "explanation":"The excerpt supports the identifier relationship.",
        "sourceIds":["source-a"],
        "quote":"A different observation appears in an unrelated document."
    });
    assert!(parse_recall_drafts(
        &recall_response(json!([
            valid_recall_card("First question?"),
            unsupported_quote
        ])),
        2,
        &allowed,
    )
    .is_err());
    assert!(parse_recall_drafts(
        &recall_response(json!([
            valid_recall_card("First question?"),
            recall_card("Which record is linked?", "  ", json!(["source-a"]))
        ])),
        2,
        &allowed,
    )
    .is_err());
}

#[test]
fn recall_schema_has_only_question_answer_explanation_and_allowed_source_ids() {
    let allowed = HashSet::from(["source-a".to_owned()]);
    let schema = recall_draft_schema(2, &allowed).to_string();
    assert!(schema.contains("question"));
    assert!(schema.contains("answer"));
    assert!(schema.contains("explanation"));
    assert!(schema.contains("quote"));
    assert!(schema.contains("source-a"));
    assert!(!schema.contains("options"));
    assert!(!schema.contains("correctIndex"));
}

#[test]
fn recall_parser_rejects_unknown_fields_and_choice_payloads() {
    let allowed = vec![source()];
    let with_choices = json!({
        "question":"A question?",
        "answer":"A stable identifier.",
        "explanation":"A supported explanation.",
        "sourceIds":["source-a"],
        "quote":"A process records each observation with a stable identifier.",
        "options":["one","two"]
    });
    assert!(parse_recall_drafts(&recall_response(json!([with_choices])), 1, &allowed,).is_err());
}

/// A course of eight lessons, the first module holding six, each planned with topics of its own.
fn planned_course() -> (LearningProgramDto, CoursePlan) {
    use crate::lessons::course_plan::PlannedLesson;
    let mut program = crate::tests::fixture();
    let lessons = &mut program.modules[0].lessons;
    let template = lessons[0].clone();
    for index in 0..4 {
        let mut extra = template.clone();
        extra.id = format!("extra-lesson-{index}");
        lessons.insert(0, extra);
    }
    let long = "A detailed objective sentence. ".repeat(40);
    for lesson in lessons.iter_mut() {
        lesson.objective = long.clone();
    }
    let plan = CoursePlan {
        lessons: program
            .modules
            .iter()
            .flat_map(|module| module.lessons.iter())
            .map(|lesson| PlannedLesson {
                lesson_id: lesson.id.clone(),
                ideas: vec![format!("ideas of {}", lesson.id)],
                terms: vec![format!("terms of {}", lesson.id)],
                actions: vec![format!("actions of {}", lesson.id)],
                contrast: format!("contrast of {}", lesson.id),
            })
            .collect(),
    };
    (program, plan)
}

#[test]
fn the_writers_context_has_whole_objectives_and_the_plan_by_position() {
    let (program, plan) = planned_course();
    let ids: Vec<String> = program
        .modules
        .iter()
        .flat_map(|module| module.lessons.iter())
        .map(|lesson| lesson.id.clone())
        .collect();
    let long = "A detailed objective sentence. ".repeat(40);

    let context = super::lesson::course_context(&program, &ids[2], &plan);

    assert_eq!(
        context["curriculum"][0]["lessons"][0]["objective"],
        long.as_str()
    );
    assert!(context.get("recentTeaching").is_none(), "{context}");
    let planned = context["coursePlan"].as_array().unwrap();
    assert_eq!(planned.len(), ids.len(), "every lesson, in course order");
    let positions: Vec<_> = planned
        .iter()
        .map(|entry| entry["position"].as_str().unwrap())
        .collect();
    assert_eq!(
        positions,
        ["earlier", "earlier", "this", "later", "later", "later", "later", "later"]
    );
    for (entry, id) in planned.iter().zip(&ids) {
        assert!(entry.get("lessonId").is_none(), "{entry}");
        assert_eq!(entry["ideas"][0], format!("ideas of {id}"));
        assert_eq!(entry["terms"][0], format!("terms of {id}"));
        assert_eq!(entry["actions"][0], format!("actions of {id}"));
        assert_eq!(entry["contrast"], format!("contrast of {id}"));
    }
    assert_eq!(
        planned[2]["title"],
        program.modules[0].lessons[2].title.as_str()
    );
}

#[test]
fn the_writers_context_is_the_same_whether_or_not_earlier_lessons_are_ready() {
    use crate::dto::{LearningBlockDto, LearningBlockKind, LearningPreparation};
    let (mut program, plan) = planned_course();
    let current = program.modules[0].lessons[4].id.clone();
    let before = super::lesson::course_context(&program, &current, &plan);

    for lesson in &mut program.modules[0].lessons[..4] {
        lesson.preparation = LearningPreparation::Ready;
        lesson.blocks = vec![LearningBlockDto::new(
            LearningBlockKind::Recap,
            "Recap".into(),
            "Recap of what the lesson established.".into(),
            vec![],
            vec![],
        )];
    }
    let after = super::lesson::course_context(&program, &current, &plan);

    assert_eq!(before, after);
    assert!(!after
        .to_string()
        .contains("Recap of what the lesson established."));
}

/// A model that is only a window: the refusal comes before any model call.
struct Window(usize);

#[async_trait::async_trait]
impl LLMPort for Window {
    async fn complete(
        &self,
        _request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        unreachable!("a lesson with no room for evidence is refused before any model call")
    }
    fn model_name(&self) -> &str {
        "Ornith-1.5-9B"
    }
    fn max_context_tokens(&self) -> usize {
        self.0
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_lesson_whose_prompt_leaves_no_room_for_evidence_is_refused_up_front() {
    let mut program = crate::tests::fixture();
    program.summary.goal = "Understand every part of the subject in depth. ".repeat(2_000);
    let lesson = program.modules[0].lessons[0].clone();
    let runtimes = crate::embedded_runtime::BuiltinRuntimes::for_tests();

    let error = super::lesson::prepare_lesson(
        &Window(24_000),
        &runtimes,
        &program,
        &lesson,
        &CoursePlan::blank(&program),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(&error, AppError::InvalidConfig(message)
            if message.starts_with("This course's goal and the lesson brief leave no room for evidence in Ornith-1.5-9B's 24000-token context window.")),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_lesson_outline_of_any_length_is_not_called_incomplete() {
    let mut program = crate::tests::fixture();
    program.summary.goal = "Understand every part of the subject in depth. ".repeat(2_000);
    let lesson = &mut program.modules[0].lessons[0];
    lesson.title = "A lesson title that runs on and on. ".repeat(10);
    lesson.objective = "An objective that runs on and on. ".repeat(50);
    let lesson = program.modules[0].lessons[0].clone();
    let runtimes = crate::embedded_runtime::BuiltinRuntimes::for_tests();

    let error = super::lesson::prepare_lesson(
        &Window(24_000),
        &runtimes,
        &program,
        &lesson,
        &CoursePlan::blank(&program),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(&error, AppError::InvalidConfig(message)
            if message.starts_with("This course's goal and the lesson brief leave no room for evidence")),
        "{error:?}"
    );
}

#[test]
fn a_verbatim_quote_longer_than_1200_characters_is_read_not_refused() {
    let excerpt = "A process records each observation with a stable identifier. ".repeat(30);
    let quote = excerpt.trim();
    assert!(quote.chars().count() > 1200);
    assert!(validate_excerpt_quote(quote, &excerpt).is_ok());
    // A quote still needs its five words and must still be in the excerpt.
    assert!(validate_excerpt_quote("Too short.", &excerpt).is_err());
    assert!(validate_excerpt_quote(
        "A process records each observation with a different identifier.",
        &excerpt
    )
    .is_err());
}

fn outline_request() -> GenerateLearningProgramRequestDto {
    GenerateLearningProgramRequestDto {
        goal: "Record observations".into(),
        prior_knowledge: String::new(),
        minutes_per_session: 30,
        document_ids: vec![],
        source_urls: vec![],
        course_depth: None,
    }
}

/// The two-module outline fixture with the text at `pointer` set to `text`.
fn decoded_with(pointer: &str, text: &str) -> Result<()> {
    let raw = crate::lessons::course_generation_tests::outline(2, 2).0;
    let mut outline: serde_json::Value = serde_json::from_str(&raw).unwrap();
    *outline.pointer_mut(pointer).unwrap() = json!(text);
    decode_outline(&outline.to_string(), &outline_request(), &[]).map(|_| ())
}

#[test]
fn a_module_or_lesson_text_of_any_length_is_read_not_refused() {
    for (pointer, text) in [
        (
            "/modules/0/title",
            "A module title that runs on. ".repeat(10),
        ),
        (
            "/modules/0/summary/text",
            "A summary that runs on and on. ".repeat(70),
        ),
        (
            "/modules/0/outcomes/0/text",
            "An outcome that runs on and on. ".repeat(200),
        ),
        (
            "/modules/0/lessons/0/title",
            "A lesson title that runs on. ".repeat(10),
        ),
        (
            "/modules/0/lessons/0/objective",
            "An objective that runs on and on. ".repeat(50),
        ),
    ] {
        let decoded = decoded_with(pointer, &text);
        assert!(decoded.is_ok(), "{pointer}: {:?}", decoded.as_ref().err());
    }
}

#[test]
fn a_module_or_lesson_text_of_two_letters_is_read_not_refused() {
    for (pointer, text) in [
        ("/modules/0/title", "AI"),
        ("/modules/0/summary/text", "Hi"),
        ("/modules/0/outcomes/0/text", "OK"),
        ("/modules/0/lessons/0/title", "Go"),
        ("/modules/0/lessons/0/objective", "Do"),
    ] {
        let decoded = decoded_with(pointer, text);
        assert!(decoded.is_ok(), "{pointer}: {:?}", decoded.as_ref().err());
    }
    assert!(decoded_with("/modules/0/title", "  ").is_err());
}

#[test]
fn a_recall_question_answer_or_explanation_of_any_length_is_read_not_refused() {
    let allowed = [source()];
    let long = "A stable identifier links the observation to its record. ".repeat(60);
    for field in ["question", "answer", "explanation"] {
        let mut card = valid_recall_card("What links the observation?");
        card[field] = json!(long);
        let parsed = parse_recall_drafts(&recall_response(json!([card])), 1, &allowed);
        assert!(parsed.is_ok(), "{field}: {:?}", parsed.as_ref().err());
    }
    let short = recall_card("What is six times seven?", "42", json!(["source-a"]));
    let parsed = parse_recall_drafts(&recall_response(json!([short])), 1, &allowed);
    assert!(parsed.is_ok(), "{:?}", parsed.as_ref().err());
}

#[tokio::test]
async fn a_lesson_block_of_any_length_does_not_stop_recall_drafts() {
    use crate::dto::{
        LearningBlockDto, LearningBlockKind, LearningPreparation, LearningProgramStatus,
    };
    use crate::lessons::course_generation_tests::ScriptedModel;
    let mut program = crate::tests::fixture();
    program.summary.status = LearningProgramStatus::Active;
    let source_id = program.sources[0].id.clone();
    let lesson = &mut program.modules[0].lessons[0];
    lesson.preparation = LearningPreparation::Ready;
    lesson.blocks = vec![
        LearningBlockDto::new(
            LearningBlockKind::Explanation,
            "A title that runs on and on. ".repeat(10),
            "A body that runs on and on without a figure in it. ".repeat(150),
            vec![source_id.clone()],
            vec![],
        ),
        LearningBlockDto::new(
            LearningBlockKind::Recap,
            "Go".into(),
            "Ok".into(),
            vec![source_id],
            vec![],
        ),
    ];
    let lesson_id = lesson.id.clone();
    let model = ScriptedModel {
        outputs: Default::default(),
        prompts: Default::default(),
    };

    let result = generate_recall_drafts(&model, &program, &lesson_id, 2).await;

    assert!(
        !matches!(&result, Err(AppError::InvalidInput(message))
            if message.contains("invalid teaching block")),
        "{result:?}"
    );
}

#[test]
fn the_outline_schema_bounds_no_text_but_a_blank() {
    for sources in [0, 1] {
        let bounds = length_bounds(&outline_schema(None, sources));
        assert!(bounds.is_empty(), "{sources} sources: {bounds:?}");
    }
}

#[test]
fn the_recall_schema_bounds_no_text_but_the_quotes_floor() {
    let allowed = HashSet::from(["source-a".to_owned()]);
    let schema = recall_draft_schema(2, &allowed);
    let floor = "/properties/cards/items/properties/quote/minLength";
    assert_eq!(schema.pointer(floor), Some(&json!(MIN_QUOTE_CHARS)));
    let bounds: Vec<_> = length_bounds(&schema)
        .into_iter()
        .filter(|bound| !bound.starts_with(floor))
        .collect();
    assert!(bounds.is_empty(), "{bounds:?}");
}

#[test]
fn the_writer_is_told_never_to_name_a_source_in_the_text() {
    let instructions = citation_instructions(true);
    assert!(instructions.contains("Never say in the lesson's text where a statement comes from"));
    assert!(instructions.contains("never refer to an excerpt, passage or tutorial"));
    assert!(instructions.contains("cite that sentence in the citations list"));
}

#[test]
fn recall_cards_are_written_for_a_learner_who_sees_only_the_lesson() {
    let lesson = crate::tests::fixture().modules[0].lessons[0].clone();
    let prompt = recall_draft_prompt(&lesson, 2, &HashSet::new(), Vec::new());
    for expected in [
        "The learner sees a card and the lesson, never the source excerpts.",
        "never point at an excerpt, passage or tutorial the learner cannot see",
    ] {
        assert!(
            prompt.contains(expected),
            "missing {expected:?} in {prompt}"
        );
    }
}

#[test]
fn the_writer_and_the_repairs_are_told_quoted_wording_takes_double_quotation_marks() {
    let schema = lesson_schema(2);
    for (who, text) in [
        ("writer", citation_instructions(true)),
        (
            "teaching repair",
            crate::lessons::text_edits::instructions(&schema),
        ),
        (
            "claim repair",
            crate::lessons::text_edits::rewrite_instructions(&schema),
        ),
    ] {
        assert!(text.contains("double quotation marks"), "{who}: {text}");
        assert!(
            text.contains("never put wording in single quotation marks"),
            "{who}"
        );
    }
}

#[test]
fn the_writer_is_told_quotation_marks_are_for_quoted_wording() {
    for has_sources in [true, false] {
        let instructions = citation_instructions(has_sources);
        assert!(
            instructions.contains(if has_sources {
                "Wording quoted from a source goes in double quotation marks, and only in a cited sentence"
            } else {
                "Quotation marks mark wording quoted from a source"
            }),
            "{has_sources}"
        );
        assert!(
            instructions.contains("without quotation marks"),
            "{has_sources}"
        );
    }
}

#[test]
fn the_writer_is_told_to_name_no_source_by_name_either() {
    let instructions = citation_instructions(true);
    assert!(instructions.contains("In the project's tutorial"));
    assert!(instructions.contains("According to the guide"));
    assert!(!instructions.contains("an attribution to a source"));
}

#[test]
fn the_writer_and_the_teaching_repair_are_told_what_the_lesson_s_own_figures_are() {
    let schema = lesson_schema(2);
    for (who, text) in [
        ("writer", citation_instructions(true)),
        (
            "teaching repair",
            crate::lessons::text_edits::instructions(&schema),
        ),
    ] {
        assert!(
            text.contains("The lesson's own figures are the quantities its own example, scenario or exercise sets, and the results calculated from them."),
            "{who}: {text}"
        );
    }
}

#[test]
fn the_outline_contract_reads_what_it_renders() {
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<outline::OutlineDescription>(&serde_json::from_value(json!({
        "modules":[{
            "title":"A module",
            "summary":{"text":"What it covers.","sourceIndex":0,"quote":"A quote."},
            "outcomes":[{"text":"An outcome.","sourceIndex":null,"quote":""}],
            "prerequisiteIndices":[],
            "project":{"title":"A project","brief":"Its brief.","deliverables":["One"],"successCriteria":["Works"]},
            "lessons":[{"title":"A lesson","objective":"Its objective.","estimatedMinutes":20,"sourceIndex":0,"quote":"A quote."}]
        }]
    })).unwrap());
}

#[test]
fn the_recall_contract_reads_what_it_renders() {
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        recall::RecallDrafts,
    >(
        &serde_json::from_value(json!({
            "cards":[{
                "question":"What is it?","answer":"This.","explanation":"Because.",
                "sourceIds":["source-a"],"quote":"A quote."
            }]
        }))
        .unwrap(),
    );
}
