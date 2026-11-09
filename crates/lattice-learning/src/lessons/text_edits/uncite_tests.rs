use super::*;

fn first_quote() -> Value {
    json!({"sentence":"Run ls in the projects folder.","sourceIndex":0,"quote":"The ls command lists the files of a folder."})
}

fn second_quote() -> Value {
    json!({"sentence":"Open notes.txt with cat.","sourceIndex":1,"quote":"The cat command prints a file to the screen."})
}

fn notes() -> Value {
    json!({"blocks":[
        {"kind":"explanation","body":"Notes live in the projects folder.","citations":[
            {"sentence":"Notes live in the projects folder.","sourceIndex":0,"quote":"Projects keep their notes in one folder."}]},
        {"kind":"explanation","body":"Nothing is cited here."}],
        "questions":[{"kind":"quiz","prompt":"Which?","options":["a","b","c","d"],
            "explanation":"Run ls in the projects folder. Open notes.txt with cat.",
            "citations":[first_quote(), second_quote()]}]})
}

fn applied(patch: &Value) -> Value {
    let outcome = apply(&notes(), &patch.to_string());
    assert!(outcome.is_ok(), "{outcome:?}");
    outcome.unwrap_or_default()
}

#[test]
fn a_citation_named_in_uncite_is_removed_and_everything_else_is_untouched() {
    let revised =
        applied(&json!({"edits":[],"citations":[],"uncite":["/questions/0/citations/0"]}));

    let mut expected = notes();
    expected["questions"][0]["citations"] = json!([second_quote()]);
    assert_eq!(revised, expected);
}

#[test]
fn citations_named_in_uncite_are_removed_together_whatever_their_order() {
    let revised = applied(&json!({"edits":[],"citations":[],"uncite":[
        "/questions/0/citations/0", "/blocks/0/citations/0", "/questions/0/citations/1", "/questions/0/citations/0"]}));

    let mut expected = notes();
    expected["questions"][0]["citations"] = json!([]);
    expected["blocks"][0]["citations"] = json!([]);
    assert_eq!(revised, expected);
}

#[test]
fn uncite_an_edit_and_an_appended_citation_apply_together_against_the_original() {
    let patch = json!({
        "edits":[
            {"path":"/questions/0/explanation","before":"Open notes.txt with cat.","after":"Open notes.txt with a pager."},
            {"path":"/questions/0/citations/1/quote","before":"The cat command prints a file to the screen.","after":"The cat command prints a file."}],
        "citations":[{"path":"/questions/0","sentence":"Open notes.txt with a pager.","sourceIndex":2,"quote":"A pager shows a file one screen at a time."}],
        "uncite":["/questions/0/citations/0"]});

    let revised = applied(&patch);

    let mut expected = notes();
    expected["questions"][0]["explanation"] =
        json!("Run ls in the projects folder. Open notes.txt with a pager.");
    expected["questions"][0]["citations"] = json!([
        {"sentence":"Open notes.txt with cat.","sourceIndex":1,"quote":"The cat command prints a file."},
        {"sentence":"Open notes.txt with a pager.","sourceIndex":2,"quote":"A pager shows a file one screen at a time."}]);
    assert_eq!(revised, expected);
}

#[test]
fn uncite_naming_no_citation_of_the_draft_is_refused_and_says_so() {
    for path in [
        "/questions/0/citations/2",
        "/blocks/1/citations/0",
        "/questions/0/explanation",
        "/blocks/0/citations",
        "/lesson/citations/0",
    ] {
        let outcome = apply(
            &notes(),
            &json!({"edits":[],"citations":[],"uncite":[path]}).to_string(),
        );
        let message = outcome.map_or_else(|error| error.to_string(), |_| String::new());
        assert!(
            message.contains("uncite 0") && message.contains(path),
            "{path}: {message}"
        );
    }
}

#[test]
fn the_schema_offers_uncite_with_exactly_the_citations_the_draft_has() {
    let lesson = crate::generation::lesson_schema(2);

    let offered = schema_for(&notes(), &lesson);

    let item = &offered["properties"]["uncite"]["items"];
    assert_eq!(
        item["enum"],
        json!([
            "/blocks/0/citations/0",
            "/questions/0/citations/0",
            "/questions/0/citations/1"
        ])
    );
    assert_eq!(
        offered["properties"]["citations"]["items"]["required"]
            .as_array()
            .map(Vec::len),
        Some(4)
    );
}

#[test]
fn a_draft_without_citations_offers_no_uncite() {
    let mut bare = notes();
    bare["blocks"][0]["citations"] = json!([]);
    bare["questions"][0]["citations"] = json!([]);
    let lesson = crate::generation::lesson_schema(2);

    let offered = schema_for(&bare, &lesson);

    assert!(offered["properties"]["citations"].is_object());
    assert!(offered["properties"].get("uncite").is_none());
    assert!(!offered.to_string().contains("\"enum\":[]"));
    let empty = apply(
        &bare,
        &json!({"edits":[],"citations":[],"uncite":[]}).to_string(),
    );
    assert!(empty.is_err(), "nothing named, nothing changed");
}

#[test]
fn the_citing_instructions_tell_a_repair_to_take_back_a_citation() {
    let lesson = crate::generation::lesson_schema(2);

    assert!(instructions(&lesson).contains("uncite"));
}
