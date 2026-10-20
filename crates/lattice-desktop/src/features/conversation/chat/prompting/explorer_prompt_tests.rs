use super::*;
use crate::features::conversation::chat::prompt_settings::normalize_prompt_settings;

const QUESTION: &str = "How does the fusion step rank results?";
const FOLDER: &str =
    "Folder open beside this chat: lattice. The user is looking at it in Explorer.";
const PASSAGE: &str =
    "[1] Document: retrieval-notes.md\nContent: Reciprocal rank fusion sums 1/(k + rank).";

fn settings() -> LLMPromptSettingsDto {
    normalize_prompt_settings(LLMPromptSettingsDto::default())
}

fn explorer_turn(settings: &LLMPromptSettingsDto) -> PromptMessageBuilder<'_> {
    PromptMessageBuilder::new(
        settings,
        QUESTION,
        false,
        SearchFlags::from_preferences(None),
    )
    .in_explorer(true)
}

fn explorer_opening() -> &'static str {
    EXPLORER_TEMPLATE
        .split("{context}")
        .next()
        .expect("the template has a context slot")
}

/// The document-answer template asks for a citation on every statement; an
/// answer about the folder explains from general knowledge too.
#[test]
fn an_explorer_turn_is_not_asked_to_cite_every_statement() {
    let settings = settings();
    let prompt = explorer_turn(&settings)
        .with_explorer_context(Some(FOLDER.into()))
        .build();

    assert!(prompt.starts_with(explorer_opening()), "{prompt}");
    assert!(!prompt.contains("Cite every factual statement"), "{prompt}");
    assert!(
        !prompt.contains("using only the provided context"),
        "{prompt}"
    );
    assert!(prompt.contains(FOLDER));
    assert!(prompt.contains(&format!("Question: {QUESTION}")));
}

/// Library passages the user turned on still carry their numbers, and the
/// template says those numbers are the only ones to cite.
#[test]
fn library_passages_in_an_explorer_turn_keep_their_numbers() {
    let settings = settings();
    let prompt = explorer_turn(&settings)
        .with_explorer_context(Some(FOLDER.into()))
        .with_kb_context(Some(PASSAGE.into()))
        .build();

    assert!(prompt.starts_with(explorer_opening()), "{prompt}");
    assert!(prompt.contains(&format!("Knowledge Base Results:\n{PASSAGE}")));
    assert!(prompt
        .contains("Bracket numbers like [1] belong only to the numbered library or web passages"));
}

/// The folder block can be the evidence cut for room; the turn is still about
/// the folder.
#[test]
fn an_explorer_turn_without_its_folder_block_keeps_the_folder_template() {
    let settings = settings();
    let prompt = explorer_turn(&settings)
        .with_kb_context(Some(PASSAGE.into()))
        .build();

    assert!(prompt.starts_with(explorer_opening()), "{prompt}");
}

/// The budget charges the template the turn will send.
#[test]
fn the_evidence_frame_is_the_template_the_turn_sends() {
    let settings = settings();
    assert!(evidence_frame(&settings, QUESTION, true).starts_with(explorer_opening()));
    let document_opening = settings
        .rag_prompt_template
        .split("{context}")
        .next()
        .expect("the template has a context slot");
    assert!(evidence_frame(&settings, QUESTION, false).starts_with(document_opening));
}

/// An ordinary chat still answers from its documents and cites them.
#[test]
fn an_ordinary_turn_keeps_the_document_answer_template() {
    let settings = settings();
    let prompt = PromptMessageBuilder::new(
        &settings,
        QUESTION,
        false,
        SearchFlags::from_preferences(None),
    )
    .with_kb_context(Some(PASSAGE.into()))
    .build();

    assert!(prompt.contains("Cite every factual statement"), "{prompt}");
    assert!(!prompt.starts_with(explorer_opening()));
}
