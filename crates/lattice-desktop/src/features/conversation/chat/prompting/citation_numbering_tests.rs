use super::*;
use crate::features::conversation::chat::retrieval::source_names::name_sources;
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::services::model_contract::kept::TurnNames;
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

/// One document contributes three chunks and a second one: the prompt's
/// numbers must match the UI's per-source list, so every [n] opens its source.
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
        assert!(context.contains(&format!("[s{n}] Document: alpha.md")));
    }
    assert!(context.contains("[s4] Document: beta.md"));
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
        context.contains("[s4] Document: beta.md"),
        "surviving chunk must keep its passage's number, got:\n{}",
        context
    );
    assert!(
        !context.contains("[s1]"),
        "a trimmed prompt must not renumber from 1:\n{}",
        context
    );
}

#[test]
fn tool_passages_append_without_renumbering_existing_citations() {
    let names = TurnNames::unstored();
    let mut original = vec![source("original", "doc-a", "alpha.md", 0)];
    original[0].citation_id = None;
    name_sources(&names, &mut original);
    let mut added = source("new", "doc-a", "alpha.md", 0);
    added.citation_id = None;
    let mut sources = vec![original[0].clone(), added];
    name_sources(&names, &mut sources);
    assert_eq!(sources[0].citation_id, original[0].citation_id);
    assert_ne!(sources[1].citation_id, sources[0].citation_id);
    assert!(sources[1].citation_id.is_some());
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

/// Every passage shows its document under one name: a passage read in order
/// carries the file's full path, a search hit only its name, and the prompt
/// showed one document under both.
#[test]
fn every_passage_names_its_document_the_same_way() {
    let mut read_in_order = chunk("c1", "doc-a", "Saltings log.md", "Swept the chimney.");
    read_in_order.path = Some("/var/folders/xy/T/Saltings log.md".into());
    let hit = chunk("c2", "doc-a", "Saltings log.md", "Relined the stove.");
    assert!(kb_passage(&read_in_order, 1).starts_with("[s1] Document: Saltings log.md\n"));
    assert!(kb_passage(&hit, 2).starts_with("[s2] Document: Saltings log.md\n"));
    let mut untitled = chunk("c3", "doc-b", "", "Text.");
    untitled.path = Some("/notes/plain.txt".into());
    assert!(kb_passage(&untitled, 3).starts_with("[s3] Document: plain.txt\n"));
}

/// A message with passages puts the context before the question and the
/// answering rules after it, and the budget charges that frame with the rest.
#[test]
fn a_message_with_passages_places_context_first_and_rules_after_the_question_and_the_frame_prices_them(
) {
    let settings = crate::features::conversation::chat::prompt_settings::normalize_prompt_settings(
        LLMPromptSettingsDto::default(),
    );
    let prompt = PromptMessageBuilder::new(
        &settings,
        "When was the chimney swept?",
        false,
        super::super::SearchFlags::from_preferences(None),
    )
    .with_kb_context(Some("[1] Document: log.md\nContent: Swept.".into()))
    .build();

    let context_pos = prompt.find("Context:\n").expect("context heading");
    let question_pos = prompt
        .find("Question: When was the chimney swept?")
        .expect("question line");
    let check_pos = prompt
        .find("Check every supplied passage")
        .expect("check-every-passage rule");
    let lang_pos = prompt
        .find("Write in the language of the question")
        .expect("language rule");
    assert!(context_pos < question_pos, "{prompt}");
    assert!(question_pos < check_pos, "{prompt}");
    assert!(check_pos < lang_pos, "{prompt}");
    assert!(prompt.ends_with("\n\nAnswer:"), "{prompt}");
    assert!(
        !prompt.contains("Citation rule:"),
        "the rule is said once, in the system prompt"
    );
    assert!(settings.system_prompt.contains("Citation rule:"));

    let hybrid_frame = evidence_frame(&settings, "When?", None, false);
    let hybrid_q = hybrid_frame
        .find("Question: When?")
        .expect("hybrid question");
    let hybrid_rule = hybrid_frame
        .find("from general knowledge without adding citations")
        .expect("hybrid rule");
    assert!(hybrid_q < hybrid_rule, "{hybrid_frame}");
    assert!(hybrid_frame.ends_with("Answer:"), "{hybrid_frame}");

    let strict_frame = evidence_frame(&settings, "When?", None, true);
    let strict_q = strict_frame
        .find("Question: When?")
        .expect("strict question");
    let strict_rule = strict_frame
        .find("using only the provided context")
        .expect("strict rule");
    assert!(strict_q < strict_rule, "{strict_frame}");
    assert!(strict_frame.ends_with("Answer:"), "{strict_frame}");
}

#[test]
fn missing_passage_never_gets_an_invented_citation() {
    let results = [chunk("c1", "doc-unknown", "orphan.md", "content")];
    let refs: Vec<&SearchResultDto> = results.iter().collect();
    let ids: HashMap<String, u32> = HashMap::new();

    let context = build_kb_context(&refs, &ids).expect("context");
    assert!(!context.contains("[s1]"));
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

    assert!(context.contains("[s3] Document: Leitfaden Zellbiologie.md"));
    assert!(context.contains("Source heading: Leitfaden > Zellteilung"));
    assert!(context.contains("[s7] Document: harmony notes.md"));
    assert!(
        !context.contains("Evidence relationship"),
        "no passage may claim a link the sources do not state:\n{context}"
    );
}

#[test]
fn the_synthesis_note_sits_once_beside_the_passages_and_names_no_document_type() {
    let flags = SearchFlags::default();
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
