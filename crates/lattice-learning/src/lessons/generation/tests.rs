use super::recall::{parse_recall_drafts, recall_draft_schema};
use super::*;
use crate::lessons::course_generation_tests::length_bounds;

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

#[test]
fn course_context_carries_whole_objectives_and_every_earlier_recap() {
    use crate::dto::{LearningBlockDto, LearningBlockKind, LearningPreparation};
    let mut program = crate::tests::fixture();
    let long = "A detailed objective sentence. ".repeat(40);
    let recap = "Recap of what the lesson established. ".repeat(80);
    let lessons = &mut program.modules[0].lessons;
    let template = lessons[0].clone();
    for index in 0..4 {
        let mut extra = template.clone();
        extra.id = format!("extra-lesson-{index}");
        lessons.insert(0, extra);
    }
    let current = lessons.last().unwrap().id.clone();
    for lesson in lessons.iter_mut() {
        lesson.objective = long.clone();
        if lesson.id != current {
            lesson.preparation = LearningPreparation::Ready;
            lesson.blocks = vec![LearningBlockDto {
                kind: LearningBlockKind::Recap,
                title: "Recap".into(),
                body: recap.clone(),
                source_ids: vec![],
                rubric: vec![],
            }];
        }
    }
    let earlier = program.modules[0].lessons.len() - 1;
    assert!(earlier > 3, "more than the former three recaps");
    let context = super::lesson::course_context(&program, &current);
    assert_eq!(
        context["curriculum"][0]["lessons"][0]["objective"],
        long.as_str()
    );
    let recaps = context["recentTeaching"].as_array().unwrap();
    assert_eq!(recaps.len(), earlier);
    assert!(recaps
        .iter()
        .all(|entry| entry["recap"][0] == recap.as_str()));
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

    let error = super::lesson::prepare_lesson(&Window(24_000), &runtimes, &program, &lesson)
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

    let error = super::lesson::prepare_lesson(&Window(24_000), &runtimes, &program, &lesson)
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
        LearningBlockDto {
            kind: LearningBlockKind::Explanation,
            title: "A title that runs on and on. ".repeat(10),
            body: "A body that runs on and on without a figure in it. ".repeat(150),
            source_ids: vec![source_id.clone()],
            rubric: vec![],
        },
        LearningBlockDto {
            kind: LearningBlockKind::Recap,
            title: "Go".into(),
            body: "Ok".into(),
            source_ids: vec![source_id],
            rubric: vec![],
        },
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
    assert!(instructions.contains("Never name a source or an excerpt in the lesson's text"));
    assert!(instructions.contains("cite that sentence in the citations list"));
}
