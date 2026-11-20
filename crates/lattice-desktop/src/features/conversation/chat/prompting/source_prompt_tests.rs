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
        reads_passages: true,
        drops_session_talk: false,
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

/// A source without a template of its own answers in the chat's active
/// document-answer template (hybrid by default, strict when `strict_sources` is on).
#[test]
fn a_source_without_a_template_keeps_the_document_answer_template() {
    let settings = settings();
    let hybrid_prompt = source_turn(&settings, None)
        .with_source_context(Some(SOURCE.into()))
        .build();
    let strict_prompt = PromptMessageBuilder::new(
        &settings,
        QUESTION,
        false,
        SearchFlags {
            strict_sources: true,
            ..SearchFlags::default()
        },
    )
    .for_source(Some(policy(None)))
    .with_source_context(Some(SOURCE.into()))
    .build();

    assert!(
        hybrid_prompt.starts_with(opening(DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE)),
        "{hybrid_prompt}"
    );
    assert!(
        strict_prompt.starts_with(opening(DEFAULT_RAG_PROMPT_TEMPLATE)),
        "{strict_prompt}"
    );
    assert!(hybrid_prompt.contains(SOURCE));
}

/// The budget charges the template the turn will send.
#[test]
fn the_evidence_frame_is_the_template_the_turn_sends() {
    let settings = settings();
    assert!(
        evidence_frame(&settings, QUESTION, Some(policy(Some(TEMPLATE))), false)
            .starts_with(opening(TEMPLATE))
    );
    let hybrid_opening = opening(DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE);
    let strict_opening = opening(DEFAULT_RAG_PROMPT_TEMPLATE);
    assert!(
        evidence_frame(&settings, QUESTION, Some(policy(None)), false).starts_with(hybrid_opening)
    );
    assert!(evidence_frame(&settings, QUESTION, None, false).starts_with(hybrid_opening));
    assert!(
        evidence_frame(&settings, QUESTION, Some(policy(None)), true).starts_with(strict_opening)
    );
    assert!(evidence_frame(&settings, QUESTION, None, true).starts_with(strict_opening));
}

/// A source that talks with the learner does not read first: its message
/// closes as its template does, and the budget charges that message.
#[test]
fn a_source_that_does_not_read_closes_on_its_own_template() {
    let settings = settings();
    let policy = TurnSourcePolicy {
        reads_passages: false,
        ..policy(Some(TEMPLATE))
    };
    let prompt = PromptMessageBuilder::new(
        &settings,
        QUESTION,
        false,
        SearchFlags::from_preferences(None),
    )
    .for_source(Some(policy))
    .with_source_context(Some(SOURCE.into()))
    .with_kb_context(Some(PASSAGE.into()))
    .reading_passages(true)
    .build();

    assert!(!prompt.contains(READING_INSTRUCTION), "{prompt}");
    assert!(prompt.ends_with("Answer:"), "{prompt}");
    let frame = evidence_frame(&settings, QUESTION, Some(policy), false);
    assert!(!frame.contains(READING_INSTRUCTION), "{frame}");
    assert!(frame.ends_with("Answer:"), "{frame}");
}

/// Strict-mode ordinary prompts stay anchored to `DEFAULT_RAG_PROMPT_TEMPLATE`,
/// while default hybrid prompts use `DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE` unless
/// the user customized `rag_prompt_template`.
#[test]
fn hybrid_and_strict_modes_select_their_own_templates_and_respect_custom_prompts() {
    let settings = settings();
    let hybrid = PromptMessageBuilder::new(
        &settings,
        QUESTION,
        false,
        SearchFlags::from_preferences(None),
    )
    .with_kb_context(Some(PASSAGE.into()))
    .build();
    let strict = PromptMessageBuilder::new(
        &settings,
        QUESTION,
        false,
        SearchFlags {
            strict_sources: true,
            ..SearchFlags::default()
        },
    )
    .with_kb_context(Some(PASSAGE.into()))
    .build();

    assert!(hybrid.starts_with(opening(DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE)));
    assert!(hybrid.contains("from general knowledge without adding citations"));
    assert!(strict.starts_with(opening(DEFAULT_RAG_PROMPT_TEMPLATE)));
    assert!(strict.contains("using only the provided context"));

    let mut custom = settings.clone();
    custom.rag_prompt_template = "Custom prompt:\n{context}\nQ: {question}\nAnswer:".into();
    let custom_hybrid = PromptMessageBuilder::new(
        &custom,
        QUESTION,
        false,
        SearchFlags::from_preferences(None),
    )
    .with_kb_context(Some(PASSAGE.into()))
    .build();
    let custom_strict = PromptMessageBuilder::new(
        &custom,
        QUESTION,
        false,
        SearchFlags {
            strict_sources: true,
            ..SearchFlags::default()
        },
    )
    .with_kb_context(Some(PASSAGE.into()))
    .build();
    assert!(custom_hybrid.starts_with("Custom prompt:"));
    assert!(custom_strict.starts_with("Custom prompt:"));

    // A saved `DEFAULT_RAG_PROMPT_TEMPLATE` also switches cleanly between hybrid and strict.
    let mut legacy_default = settings;
    legacy_default.rag_prompt_template = DEFAULT_RAG_PROMPT_TEMPLATE.into();
    let legacy_hybrid = PromptMessageBuilder::new(
        &legacy_default,
        QUESTION,
        false,
        SearchFlags::from_preferences(None),
    )
    .with_kb_context(Some(PASSAGE.into()))
    .build();
    let legacy_strict = PromptMessageBuilder::new(
        &legacy_default,
        QUESTION,
        false,
        SearchFlags {
            strict_sources: true,
            ..SearchFlags::default()
        },
    )
    .with_kb_context(Some(PASSAGE.into()))
    .build();
    assert!(legacy_hybrid.starts_with(opening(DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE)));
    assert!(legacy_strict.starts_with(opening(DEFAULT_RAG_PROMPT_TEMPLATE)));
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

    assert!(
        prompt.contains("citing every factual statement"),
        "{prompt}"
    );
    assert!(!prompt.starts_with(opening(TEMPLATE)));
}
