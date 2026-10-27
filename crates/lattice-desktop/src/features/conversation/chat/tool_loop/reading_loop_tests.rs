//! The reading round, the writing round and the coverage check, run through
//! the whole loop.

use super::super::reading::{
    prompt_tokens, CONTINUE_INSTRUCTION, READING_INSTRUCTION, WRITE_INSTRUCTION,
};
use super::*;

const ROOF: &str = "The roof was mended in 1851. The parish paid for it.";
const OPENING: &str = "The bridge opened on 1932-05-01 to traffic.";
const TOLL: &str = "A toll of two pence was charged from the start.";
const COOKING: &str = "Bake the loaf for forty minutes.";

fn source(label: u32, content: &str) -> SourceDto {
    SourceDto {
        page_number: None,
        from_ocr: false,
        document_id: format!("d{label}"),
        chunk_id: format!("c{label}"),
        content: content.into(),
        score: 0.9,
        path: None,
        position: None,
        file_name: format!("d{label}.md"),
        file_path: format!("d{label}.md"),
        mime_type: "text/markdown".into(),
        category: "Text".into(),
        file_size_bytes: 0,
        modified_at: String::new(),
        excerpt: None,
        highlights: None,
        section: None,
        chunk_index: None,
        chunk_excerpts: None,
        citation_id: Some(label),
        web_snapshot: None,
    }
}

const PASSAGES: [(u32, &str); 4] = [(2, ROOF), (5, OPENING), (7, TOLL), (9, COOKING)];

/// A message showing the four passages, closed as the prompt builder closes it.
fn message() -> String {
    let passages = PASSAGES
        .iter()
        .map(|(label, content)| format!("[{label}] Document: d{label}.md\nContent: {content}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("{passages}\n\nQuestion: Tell me about the bridge.\n\nAnswer:\n\n{READING_INSTRUCTION}")
}

async fn grounded(model: &Arc<Model>) -> Run {
    let message = message();
    let turn = Turn {
        message: &message,
        sources: PASSAGES
            .iter()
            .map(|(label, content)| source(*label, content))
            .collect(),
        time_budget: TURN_TIME,
    };
    run_turn(
        FakeRuntime::default(),
        model,
        &[tool("search_documents")],
        ROOMY,
        5,
        "reading",
        turn,
    )
    .await
}

fn streamed(run: &Run) -> String {
    run.events
        .iter()
        .filter_map(|event| event.content.clone())
        .collect()
}

fn reply(text: &str) -> CompletionResponse {
    CompletionResponse::from_text(text)
}

const NOTES: &str = "[2] The roof was mended in 1851.\n[5] The bridge opened on 1932-05-01 to traffic.\n[7] A toll of two pence was charged from the start.\n[9] -";
const ANSWER: &str = "The roof was mended in 1851 [2]. The bridge opened on 1932-05-01 [5].";

#[tokio::test]
async fn a_grounded_answer_reads_first_and_adds_the_passage_it_left_out() {
    let model = Model::new([
        reply(NOTES),
        reply(ANSWER),
        reply("A toll of two pence was charged from the start [7]."),
    ]);
    let run = grounded(&model).await;
    let outcome = run.outcome.as_ref().expect("an answer");
    let added = "A toll of two pence was charged from the start [7].";
    assert_eq!(outcome.response, format!("{ANSWER}\n\n{added}"));
    assert_eq!(streamed(&run), format!("{ANSWER}\n\n{added}"));

    let requests = model.requests();
    assert_eq!(requests.len(), 3, "four passages are one group");
    let (reading, writing, continuation) = (&requests[0], &requests[1], &requests[2]);
    assert_eq!(
        reading.input.len(),
        2,
        "the planned prompt, its message closed by the group"
    );
    assert!(last_text(reading).contains(&format!("\n\n[9] {COOKING}\n\n")));
    assert_eq!(reading.tool_choice, Some(ToolChoice::None));
    assert_eq!(reading.thinking, ThinkingMode::Off);
    assert_eq!(reading.max_output_tokens, Some(512));

    assert_eq!(json_of(&writing.input[0]), json_of(&reading.input[0]));
    assert_eq!(
        writing.input[1].text(),
        message(),
        "the answer is written on the planned message"
    );
    assert_eq!(writing.input[2].text(), NOTES);
    assert_eq!(last_text(writing), WRITE_INSTRUCTION);
    assert!(
        writing.may_call_tools(),
        "the writing round may still search"
    );

    let asked = last_text(continuation);
    assert!(asked.contains(&format!("[7] \"{TOLL}\"")), "{asked}");
    assert!(!asked.contains("[2]") && !asked.contains("[5]"), "{asked}");
    assert_eq!(
        json_of(&continuation.input[..writing.input.len()].to_vec()),
        json_of(&writing.input),
        "the continuation reads on from the writing round's prompt"
    );
    assert_eq!(continuation.tool_choice, Some(ToolChoice::None));
    assert_eq!(continuation.thinking, ThinkingMode::Off);

    let labels: Vec<&str> = run.steps.iter().map(|step| step.label.as_str()).collect();
    for label in [
        "Reading the passages",
        "Writing the answer",
        "Adding what was left out",
    ] {
        assert!(labels.contains(&label), "{labels:?}");
    }
}

/// The turn's request as planned, measured as the send-time gates measure it.
async fn planned_tokens(model: &Model) -> usize {
    let planned = CompletionRequest {
        input: vec![
            CompletionInput::Message {
                role: "system".into(),
                content: "Answer from sources.".into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: message(),
            },
        ],
        tools: vec![tool("search_documents")],
        ..Default::default()
    };
    prompt_tokens(model, &planned).await
}

/// A window the planned prompt fills up to the answer's reservation, as the
/// context assembler plans one.
async fn planned_to_the_reservation(model: &Model) {
    let window = planned_tokens(model).await + 512;
    model.window.store(window, Ordering::Relaxed);
}

#[tokio::test]
async fn the_answer_gets_what_the_window_has_left_after_the_notes() {
    let answer = "The roof was mended in 1851 [2]. It opened on 1932-05-01 [5], with a toll [7].";
    let model = Model::new([reply(NOTES), reply(answer)]);
    planned_to_the_reservation(&model).await;
    let run = grounded(&model).await;
    assert_eq!(run.outcome.as_ref().expect("an answer").response, answer);
    assert_eq!(streamed(&run), answer);
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    let writing = &requests[1];
    assert_eq!(writing.input[2].text(), NOTES);
    let room = model.window.load(Ordering::Relaxed) - prompt_tokens(&*model, writing).await;
    assert!(room < 512, "the notes took part of the reservation: {room}");
    assert_eq!(
        writing.max_output_tokens,
        Some(u32::try_from(room).unwrap())
    );
}

#[tokio::test]
async fn notes_that_would_crowd_out_the_answer_are_taken_back() {
    let long = format!("[2] {}\n[5] -\n[7] -\n[9] -", [ROOF; 20].join(" "));
    let model = Model::new([reply(&long), reply(ANSWER)]);
    planned_to_the_reservation(&model).await;
    let run = grounded(&model).await;
    let outcome = run.outcome.as_ref().expect("an answer");
    assert_eq!(outcome.response, ANSWER);
    assert_eq!(streamed(&run), outcome.response);
    let requests = model.requests();
    assert_eq!(requests.len(), 2, "without notes there is nothing to add");
    let answering = &requests[1];
    assert_eq!(answering.input.len(), 2, "no notes");
    assert!(!last_text(answering).contains(READING_INSTRUCTION));
    assert!(last_text(answering).ends_with("Answer:"));
    assert_eq!(answering.max_output_tokens, Some(512));
}

#[tokio::test]
async fn a_continuation_the_provider_fails_leaves_the_answer_as_it_was() {
    let model = Model::new([
        Scripted::from(reply(NOTES)),
        Scripted::from(reply(ANSWER)),
        Scripted::Fail("The provider is down".into()),
    ]);
    let run = grounded(&model).await;
    let outcome = run.outcome.as_ref().expect("the finished answer stands");
    assert_eq!(outcome.response, ANSWER);
    assert_eq!(streamed(&run), outcome.response);
    assert_eq!(model.requests().len(), 3);
}

#[tokio::test]
async fn a_call_a_continuation_writes_as_text_is_neither_shown_nor_kept() {
    let added = "A toll of two pence was charged from the start [7].";
    let call = "<tool_call>\n{\"name\": \"search_documents\", \"arguments\": {\"query\": \"toll\"}}\n</tool_call>";
    let model = Model::new([
        reply(NOTES),
        reply(ANSWER),
        reply(&format!("{added}\n{call}")),
    ]);
    let run = grounded(&model).await;
    let outcome = run.outcome.as_ref().expect("an answer");
    assert_eq!(outcome.response, format!("{ANSWER}\n\n{added}"));
    assert_eq!(streamed(&run), outcome.response);
    assert_eq!(model.requests().len(), 3, "[7] added: nothing left out");
}

#[tokio::test]
async fn a_window_with_no_room_left_adds_nothing_to_the_answer() {
    let model = Model::new([reply(NOTES), reply(ANSWER)]);
    // The continuation's request, as the loop would send it, fills the window.
    let continuation = CompletionRequest {
        input: vec![
            CompletionInput::Message {
                role: "system".into(),
                content: "Answer from sources.".into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: message(),
            },
            CompletionInput::Message {
                role: "assistant".into(),
                content: NOTES.into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: WRITE_INSTRUCTION.into(),
            },
            CompletionInput::Message {
                role: "assistant".into(),
                content: ANSWER.into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: CONTINUE_INSTRUCTION.replace("{quotes}", &format!("[7] \"{TOLL}\"")),
            },
        ],
        tools: vec![tool("search_documents")],
        ..Default::default()
    };
    let window = prompt_tokens(&*model, &continuation).await;
    model.window.store(window, Ordering::Relaxed);
    let run = grounded(&model).await;
    let outcome = run.outcome.as_ref().expect("an answer");
    assert_eq!(outcome.response, ANSWER);
    assert_eq!(streamed(&run), outcome.response);
    let requests = model.requests();
    assert_eq!(requests.len(), 2, "no continuation");
    assert_eq!(
        requests[1].input[2].text(),
        NOTES,
        "the notes fit beside the answer"
    );
    assert!(!run
        .steps
        .iter()
        .any(|step| step.label == "Adding what was left out"));
}

#[tokio::test]
async fn a_continuation_that_adds_no_passage_ends_the_check() {
    let model = Model::new([
        reply(NOTES),
        reply("The roof was mended in 1851 [2]."),
        reply("Nothing more to add."),
    ]);
    let run = grounded(&model).await;
    assert_eq!(
        run.outcome.expect("an answer").response,
        "The roof was mended in 1851 [2].\n\nNothing more to add."
    );
    assert_eq!(model.requests().len(), 3, "no second continuation");
}

#[tokio::test]
async fn a_note_that_is_not_a_copy_never_asks_for_more() {
    let model = Model::new([
        reply("[2] The roof was repaired in 1851.\n[5] -\n[7] none\n[9] -"),
        reply("The bridge is old."),
    ]);
    let run = grounded(&model).await;
    assert_eq!(
        run.outcome.expect("an answer").response,
        "The bridge is old."
    );
    assert_eq!(model.requests().len(), 2);
}

#[tokio::test]
async fn a_reading_with_no_notes_is_taken_back_and_the_answer_asked_for_as_before() {
    let model = Model::new([reply("The bridge opened in 1932."), reply(ANSWER)]);
    let run = grounded(&model).await;
    assert_eq!(run.outcome.expect("an answer").response, ANSWER);
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    let answering = &requests[1];
    assert_eq!(answering.input.len(), 2);
    assert!(last_text(answering).ends_with("Answer:"));
    assert!(!last_text(answering).contains(READING_INSTRUCTION));
}

#[tokio::test]
async fn a_reading_that_copies_no_passage_is_taken_back_and_the_answer_asked_for_as_before() {
    let model = Model::new([
        reply("1. [2] -\n2. [5] -\n3. [7] -\n4. [9] -"),
        reply(ANSWER),
    ]);
    let run = grounded(&model).await;
    assert_eq!(run.outcome.expect("an answer").response, ANSWER);
    let requests = model.requests();
    assert_eq!(requests.len(), 2, "without notes there is nothing to add");
    let answering = &requests[1];
    assert_eq!(answering.input.len(), 2, "no notes");
    assert!(last_text(answering).ends_with("Answer:"));
    assert!(!last_text(answering).contains(READING_INSTRUCTION));
}

#[tokio::test]
async fn the_writer_is_given_the_notes_as_kept_and_only_the_reading_stops_at_the_labels() {
    let answer = "The bridge opened on 1932-05-01 [5]. A toll of two pence was charged [7].";
    let model = Model::new([
        reply(&format!(
            "[2] {TOLL}\n[5] {OPENING}\n[7] -\n[9] -\n\nThe bridge opened in 1932 [5]."
        )),
        reply(answer),
    ]);
    let run = grounded(&model).await;
    assert_eq!(run.outcome.as_ref().expect("an answer").response, answer);
    let requests = model.requests();
    assert_eq!(requests.len(), 2, "every passage the notes copy is cited");
    let (reading, writing) = (&requests[0], &requests[1]);
    assert_eq!(reading.stop, ["[10]", "\n10."]);
    assert!(
        writing.stop.is_empty(),
        "the answer is not bound by the labels"
    );
    // [2]'s line copies [7]: it is [7]'s note, and not replayed under [2].
    let kept = format!("[2] -\n[5] {OPENING}\n[7] {TOLL}\n[9] -");
    assert_eq!(writing.input[2].text(), kept);
    let step = run
        .steps
        .iter()
        .find(|step| step.label == "Reading the passages")
        .expect("a reading step");
    assert_eq!(
        step.reasoning.as_deref(),
        Some(kept.as_str()),
        "the turn record shows the notes the writer was given"
    );
}

#[tokio::test]
async fn a_message_without_numbered_passages_reads_nothing() {
    let model = Model::new([reply("It opened in 1932.")]);
    let run = run_on(
        FakeRuntime::default(),
        &model,
        &[tool("search_documents")],
        ROOMY,
        5,
        "no-reading",
    )
    .await;
    assert_eq!(
        run.outcome.expect("an answer").response,
        "It opened in 1932."
    );
    let requests = model.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(last_text(&requests[0]), "When did the bridge open?");
    assert!(!run
        .steps
        .iter()
        .any(|step| step.label == "Reading the passages"));
}
