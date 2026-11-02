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

/// Every prompt a turn outside the tutor renders, byte for byte. The digest
/// was taken from the code before the tutor existed: it moves only when one of
/// those prompts does.
#[test]
fn prompts_outside_the_tutor_are_unchanged() {
    use sha2::{Digest, Sha256};
    let settings = settings();
    let ordinary = |is_greeting: bool| {
        PromptMessageBuilder::new(
            &settings,
            QUESTION,
            is_greeting,
            SearchFlags::from_preferences(None),
        )
    };
    let prompts = [
        ordinary(false).build(),
        ordinary(true).build(),
        ordinary(false).with_kb_attempted(true).build(),
        ordinary(false)
            .with_kb_context(Some(PASSAGE.into()))
            .with_kb_attempted(true)
            .reading_passages(true)
            .build(),
        ordinary(false)
            .with_web_context(Some(PASSAGE.into()))
            .reading_passages(true)
            .build(),
        source_turn(&settings, Some(TEMPLATE))
            .with_source_context(Some(SOURCE.into()))
            .build(),
        source_turn(&settings, Some(TEMPLATE))
            .with_source_context(Some(SOURCE.into()))
            .with_kb_context(Some(PASSAGE.into()))
            .reading_passages(true)
            .build(),
        evidence_frame(&settings, QUESTION, None),
        evidence_frame(&settings, QUESTION, Some(policy(Some(TEMPLATE)))),
        settings.system_prompt.clone(),
    ];
    let digest = format!("{:x}", Sha256::digest(prompts.join("\u{0}")));
    assert_eq!(
        digest,
        "446432fefdeb1630aca0419cb828bb57bbec79f51c9af127e698b4f2e97b9384"
    );
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
