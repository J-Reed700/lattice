use super::*;
use crate::features::conversation::chat::retrieval::Rendered;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::test_support::snapshot::assert_requests;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};

fn message(role: &str, content: &str) -> CompletionInput {
    CompletionInput::Message {
        role: role.into(),
        content: content.into(),
    }
}

fn call(id: &str) -> CompletionInput {
    CompletionInput::ToolCall {
        id: id.into(),
        name: "fetch_url_content".into(),
        arguments: serde_json::json!({ "url": format!("https://example.test/{id}") }),
    }
}

fn result(id: &str, chars: usize) -> CompletionInput {
    CompletionInput::ToolResult {
        id: id.into(),
        output: "r".repeat(chars),
    }
}

fn transcript(rounds: &[&[usize]]) -> Vec<CompletionInput> {
    let mut input = vec![message("system", "policy"), message("user", "question")];
    for (round, results) in rounds.iter().enumerate() {
        for index in 0..results.len() {
            input.push(call(&format!("r{round}c{index}")));
        }
        for (index, chars) in results.iter().enumerate() {
            input.push(result(&format!("r{round}c{index}"), *chars));
        }
    }
    input
}

#[test]
fn the_results_count_and_the_planned_request_does_not() {
    let input = transcript(&[&[100, 200], &[300]]);
    assert_eq!(loop_result_chars(&input, 2), 600);
    let mut with_notes = input.clone();
    with_notes.insert(2, message("user", &format!("{NOTES_HEADER}\n\nfacts")));
    assert_eq!(
        loop_result_chars(&with_notes, 2),
        600 + NOTES_HEADER.len() + 7
    );
}

#[test]
fn it_condenses_past_the_share_and_at_most_twice() {
    let input = transcript(&[&[7_000]]);
    assert!(should_condense(10_000, &input, 2, 0));
    assert!(should_condense(10_000, &input, 2, 1));
    assert!(!should_condense(10_000, &input, 2, MAX_CONDENSES_PER_TURN));
    assert!(!should_condense(20_000, &input, 2, 0), "under the share");
    assert!(!should_condense(0, &input, 2, 0), "no window reported");
}

#[test]
fn everything_after_the_planned_request_is_condensed_earlier_notes_included() {
    let mut input = transcript(&[&[3_000], &[2_000, 1_000]]);
    assert_eq!(condensable_span(&input, 2), Some(2..input.len()));
    input.splice(
        2..4,
        [message(
            "user",
            &format!("{NOTES_HEADER}\n\n{}", "n".repeat(400)),
        )],
    );
    assert_eq!(condensable_span(&input, 2), Some(2..input.len()));
}

#[test]
fn nothing_to_condense_without_results() {
    let input = transcript(&[]);
    assert_eq!(condensable_span(&input, 2), None);
}

const QUESTION: &str = "When did the harbour bridge open?";
const OPENED: &str = "The harbour bridge opened in 1932 [1].";
const WIDENED: &str = "Its deck was widened in 1960 [2].";
const NOISE: &str =
    "Sign up for the newsletter to hear about tide tables and kiln rotas every single week.";

fn noise() -> String {
    [NOISE; 10].join(" ")
}

fn request_of(items: Vec<CompletionInput>) -> CompletionRequest {
    legacy::completion(legacy::Completion {
        input: items,
        ..Default::default()
    })
}

/// The pages the harbour tools returned: a call and its result for each.
fn pages(pages: &[(&str, String)]) -> Vec<CompletionInput> {
    let mut input = vec![message("system", "policy"), message("user", QUESTION)];
    input.extend(pages.iter().map(|(id, _)| call(id)));
    input.extend(pages.iter().map(|(id, text)| CompletionInput::ToolResult {
        id: (*id).into(),
        output: text.clone(),
    }));
    input
}

fn two_pages() -> Vec<CompletionInput> {
    pages(&[
        ("a", format!("{OPENED} {}", noise())),
        ("b", format!("{WIDENED} {}", noise())),
    ])
}

fn prompt_of(sent: &Sent) -> String {
    shown_in(&sent.request)
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn cites(text: &str) -> bool {
    text.contains('[')
}

async fn condensed(model: &ScriptedModel, request: &mut CompletionRequest) -> Option<Condensed> {
    condensed_with(model, request, &mut Labels::default()).await
}

async fn condensed_with(
    model: &ScriptedModel,
    request: &mut CompletionRequest,
    labels: &mut Labels,
) -> Option<Condensed> {
    condense(
        model,
        request,
        2,
        &TurnRecorder::silent("conversation", "request"),
        &CancellationToken::new(),
        &TurnClock::new(
            std::time::Duration::from_secs(60),
            std::time::Duration::ZERO,
        ),
        labels,
    )
    .await
    .unwrap()
}

fn notes_in(request: &CompletionRequest) -> String {
    match request.input().last() {
        Some(CompletionInput::Message { content, .. }) => content.clone(),
        other => panic!("the notes should end the transcript: {other:?}"),
    }
}

#[tokio::test]
async fn the_chosen_sentences_are_written_as_they_stand() {
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(&prompt_of(sent), |_| true, cites))
    });
    let mut request = request_of(two_pages());
    let condensed = condensed(&model, &mut request).await.expect("notes");
    assert!(condensed.chars_after < condensed.chars_before);
    assert_eq!(
        condensed.result_ids,
        HashSet::from(["a".to_string(), "b".to_string()])
    );
    let notes = notes_in(&request);
    assert!(notes.starts_with(NOTES_HEADER));
    for kept in [OPENED, WIDENED, "example.test/a", "example.test/b"] {
        assert!(notes.contains(kept), "{kept}");
    }
    assert!(!notes.contains(NOISE));
    assert_eq!(request.input().len(), 3, "the notes replace the results");
    assert!(prompt_of(&model.sent()[0]).contains(QUESTION));
}

#[tokio::test]
async fn a_result_left_out_is_asked_for_again_alone_and_the_others_stand() {
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, nth| {
        let first = nth == 0;
        reply(scripted_answer(
            &prompt_of(sent),
            |heading| !first || !heading.contains("example.test/b"),
            cites,
        ))
    });
    let mut request = request_of(two_pages());
    condensed(&model, &mut request).await.expect("notes");
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    let second = prompt_of(&sent[1]);
    assert!(second.contains(WIDENED) && !second.contains(OPENED));
    let notes = notes_in(&request);
    assert!(notes.contains(OPENED) && notes.contains(WIDENED));
}

#[tokio::test]
async fn a_result_that_stays_unanswered_is_kept_whole() {
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(
            &prompt_of(sent),
            |heading| !heading.contains("example.test/b"),
            cites,
        ))
    });
    let mut request = request_of(two_pages());
    condensed(&model, &mut request).await.expect("notes");
    let notes = notes_in(&request);
    let (first, second) = notes.split_once("example.test/b").expect("both results");
    assert!(first.contains(OPENED) && !first.contains(NOISE));
    assert!(second.contains(WIDENED) && second.contains(NOISE));
}

#[tokio::test]
async fn a_result_of_which_nothing_is_kept_says_so() {
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(
            &prompt_of(sent),
            |_| true,
            |text| text == OPENED,
        ))
    });
    let mut request = request_of(two_pages());
    condensed(&model, &mut request).await.expect("notes");
    let notes = notes_in(&request);
    let (_, second) = notes.split_once("example.test/b").expect("both results");
    assert!(second.contains(NOTHING_KEPT) && !second.contains(WIDENED));
}

#[tokio::test]
async fn notes_no_shorter_than_the_results_are_not_used() {
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(&prompt_of(sent), |_| true, |_| true))
    });
    let mut request = request_of(two_pages());
    assert!(condensed(&model, &mut request).await.is_none());
    assert_eq!(request.input().len(), two_pages().len());
}

#[tokio::test]
async fn earlier_notes_are_condensed_with_the_results() {
    let lines: Vec<&str> = [OPENED, NOISE].into_iter().cycle().take(12).collect();
    let earlier = format!(
        "{NOTES_HEADER}\n\nResult of earlier:\n{}",
        lines
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let mut labels = Labels {
        notes: vec![Source {
            id: "earlier".into(),
            heading: Told("Result of earlier".into()),
            sentences: lines
                .iter()
                .enumerate()
                .map(|(at, line)| Sentence {
                    at,
                    text: Told((*line).to_owned()),
                })
                .collect(),
            legend: Vec::new(),
        }],
        ..Labels::default()
    };
    let mut items = vec![
        message("system", "policy"),
        message("user", QUESTION),
        message("user", &earlier),
    ];
    items.push(call("b"));
    items.push(CompletionInput::ToolResult {
        id: "b".into(),
        output: format!("{WIDENED} {}", noise()),
    });
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(&prompt_of(sent), |_| true, cites))
    });
    let mut request = request_of(items);
    condensed_with(&model, &mut request, &mut labels)
        .await
        .expect("notes");
    let prompt = prompt_of(&model.sent()[0]);
    assert!(prompt.contains("Result of earlier") && prompt.contains(OPENED));
    let notes = notes_in(&request);
    assert!(notes.contains("Result of earlier:") && notes.contains(WIDENED));
    assert_eq!(request.input().len(), 3);
}

#[test]
fn the_notes_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Input {
        question: marks.told(QUESTION.to_owned()),
        results: vec![Source {
            id: marks.reference("call-0123456789".to_owned()),
            heading: marks.told("Result of fetch_url_content".to_owned()),
            sentences: vec![
                Sentence {
                    at: 0,
                    text: marks.told(OPENED.to_owned()),
                },
                Sentence {
                    at: 1,
                    text: marks.told(NOISE.to_owned()),
                },
            ],
            legend: Vec::new(),
        }],
    };
    holds::<ResearchNotes>(&input, &marks);
}

const BRIDGE: &str = "The harbour bridge opened in 1932.";
const DECK: &str = "Its deck was widened in 1960.";
const TIDES: &str = "Tides rise twice a day.";
const HEADING: &str = "Citable passage [s3] — Page B";
const LEGEND_A: &str =
    "Citable passage [s1] — Page A (https://example.test/a): its text is in this result above.";
const LEGEND_B: &str =
    "Citable passage [s2] — Page B (https://example.test/b): its text is in this result above.";

fn passage_of(heading: &str, body: &str) -> Rendered {
    let text = format!("{heading}\n{body}");
    Rendered {
        passages: vec![Passage {
            heading: heading.to_owned(),
            line: 0..heading.len(),
            body: heading.len() + 1..text.len(),
        }],
        text,
        legend: Vec::new(),
    }
}

fn legend_of(line: &str) -> Rendered {
    Rendered {
        text: line.to_owned(),
        passages: Vec::new(),
        legend: vec![Legend {
            line: line.to_owned(),
            at: 0..line.len(),
        }],
    }
}

fn labelled_result(id: &str, rendered: &Rendered, labels: &mut Labels) -> CompletionInput {
    labels.keep(id, rendered.passages.clone(), rendered.legend.clone());
    CompletionInput::ToolResult {
        id: id.into(),
        output: rendered.text.clone(),
    }
}

fn no_label_is_a_sentence(prompt: &str) {
    for line in prompt.lines().filter(|line| line.starts_with("[u")) {
        assert!(!line.contains("Citable passage"), "{line}");
    }
}

fn with_passage_and_noise() -> (Vec<CompletionInput>, Labels) {
    let mut result = Rendered::from(format!("Page A\n{}\n\n", noise()));
    result.append(passage_of(
        HEADING,
        &format!("{TIDES} {BRIDGE} {}", noise()),
    ));
    let mut labels = Labels::default();
    let output = labelled_result("a", &result, &mut labels);
    let items = vec![
        message("system", "policy"),
        message("user", QUESTION),
        call("a"),
        output,
    ];
    (items, labels)
}

#[tokio::test]
async fn a_kept_sentence_stands_under_its_passage_s_label_without_the_model_keeping_it() {
    let (items, mut labels) = with_passage_and_noise();
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(
            &prompt_of(sent),
            |_| true,
            |text| text == BRIDGE,
        ))
    });
    let mut request = request_of(items);
    condensed_with(&model, &mut request, &mut labels)
        .await
        .expect("notes");
    let notes = notes_in(&request);
    assert!(
        notes.contains(&format!("{HEADING}:\n- {BRIDGE}")),
        "{notes}"
    );
    assert!(!notes.contains(TIDES), "{notes}");
    let prompt = prompt_of(&model.sent()[0]);
    no_label_is_a_sentence(&prompt);
    assert!(
        prompt
            .lines()
            .any(|line| line.starts_with("[r") && line.ends_with(HEADING)),
        "{prompt}"
    );
}

#[tokio::test]
async fn a_page_labelled_by_a_legend_line_keeps_it_with_a_kept_sentence_only() {
    let mut labels = Labels::default();
    let mut first = Rendered::from(format!("{BRIDGE} {}\n\n", noise()));
    first.append(legend_of(LEGEND_A));
    let mut second = Rendered::from(format!("{DECK} {}\n\n", noise()));
    second.append(legend_of(LEGEND_B));
    let mut items = vec![
        message("system", "policy"),
        message("user", QUESTION),
        call("a"),
        call("b"),
    ];
    items.push(labelled_result("a", &first, &mut labels));
    items.push(labelled_result("b", &second, &mut labels));
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(
            &prompt_of(sent),
            |_| true,
            |text| text == BRIDGE,
        ))
    });
    let mut request = request_of(items);
    condensed_with(&model, &mut request, &mut labels)
        .await
        .expect("notes");
    let notes = notes_in(&request);
    assert!(
        notes.contains(&format!("- {BRIDGE}\n{LEGEND_A}")),
        "{notes}"
    );
    assert!(!notes.contains(LEGEND_B), "{notes}");
    assert!(notes.contains(NOTHING_KEPT), "{notes}");
    no_label_is_a_sentence(&prompt_of(&model.sent()[0]));
}

#[tokio::test]
async fn a_second_condensing_keeps_the_labels_of_the_first() {
    let (items, mut labels) = with_passage_and_noise();
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(
            &prompt_of(sent),
            |_| true,
            |text| text.contains("1932"),
        ))
    });
    let mut request = request_of(items);
    condensed_with(&model, &mut request, &mut labels)
        .await
        .expect("notes");
    let mut later = request.input().to_vec();
    later.push(call("c"));
    later.push(result("c", 1_200));
    let mut request = request_of(later);
    condensed_with(&model, &mut request, &mut labels)
        .await
        .expect("notes");
    let notes = notes_in(&request);
    assert!(notes.contains(&format!("{HEADING}:\n- ")), "{notes}");
    assert!(notes.contains(BRIDGE), "{notes}");
    assert!(!notes.contains(TIDES), "{notes}");
    let prompt = prompt_of(&model.sent()[1]);
    no_label_is_a_sentence(&prompt);
    assert!(
        !prompt.contains("Earlier notes"),
        "every earlier block is offered under its own heading: {prompt}"
    );
}

fn heading_for(id: &str) -> String {
    format!(
        "Result of fetch_url_content {}",
        serde_json::json!({ "url": format!("https://example.test/{id}") })
    )
}

#[tokio::test]
async fn each_block_keeps_its_own_legend_line_through_two_condensings() {
    let mut labels = Labels::default();
    let mut first = Rendered::from(format!("{BRIDGE} {}\n\n", noise()));
    first.append(legend_of(LEGEND_A));
    let mut second = Rendered::from(format!("{DECK} {}\n\n", noise()));
    second.append(legend_of(LEGEND_B));
    let mut items = vec![
        message("system", "policy"),
        message("user", QUESTION),
        call("a"),
        call("b"),
    ];
    items.push(labelled_result("a", &first, &mut labels));
    items.push(labelled_result("b", &second, &mut labels));
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(
            &prompt_of(sent),
            |_| true,
            |text| text.contains("1932") || text.contains("1960"),
        ))
    });
    let mut request = request_of(items);
    condensed_with(&model, &mut request, &mut labels)
        .await
        .expect("notes");
    let mut later = request.input().to_vec();
    later.push(call("c"));
    later.push(result("c", 1_200));
    let mut request = request_of(later);
    condensed_with(&model, &mut request, &mut labels)
        .await
        .expect("notes");
    let notes = notes_in(&request);
    assert!(!notes.contains("- - "), "{notes}");
    let (head_a, head_b) = (heading_for("a"), heading_for("b"));
    let at = |needle: &str| notes.find(needle).unwrap_or(usize::MAX);
    assert!(at(&head_a) < at(BRIDGE), "{notes}");
    assert!(at(BRIDGE) < at(LEGEND_A), "{notes}");
    assert!(at(LEGEND_A) < at(&head_b), "{notes}");
    assert!(at(&head_b) < at(DECK), "{notes}");
    assert!(at(DECK) < at(LEGEND_B), "{notes}");
    assert_eq!(notes.matches(LEGEND_A).count(), 1, "{notes}");
    assert_eq!(notes.matches(LEGEND_B).count(), 1, "{notes}");
    let prompt = prompt_of(&model.sent()[1]);
    no_label_is_a_sentence(&prompt);
    assert!(!prompt.contains("Earlier notes"), "{prompt}");
    assert!(
        prompt
            .lines()
            .filter(|line| line.starts_with("[u"))
            .all(|line| !line.contains("] - ")),
        "{prompt}"
    );
}

#[tokio::test]
async fn notes_are_offered_again_as_the_sentences_they_were_written_from() {
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(
            &prompt_of(sent),
            |_| true,
            |text| text == OPENED,
        ))
    });
    let mut labels = Labels::default();
    let mut request = request_of(two_pages());
    condensed_with(&model, &mut request, &mut labels)
        .await
        .expect("notes");
    assert!(notes_in(&request).contains(NOTHING_KEPT));
    let mut later = request.input().to_vec();
    later.push(call("c"));
    later.push(result("c", 1_200));
    let mut request = request_of(later);
    condensed_with(&model, &mut request, &mut labels)
        .await
        .expect("notes");
    let prompt = prompt_of(&model.sent()[1]);
    let offered: Vec<&str> = prompt
        .lines()
        .filter(|line| line.starts_with("[u"))
        .collect();
    assert!(
        offered
            .iter()
            .any(|line| line.ends_with(OPENED) && !line.contains("] - ")),
        "{prompt}"
    );
    assert!(
        offered.iter().all(|line| !line.contains(NOTHING_KEPT)),
        "{prompt}"
    );
    assert!(!notes_in(&request).contains("- - "));
}

#[test]
fn a_result_is_cut_where_the_app_recorded_its_labels() {
    let result = passage_of(HEADING, &format!("{TIDES} {BRIDGE}"));
    let marked = Marked {
        passages: result.passages.clone(),
        legend: Vec::new(),
    };
    let sources = sources_in("a", "Result of x", &result.text, Some(&marked));
    assert_eq!(sources.len(), 1, "no text is left outside the passage");
    assert_eq!(sources[0].heading.0, HEADING);
    let texts: Vec<&str> = sources[0]
        .sentences
        .iter()
        .map(|sentence| sentence.text.0.as_str())
        .collect();
    assert_eq!(texts, [TIDES, BRIDGE]);
}

#[test]
fn a_range_that_is_not_in_the_result_is_ignored_and_the_result_stands_whole() {
    let text = format!("{TIDES} {BRIDGE}");
    let marked = Marked {
        passages: vec![Passage {
            heading: HEADING.to_owned(),
            line: 0..5,
            body: 6..text.len() + 40,
        }],
        legend: Vec::new(),
    };
    let sources = sources_in("a", "Result of x", &text, Some(&marked));
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].heading.0, "Result of x");
    assert_eq!(sources[0].sentences.len(), 2);
}

#[test]
fn results_that_were_condensed_lose_their_labels() {
    let mut labels = Labels::default();
    let result = passage_of(HEADING, TIDES);
    labels.keep("a", result.passages.clone(), Vec::new());
    labels.keep("b", result.passages, Vec::new());
    labels.forget(&HashSet::from(["a".to_owned()]));
    assert!(!labels.by_result.contains_key("a"));
    assert!(labels.by_result.contains_key("b"));
}

#[tokio::test]
async fn prompt_snapshot_research_notes() {
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(&prompt_of(sent), |_| true, cites))
    });
    let mut request = request_of(two_pages());
    condensed(&model, &mut request).await.expect("notes");
    let requests: Vec<_> = model.sent().into_iter().map(|sent| sent.request).collect();
    assert_requests(env!("CARGO_MANIFEST_DIR"), "research-notes", &requests);
}

#[tokio::test]
async fn the_condensing_request_asks_for_no_thinking() {
    let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
        reply(scripted_answer(&prompt_of(sent), |_| true, cites))
    });
    let mut request = request_of(two_pages());
    condensed(&model, &mut request).await.expect("notes");
    let sent = model.sent();
    assert!(!sent.is_empty());
    for sent in &sent {
        assert_eq!(sent.request.reasoning_effort.as_deref(), Some("none"));
    }
}
