use super::*;
use lattice_core::application::services::tool_handles::ToolHandles;
use serde_json::json;

fn rendered(tool: &str, data: Value) -> (String, ToolHandles) {
    let handles = ToolHandles::new();
    let arguments = json!({});
    let text = render(tool, &data, &CallView::new(tool, &arguments, &handles)).unwrap();
    (text, handles)
}

fn listed(id: &str, name: &str) -> Value {
    json!({
        "document_id": id, "filename": name, "file_path": format!("/harbour/{name}"),
        "mime_type": "text/plain", "extension": "txt", "size_bytes": 120,
        "modified_at": "2026-03-04T10:00:00Z", "tags": ["tides"], "is_favorite": true,
        "access_count": 3
    })
}

#[test]
fn a_listing_names_each_document_and_shows_no_id_or_path() {
    let (text, handles) = rendered(
        "list_documents",
        json!({
            "documents": [listed("doc-a", "tides.txt"), listed("doc-b", "kiln.txt")],
            "total": 2, "limit": 50, "offset": 0, "has_more": false
        }),
    );
    assert!(
        text.contains(
            "[d1] tides.txt (text/plain; 120 bytes; modified 2026-03-04; tags: tides; a favorite)"
        ),
        "{text}"
    );
    assert!(text.contains("[d2] kiln.txt"), "{text}");
    assert!(!text.contains("doc-a"), "{text}");
    assert!(!text.contains("/harbour"), "{text}");
    let called = handles
        .read_call("get_document", &json!({"document": "[d2]"}))
        .unwrap();
    assert_eq!(called.arguments["document_id"], "doc-b");
}

#[test]
fn a_listing_with_more_names_the_point_to_read_on_from() {
    let handles = ToolHandles::new();
    let arguments = json!({"filter_mode": "recent"});
    let data = json!({
        "documents": [listed("doc-a", "tides.txt")],
        "total": 90, "limit": 1, "offset": 10, "has_more": true
    });
    let text = render(
        "list_documents",
        &data,
        &CallView::new("list_documents", &arguments, &handles),
    )
    .unwrap();
    assert!(
        text.contains("To read on, call read_more with [m1]."),
        "{text}"
    );
    assert!(!text.contains("offset"), "{text}");
    let resumed = handles
        .read_call("read_more", &json!({"handle": "m1"}))
        .unwrap();
    assert_eq!(resumed.tool, "list_documents");
    assert_eq!(
        resumed.arguments,
        json!({"filter_mode": "recent", "offset": 11})
    );
}

#[test]
fn an_attachment_is_named_for_get_document_and_shows_no_id() {
    let (text, _) = rendered(
        "list_attachments",
        json!({
            "attachments": [{
                "document_id": "doc-att", "filename": "survey.pdf", "extension": "pdf",
                "size_bytes": 900, "word_count": 150, "indexed_at": "2026-03-05T08:00:00Z"
            }],
            "total": 1
        }),
    );
    assert!(
        text.contains("[d1] survey.pdf (900 bytes, 150 words, attached 2026-03-05)"),
        "{text}"
    );
    assert!(!text.contains("doc-att"), "{text}");
}
