use super::*;
use lattice_core::application::services::tool_handles::ToolHandles;
use serde_json::json;

fn rendered(tool: &str, arguments: Value, data: Value, handles: &ToolHandles) -> String {
    render(tool, &data, &CallView::new(tool, &arguments, handles)).unwrap()
}

fn search_data() -> Value {
    json!({
        "conversation_scope": "current",
        "matches": [
            {"message_id": "msg-aa", "sequence": 4, "role": "user",
             "excerpt": "The kiln opens at dawn", "excerpt_is_partial": false,
             "matched_by": "ranked"},
            {"message_id": "msg-bb", "sequence": 9, "role": "assistant",
             "excerpt": "Tide tables are posted at the harbour", "excerpt_is_partial": true,
             "matched_by": "exact_identifier"}
        ],
        "returned": 2, "offset": 0, "next_offset": 2,
        "continuation": {"read_with": "read_conversation_history", "message_ids": ["msg-aa", "msg-bb"]},
        "retrieval": {"availability": "searched_with_matches", "lexical_ran": true,
                      "index_error": null, "note": "These are exact source excerpts."}
    })
}

#[test]
fn a_history_search_names_each_message_and_shows_no_id_or_sequence() {
    let handles = ToolHandles::new();
    let text = rendered(
        TOOL_SEARCH_CONVERSATION_HISTORY,
        json!({"query": "kiln"}),
        search_data(),
        &handles,
    );
    assert!(text.contains("[h1] user: The kiln opens at dawn"), "{text}");
    assert!(
        text.contains("[h2] assistant: Tide tables are posted at the harbour ..."),
        "{text}"
    );
    for hidden in ["msg-aa", "msg-bb", "sequence", "offset", "continuation"] {
        assert!(!text.contains(hidden), "{hidden}: {text}");
    }
    let read = handles
        .read_call("read_conversation_history", &json!({"message": "[h2]"}))
        .unwrap();
    assert_eq!(read.arguments["message_ids"], json!(["msg-bb"]));
}

#[test]
fn a_history_search_with_more_matches_resumes_at_the_next_one() {
    let handles = ToolHandles::new();
    let text = rendered(
        TOOL_SEARCH_CONVERSATION_HISTORY,
        json!({"query": "kiln"}),
        search_data(),
        &handles,
    );
    assert!(
        text.contains("To read on, call read_more with [m1]."),
        "{text}"
    );
    let resumed = handles
        .read_call("read_more", &json!({"handle": "[m1]"}))
        .unwrap();
    assert_eq!(resumed.tool, TOOL_SEARCH_CONVERSATION_HISTORY);
    assert_eq!(resumed.arguments, json!({"query": "kiln", "offset": 2}));
}

#[test]
fn a_cut_history_read_resumes_the_same_read() {
    let handles = ToolHandles::new();
    let data = json!({
        "conversation_scope": "current",
        "messages": [{"message_id": "msg-cc", "sequence": 3, "role": "user",
                      "text": "Ropes and tar", "start_byte": 0, "end_byte": 13,
                      "is_whole_message": false}],
        "truncated": true,
        "continuation": {"from_sequence": 3, "start_byte": 13},
        "not_readable_message_ids": []
    });
    let first = rendered(
        TOOL_READ_CONVERSATION_HISTORY,
        json!({"message_ids": ["msg-cc"]}),
        data,
        &handles,
    );
    assert!(
        first.contains("[h1] user (part of the message):\nRopes and tar"),
        "{first}"
    );
    assert!(first.contains("call read_more with [m2]"), "{first}");
    assert!(
        !first.contains("start_byte") && !first.contains("msg-cc"),
        "{first}"
    );
    let resumed = handles
        .read_call("read_more", &json!({"handle": "m2"}))
        .unwrap();
    assert_eq!(resumed.tool, TOOL_READ_CONVERSATION_HISTORY);
}

#[test]
fn saved_knowledge_shows_the_item_and_its_evidence_without_any_id() {
    let handles = ToolHandles::new().in_conversation("conv-1");
    let item = |conversation: &str| {
        json!({
            "id": "item-9", "conversationId": conversation,
            "conversationTitle": "Harbour plans", "label": "Kiln opens at dawn",
            "kind": "established_fact", "state": "active", "scope": "space",
            "learnedAt": "2026-03-01", "validFrom": null, "validUntil": null,
            "verifiedAt": null, "forgotten": false, "supersededBy": "item-10",
            "evidence": [{"messageId": "msg-dd", "sequence": 2, "role": "user",
                          "purpose": "assertion", "startByte": 0, "endByte": 20,
                          "text": "The kiln opens at dawn"}],
            "availabilityReason": "Shared by this space"
        })
    };
    let own = rendered(
        TOOL_SEARCH_SAVED_KNOWLEDGE,
        json!({"query": "kiln"}),
        json!({"items": [item("conv-1")], "truncated": false, "note": "Empty results do not prove absence."}),
        &handles,
    );
    assert!(own.contains("Kiln opens at dawn (established_fact; active; space scope; learned 2026-03-01; replaced by a later item)"), "{own}");
    assert!(
        own.contains("[h1] user (assertion): The kiln opens at dawn"),
        "{own}"
    );
    for hidden in ["item-9", "item-10", "msg-dd", "conv-1", "Byte"] {
        assert!(!own.contains(hidden), "{hidden}: {own}");
    }
    let other = rendered(
        TOOL_SEARCH_SAVED_KNOWLEDGE,
        json!({"query": "kiln"}),
        json!({"items": [item("conv-2")]}),
        &handles,
    );
    assert!(
        other.contains("  user (assertion): The kiln opens at dawn"),
        "{other}"
    );
    assert!(!other.contains("[h"), "{other}");
}
