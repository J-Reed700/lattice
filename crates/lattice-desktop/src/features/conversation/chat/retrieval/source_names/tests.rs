use super::*;

fn source(document_id: &str, chunk_id: &str) -> SourceDto {
    SourceDto {
        page_number: None,
        from_ocr: false,
        document_id: document_id.to_string(),
        chunk_id: chunk_id.to_string(),
        content: "A passage.".to_string(),
        score: 1.0,
        path: None,
        position: None,
        file_name: "notes.md".to_string(),
        file_path: "notes.md".to_string(),
        mime_type: "text/markdown".to_string(),
        category: "Markdown".to_string(),
        file_size_bytes: 0,
        modified_at: String::new(),
        excerpt: None,
        highlights: None,
        section: None,
        chunk_index: None,
        chunk_excerpts: None,
        citation_id: None,
        web_snapshot: None,
    }
}

fn numbers(sources: &[SourceDto]) -> Vec<Option<u32>> {
    sources.iter().map(|source| source.citation_id).collect()
}

#[test]
fn a_source_gets_the_number_of_its_handle() {
    let names = TurnNames::unstored();
    let mut sources = vec![source("doc-a", "c1"), source("doc-a", "c2")];
    name_sources(&names, &mut sources);
    assert_eq!(numbers(&sources), [Some(1), Some(2)]);
    assert_eq!(label(&sources[1]), "[s2]");
}

#[test]
fn a_source_keeps_its_number_across_two_turns() {
    let names = TurnNames::unstored();
    let mut first = vec![source("doc-a", "c1"), source("doc-b", "c1")];
    name_sources(&names, &mut first);

    let mut second = vec![source("doc-b", "c1"), source("doc-a", "c1")];
    name_sources(&names, &mut second);

    assert_eq!(second[0].citation_id, first[1].citation_id);
    assert_eq!(second[1].citation_id, first[0].citation_id);
}

#[test]
fn two_turns_sources_never_share_a_number() {
    let names = TurnNames::unstored();
    let mut first = vec![source("doc-a", "c1"), source("doc-a", "c2")];
    name_sources(&names, &mut first);
    let mut second = vec![source("doc-b", "c1"), source("doc-b", "c2")];
    name_sources(&names, &mut second);

    let mut all: Vec<_> = first.iter().chain(&second).map(|s| s.citation_id).collect();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 4);
}

#[test]
fn a_recalled_source_is_shown_under_the_number_it_had() {
    let names = TurnNames::unstored();
    let mut original = vec![source("doc-a", "c1")];
    name_sources(&names, &mut original);

    let mut recalled = vec![source("doc-a", "recalled:doc-a:c1")];
    name_sources(&names, &mut recalled);

    assert_eq!(recalled[0].citation_id, original[0].citation_id);
}

#[test]
fn a_web_page_is_one_source_however_many_searches_returned_it() {
    let names = TurnNames::unstored();
    let mut sources = vec![
        source("web:https://a.test/one", "web-result-1"),
        source("web:https://a.test/one", "web-result-3"),
        source("web:https://a.test/two", "web-result-1"),
    ];
    name_sources(&names, &mut sources);
    assert_eq!(sources[0].citation_id, sources[1].citation_id);
    assert_ne!(sources[0].citation_id, sources[2].citation_id);
}

#[test]
fn a_source_with_no_number_has_no_label() {
    assert_eq!(label(&source("doc-a", "c1")), "");
}

#[test]
fn naming_a_source_replaces_a_number_it_carried() {
    let names = TurnNames::unstored();
    let mut sources = vec![source("doc-a", "c1")];
    sources[0].citation_id = Some(40);
    name_sources(&names, &mut sources);
    assert_eq!(sources[0].citation_id, Some(1));
}
