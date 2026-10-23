use super::*;
use lattice_core::application::contracts::sources::SourceDto;
use std::collections::HashMap;

fn chunk(id: &str, document_id: &str, title: &str, content: &str) -> SearchResultDto {
    SearchResultDto {
        id: id.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        score: 0.9,
        path: Some(title.to_string()),
        document_id: Some(document_id.to_string()),
        position: None,
        vector_score: None,
        bm25_score: None,
        vector_rank: None,
        bm25_rank: None,
        metadata: HashMap::new(),
    }
}

fn source(chunk_id: &str, document_id: &str, file_name: &str, citation_id: u32) -> SourceDto {
    SourceDto {
        page_number: None,
        from_ocr: false,
        document_id: document_id.to_string(),
        chunk_id: chunk_id.to_string(),
        content: String::new(),
        score: 0.9,
        path: Some(file_name.to_string()),
        position: None,
        file_name: file_name.to_string(),
        file_path: file_name.to_string(),
        mime_type: "text/plain".to_string(),
        category: "Text".to_string(),
        file_size_bytes: 0,
        modified_at: String::new(),
        excerpt: None,
        highlights: None,
        section: None,
        chunk_index: None,
        chunk_excerpts: None,
        citation_id: Some(citation_id),

        web_snapshot: None,
    }
}

/// The exact scenario from the audit: one document contributes three
/// chunks and a second contributes one. The prompt used to number the
/// four *chunks* [1][2][3][4] while the UI resolved [n] against a
/// two-entry, per-document list — so [3] and [4] pointed at nothing
/// sensible and [2] opened the wrong file.
#[test]
fn separate_passages_from_one_document_get_distinct_citations() {
    let results = [
        chunk("c1", "doc-a", "alpha.md", "first chunk of A"),
        chunk("c2", "doc-a", "alpha.md", "second chunk of A"),
        chunk("c3", "doc-a", "alpha.md", "third chunk of A"),
        chunk("c4", "doc-b", "beta.md", "only chunk of B"),
    ];
    let refs: Vec<&SearchResultDto> = results.iter().collect();

    let sources = crate::features::conversation::chat::retrieval::deduplicate_sources(vec![
        source("c1", "doc-a", "alpha.md", 1),
        source("c2", "doc-a", "alpha.md", 2),
        source("c3", "doc-a", "alpha.md", 3),
        source("c4", "doc-b", "beta.md", 4),
        source("c1", "doc-a", "alpha.md", 1),
    ]);
    assert_eq!(sources.len(), 4);
    let ids = crate::features::conversation::chat::retrieval::citation_ids_by_chunk(&sources);
    let context = build_kb_context(&refs, &ids).expect("context");
    for n in 1..=3 {
        assert!(context.contains(&format!("[{n}] Document: alpha.md")));
    }
    assert!(context.contains("[4] Document: beta.md"));
}

/// Budget trimming drops chunks from the prompt. The surviving chunks
/// must keep their passage's number rather than being renumbered from 1.
#[test]
fn budget_trimming_does_not_renumber_surviving_chunks() {
    // doc-a was dropped by the token budget; only doc-b's chunk survives.
    let results = [chunk("c4", "doc-b", "beta.md", "only chunk of B")];
    let refs: Vec<&SearchResultDto> = results.iter().collect();

    let sources = vec![
        source("c1", "doc-a", "alpha.md", 1),
        source("c4", "doc-b", "beta.md", 4),
    ];
    let ids = crate::features::conversation::chat::retrieval::citation_ids_by_chunk(&sources);

    let context = build_kb_context(&refs, &ids).expect("context");

    assert!(
        context.contains("[4] Document: beta.md"),
        "surviving chunk must keep its passage's number, got:\n{}",
        context
    );
    assert!(
        !context.contains("[1]"),
        "a trimmed prompt must not renumber from 1:\n{}",
        context
    );
}

#[test]
fn tool_passages_append_without_renumbering_existing_citations() {
    let mut added = source("new", "doc-a", "alpha.md", 99);
    added.citation_id = None;
    let mut sources = vec![source("original", "doc-a", "alpha.md", 1), added];
    crate::features::conversation::chat::retrieval::assign_citation_ids(&mut sources);
    assert_eq!(sources[0].citation_id, Some(1));
    assert_eq!(sources[1].citation_id, Some(2));
}

/// A passage OCR read off a scan says so in its header, so the model can
/// take an odd character for a recognition error rather than a fact. A
/// passage from a text layer carries no such line.
#[test]
fn only_a_passage_read_by_ocr_is_labelled_as_ocr_text() {
    let mut scanned = chunk("c1", "doc-a", "scan.pdf", "Tbe quick brown fox");
    scanned
        .metadata
        .insert("ocr".into(), serde_json::json!(true));
    let typed = chunk("c2", "doc-b", "typed.pdf", "The quick brown fox");

    let scanned = kb_passage(&scanned, 1);
    let typed = kb_passage(&typed, 2);

    let label = scanned
        .find("Text: OCR of a scanned page")
        .expect("an OCR passage is labelled");
    assert!(label < scanned.find("\nContent:").unwrap(), "{scanned}");
    assert!(!typed.contains("OCR"), "{typed}");
}

#[test]
fn missing_passage_never_gets_an_invented_citation() {
    let results = [chunk("c1", "doc-unknown", "orphan.md", "content")];
    let refs: Vec<&SearchResultDto> = results.iter().collect();
    let ids: HashMap<String, u32> = HashMap::new();

    let context = build_kb_context(&refs, &ids).expect("context");
    assert!(!context.contains("[1]"));
}

#[test]
fn a_passage_carries_only_its_own_source_lines() {
    let mut cell = chunk(
        "zelle-0",
        "zelle",
        "Leitfaden Zellbiologie.md",
        "Die Mitose teilt den Zellkern in zwei identische Tochterkerne.",
    );
    cell.metadata.insert(
        "section".into(),
        serde_json::json!("Leitfaden > Zellteilung"),
    );
    let harmony = chunk(
        "harmony-0",
        "harmony",
        "harmony notes.md",
        "A dominant seventh chord resolves to the tonic.",
    );
    let refs = [&cell, &harmony];
    let ids = HashMap::from([("zelle-0".to_string(), 3), ("harmony-0".to_string(), 7)]);

    let context = build_kb_context(&refs, &ids).expect("context");

    assert!(context.contains("[3] Document: Leitfaden Zellbiologie.md"));
    assert!(context.contains("Source heading: Leitfaden > Zellteilung"));
    assert!(context.contains("[7] Document: harmony notes.md"));
    assert!(
        !context.contains("Evidence relationship"),
        "no passage may claim a link the sources do not state:\n{context}"
    );
}

#[test]
fn the_synthesis_note_sits_once_beside_the_passages_and_names_no_document_type() {
    let flags = SearchFlags {
        force_kb_search: false,
        force_web_search: false,
        web_fallback: false,
        force_wiki_search: false,
        deep_research_mode: false,
        force_followup_mode: false,
        closed_book: false,
    };
    let settings = LLMPromptSettingsDto::default();
    let prompt = |kb: Option<&str>| {
        PromptMessageBuilder::new(&settings, "¿Qué fase sigue a la profase?", false, flags)
            .with_kb_context(kb.map(str::to_string))
            .build()
    };

    let with_passages = prompt(Some("[1] La metafase sigue a la profase."));
    assert_eq!(with_passages.matches(EVIDENCE_SYNTHESIS_NOTE).count(), 1);
    assert!(!prompt(None).contains(EVIDENCE_SYNTHESIS_NOTE));
    assert!(!EVIDENCE_SYNTHESIS_NOTE.contains("alternatives"));
    assert!(!EVIDENCE_SYNTHESIS_NOTE.contains("template"));
}
