//! The findings a patch names, and the patch of only some of them.
use super::*;
use serde_json::json;

fn cite(finding: Option<&str>) -> Value {
    let mut cite =
        json!({"path":"/blocks/0","sentence":"A sentence.","sourceIndex":0,"quote":"A quote."});
    if let Some(finding) = finding {
        cite["finding"] = json!(finding);
    }
    cite
}

fn edit(finding: Option<&str>) -> Value {
    let mut edit = json!({"path":"/blocks/0/body","before":"a","after":"b"});
    if let Some(finding) = finding {
        edit["finding"] = json!(finding);
    }
    edit
}

fn patch() -> Patch {
    read_patch(
        &json!({
            "edits": [edit(Some("b")), edit(None), edit(Some("a")), edit(Some("b"))],
            "citations": [cite(Some("c")), cite(Some("a")), cite(None)],
            "uncite": [
                "/blocks/0/citations/0",
                {"finding":"d","path":"/blocks/0/citations/1"},
                {"finding":"b","path":"/blocks/0/citations/2"}
            ],
        })
        .to_string(),
    )
    .unwrap()
}

#[test]
fn the_findings_a_patch_names_come_once_each_in_the_order_first_named() {
    assert_eq!(patch().findings(), ["b", "a", "c", "d"]);
}

#[test]
fn a_patch_with_no_named_change_names_no_finding() {
    let unnamed =
        read_patch(&json!({"edits":[edit(None)],"uncite":["/blocks/0/citations/0"]}).to_string())
            .unwrap();
    assert!(unnamed.findings().is_empty());
}

#[test]
fn the_patch_of_one_finding_keeps_its_edits_citations_and_named_uncites() {
    let only = patch().only(&["b".to_owned()]);
    assert_eq!(only.edits.len(), 2);
    assert!(only
        .edits
        .iter()
        .all(|edit| edit.finding.as_deref() == Some("b")));
    assert!(only.citations.is_empty());
    assert_eq!(only.uncite.len(), 1);
    assert_eq!(only.uncite[0].finding(), Some("b"));

    let only = patch().only(&["a".to_owned(), "c".to_owned()]);
    assert_eq!(only.edits.len(), 1);
    assert_eq!(only.citations.len(), 2);
    assert!(only.uncite.is_empty());
}

#[test]
fn the_patch_of_some_findings_drops_unnamed_edits_and_citations_and_path_only_uncites() {
    let all: Vec<String> = patch().findings();
    let only = patch().only(&all);
    assert_eq!(only.edits.len(), 3);
    assert_eq!(only.citations.len(), 2);
    assert_eq!(only.uncite.len(), 2);
    assert!(only.uncite.iter().all(|uncite| uncite.finding().is_some()));
}
