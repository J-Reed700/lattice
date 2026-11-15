use super::*;
use crate::features::conversation::chat::retrieval::source_names::{label, name_sources};
use lattice_core::application::contracts::sources::SourceDto;
use serde_json::json;

fn source(document_id: &str) -> SourceDto {
    SourceDto {
        page_number: None,
        from_ocr: false,
        document_id: document_id.to_string(),
        chunk_id: format!("{document_id}-chunk"),
        content: "Tide tables for the harbour.".to_string(),
        score: 0.9,
        path: None,
        position: None,
        file_name: "tides.txt".to_string(),
        file_path: "/harbour/tides.txt".to_string(),
        mime_type: "text/plain".to_string(),
        category: "Text File".to_string(),
        file_size_bytes: 10,
        modified_at: "2024-01-15T12:00:00Z".to_string(),
        excerpt: None,
        highlights: None,
        section: None,
        chunk_index: None,
        chunk_excerpts: None,
        citation_id: None,
        web_snapshot: None,
    }
}

#[test]
fn a_document_a_result_showed_opens_by_its_name() {
    let handles = ToolHandles::new();
    let name = handles.document("doc-kiln").unwrap();
    let called = handles
        .read_call("get_document", &json!({"document": name}))
        .unwrap();
    assert_eq!(called.tool, "get_document");
    assert_eq!(called.arguments, json!({"document_id": "doc-kiln"}));
}

#[test]
fn a_name_is_written_with_or_without_brackets() {
    let handles = ToolHandles::new();
    let name = handles.document("doc-kiln").unwrap();
    let bare = name.trim_matches(|c| c == '[' || c == ']').to_string();
    let called = handles
        .read_call("get_document", &json!({"document": bare}))
        .unwrap();
    assert_eq!(called.arguments["document_id"], "doc-kiln");
}

#[test]
fn a_source_of_the_context_opens_by_its_name() {
    let names = Arc::new(TurnNames::unstored());
    let mut sources = [source("doc-a"), source("doc-b")];
    name_sources(&names, &mut sources);
    let handles = ToolHandles::in_turn(names);
    let called = handles
        .read_call("get_document", &json!({"document": label(&sources[1])}))
        .unwrap();
    assert_eq!(called.arguments["document_id"], "doc-b");
}

#[test]
fn a_source_is_opened_by_its_name_in_a_later_turn_of_the_conversation() {
    let names = Arc::new(TurnNames::unstored());
    let mut first = [source("doc-a")];
    name_sources(&names, &mut first);
    let mut later = [source("doc-b"), source("doc-a")];
    name_sources(&names, &mut later);
    assert_eq!(later[1].citation_id, first[0].citation_id);
    let handles = ToolHandles::in_turn(names);
    let called = handles
        .read_call("get_document", &json!({"document": label(&first[0])}))
        .unwrap();
    assert_eq!(called.arguments["document_id"], "doc-a");
}

#[test]
fn a_source_name_nothing_showed_is_an_error_the_model_can_act_on() {
    let names = Arc::new(TurnNames::unstored());
    let mut sources = [source("doc-a")];
    name_sources(&names, &mut sources);
    let handles = ToolHandles::in_turn(names);
    let error = handles
        .read_call("get_document", &json!({"document": "[s7]"}))
        .unwrap_err();
    assert!(error.0.contains("names nothing shown"), "{error:?}");
}

#[test]
fn a_bare_number_is_no_name() {
    let names = Arc::new(TurnNames::unstored());
    let mut sources = [source("doc-a")];
    name_sources(&names, &mut sources);
    let handles = ToolHandles::in_turn(names);
    let error = handles
        .read_call("get_document", &json!({"document": "[1]"}))
        .unwrap_err();
    assert!(error.0.contains("names nothing shown"), "{error:?}");
}

#[test]
fn an_unknown_name_is_an_error_and_not_a_document() {
    let handles = ToolHandles::new();
    let error = handles
        .read_call("get_document", &json!({"document": "d9"}))
        .unwrap_err();
    assert!(error.0.contains("names nothing shown"), "{error:?}");
}

#[test]
fn an_id_or_an_offset_the_model_writes_is_not_taken() {
    let handles = ToolHandles::new();
    let name = handles.document("doc-kiln").unwrap();
    let called = handles
        .read_call(
            "get_document",
            &json!({"document": name, "document_id": "other", "offset": 40}),
        )
        .unwrap();
    assert_eq!(called.arguments, json!({"document_id": "doc-kiln"}));
}

#[test]
fn a_page_and_an_offset_a_search_is_given_are_dropped() {
    let handles = ToolHandles::new();
    let called = handles
        .read_call(
            "web_search",
            &json!({"query": "tide tables", "page": 4, "offset": 30, "max_results": 5}),
        )
        .unwrap();
    assert_eq!(
        called.arguments,
        json!({"query": "tide tables", "max_results": 5})
    );
}

#[test]
fn read_more_runs_the_call_that_stopped_from_where_it_stopped() {
    let handles = ToolHandles::new();
    let call = json!({"document_id": "doc-kiln"});
    let view = CallView::new("get_document", &call, &handles);
    let name = view.read_on(&[("offset", json!(4000))]);
    let called = handles
        .read_call(READ_MORE, &json!({"handle": name}))
        .unwrap();
    assert_eq!(called.tool, "get_document");
    assert_eq!(
        called.arguments,
        json!({"document_id": "doc-kiln", "offset": 4000})
    );
}

#[test]
fn the_same_stopping_point_has_one_name() {
    let handles = ToolHandles::new();
    let call = json!({"query": "kiln"});
    let view = CallView::new("semantic_search", &call, &handles);
    assert_eq!(
        view.read_on(&[("offset", json!(10))]),
        view.read_on(&[("offset", json!(10))])
    );
    assert_ne!(
        view.read_on(&[("offset", json!(10))]),
        view.read_on(&[("offset", json!(20))])
    );
}

#[test]
fn read_more_with_a_name_nothing_showed_says_how_to_get_one() {
    let handles = ToolHandles::new();
    let error = handles
        .read_call(READ_MORE, &json!({"handle": "m3"}))
        .unwrap_err();
    assert!(error.0.contains("names nothing shown"), "{error:?}");
}

#[test]
fn a_history_message_is_read_by_its_name_and_never_by_sequence_or_byte() {
    let handles = ToolHandles::new();
    let name = handles.message("msg-1").unwrap();
    let called = handles
        .read_call(
            READ_CONVERSATION_HISTORY,
            &json!({"message": name, "from_sequence": 3, "start_byte": 90, "max_bytes": 2000}),
        )
        .unwrap();
    assert_eq!(
        called.arguments,
        json!({"message_ids": ["msg-1"], "max_bytes": 2000})
    );
}

#[test]
fn the_handle_parameters_are_plain_strings_in_a_static_schema() {
    assert_eq!(document_property()["type"], "string");
    assert!(document_property().get("enum").is_none());
    assert_eq!(message_property()["type"], "string");
    let definition = read_more_definition().unwrap();
    assert_eq!(definition.name, READ_MORE);
    assert_eq!(definition.input_schema["required"], json!(["handle"]));
}
