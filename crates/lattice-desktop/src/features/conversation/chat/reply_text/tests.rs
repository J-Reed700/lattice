use super::*;
use crate::features::conversation::chat::retrieval::source_names::name_sources;
use serde_json::json;

fn source(document: &str, old_number: Option<u32>) -> SourceDto {
    SourceDto {
        page_number: None,
        from_ocr: false,
        document_id: document.to_owned(),
        chunk_id: format!("{document}-chunk"),
        content: "Passage".to_owned(),
        score: 0.5,
        path: None,
        position: None,
        file_name: format!("{document}.txt"),
        file_path: format!("/notes/{document}.txt"),
        mime_type: "text/plain".to_owned(),
        category: "Text File".to_owned(),
        file_size_bytes: 10,
        modified_at: "2025-01-01T00:00:00Z".to_owned(),
        excerpt: None,
        highlights: None,
        section: None,
        chunk_index: None,
        chunk_excerpts: None,
        citation_id: old_number,
        web_snapshot: None,
    }
}

/// A conversation that already names one source of its own, so the numbers an
/// old message's sources get are not the numbers they had.
fn names_with_one_source() -> TurnNames {
    let names = TurnNames::unstored();
    let mut earlier = vec![source("doc-earlier", None)];
    name_sources(&names, &mut earlier);
    names
}

fn named(names: &TurnNames, documents: &[&str]) -> Vec<SourceDto> {
    let mut sources: Vec<SourceDto> = documents
        .iter()
        .map(|document| source(document, None))
        .collect();
    name_sources(names, &mut sources);
    sources
}

#[test]
fn an_old_marker_becomes_a_reference_to_its_own_source() {
    let names = names_with_one_source();
    let sources = vec![
        source("doc-a", Some(1)),
        source("doc-b", Some(2)),
        source("doc-c", Some(3)),
    ];
    let reply = ReplyText::legacy(&names, "Start [2] and [1, 3]. Not [9].", &sources);
    assert_eq!(reply.text, "Start [s3] and [s2][s4]. Not [9].");
    assert_eq!(reply.cited(), vec![3, 2, 4]);
    assert_eq!(reply.references.len(), 3);
}

#[test]
fn an_old_marker_in_code_or_a_link_or_a_formula_is_left_alone() {
    let names = names_with_one_source();
    let sources = vec![source("doc-a", Some(1))];
    let content = "Use `v[1]` here, see [1](https://example.com), $a[1]$.\n```\nv[1]\n```\n";
    let reply = ReplyText::legacy(&names, content, &sources);
    assert_eq!(reply.text, content);
    assert!(reply.references.is_empty());
}

#[test]
fn an_old_number_two_sources_share_names_nothing() {
    let names = names_with_one_source();
    let sources = vec![source("doc-a", Some(1)), source("doc-b", Some(1))];
    let reply = ReplyText::legacy(&names, "Both [1].", &sources);
    assert_eq!(reply.text, "Both [1].");
    assert!(reply.references.is_empty());
}

#[test]
fn a_new_reply_keeps_resolved_markers_and_leaves_the_rest_as_text() {
    let names = TurnNames::unstored();
    named(&names, &["doc-a", "doc-b"]);
    let reply = ReplyText::read(
        &names,
        markers(false),
        "`[s1]` code, then [s2] and [s9] and [3].",
    );
    assert_eq!(reply.text, "`[s1]` code, then [s2] and [s9] and [3].");
    assert_eq!(reply.cited(), vec![2]);
}

#[test]
fn a_reply_is_stored_as_text_and_places_and_read_back_the_same() {
    let names = TurnNames::unstored();
    named(&names, &["doc-a", "doc-b", "doc-c"]);
    let reply = ReplyText::read(&names, markers(false), "Rule [s1], and é [s3]; also [s2].");
    let metadata = json!({ "references": reply.metadata() });
    assert_eq!(
        ReplyText::stored(&reply.text, &metadata),
        Some(reply.clone())
    );
    assert_eq!(reply.cited(), vec![1, 3, 2]);
    assert!(ReplyText::stored(&reply.text, &json!({ "sources": [] })).is_none());
}

#[test]
fn a_stored_place_that_does_not_fit_its_text_is_dropped() {
    let metadata = json!({ "references": [
        { "start": 0, "end": 99, "prefix": "s", "number": 1, "lines": null, "reference": null },
        { "start": 2, "end": 6, "prefix": "z", "number": 1, "lines": null, "reference": null },
    ]});
    let reply = ReplyText::stored("a [s1] b", &metadata).unwrap();
    assert!(reply.references.is_empty());
}

#[test]
fn what_a_range_cites_and_says_leaves_out_the_markers() {
    let names = TurnNames::unstored();
    named(&names, &["doc-a", "doc-b"]);
    let text = "First claim [s1][s2]. Second claim [s2].";
    let reply = ReplyText::read(&names, markers(false), text);
    let split = text.find(". Second").unwrap() + 1;
    assert_eq!(reply.cited_in(0..split), vec![1, 2]);
    assert_eq!(reply.cited_in(split..text.len()), vec![2]);
    assert_eq!(reply.without_references_in(0..split), "First claim.");
    assert_eq!(reply.without_references(), "First claim. Second claim.");
}

#[test]
fn the_frontend_gets_text_and_references_in_order() {
    let names = TurnNames::unstored();
    named(&names, &["doc-a"]);
    let reply = ReplyText::read(&names, markers(false), "See [s1] now");
    let pieces = reply.pieces();
    assert_eq!(pieces.len(), 3);
    assert_eq!(
        pieces[0],
        ReplyPieceDto::Text {
            text: "See ".to_owned()
        }
    );
    match &pieces[1] {
        ReplyPieceDto::Reference { reference } => {
            assert_eq!(reference.written, "[s1]");
            assert_eq!(reference.kind, "source");
            assert_eq!(reference.number, 1);
            assert!(reference.lines.is_none() && reference.name.is_none());
        }
        other => panic!("not a reference: {other:?}"),
    }
}
