use super::*;
use crate::features::conversation::chat::prompt_settings::normalize_prompt_settings;

const QUESTION: &str = "How does the fusion step rank results?";
const SOURCE: &str =
    "Folder open beside this chat: lattice. The user is looking at it in Explorer.";
const PASSAGE: &str =
    "[1] Document: retrieval-notes.md\nContent: Reciprocal rank fusion sums 1/(k + rank).";
const TEMPLATE: &str =
    "Answer about the source beside this chat.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:";

fn settings() -> LLMPromptSettingsDto {
    normalize_prompt_settings(LLMPromptSettingsDto::default())
}

fn policy(evidence_template: Option<&'static str>) -> TurnSourcePolicy {
    TurnSourcePolicy {
        routes_to_library: false,
        offers_library_tools: false,
        infers_intent: false,
        verifies_cited_only: true,
        tool_rounds: 10,
        evidence_template,
    }
}

fn source_turn<'a>(
    settings: &'a LLMPromptSettingsDto,
    template: Option<&'static str>,
) -> PromptMessageBuilder<'a> {
    PromptMessageBuilder::new(
        settings,
        QUESTION,
        false,
        SearchFlags::from_preferences(None),
    )
    .for_source(Some(policy(template)))
}

fn opening(template: &str) -> &str {
    template
        .split("{context}")
        .next()
        .expect("the template has a context slot")
}

/// The source's block leads the context, in the source's own template.
#[test]
fn a_source_turn_renders_in_the_source_s_template() {
    let settings = settings();
    let prompt = source_turn(&settings, Some(TEMPLATE))
        .with_source_context(Some(SOURCE.into()))
        .build();

    assert!(prompt.starts_with(opening(TEMPLATE)), "{prompt}");
    assert!(!prompt.contains("Cite every factual statement"), "{prompt}");
    assert!(prompt.contains(SOURCE));
    assert!(prompt.contains(&format!("Question: {QUESTION}")));
}

/// Library passages the user turned on still carry their numbers beside the
/// source's block.
#[test]
fn library_passages_in_a_source_turn_keep_their_numbers() {
    let settings = settings();
    let prompt = source_turn(&settings, Some(TEMPLATE))
        .with_source_context(Some(SOURCE.into()))
        .with_kb_context(Some(PASSAGE.into()))
        .build();

    assert!(prompt.starts_with(opening(TEMPLATE)), "{prompt}");
    assert!(prompt.contains(&format!("Knowledge Base Results:\n{PASSAGE}")));
    assert!(prompt.find(SOURCE) < prompt.find(PASSAGE), "{prompt}");
}

/// The source's block can be the evidence cut for room; the turn is still about
/// the source.
#[test]
fn a_source_turn_without_its_block_keeps_the_source_s_template() {
    let settings = settings();
    let prompt = source_turn(&settings, Some(TEMPLATE))
        .with_kb_context(Some(PASSAGE.into()))
        .build();

    assert!(prompt.starts_with(opening(TEMPLATE)), "{prompt}");
}

/// A source without a template of its own answers in the document-answer
/// template, which asks for citations.
#[test]
fn a_source_without_a_template_keeps_the_document_answer_template() {
    let settings = settings();
    let prompt = source_turn(&settings, None)
        .with_source_context(Some(SOURCE.into()))
        .build();

    assert!(
        prompt.starts_with(opening(&settings.rag_prompt_template)),
        "{prompt}"
    );
    assert!(prompt.contains(SOURCE));
}

/// The budget charges the template the turn will send.
#[test]
fn the_evidence_frame_is_the_template_the_turn_sends() {
    let settings = settings();
    assert!(
        evidence_frame(&settings, QUESTION, Some(policy(Some(TEMPLATE))))
            .starts_with(opening(TEMPLATE))
    );
    let document_opening = opening(&settings.rag_prompt_template);
    assert!(evidence_frame(&settings, QUESTION, Some(policy(None))).starts_with(document_opening));
    assert!(evidence_frame(&settings, QUESTION, None).starts_with(document_opening));
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
    assert!(!prompt.starts_with(opening(TEMPLATE)));
}
