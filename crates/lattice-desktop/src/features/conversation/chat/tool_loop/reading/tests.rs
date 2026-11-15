use super::contract::{
    scripted_answer, shown_in, Excerpt, Input, PassageNotes, Sentence, BEARING_INSTRUCTION,
    READ_INSTRUCTION,
};
use super::*;
use crate::features::conversation::chat::cancellation::is_cancellation;
use crate::features::conversation::chat::verification::test_support::reply as reply_of;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::ports::ToolDefinition;
use lattice_core::application::services::model_contract::test_support::snapshot::assert_requests;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::ScriptedModel;
use lattice_core::shared::error::AppError;
use std::time::Duration;

const SLUICE_FIRST: &str = "The sluice gate was rebuilt in oak in 1902.";
const SLUICE: &str = "The sluice gate was rebuilt in oak in 1902. The oak came from the old quay.";
const WEIR: &str = "Floods in 1911 breached the weir twice.";
const KILN_FIRST: &str = "The kiln was fired every Friday.";
const KILN_SECOND: &str = "Its chimney fell in 1952.";
const TIDES: &str = "Tide tables were printed in March.";

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

fn sources() -> Vec<SourceDto> {
    vec![
        source(1, SLUICE),
        source(2, WEIR),
        source(3, &format!("{KILN_FIRST} {KILN_SECOND}")),
        source(4, TIDES),
    ]
}

/// The turn's question, as the loop hands it to the reading.
const QUESTION: &str = "Question: What happened at the harbour?";

/// The turn's message as the prompt builder closes it.
fn message(labels: &[u32]) -> String {
    let passages = labels
        .iter()
        .map(|label| format!("[s{label}] Document: d{label}.md\nContent: …"))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("{passages}\n\nQuestion: What happened at the harbour?\n\n{READING_INSTRUCTION}")
}

fn planned(labels: &[u32]) -> CompletionRequest {
    legacy::completion(legacy::Completion {
        input: vec![
            CompletionInput::Message {
                role: "system".into(),
                content: "Answer from sources.".into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: message(labels),
            },
        ],
        tools: vec![ToolDefinition {
            name: "search_documents".into(),
            description: "Search.".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }],
        max_output_tokens: Some(777),
        cache_key: Some("conversation".into()),
        ..Default::default()
    })
}

fn passages() -> Passages {
    asked(planned(&[1, 2, 3, 4]).input(), &sources()).expect("the message asks for a reading")
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn notes_of(reading: Reading) -> Option<Notes> {
    match reading {
        Reading::Noted(notes) => Some(notes),
        Reading::NothingBears | Reading::NoNotes => None,
    }
}

async fn reading_by(model: &ScriptedModel) -> Reading {
    read_passages(
        model,
        &planned(&[1, 2, 3, 4]),
        &passages(),
        QUESTION,
        &TurnRecorder::silent("conversation", "request"),
        &CancellationToken::new(),
        &TurnClock::new(Duration::from_secs(60), Duration::ZERO),
    )
    .await
    .expect("only Stop and the deadline are errors")
}

async fn read(model: &ScriptedModel) -> Option<Notes> {
    notes_of(reading_by(model).await)
}

fn choosing(keep: fn(&str) -> bool) -> ScriptedModel {
    ScriptedModel::new().on_request::<PassageNotes>(move |sent, _| {
        reply(scripted_answer(&shown_in(&sent.request), |_| true, keep))
    })
}

#[tokio::test]
async fn a_note_is_the_chosen_sentences_written_from_the_passage() {
    let model = choosing(|sentence| sentence == SLUICE_FIRST || sentence == KILN_SECOND);
    let notes = read(&model).await.expect("notes");
    assert_eq!(
        notes.text,
        format!("[s1] {SLUICE_FIRST}\n[s2] -\n[s3] {KILN_SECOND}\n[s4] -")
    );
    assert_eq!(notes.noted(), [1, 3].into());
    assert_eq!(notes.tokens, model.count_tokens(&notes.text));
    assert_eq!(model.sent().len(), 1, "every passage is one request");
}

#[tokio::test]
async fn a_note_holds_its_sentences_in_the_passages_own_order() {
    let model = ScriptedModel::new().on_request::<PassageNotes>(|_, _| {
        reply(
            r#"{"entries":[{"of":"p1","keep":[]},{"of":"p2","keep":[]},{"of":"p3","keep":["u2","u1","u2"]},{"of":"p4","keep":[]}]}"#
                .into(),
        )
    });
    let notes = read(&model).await.expect("notes");
    assert_eq!(
        notes.lines.get(&3),
        Some(&Some(format!("{KILN_FIRST} {KILN_SECOND}")))
    );
}

#[tokio::test]
async fn a_sentence_the_passage_does_not_have_makes_no_note() {
    let model = ScriptedModel::new().on_request::<PassageNotes>(|_, _| {
        reply(
            r#"{"entries":[{"of":"p1","keep":["u1"]},{"of":"p2","keep":["u2"]},{"of":"p3","keep":[]},{"of":"p4","keep":[]}]}"#
                .into(),
        )
    });
    let notes = read(&model).await.expect("notes");
    assert_eq!(
        notes.lines.get(&1),
        Some(&Some(SLUICE_FIRST.to_string()))
    );
    assert_eq!(
        notes.lines.get(&2),
        Some(&None),
        "the weir has one sentence: u2 is another passage's"
    );
}

#[tokio::test]
async fn a_passage_left_out_of_the_answer_is_asked_for_again_alone_and_the_others_stand() {
    let model = ScriptedModel::new().on_request::<PassageNotes>(|sent, nth| {
        let first = nth == 0;
        reply(scripted_answer(
            &shown_in(&sent.request),
            |document| !first || !document.contains("d4.md"),
            |sentence| sentence == SLUICE_FIRST || sentence == TIDES,
        ))
    });
    let notes = read(&model).await.expect("notes");
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    let second = shown_in(&sent[1].request);
    assert!(second.contains(TIDES) && !second.contains(SLUICE_FIRST));
    assert_eq!(notes.noted(), [1, 4].into());
}

#[tokio::test]
async fn a_passage_that_stays_unanswered_has_no_line() {
    let model = ScriptedModel::new().on_request::<PassageNotes>(|sent, _| {
        reply(scripted_answer(
            &shown_in(&sent.request),
            |document| !document.contains("d4.md"),
            |sentence| sentence == SLUICE_FIRST,
        ))
    });
    let notes = read(&model).await.expect("the others are noted");
    assert_eq!(notes.text, format!("[s1] {SLUICE_FIRST}\n[s2] -\n[s3] -"));
    assert!(!notes.lines.contains_key(&4));
}

#[tokio::test]
async fn no_sentence_chosen_from_any_passage_means_that_nothing_bears() {
    let model = choosing(|_| false);
    assert!(matches!(
        reading_by(&model).await,
        Reading::NothingBears
    ));
}

#[tokio::test]
async fn no_sentence_chosen_while_passages_stay_unread_gives_no_notes() {
    let model = ScriptedModel::new().on_request::<PassageNotes>(|sent, _| {
        reply(scripted_answer(
            &shown_in(&sent.request),
            |document| document.contains("d1.md") || document.contains("d2.md"),
            |_| false,
        ))
    });
    assert!(matches!(reading_by(&model).await, Reading::NoNotes));
}

#[tokio::test]
async fn a_provider_that_fails_gives_no_notes() {
    let model = ScriptedModel::new()
        .failing_every_request(|_| AppError::ServiceNotAvailable("The provider is down".into()));
    assert!(matches!(reading_by(&model).await, Reading::NoNotes));
}

#[tokio::test]
async fn stop_while_reading_is_an_error() {
    let model = choosing(|_| true);
    let stop = CancellationToken::new();
    stop.cancel();
    let error = read_passages(
        &model,
        &planned(&[1, 2, 3, 4]),
        &passages(),
        QUESTION,
        &TurnRecorder::silent("conversation", "request"),
        &stop,
        &TurnClock::new(Duration::from_secs(60), Duration::ZERO),
    )
    .await
    .expect_err("Stop is an error");
    assert!(is_cancellation(&error));
}

#[tokio::test]
async fn the_request_shows_each_passage_by_handle_with_its_document_and_the_question_after() {
    let model = choosing(|_| true);
    read(&model).await.expect("notes");
    let request = &model.sent()[0].request;
    let prompt = shown_in(request);
    assert!(prompt.contains(READ_INSTRUCTION));
    assert!(prompt.contains(&format!("[p3] Document: d3.md\n[u1] {KILN_FIRST}\n[u2] {KILN_SECOND}")));
    assert!(!prompt.contains("[s3]"), "a passage is shown by its handle only");
    let (passages, closing) = prompt.split_once("My question is: ").expect("the question");
    assert!(passages.contains(TIDES));
    assert_eq!(
        closing,
        format!("\"{QUESTION}\"\n\n{BEARING_INSTRUCTION}")
    );
}

#[test]
fn the_reading_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Input {
        question: marks.told(QUESTION.to_owned()),
        passages: vec![Excerpt {
            label: marks.reference(4417u32),
            document: marks.told("harbour-notes.md".to_owned()),
            sentences: vec![
                Sentence {
                    at: 0,
                    text: marks.told(SLUICE_FIRST.to_owned()),
                },
                Sentence {
                    at: 1,
                    text: marks.told(WEIR.to_owned()),
                },
            ],
        }],
    };
    holds::<PassageNotes>(&input, &marks);
}

#[tokio::test]
async fn prompt_snapshot_passage_notes() {
    let model = ScriptedModel::new().on_request::<PassageNotes>(|sent, nth| {
        let first = nth == 0;
        reply(scripted_answer(
            &shown_in(&sent.request),
            |document| !first || !document.contains("d4.md"),
            |sentence| sentence == SLUICE_FIRST,
        ))
    });
    read(&model).await.expect("notes");
    let requests: Vec<_> = model.sent().into_iter().map(|sent| sent.request).collect();
    assert_requests(env!("CARGO_MANIFEST_DIR"), "passage-notes", &requests);
}

#[test]
fn a_message_that_numbers_no_source_the_turn_holds_asks_for_no_reading() {
    let planned = planned(&[7, 8]);
    assert!(planned.input()[1].text().contains(READING_INSTRUCTION));
    assert!(asked(planned.input(), &sources()).is_none());
}

#[test]
fn only_the_labels_the_message_shows_are_read() {
    let mut planned = planned(&[1, 2]);
    let passages = asked(planned.input(), &sources()).unwrap();
    assert_eq!(passages.labels().collect::<Vec<_>>(), vec![1, 2]);
    withdraw(legacy::says(&mut planned).input);
    assert!(
        asked(planned.input(), &sources()).is_none(),
        "a message without the instruction asks for no reading"
    );
    assert!(!planned.input()[1].text().contains(READING_INSTRUCTION));
    assert!(planned.input()[1]
        .text()
        .ends_with("Question: What happened at the harbour?"));
}

#[test]
fn sources_sharing_a_label_read_as_one_passage_under_the_first_document() {
    let mut chunks = sources();
    let mut second = source(1, "A second chunk.");
    second.file_name = "other.md".into();
    chunks.push(second);
    let passages = asked(planned(&[1]).input(), &chunks).unwrap();
    let passage = &passages.0[&1];
    assert_eq!(passage.document, "d1.md");
    assert!(passage.text.ends_with("A second chunk."));
}

#[test]
fn every_passage_the_reply_cites_is_used() {
    let noted: BTreeSet<u32> = [1, 2, 3, 4, 5, 6].into();
    let held: Vec<SourceDto> = (1..=6).map(|label| source(label, "A passage.")).collect();
    let answer = reply_of(
        "The kitchen got a grant [s1][s2]. The mill retired [s3] [s4]. It was sold.[s5]",
        &held,
    );
    assert_eq!(
        left_out(&noted, &answer.cited().into_iter().collect()),
        [6].into()
    );
    let nothing = reply_of(
        "Nothing cited, `[s3]` is code, and [s9] names no passage.",
        &held,
    );
    assert_eq!(
        left_out(&noted, &nothing.cited().into_iter().collect()),
        noted
    );
}

#[test]
fn a_resumed_turn_knows_its_saved_rounds_hold_the_reading() {
    let read = vec![
        CompletionInput::Message {
            role: "assistant".into(),
            content: "[s1] -".into(),
        },
        CompletionInput::Message {
            role: "user".into(),
            content: WRITE_INSTRUCTION.into(),
        },
    ];
    assert!(noted_in(&read));
    let searching = vec![CompletionInput::Message {
        role: "user".into(),
        content: SEARCH_OR_WRITE_INSTRUCTION.into(),
    }];
    assert!(noted_in(&searching));
    let unread = vec![CompletionInput::ToolResult {
        id: "1".into(),
        output: "Results.".into(),
    }];
    assert!(!noted_in(&unread));
}

#[test]
fn a_search_takes_the_readings_place_only_in_a_round_that_can_search() {
    let mut closed = planned(&[1]);
    legacy::says(&mut closed).tools.clear();
    search_instead(&mut closed);
    assert!(closed
        .input()
        .last()
        .unwrap()
        .text()
        .contains(READING_INSTRUCTION));

    let mut open = planned(&[1]);
    *legacy::says(&mut open).tools = vec![ToolDefinition {
        name: "semantic_search".into(),
        description: String::new(),
        parameters: serde_json::json!({}),
    }];
    search_instead(&mut open);
    let message = open.input().last().unwrap().text();
    assert!(!message.contains(READING_INSTRUCTION));
    assert!(message.ends_with(NOTHING_BEARS_INSTRUCTION));
}

#[tokio::test]
async fn the_writing_round_is_the_reading_round_with_the_notes_and_the_instruction_after_it() {
    let request = planned(&[1, 2]);
    let mut writing = request.clone();
    let notes = Notes {
        text: "[s1] -\n[s2] -".into(),
        lines: BTreeMap::from([(1, None), (2, None)]),
        tokens: 3,
        labels: BTreeMap::new(),
    };
    assert!(append_notes(&ScriptedModel::new(), &mut writing, &notes).await);
    assert_eq!(
        writing.max_output_tokens(),
        Some(777),
        "a window with room keeps the reservation"
    );
    assert_eq!(
        json(&writing.input()[..request.input().len()].to_vec()),
        json(&request.input())
    );
    assert_eq!(writing.input().len(), request.input().len() + 2);
    assert!(matches!(
        &writing.input()[request.input().len()],
        CompletionInput::Message { role, content } if role == "assistant" && content == "[s1] -\n[s2] -"
    ));
    assert_eq!(
        writing.input().last().unwrap().text(),
        SEARCH_OR_WRITE_INSTRUCTION
    );
}

/// A round that can search is told to, for what the passages leave open: a
/// shortlist reaches further by searching again. One that cannot is told to
/// answer, or it would promise a search it cannot make.
#[tokio::test]
async fn only_a_round_with_tools_is_told_to_search_for_what_is_missing() {
    let notes = Notes {
        text: "[s1] -".into(),
        lines: BTreeMap::from([(1, None)]),
        tokens: 3,
        labels: BTreeMap::new(),
    };
    let mut closed = planned(&[1]);
    legacy::says(&mut closed).tools.clear();
    assert!(append_notes(&ScriptedModel::new(), &mut closed, &notes).await);
    assert_eq!(closed.input().last().unwrap().text(), WRITE_INSTRUCTION);

    let mut open = planned(&[1]);
    *legacy::says(&mut open).tools = vec![ToolDefinition {
        name: "semantic_search".into(),
        description: String::new(),
        parameters: serde_json::json!({}),
    }];
    assert!(append_notes(&ScriptedModel::new(), &mut open, &notes).await);
    let instruction = open.input().last().unwrap().text();
    assert_eq!(instruction, SEARCH_OR_WRITE_INSTRUCTION);
    assert!(instruction.starts_with("If the passages leave any part of my question unanswered"));
}

#[tokio::test]
async fn the_answer_gets_what_the_window_has_left_after_the_notes() {
    let notes = Notes {
        text: "[s1] A note.".into(),
        lines: BTreeMap::from([(1, Some("A note.".into()))]),
        tokens: 10,
        labels: BTreeMap::new(),
    };
    let mut measured = planned(&[1]);
    assert!(append_notes(&ScriptedModel::new(), &mut measured, &notes).await);
    let needed = prompt_tokens(&ScriptedModel::new(), &measured).await;
    let model = ScriptedModel::new().window(needed + 30);
    let mut writing = planned(&[1]);
    assert!(append_notes(&model, &mut writing, &notes).await);
    assert_eq!(
        writing.max_output_tokens(),
        Some(30),
        "the answer is held to the room the notes left"
    );
}

#[tokio::test]
async fn notes_that_would_crowd_out_the_answer_are_taken_back() {
    let notes = Notes {
        text: "[s1] A note.".into(),
        lines: BTreeMap::from([(1, Some("A note.".into()))]),
        tokens: 100,
        labels: BTreeMap::new(),
    };
    let mut measured = planned(&[1]);
    assert!(append_notes(&ScriptedModel::new(), &mut measured, &notes).await);
    let needed = prompt_tokens(&ScriptedModel::new(), &measured).await;
    let model = ScriptedModel::new().window(needed + 30);
    let request = planned(&[1]);
    let mut writing = request.clone();
    assert!(!append_notes(&model, &mut writing, &notes).await);
    assert_eq!(json(&writing.input()), json(&request.input()), "no notes");
    assert_eq!(writing.max_output_tokens(), Some(777));
}

#[test]
fn the_writing_round_asks_for_the_answer_in_the_passages_own_words() {
    assert_eq!(
        WRITE_INSTRUCTION,
        "Now answer my question directly, in the passages' own words, keeping their dates, figures and names as given. Cover every passage your notes quote and cite each with its label. Do not describe the notes or the passages, and say nothing about passages that do not bear on my question. Write in the language of my question, translating any passage in another."
    );
}

#[test]
fn a_continuation_quotes_each_left_out_note() {
    let notes = Notes {
        text: String::new(),
        lines: BTreeMap::from([(6, Some("A.".into())), (7, Some("B.".into()))]),
        tokens: 4,
        labels: BTreeMap::from([(6, "[s6]".into()), (7, "[s7]".into())]),
    };
    assert_eq!(
        continue_instruction(&[6, 7].into(), &notes),
        "Your answer does not use these passages, which your notes quote as bearing on my question: [s6] \"A.\" [s7] \"B.\". Add only what my question still needs from them, continuing your answer in its own form and citing each with its label; do not repeat what you have written. If your answer already covers my question, reply with a single hyphen. Do not mention your notes or correct them."
    );
}

#[test]
fn only_a_label_with_a_note_can_be_left_out() {
    let notes = Notes {
        text: String::new(),
        lines: BTreeMap::from([(1, None), (2, Some("A.".into())), (3, Some("B.".into()))]),
        tokens: 4,
        labels: BTreeMap::new(),
    };
    assert_eq!(notes.noted(), [2, 3].into());
    assert_eq!(left_out(&notes.noted(), &[3].into()), [2].into());
}

fn json(value: &impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}
