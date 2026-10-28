//! The reading, a group of passages at a time.

use super::super::groups::{group_input, planned as plan_groups, shown, PASSAGES_PER_GROUP};
use super::*;
use crate::features::conversation::chat::cancellation::is_cancellation;

const SLUICE: &str = "The sluice gate was rebuilt in oak in 1902.";
const WEIR: &str = "Floods in 1911 breached the weir twice.";
const LEAT: &str = "The leat was filled in when the wheel stopped.";
const GRANARY: &str = "A granary stood beside the mill until 1934.";

/// Eight passages: two groups.
fn eight() -> Vec<SourceDto> {
    let mut all = sources();
    for (label, content) in [(5, SLUICE), (6, WEIR), (7, LEAT), (8, GRANARY)] {
        all.push(source(label, content));
    }
    all
}

fn eight_passages() -> Passages {
    asked(&planned_request(), &eight()).expect("the message asks for a reading")
}

fn planned_request() -> Vec<CompletionInput> {
    planned(&[1, 2, 3, 4, 5, 6, 7, 8]).input
}

async fn read_eight(model: &Model) -> Result<Option<Notes>> {
    read_with(model, &CancellationToken::new()).await
}

async fn read_with(model: &Model, cancel: &CancellationToken) -> Result<Option<Notes>> {
    read_passages(
        model,
        &planned(&[1, 2, 3, 4, 5, 6, 7, 8]),
        &eight_passages(),
        &TurnRecorder::silent("conversation", "request"),
        cancel,
        &TurnClock::new(Duration::from_secs(60), Duration::ZERO),
    )
    .await
}

#[test]
fn a_group_holds_four_passages() {
    assert_eq!(PASSAGES_PER_GROUP, 4, "the size the A/B chose");
}

#[tokio::test]
async fn every_group_is_read_in_label_order_with_only_its_own_passages_shown_again() {
    let model = Model::new([
        reply(&format!("[1] {KITCHEN}\n[2] -\n[3] -\n[4] -"), "stop"),
        reply(&format!("[5] {SLUICE}\n[6] -\n[7] -\n[8] -"), "stop"),
    ]);
    let notes = read_eight(&model).await.unwrap().expect("notes");
    let sent = model.requests();
    assert_eq!(sent.len(), 2, "one request per group");
    for (request, labels) in sent.iter().zip([[1, 2, 3, 4], [5, 6, 7, 8]]) {
        let message = request.input[1].text();
        for label in 1..=8 {
            let shown_again = message.matches(&format!("[{label}] ")).count() > 1;
            assert_eq!(
                shown_again,
                labels.contains(&label),
                "[{label}] in {labels:?}"
            );
        }
        let next = labels[3] + 1;
        assert_eq!(request.stop, [format!("[{next}]"), format!("\n{next}.")]);
        assert!(request.max_output_tokens.is_some());
    }
    assert_eq!(
        notes.text,
        format!("[1] {KITCHEN}\n[2] -\n[3] -\n[4] -\n[5] {SLUICE}\n[6] -\n[7] -\n[8] -")
    );
    assert_eq!(notes.noted, [1, 5].into());
}

#[tokio::test]
async fn a_backend_that_keeps_read_prompts_reads_the_message_alone_before_the_groups() {
    let model = Model::keeping_read_prompts([
        reply("", "length"),
        reply(&format!("[1] {KITCHEN}\n[2] -\n[3] -\n[4] -"), "stop"),
        reply(&format!("[5] {SLUICE}\n[6] -\n[7] -\n[8] -"), "stop"),
    ]);
    let notes = read_eight(&model).await.unwrap().expect("notes");
    let sent = model.requests();
    assert_eq!(sent.len(), 3, "the message, then one request per group");
    assert_eq!(sent[0].max_output_tokens, Some(1));
    let ahead = sent[0].input[1].text();
    assert_eq!(ahead.matches("[5] ").count(), 1, "no passage shown again");
    for group in &sent[1..] {
        let message = group.input[1].text();
        assert!(message.len() > ahead.len() && message.starts_with(&*ahead));
    }
    assert_eq!(notes.noted, [1, 5].into());
}

#[tokio::test]
async fn a_group_writes_only_its_own_labels() {
    let model = Model::new([
        // The first group runs on into a label of the second.
        reply(&format!("[1] -\n[2] -\n[3] -\n[4] -\n[5] {SLUICE}"), "stop"),
        reply("[5] -\n[6] -\n[7] -\n[8] -", "stop"),
    ]);
    let notes = read_eight(&model).await.unwrap();
    assert!(
        notes.is_none(),
        "[5] was read by its own group, as a hyphen"
    );
}

#[tokio::test]
async fn the_reservation_holds_every_group_together() {
    let first = format!("[1] {KITCHEN}\n[2] -\n[3] -\n[4] -");
    let counter = Model::new([]);
    let reserved = counter.count_tokens(&first);
    let model = Model::new([reply(&first, "stop")]);
    let mut request = planned(&[1, 2, 3, 4, 5, 6, 7, 8]);
    request.max_output_tokens = Some(u32::try_from(reserved).unwrap());
    let notes = read_passages(
        &model,
        &request,
        &eight_passages(),
        &TurnRecorder::silent("conversation", "request"),
        &CancellationToken::new(),
        &TurnClock::new(Duration::from_secs(60), Duration::ZERO),
    )
    .await
    .unwrap()
    .expect("notes");
    assert_eq!(
        model.requests().len(),
        1,
        "the first group spent the reservation: the second is not asked for"
    );
    assert_eq!(notes.tokens, reserved);
}

#[tokio::test]
async fn a_group_the_window_cannot_hold_is_split_and_a_passage_it_cannot_hold_is_read_where_it_stands(
) {
    let request = planned(&[1, 2, 3, 4]);
    let passages = passages();
    let counter = Model::new([]);
    let sent_with = |group: &[u32]| {
        let input = group_input(&request, &shown(&passages, group));
        CompletionRequest {
            input,
            tools: request.tools.clone(),
            ..Default::default()
        }
    };
    // Room for [1] and [2] shown again with a copy of them, not for all four.
    let pair = sent_with(&[1, 2]);
    let window =
        prompt_tokens(&counter, &pair).await + counter.count_tokens(&shown(&passages, &[1, 2]));
    let model = Model::with_context_tokens(window, []);
    let readings = plan_groups(&model, &request, &passages).await;
    let labels: Vec<_> = readings
        .iter()
        .map(|reading| reading.labels.clone())
        .collect();
    assert_eq!(labels[0], [1, 2], "four do not fit; two do");
    assert!(
        labels.concat() == [1, 2, 3, 4],
        "every passage is read: {labels:?}"
    );

    // A window that holds no passage shown again: each is read by its label.
    let model = Model::with_context_tokens(prompt_tokens(&counter, &request).await + 1, []);
    let readings = plan_groups(&model, &request, &passages).await;
    assert_eq!(readings.len(), 4);
    for reading in &readings {
        let message = reading.input[1].text();
        let label = reading.labels[0];
        assert!(
            message.ends_with(&LABELS_INSTRUCTION.replace("{labels}", &format!("[{label}]"))),
            "{message}"
        );
    }
}

#[tokio::test]
async fn a_group_the_provider_fails_keeps_what_the_others_read() {
    let model = Model::going_down_after([reply(
        &format!("[1] {KITCHEN}\n[2] -\n[3] -\n[4] -"),
        "stop",
    )]);
    let notes = read_eight(&model)
        .await
        .expect("a failed group is no error")
        .expect("the first group's notes");
    assert_eq!(model.requests().len(), 2);
    assert_eq!(notes.noted, [1].into());
    assert!(!notes.lines.contains_key(&5));
}

#[tokio::test]
async fn stop_while_reading_is_an_error() {
    let model = Model::new([reply(&format!("[1] {KITCHEN}"), "stop")]);
    let stop = CancellationToken::new();
    stop.cancel();
    let error = read_with(&model, &stop)
        .await
        .expect_err("Stop is an error");
    assert!(is_cancellation(&error));
}

#[tokio::test]
async fn a_note_copied_from_a_passage_of_another_group_is_that_passages_note() {
    let model = Model::new([
        // [1]'s line copies [5], a passage of the next group.
        reply(&format!("[1] {SLUICE}\n[2] -\n[3] -\n[4] -"), "stop"),
        reply("[5] -\n[6] -\n[7] -\n[8] -", "stop"),
    ]);
    let notes = read_eight(&model).await.unwrap().expect("notes");
    assert_eq!(notes.noted, [5].into());
    assert_eq!(notes.lines.get(&5), Some(&Some(SLUICE.to_string())));
}
