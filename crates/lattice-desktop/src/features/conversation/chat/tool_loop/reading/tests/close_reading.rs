//! The close read: the passages the notes mark with a hyphen, shown again.

use super::super::close_read::{close_read, pack};
use super::*;
use crate::features::conversation::chat::cancellation::is_cancellation;

const MILL_NOTE: &str =
    "1887-12-21. The last miller at Greassby Mill retired, and the building was let as a store.";

/// A reading that copies [1] and marks the rest with a hyphen.
fn marked() -> String {
    format!("[1] {KITCHEN}\n[2] -\n[3] -\n[4] -")
}

fn shown_again(passages: &str) -> String {
    CLOSE_READ_INSTRUCTION.replace("{passages}", passages)
}

#[tokio::test]
async fn the_marked_passages_are_shown_again_and_a_copy_takes_the_hyphens_place() {
    let model = Model::new([
        reply(&marked(), "stop"),
        reply(&format!("[2] {MILL_NOTE}\n[3] -\n[4] -"), "stop"),
    ]);
    let request = planned(&[1, 2, 3, 4]);
    let notes = read(&model, &request).await.unwrap();
    let kept = format!("[1] {KITCHEN}\n[2] {MILL_NOTE}\n[3] -\n[4] -");
    assert_eq!(notes.text, kept);
    assert_eq!(notes.noted, [1, 2].into());
    assert_eq!(notes.tokens, model.count_tokens(&kept));

    let sent = model.requests();
    assert_eq!(
        sent.len(),
        2,
        "the reading and one showing of the marked passages"
    );
    let first = &sent[1];
    let planned_len = request.input.len();
    assert_eq!(
        json(&first.input[..planned_len].to_vec()),
        json(&request.input)
    );
    assert_eq!(first.input.len(), planned_len + 2);
    assert!(matches!(
        &first.input[planned_len],
        CompletionInput::Message { role, content } if role == "assistant" && *content == marked()
    ));
    assert_eq!(
        first.input.last().unwrap().text(),
        shown_again(&format!("[2] {MILL}\n\n[3] {MÜHLE}\n\n[4] {MOLINO}")),
        "in label order, a blank line apart"
    );
    assert_eq!(json(&first.tools), json(&request.tools));
    assert_eq!(first.tool_choice, Some(ToolChoice::None));
    assert_eq!(
        json(&first.sampling),
        json(&Some(SamplingOverride::deterministic()))
    );
    assert_eq!(first.thinking, ThinkingMode::Off);
    assert_eq!(first.max_output_tokens, Some(777));
    assert_eq!(first.stop, ["[5]", "\n5."]);
}

#[tokio::test]
async fn only_a_line_that_copies_its_own_passage_takes_a_hyphens_place() {
    let model = Model::new([
        reply(&marked(), "stop"),
        reply(
            &format!(
                "[2] The miller retired in December 1887.\n[3] {MOLINO}\n[4] {MOLINO}\n[1] {MÜHLE}"
            ),
            "stop",
        ),
    ]);
    let notes = read(&model, &planned(&[1, 2, 3, 4])).await.unwrap();
    assert_eq!(notes.lines.get(&2), Some(&None), "a paraphrase is no copy");
    assert_eq!(
        notes.lines.get(&3),
        Some(&None),
        "a copy of another passage is not this one's"
    );
    assert_eq!(notes.lines.get(&4), Some(&Some(MOLINO.to_string())));
    assert_eq!(
        notes.lines.get(&1),
        Some(&Some(KITCHEN.to_string())),
        "a label not shown again keeps its note"
    );
    assert_eq!(notes.noted, [1, 4].into());
    assert_eq!(
        notes.text,
        format!("[1] {KITCHEN}\n[2] -\n[3] -\n[4] {MOLINO}")
    );
    assert_eq!(model.requests().len(), 2);
}

#[tokio::test]
async fn each_marked_passage_is_shown_again_once() {
    let model = Model::new([
        reply(&marked(), "stop"),
        reply(&format!("[2] {MILL_NOTE}\n[3] -\n[4] -"), "stop"),
        reply(&format!("[3] {MÜHLE}"), "stop"),
    ]);
    let notes = read(&model, &planned(&[1, 2, 3, 4])).await.unwrap();
    assert_eq!(
        model.requests().len(),
        2,
        "no second showing, even after one that took a line"
    );
    assert_eq!(notes.noted, [1, 2].into());
    assert_eq!(notes.lines.get(&3), Some(&None));
}

#[test]
fn the_close_read_asks_for_its_lines_after_the_passages() {
    let said = shown_again("[2] PASSAGE");
    let (before, after) = said.split_once("[2] PASSAGE").unwrap();
    assert!(before.ends_with("marked with a hyphen.\n\n"));
    assert!(after.starts_with("\n\nWrite one line for each of these passages"));
    assert!(
        after.contains("the sentences of that passage that bear on my question")
            && after.contains("a single hyphen when it says nothing bearing on my question"),
        "the reading's own criterion, said again"
    );
    assert!(after.ends_with("Write only these lines."));
}

#[tokio::test]
async fn passages_that_do_not_fit_one_request_go_in_the_next_in_its_place() {
    let request = planned(&[1, 2, 3, 4]);
    let counter = Model::new([]);
    let mut bare = request.clone();
    bare.input.push(assistant(&marked()));
    say_last(&mut bare.input, &shown_again(""));
    let size = |label: u32, text: &str| counter.count_tokens(&format!("[{label}] {text}\n\n"));
    // Room for the largest marked passage alone, so for no two of them.
    let room = size(2, MILL).max(size(3, MÜHLE)).max(size(4, MOLINO));
    let window = prompt_tokens(&counter, &bare).await + 777 + room;
    let model = Model::with_context_tokens(window, [reply(&marked(), "stop")]);
    let notes = read(&model, &request).await.unwrap();
    assert_eq!(notes.text, marked(), "nothing taken");
    let sent = model.requests();
    assert_eq!(
        sent.len(),
        4,
        "the reading and one request for each marked passage"
    );
    let planned_len = request.input.len();
    let each = [
        format!("[2] {MILL}"),
        format!("[3] {MÜHLE}"),
        format!("[4] {MOLINO}"),
    ];
    for (part, shown) in sent[1..].iter().zip(each) {
        assert_eq!(
            part.input.len(),
            planned_len + 2,
            "each request replaces the one before"
        );
        assert_eq!(part.input[planned_len].text(), marked());
        assert_eq!(part.input.last().unwrap().text(), shown_again(&shown));
    }
}

#[test]
fn passages_are_packed_to_the_room_in_label_order() {
    assert_eq!(
        pack(&[(2, 30), (3, 26), (4, 10), (6, 5)], 40),
        vec![vec![2], vec![3, 4], vec![6]]
    );
    assert_eq!(pack(&[(2, 10), (3, 10), (4, 20)], 40), vec![vec![2, 3, 4]]);
}

#[test]
fn a_passage_larger_than_the_room_is_not_shown_again() {
    assert_eq!(
        pack(&[(2, 50), (3, 20), (4, 30)], 40),
        vec![vec![3], vec![4]]
    );
    assert!(pack(&[(2, 50)], 40).is_empty());
    assert!(pack(&[(2, 1)], 0).is_empty());
}

#[tokio::test]
async fn a_close_read_the_provider_fails_keeps_the_notes_as_they_were() {
    let model = Model::going_down_after([reply(&marked(), "stop")]);
    let notes = read(&model, &planned(&[1, 2, 3, 4]))
        .await
        .expect("a failed close read takes nothing away");
    assert_eq!(model.requests().len(), 2);
    assert_eq!(notes.text, marked());
    assert_eq!(notes.noted, [1].into());
    assert_eq!(notes.tokens, model.count_tokens(&marked()));
}

#[tokio::test]
async fn stop_during_the_close_read_is_an_error() {
    let model = Model::new([reply(&format!("[2] {MILL_NOTE}"), "stop")]);
    let mut notes = Notes {
        text: marked(),
        lines: BTreeMap::from([
            (1, Some(KITCHEN.to_string())),
            (2, None),
            (3, None),
            (4, None),
        ]),
        noted: [1].into(),
        tokens: model.count_tokens(&marked()),
    };
    let stop = CancellationToken::new();
    stop.cancel();
    let error = close_read(
        &model,
        &planned(&[1, 2, 3, 4]),
        &passages(),
        &[],
        &mut notes,
        &TurnRecorder::silent("conversation", "request"),
        &stop,
        &TurnClock::new(Duration::from_secs(60), Duration::ZERO),
    )
    .await
    .expect_err("Stop is an error");
    assert!(is_cancellation(&error));
    assert_eq!(notes.text, marked());
    assert_eq!(notes.lines.get(&2), Some(&None));
}

#[tokio::test]
async fn a_reading_taken_back_is_not_read_again() {
    let model = Model::new([reply(
        "[1] -\n[2] The last miller retired in December 1887.\n[3] -\n[4] -",
        "stop",
    )]);
    assert!(read(&model, &planned(&[1, 2, 3, 4])).await.is_none());
    assert_eq!(model.requests().len(), 1, "no copy, so no close read");
}
