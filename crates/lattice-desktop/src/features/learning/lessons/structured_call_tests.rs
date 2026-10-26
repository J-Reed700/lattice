//! The two-step call against a recording model: what each step sends, when a
//! call stays in one step, and how a saved draft resumes.
#![allow(clippy::indexing_slicing)]
use super::*;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use std::sync::Mutex;

const SYSTEM: &str = "Teach one idea to a beginner.";
const PROMPT: &str = r#"{"task":"Explain why the sky looks blue","priorKnowledge":"none"}"#;
const BRIEF: &str = "Work out the explanation step by step and check each step.";
const DRAFT: &str = "Sunlight scatters off air molecules, and shorter wavelengths scatter most.";
const ANSWER: &str = r#"{"answer":"Air scatters blue light more than red light."}"#;

/// Answers the prose step with a draft and the schema step with an answer, or
/// fails the schema step as a dropped connection would. Schema steps answer
/// with `first_answers`, in order, before `answer`. With `exact`, its
/// tokenizer counts twice the tokens its estimate does.
struct Recorder {
    window: usize,
    exact: bool,
    /// Whether it thinks before every answer, as Claude does.
    thinks: bool,
    draft: CompletionResponse,
    answer: Option<&'static str>,
    first_answers: Mutex<Vec<&'static str>>,
    sent: Mutex<Vec<CompletionRequest>>,
}

impl Recorder {
    fn new(window: usize) -> Self {
        Self {
            window,
            exact: false,
            thinks: false,
            draft: CompletionResponse::from_text(DRAFT),
            answer: Some(ANSWER),
            first_answers: Mutex::new(Vec::new()),
            sent: Mutex::new(Vec::new()),
        }
    }

    fn answering_first(window: usize, answers: &[&'static str]) -> Self {
        let recorder = Self::new(window);
        *recorder.first_answers.lock().unwrap() = answers.to_vec();
        recorder
    }

    fn sent(&self) -> Vec<CompletionRequest> {
        self.sent
            .lock()
            .map(|sent| sent.clone())
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl LLMPort for Recorder {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if let Ok(mut sent) = self.sent.lock() {
            sent.push(request.clone());
        }
        if request.json_schema.is_none() {
            return Ok(self.draft.clone());
        }
        if let Some(first) = self
            .first_answers
            .lock()
            .ok()
            .filter(|answers| !answers.is_empty())
            .map(|mut answers| answers.remove(0))
        {
            return Ok(CompletionResponse::from_text(first));
        }
        self.answer
            .map(CompletionResponse::from_text)
            .ok_or_else(|| AppError::Network("fixture connection dropped".into()))
    }
    fn model_name(&self) -> &str {
        "two-step-recorder"
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        Ok(if self.exact {
            text.len() / 2
        } else {
            self.count_tokens(text)
        })
    }
    fn counts_tokens_exactly(&self) -> bool {
        self.exact
    }
    fn thinks_before_answering(&self) -> bool {
        self.thinks
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["answer"],"properties":{"answer":{"type":"string"}}})
}

async fn call(llm: &Recorder, kind: CallKind) -> Result<String> {
    complete_reasoned_json(
        llm,
        SYSTEM,
        PROMPT.into(),
        schema(),
        1000,
        None,
        ReasonFirst {
            kind,
            subject: "the explanation",
            brief: BRIEF,
        },
    )
    .await
}

fn messages(request: &CompletionRequest) -> Vec<(&str, &str)> {
    request
        .input
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { role, content } => Some((role.as_str(), content.as_str())),
            _ => None,
        })
        .collect()
}

fn two_step_system() -> String {
    format!("{SYSTEM}\n\n{TWO_STEPS} {BRIEF} {DRAFT_CLOSE}")
}

/// What a request would say if it still asked for the format in words.
const FORMAT_DEMANDS: [&str; 6] = [
    "JSON",
    "schema",
    "format",
    "Return only",
    "no prose",
    "no other text",
];

#[tokio::test]
async fn the_draft_is_answered_from_under_the_schema_with_no_format_named() -> Result<()> {
    let llm = Recorder::new(32_000);
    let answer = call(&llm, CallKind::Authoring).await?;
    assert_eq!(answer, ANSWER);

    let sent = llm.sent();
    assert_eq!(sent.len(), 2, "one prose step, one answer step");
    let (draft, structured) = (&sent[0], &sent[1]);
    let system = two_step_system();

    assert_eq!(draft.json_schema, None);
    assert!(
        draft.sampling.is_none(),
        "the draft samples as the chat does"
    );
    assert_eq!(draft.max_output_tokens, Some(1000));
    assert_eq!(
        messages(draft),
        vec![("system", system.as_str()), ("user", PROMPT)]
    );

    // The answer step continues the draft's conversation, so it opens with
    // exactly the messages the server has just read.
    assert_eq!(structured.json_schema, Some(schema()));
    assert_eq!(
        messages(structured),
        vec![
            ("system", system.as_str()),
            ("user", PROMPT),
            ("assistant", DRAFT),
            ("user", ANSWER_STEP),
        ]
    );
    let sampling = structured.sampling.unwrap_or_default();
    assert_eq!(sampling.temperature, Some(0.0));
    assert_eq!(sampling.top_k, Some(1));
    for request in [draft, structured] {
        for (_, content) in messages(request) {
            for demand in FORMAT_DEMANDS {
                assert!(
                    !content.contains(demand),
                    "a two-step request still names a format: {demand}"
                );
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_window_without_room_for_a_draft_answers_in_one_step() -> Result<()> {
    // Room for the prompt and one answer, not for a draft beside it.
    let llm = Recorder::new(2_400);
    assert_eq!(call(&llm, CallKind::Authoring).await?, ANSWER);
    let sent = llm.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].json_schema, Some(schema()));
    assert_eq!(
        messages(&sent[0]),
        vec![("system", SYSTEM), ("user", PROMPT)]
    );
    Ok(())
}

#[tokio::test]
async fn an_answer_step_only_the_exact_count_rejects_falls_back_to_one_step() -> Result<()> {
    // The estimate holds this draft beside the task and its answer; the
    // tokenizer counts it at twice that and does not.
    let mut llm = Recorder::new(8_192);
    llm.exact = true;
    llm.draft = CompletionResponse::from_text("Sunlight scatters off the air. ".repeat(600));
    assert_eq!(call(&llm, CallKind::Authoring).await?, ANSWER);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].json_schema, None);
    assert_eq!(sent[1].json_schema, Some(schema()));
    assert_eq!(
        messages(&sent[1]),
        vec![("system", SYSTEM), ("user", PROMPT)]
    );
    Ok(())
}

#[tokio::test]
async fn each_kind_follows_its_knob() -> Result<()> {
    for (kind, reasons_first) in [
        (CallKind::Authoring, model_call::REASON_FIRST_AUTHORING),
        (CallKind::Classifier, model_call::REASON_FIRST_CLASSIFIERS),
    ] {
        let llm = Recorder::new(32_000);
        call(&llm, kind).await?;
        assert_eq!(llm.sent().len(), if reasons_first { 2 } else { 1 });
    }
    Ok(())
}

/// A model that thinks before every answer reasons first already, and
/// Claude refuses the prose draft as a request for its chain of thought, so
/// the call is made in one step.
#[tokio::test]
async fn a_model_that_thinks_before_answering_is_asked_in_one_step() -> Result<()> {
    let mut llm = Recorder::new(32_000);
    llm.thinks = true;
    assert_eq!(call(&llm, CallKind::Authoring).await?, ANSWER);
    let sent = llm.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].json_schema, Some(schema()));
    assert_eq!(
        messages(&sent[0]),
        vec![("system", SYSTEM), ("user", PROMPT)]
    );
    Ok(())
}

/// Claude's classifier can decline a draft partway, with some text written;
/// the call answers in one step rather than from a draft that was refused.
#[tokio::test]
async fn a_declined_draft_falls_back_to_the_one_step_request() -> Result<()> {
    let mut llm = Recorder::new(32_000);
    llm.draft = CompletionResponse {
        text: "Sunlight scatters".into(),
        finish_reason: "refusal".into(),
        ..Default::default()
    };
    assert_eq!(call(&llm, CallKind::Authoring).await?, ANSWER);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(
        messages(&sent[1]),
        vec![("system", SYSTEM), ("user", PROMPT)]
    );
    Ok(())
}

/// A declined answer is unusable, not malformed: the lesson's passes work
/// around it as they do a cut-off one.
#[test]
fn a_declined_answer_is_unusable() {
    assert!(matches!(
        reject_incomplete_finish_reason("refusal"),
        Err(AppError::ModelAnswerUnusable(_))
    ));
    assert!(reject_incomplete_finish_reason("end_turn").is_ok());
}

#[tokio::test]
async fn a_draft_that_ran_out_of_room_is_still_answered_from() -> Result<()> {
    let mut llm = Recorder::new(32_000);
    llm.draft = CompletionResponse {
        text: "Sunlight scatters off air molecules, and".into(),
        finish_reason: "length".into(),
        ..Default::default()
    };
    assert_eq!(call(&llm, CallKind::Authoring).await?, ANSWER);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    assert!(messages(&sent[1]).contains(&("assistant", "Sunlight scatters off air molecules, and")));
    Ok(())
}

#[tokio::test]
async fn an_empty_draft_falls_back_to_the_one_step_request() -> Result<()> {
    let mut llm = Recorder::new(32_000);
    llm.draft = CompletionResponse::from_text("  ");
    assert_eq!(call(&llm, CallKind::Authoring).await?, ANSWER);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(
        messages(&sent[1]),
        vec![("system", SYSTEM), ("user", PROMPT)]
    );
    Ok(())
}

#[tokio::test]
async fn a_lesson_job_interrupted_before_the_answer_resumes_from_its_saved_draft() -> Result<()> {
    use crate::features::learning::{
        curriculum::LearningGenerationJobKind, curriculum_repository::LearningCurriculumRepository,
        dto::AcceptLearningProgramRequestDto, plan_dto::StartLearningGenerationJobRequestDto,
        repository::LearningRepository,
    };
    use lattice_core::shared::runtime::jobs::JobStore;
    let pool = crate::features::learning::tests::pool().await?;
    let programs = LearningRepository::new(pool.clone());
    let program = crate::features::learning::tests::fixture();
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program = programs.get(&program.summary.id).await?;
    let lesson = program
        .modules
        .first()
        .and_then(|module| module.lessons.first())
        .map(|lesson| lesson.id.clone())
        .ok_or_else(|| AppError::InternalError("fixture has no lesson".into()))?;
    let jobs = LearningCurriculumRepository::new(pool.clone());
    let job = jobs
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: json!({ "lessonIds": [lesson] }).to_string(),
            progress_total: 1,
        })
        .await?;
    JobStore::new(pool.clone()).claim(&job.id).await?;

    let mut interrupted = Recorder::new(32_000);
    interrupted.answer = None;
    let first = lesson_drafts::run(
        &jobs,
        &job.id,
        &lesson,
        call(&interrupted, CallKind::Authoring),
    )
    .await;
    assert!(matches!(first, Err(AppError::Network(_))));
    assert_eq!(interrupted.sent().len(), 2);

    let resumed = Recorder::new(32_000);
    let answer =
        lesson_drafts::run(&jobs, &job.id, &lesson, call(&resumed, CallKind::Authoring)).await?;
    assert_eq!(answer, ANSWER);
    let sent = resumed.sent();
    assert_eq!(sent.len(), 1, "the saved draft is not written again");
    assert_eq!(sent[0].json_schema, Some(schema()));
    assert!(messages(&sent[0]).contains(&("assistant", DRAFT)));
    Ok(())
}

#[test]
fn finish_reasons_that_cut_the_answer_short_are_rejected() {
    for reason in [
        "length",
        "max_tokens",
        "MAX_OUTPUT_TOKENS",
        "incomplete",
        "content_filter",
    ] {
        assert!(reject_incomplete_finish_reason(reason).is_err(), "{reason}");
    }
    assert!(reject_incomplete_finish_reason("stop").is_ok());
    assert!(cut_off("length") && !cut_off("content_filter"));
}

#[test]
fn the_fixture_view_of_an_answer_step_is_the_one_step_task() {
    let input = |messages: &[(&str, &str)]| CompletionRequest {
        input: messages
            .iter()
            .map(|(role, content)| CompletionInput::Message {
                role: (*role).into(),
                content: (*content).into(),
            })
            .collect(),
        json_schema: Some(schema()),
        ..Default::default()
    };
    let system = two_step_system();
    let answer = input(&[
        ("system", system.as_str()),
        ("user", PROMPT),
        ("assistant", DRAFT),
        ("user", ANSWER_STEP),
    ]);
    assert_eq!(task_messages(&answer), (SYSTEM, PROMPT));
    assert!(!is_draft_request(&answer));
    let mut draft = input(&[("system", system.as_str()), ("user", PROMPT)]);
    draft.json_schema = None;
    assert!(is_draft_request(&draft));
    let direct = input(&[("system", SYSTEM), ("user", PROMPT)]);
    assert_eq!(task_messages(&direct), (SYSTEM, PROMPT));
}

/// llama.cpp drops a request's whole grammar when any repetition bound in its
/// schema exceeds `GRAMMAR_REPETITION_LIMIT`, and a learning prompt no longer
/// asks for the format in words, so a grammar silently dropped would leave
/// nothing holding the shape. Bounds computed at run time are checked on
/// every request the learning fixtures answer; this catches the literal ones,
/// including schemas whose calls have no fixture.
#[test]
fn no_learning_schema_literal_exceeds_the_grammar_repetition_limit() -> Result<()> {
    use crate::features::conversation::memory::prompts::GRAMMAR_REPETITION_LIMIT;
    let pattern = regex::Regex::new(r#""(maxLength|maxItems|minLength|minItems)"\s*:\s*(\d+)"#)
        .map_err(|error| AppError::InternalError(error.to_string()))?;
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/src/features/learning");
    let mut scanned = 0;
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.map_err(|error| AppError::InternalError(error.to_string()))?;
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        scanned += 1;
        let source = std::fs::read_to_string(entry.path())?;
        for found in pattern.captures_iter(&source) {
            let bound: usize = found
                .get(2)
                .and_then(|digits| digits.as_str().parse().ok())
                .unwrap_or(usize::MAX);
            assert!(
                bound <= GRAMMAR_REPETITION_LIMIT,
                "{} sets a schema bound of {bound}, above llama.cpp's grammar limit of {GRAMMAR_REPETITION_LIMIT}",
                entry.path().display()
            );
        }
    }
    assert!(scanned > 50, "the learning sources were not found");
    Ok(())
}

/// Cuts its first structured answer off at the output limit, then answers.
struct TruncatesOnce {
    sent: Mutex<Vec<CompletionRequest>>,
}

#[async_trait::async_trait]
impl LLMPort for TruncatesOnce {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut sent = self.sent.lock().unwrap();
        sent.push(request.clone());
        let mut response = CompletionResponse::from_text(if sent.len() == 1 {
            "{\"answer\":\"cut of"
        } else {
            ANSWER
        });
        if sent.len() == 1 {
            response.finish_reason = "length".into();
        }
        Ok(response)
    }
    fn model_name(&self) -> &str {
        "truncates-once"
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn an_answer_cut_at_its_output_limit_is_asked_for_once_more_with_twice_the_room() -> Result<()>
{
    let llm = TruncatesOnce {
        sent: Mutex::new(Vec::new()),
    };
    let raw = complete_json(&llm, "Answer.", "Task.".into(), schema(), 1_000).await?;
    assert_eq!(raw, ANSWER);
    let sent = llm.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    let first = sent[0].max_output_tokens.unwrap();
    assert_eq!(sent[1].max_output_tokens.unwrap(), 2 * first);
    Ok(())
}

/// Accepts only [`ANSWER`]; refuses anything else, naming it.
fn only_the_answer(raw: &str) -> Result<String> {
    if raw == ANSWER {
        return Ok(raw.to_owned());
    }
    Err(AppError::InvalidInput(format!(
        "The answer {raw} explains nothing."
    )))
}

async fn checked(llm: &Recorder, reasoning: Option<ReasonFirst<'_>>) -> Result<String> {
    complete_checked(
        llm,
        SYSTEM,
        PROMPT.into(),
        schema(),
        1000,
        None,
        reasoning,
        only_the_answer,
    )
    .await
}

fn reask(reason: &str) -> String {
    format!("{REASK} {reason} Answer the same task again, fixing that.")
}

#[tokio::test]
async fn a_refused_answer_is_asked_for_once_more_with_the_reason() -> Result<()> {
    let llm = Recorder::answering_first(32_000, &["{\"answer\":\"Blue.\"}"]);
    assert_eq!(checked(&llm, None).await?, ANSWER);

    let sent = llm.sent();
    assert_eq!(sent.len(), 2, "the first answer and one more");
    let reason = reask("The answer {\"answer\":\"Blue.\"} explains nothing.");
    assert_eq!(
        messages(&sent[1]),
        vec![
            ("system", SYSTEM),
            ("user", PROMPT),
            ("assistant", "{\"answer\":\"Blue.\"}"),
            ("user", reason.as_str()),
        ]
    );
    // The request asking again opens with every message of the first, so the
    // server reuses what it has just read.
    assert_eq!(messages(&sent[0]), messages(&sent[1])[..2].to_vec());
    assert_eq!(sent[1].json_schema, Some(schema()));
    assert_eq!(task_messages(&sent[1]), (SYSTEM, PROMPT));
    Ok(())
}

#[tokio::test]
async fn a_second_refusal_fails_with_its_own_reason_without_blaming_the_input() {
    let llm = Recorder::answering_first(32_000, &["first", "second"]);
    let error = checked(&llm, None).await.unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason == "The answer second explains nothing."),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "The model's answer could not be used: The answer second explains nothing."
    );
    assert_eq!(llm.sent().len(), 2, "asked once more, never twice");
}

#[tokio::test]
async fn a_window_too_full_for_the_refused_answer_sends_the_task_again_with_the_reason(
) -> Result<()> {
    let refused: &'static str = Box::leak("blue ".repeat(4_000).into_boxed_str());
    let llm = Recorder::answering_first(4_000, &[refused]);
    let accept = |raw: &str| {
        if raw == ANSWER {
            return Ok(raw.to_owned());
        }
        Err(AppError::InvalidInput("It explains nothing.".into()))
    };
    let value = complete_checked(
        &llm,
        SYSTEM,
        PROMPT.into(),
        schema(),
        1000,
        None,
        None,
        accept,
    )
    .await?;
    assert_eq!(value, ANSWER);

    let sent = llm.sent();
    assert_eq!(sent.len(), 2, "the first answer and one more");
    let task = format!("{PROMPT}\n\n{RESTATE} It explains nothing.");
    assert_eq!(
        messages(&sent[1]),
        vec![("system", SYSTEM), ("user", task.as_str())]
    );
    Ok(())
}

#[tokio::test]
async fn errors_that_say_nothing_about_the_answer_are_not_asked_again() {
    let mut dropped = Recorder::new(32_000);
    dropped.answer = None;
    let error = checked(&dropped, None).await.unwrap_err();
    assert!(matches!(error, AppError::Network(_)), "{error:?}");
    assert_eq!(dropped.sent().len(), 1);

    let llm = Recorder::new(32_000);
    let error = complete_checked(
        &llm,
        SYSTEM,
        PROMPT.into(),
        schema(),
        1000,
        None,
        None,
        |_| -> Result<()> { Err(AppError::InvalidState("not the answer's fault".into())) },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, AppError::InvalidState(_)), "{error:?}");
    assert_eq!(llm.sent().len(), 1);
}

#[tokio::test]
async fn a_two_step_call_asks_again_for_its_answer_step_only() -> Result<()> {
    let llm = Recorder::answering_first(32_000, &["{}"]);
    let answer = checked(
        &llm,
        Some(ReasonFirst {
            kind: CallKind::Authoring,
            subject: "the explanation",
            brief: BRIEF,
        }),
    )
    .await?;
    assert_eq!(answer, ANSWER);

    let sent = llm.sent();
    assert_eq!(sent.len(), 3, "one draft, the answer, and one more answer");
    assert_eq!(
        sent.iter()
            .filter(|request| is_draft_request(request))
            .count(),
        1
    );
    let system = two_step_system();
    let reason = reask("The answer {} explains nothing.");
    assert_eq!(
        messages(&sent[2]),
        vec![
            ("system", system.as_str()),
            ("user", PROMPT),
            ("assistant", DRAFT),
            ("user", ANSWER_STEP),
            ("assistant", "{}"),
            ("user", reason.as_str()),
        ]
    );
    assert_eq!(messages(&sent[1]), messages(&sent[2])[..4].to_vec());
    assert_eq!(task_messages(&sent[2]), (SYSTEM, PROMPT));
    Ok(())
}
