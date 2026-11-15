use super::*;
use crate::features::conversation::chat::reply_text::{markers, ReplyText};
use crate::features::conversation::chat::retrieval::source_names::Source;

fn names_with_sources(count: u32) -> Arc<TurnNames> {
    let names = Arc::new(TurnNames::unstored());
    let sources: Vec<Source> = (1..=count)
        .map(|number| Source {
            document_id: format!("doc-{number}"),
            chunk_id: format!("chunk-{number}"),
        })
        .collect();
    names
        .with(|scope| scope.request().show(&sources).map(|_| ()))
        .unwrap();
    names
}

/// The events of an emitter that is fed `rounds` (each a list of chunks) and flushed after each.
fn stream(names: Arc<TurnNames>, rounds: &[&[&str]]) -> Vec<ChatStreamEventDto> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let sink: ChatEventSink = Arc::new(move |event| {
        captured.lock().unwrap().push(event);
        Ok(())
    });
    {
        let emitter = StreamEmitter::new(&sink, "conversation", "request", names, markers(false));
        for round in rounds {
            for chunk in *round {
                emitter.content(chunk).unwrap();
            }
            emitter.flush().unwrap();
        }
    }
    let events = events.lock().unwrap().clone();
    events
}

fn references(events: &[ChatStreamEventDto]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| event.reference.as_ref())
        .map(|reference| reference.written.clone())
        .collect()
}

fn text(events: &[ChatStreamEventDto]) -> String {
    events
        .iter()
        .filter_map(|event| event.content.as_deref())
        .collect()
}

#[test]
fn a_marker_split_across_two_chunks_is_one_reference_event() {
    let events = stream(names_with_sources(2), &[&["see [s", "1] and more"]]);
    assert_eq!(references(&events), vec!["[s1]".to_owned()]);
    let shown: Vec<(Option<&str>, bool)> = events
        .iter()
        .map(|event| (event.content.as_deref(), event.reference.is_some()))
        .collect();
    assert_eq!(
        shown,
        vec![
            (Some("see "), false),
            (None, true),
            (Some(" and more"), false),
            (None, false),
        ]
    );
    let reference = events[1].reference.as_ref().unwrap();
    assert_eq!((reference.kind.as_str(), reference.number), ("source", 1));
    assert!(events[3].done);
}

#[test]
fn a_marker_in_a_code_span_or_fence_is_text() {
    let reply = "Use `[s1]` and\n```\n[s1]\n```\nthen [s2]";
    let events = stream(names_with_sources(2), &[&[reply]]);
    assert_eq!(references(&events), vec!["[s2]".to_owned()]);
    assert_eq!(text(&events), "Use `[s1]` and\n```\n[s1]\n```\nthen ");
}

#[test]
fn a_bracketed_token_that_names_nothing_is_text() {
    let reply = "Not [s9] nor [x1] nor [3].";
    let events = stream(names_with_sources(2), &[&[reply]]);
    assert!(references(&events).is_empty());
    assert_eq!(text(&events), reply);
}

#[test]
fn the_end_of_a_round_makes_what_was_held_text() {
    let events = stream(names_with_sources(2), &[&["tail [s"], &["1] next"]]);
    assert!(references(&events).is_empty());
    assert_eq!(text(&events), "tail [s1] next");
}

#[test]
fn the_references_a_reply_is_saved_with_are_the_ones_its_last_round_streamed() {
    let names = names_with_sources(3);
    let chunks: &[&str] = &["One [s", "1], two [s3] and `[s", "2]`, then [s2]."];
    let events = stream(Arc::clone(&names), &[chunks]);
    let saved = ReplyText::read(&names, markers(false), &chunks.concat());
    let kept: Vec<String> = saved
        .references
        .iter()
        .map(|placed| ReplyReferenceDto::from(&placed.reference).written)
        .collect();
    assert_eq!(references(&events), kept);
    assert_eq!(kept, vec!["[s1]", "[s3]", "[s2]"]);
}
