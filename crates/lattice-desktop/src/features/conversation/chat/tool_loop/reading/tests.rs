use super::*;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::ports::ToolDefinition;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

const KITCHEN: &str = "[18:56:58] Councillor Reallhollow: I'm putting £6,497.92 from my ward budget towards the new kitchen, and that is settled. [18:57:10] Chair: Thank you.";
const MILL: &str = "Entries gathered in June\n1887-12-21. The last miller at Greassby Mill retired, and the building was let as a store.";
const MÜHLE: &str = "Die Mühle wurde 1850 nach dem Hochwasser wieder in Betrieb genommen. Der Müller zog später fort.";
const MOLINO: &str =
    "El molino se vendió en 1860 a la familia del molinero. La escritura se firmó en la posada.";

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
        source(1, KITCHEN),
        source(2, MILL),
        source(3, MÜHLE),
        source(4, MOLINO),
    ]
}

/// The turn's message as the prompt builder closes it.
fn message(labels: &[u32]) -> String {
    let passages = labels
        .iter()
        .map(|label| format!("[{label}] Document: d{label}.md\nContent: …"))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("{passages}\n\nQuestion: What happened?\n\nAnswer:\n\n{READING_INSTRUCTION}")
}

fn planned(labels: &[u32]) -> CompletionRequest {
    CompletionRequest {
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
    }
}

fn passages() -> Passages {
    asked(&planned(&[1, 2, 3, 4]).input, &sources()).expect("the message asks for a reading")
}

fn reply(text: &str, finish_reason: &str) -> CompletionResponse {
    CompletionResponse {
        finish_reason: finish_reason.into(),
        ..CompletionResponse::from_text(text)
    }
}

/// Answers from a script, keeping every request it was sent.
struct Model {
    script: Mutex<VecDeque<CompletionResponse>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl Model {
    fn new(script: impl IntoIterator<Item = CompletionResponse>) -> Self {
        Self {
            script: Mutex::new(script.into_iter().collect()),
            requests: Mutex::default(),
        }
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LLMPort for Model {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.requests.lock().unwrap().push(request.clone());
        let reply = self.script.lock().unwrap().pop_front();
        Ok(reply.unwrap_or_else(reply_nothing))
    }
    fn model_name(&self) -> &str {
        "scripted"
    }
    fn max_context_tokens(&self) -> usize {
        32_768
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn reply_nothing() -> CompletionResponse {
    reply("", "stop")
}

async fn read(model: &Model, request: &CompletionRequest) -> Option<Notes> {
    let recorder = TurnRecorder::silent("conversation", "request");
    let clock = TurnClock::new(Duration::from_secs(60), Duration::ZERO);
    read_passages(
        model,
        request,
        &passages(),
        &recorder,
        &CancellationToken::new(),
        &clock,
    )
    .await
    .expect("only Stop and the deadline are errors")
}

#[test]
fn notes_are_read_label_by_label_hyphens_and_all() {
    let notes = "[1] I'm putting £6,497.92 from my ward budget towards the new kitchen, and that is settled.\n\
                 [2] -\n\
                 3: none\n\
                 [4]. El molino se vendió en 1860 a la familia del molinero.\n\
                 The answer comes after.\n\
                 [5] —";
    assert_eq!(
        parse_notes(notes),
        vec![
            (
                1,
                Some("I'm putting £6,497.92 from my ward budget towards the new kitchen, and that is settled.".into())
            ),
            (2, None),
            (3, Some("none".into())),
            (4, Some("El molino se vendió en 1860 a la familia del molinero.".into())),
            (5, None),
        ]
    );
    assert_eq!(
        parse_notes("[3] Die Mühle wurde 1850 nach dem Hochwasser wieder in Betrieb genommen."),
        vec![(
            3,
            Some("Die Mühle wurde 1850 nach dem Hochwasser wieder in Betrieb genommen.".into())
        )]
    );
}

#[test]
fn a_reading_cut_at_its_limit_keeps_only_its_complete_lines() {
    let text = "[1] -\n[2] 1887-12-21. The last miller at Greassby";
    assert_eq!(whole_lines(text, "length"), "[1] -");
    assert_eq!(whole_lines(text, "stop"), text);
    assert_eq!(whole_lines("[1] The last", "length"), "");
    assert_eq!(
        whole_lines(
            "[1] -\n<tool_call>{\"name\": \"search\"}</tool_call>",
            "stop"
        )
        .trim(),
        "[1] -"
    );
}

#[test]
fn a_note_counts_only_when_it_is_copied_from_its_own_passage() {
    let passages = passages();
    let mill = passages.0.get(&2).unwrap();
    assert!(verbatim(
        "1887-12-21. The last miller at Greassby Mill   retired,\nand the building was let as a store.",
        mill
    ));
    assert!(verbatim(
        "\"The last miller at Greassby Mill retired, and the building was let as a store.\"",
        mill
    ));
    assert!(!verbatim("The last miller retired in December 1887.", mill));
    assert!(!verbatim(
        "I'm putting £6,497.92 from my ward budget towards the new kitchen.",
        mill
    ));
    assert!(!verbatim("none", mill));
    let mühle = passages.0.get(&3).unwrap();
    assert!(verbatim(
        "Die Mühle wurde 1850 nach dem Hochwasser wieder in Betrieb genommen. Der Müller zog später fort.",
        mühle
    ));
    assert!(
        verbatim("„Der Müller zog später fort.“", mühle),
        "quotation marks around a copy are not part of it"
    );
}

#[test]
fn only_the_labels_the_message_shows_are_read() {
    let mut planned = planned(&[1, 2]);
    let passages = asked(&planned.input, &sources()).unwrap();
    assert_eq!(passages.labels().collect::<Vec<_>>(), vec![1, 2]);
    withdraw(&mut planned.input);
    assert!(
        asked(&planned.input, &sources()).is_none(),
        "a message without the instruction asks for no reading"
    );
    assert!(!planned.input[1].text().contains(READING_INSTRUCTION));
    assert!(planned.input[1].text().ends_with("Answer:"));
}

#[test]
fn every_way_of_citing_a_passage_uses_it() {
    let noted: BTreeSet<u32> = [1, 2, 3, 4, 5, 6].into();
    let answer = "The kitchen got £6,497.92 [1][2]. The mill retired [3, 4]. It was sold.[5]";
    assert_eq!(left_out(&noted, &cited_labels(answer)), [6].into());
    assert_eq!(
        left_out(
            &noted,
            &cited_labels("Nothing cited, and $a[3]$ is a formula.")
        ),
        noted
    );
}

#[tokio::test]
async fn the_reading_round_is_the_planned_request_with_calls_forbidden_and_no_sampling() {
    let model = Model::new([reply(
        "[1] I'm putting £6,497.92 from my ward budget towards the new kitchen, and that is settled.\n[2] -\n[3] -\n[4] -",
        "stop",
    )]);
    let request = planned(&[1, 2, 3, 4]);
    let notes = read(&model, &request).await.unwrap();
    let sent = model.requests();
    assert_eq!(sent.len(), 1, "every label accounted for: no sweep");
    let reading = &sent[0];
    assert_eq!(json(&reading.input), json(&request.input));
    assert_eq!(json(&reading.tools), json(&request.tools));
    assert_eq!(reading.tool_choice, Some(ToolChoice::None));
    assert_eq!(
        json(&reading.sampling),
        json(&Some(SamplingOverride::deterministic()))
    );
    assert_eq!(reading.thinking, ThinkingMode::Off);
    assert_eq!(reading.max_output_tokens, Some(777));
    assert_eq!(notes.noted, [1].into());
}

#[tokio::test]
async fn labels_the_notes_left_out_are_asked_for_until_a_sweep_adds_none() {
    let model = Model::new([
        // Cut mid-line: [2] is lost with it and asked for again.
        reply("[1] -\n[2] 1887-12-21. The last", "length"),
        reply("[2] 1887-12-21. The last miller at Greassby Mill retired, and the building was let as a store.\n[3] -", "stop"),
        reply("[1] overwritten?", "stop"),
    ]);
    let notes = read(&model, &planned(&[1, 2, 3, 4])).await.unwrap();
    let sent = model.requests();
    assert_eq!(sent.len(), 3, "the third sweep added nothing and ended it");
    assert_eq!(
        sent[1].input.last().unwrap().text(),
        "You did not account for [2], [3], [4]. Write one line for each of them, in the same form."
    );
    assert_eq!(
        sent[1].input[sent[1].input.len() - 2].text(),
        "[1] -",
        "the notes so far, whole lines only, as the model's own message"
    );
    assert_eq!(
        sent[2].input.last().unwrap().text(),
        "You did not account for [4]. Write one line for each of them, in the same form."
    );
    assert_eq!(
        notes.lines.get(&1),
        Some(&None),
        "a sweep cannot overwrite a note"
    );
    assert_eq!(notes.noted, [2].into());
    assert!(!notes.lines.contains_key(&4));
}

#[tokio::test]
async fn every_sweep_writes_only_what_the_answer_reservation_has_left() {
    let first = "[1] -";
    let second = "[2] 1887-12-21. The last miller at Greassby Mill retired, and the building was let as a store.";
    let notes = format!("{first}\n{second}");
    let counter = Model::new([]);
    let reserved = counter.count_tokens(&notes);
    let model = Model::new([
        // Each cut at the output limit mid-line: only complete lines are kept.
        reply(&format!("{first}\n[2] 1887-12-21. The last"), "length"),
        reply(&format!("{second}\n[3] Die Mühle"), "length"),
        reply("[3] -\n[4] -", "stop"),
    ]);
    let mut request = planned(&[1, 2, 3, 4]);
    request.max_output_tokens = Some(u32::try_from(reserved).unwrap());
    let read = read(&model, &request).await.unwrap();
    let sent = model.requests();
    assert_eq!(
        sent.len(),
        2,
        "the reservation is spent: [3] and [4] are not asked for"
    );
    let ceilings: Vec<_> = sent.iter().map(|sent| sent.max_output_tokens).collect();
    let after_first = reserved - counter.count_tokens(first);
    assert_eq!(
        ceilings,
        vec![
            Some(u32::try_from(reserved).unwrap()),
            Some(u32::try_from(after_first).unwrap())
        ]
    );
    assert_eq!(read.text, notes, "complete lines only");
    assert_eq!(read.tokens, reserved, "no more than the answer may");
    assert_eq!(read.noted, [2].into());
    assert!(!read.lines.contains_key(&3));
}

#[test]
fn a_message_that_numbers_no_source_the_turn_holds_asks_for_no_reading() {
    let planned = planned(&[7, 8]);
    assert!(planned.input[1].text().contains(READING_INSTRUCTION));
    assert!(asked(&planned.input, &sources()).is_none());
}

#[test]
fn a_note_in_a_script_without_spaces_is_split_at_its_own_full_stops() {
    let passage = normalize_whitespace(
        "磨坊于1850年洪水后重新启用。磨坊主后来搬走了。磨坊在1860年卖给了磨坊主的家人。",
    );
    let notes =
        parse_notes("［5］：磨坊于1850年洪水后重新启用。磨坊在1860年卖给了磨坊主的家人。\n[6]：-");
    assert_eq!(
        notes,
        vec![
            (
                5,
                Some("磨坊于1850年洪水后重新启用。磨坊在1860年卖给了磨坊主的家人。".into())
            ),
            (6, None),
        ]
    );
    let note = notes[0].1.as_deref().unwrap();
    assert!(
        verbatim(note, &passage),
        "two sentences copied, not side by side"
    );
    assert!(!verbatim("磨坊于1850年重新启用。", &passage));
}

#[test]
fn a_resumed_turn_knows_its_saved_rounds_hold_the_reading() {
    let read = vec![
        CompletionInput::Message {
            role: "assistant".into(),
            content: "[1] -".into(),
        },
        CompletionInput::Message {
            role: "user".into(),
            content: WRITE_INSTRUCTION.into(),
        },
    ];
    assert!(noted_in(&read));
    let unread = vec![CompletionInput::ToolResult {
        id: "1".into(),
        output: "Results.".into(),
    }];
    assert!(!noted_in(&unread));
}

#[tokio::test]
async fn a_reading_with_no_label_in_it_gives_no_notes() {
    let model = Model::new([reply("The mill closed in 1887.", "stop")]);
    assert!(read(&model, &planned(&[1, 2, 3, 4])).await.is_none());
    assert_eq!(model.requests().len(), 1);
}

#[tokio::test]
async fn the_writing_round_is_the_reading_round_with_the_notes_and_the_instruction_after_it() {
    let request = planned(&[1, 2]);
    let mut writing = request.clone();
    let notes = Notes {
        text: "[1] -\n[2] -".into(),
        lines: BTreeMap::from([(1, None), (2, None)]),
        noted: BTreeSet::new(),
        tokens: 3,
    };
    assert!(append_notes(&Model::new([]), &mut writing, &notes).await);
    assert_eq!(
        writing.max_output_tokens,
        Some(777),
        "a window with room keeps the reservation"
    );
    assert_eq!(
        json(&writing.input[..request.input.len()].to_vec()),
        json(&request.input)
    );
    assert_eq!(writing.input.len(), request.input.len() + 2);
    assert!(matches!(
        &writing.input[request.input.len()],
        CompletionInput::Message { role, content } if role == "assistant" && content == "[1] -\n[2] -"
    ));
    assert_eq!(writing.input.last().unwrap().text(), WRITE_INSTRUCTION);
}

#[test]
fn a_continuation_quotes_each_left_out_note() {
    let notes = Notes {
        text: String::new(),
        lines: BTreeMap::from([(6, Some("A.".into())), (7, Some("B.".into()))]),
        noted: [6, 7].into(),
        tokens: 4,
    };
    assert_eq!(
        continue_instruction(&[6, 7].into(), &notes),
        "Your answer does not use these passages, which your notes quote as bearing on my question: [6] \"A.\" [7] \"B.\". Continue your answer with what each of them says, in the same form as above, citing each with its label; do not repeat what you have written. If one of those notes was mistaken, write nothing for it."
    );
}

fn json(value: &impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}
