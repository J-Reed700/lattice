//! The reading round, the writing round and the coverage check, run through
//! the whole loop.

use super::super::reading::{READING_INSTRUCTION, WRITE_INSTRUCTION};
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
    assert_eq!(requests.len(), 3);
    let (reading, writing, continuation) = (&requests[0], &requests[1], &requests[2]);
    assert_eq!(reading.input.len(), 2, "the planned request, as it is");
    assert_eq!(reading.tool_choice, Some(ToolChoice::None));
    assert_eq!(reading.thinking, ThinkingMode::Off);
    assert_eq!(reading.max_output_tokens, Some(512));

    assert_eq!(
        json_of(&writing.input[..2].to_vec()),
        json_of(&reading.input)
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
