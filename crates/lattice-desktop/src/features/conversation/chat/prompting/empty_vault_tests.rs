use super::*;

fn flags() -> SearchFlags {
    SearchFlags {
        force_kb_search: false,
        force_web_search: false,
        web_fallback: false,
        force_wiki_search: false,
        deep_research_mode: false,
        force_followup_mode: false,
        closed_book: false,
    }
}

fn prompt(reason: Option<&str>, kb_attempted: bool) -> String {
    let settings = LLMPromptSettingsDto::default();
    PromptMessageBuilder::new(&settings, "What should I grow?", false, flags())
        .with_kb_unavailable_reason(reason.map(str::to_string))
        .with_kb_attempted(kb_attempted)
        .build()
}

#[test]
fn a_closed_book_turn_is_sent_as_written() {
    let settings = LLMPromptSettingsDto::default();
    let flags = SearchFlags {
        closed_book: true,
        ..flags()
    };

    let prompt = PromptMessageBuilder::new(&settings, "Merge these notes.", false, flags).build();

    assert_eq!(prompt, "Merge these notes.");
}

/// A space holding nothing is not reported as documents searched that had
/// nothing to offer.
#[test]
fn an_empty_space_is_not_reported_as_a_search_that_found_nothing() {
    let prompt = prompt(
        Some("no indexed documents are assigned to this conversation’s space"),
        false,
    );

    assert!(
        prompt.contains("not searched for this turn"),
        "the reader must be told the vault was never reached:\n{prompt}"
    );
    assert!(
        prompt.contains("no indexed documents are assigned to this conversation’s space"),
        "retrieval already worked out why; the prompt must carry it:\n{prompt}"
    );
    assert!(
        !prompt.contains("No relevant documents were found"),
        "this wording claims a search ran and rejected the documents:\n{prompt}"
    );
}

#[test]
fn a_search_that_really_ran_still_says_so() {
    let prompt = prompt(None, true);

    assert!(
        prompt.contains("were searched for this turn"),
        "a vault that was read and came back empty is a different fact:\n{prompt}"
    );
}

/// Neither of the two sentences above fits a turn that never planned a
/// knowledge-base search, so it gets one that claims neither.
#[test]
fn a_turn_that_never_planned_a_search_claims_neither() {
    let prompt = prompt(None, false);

    assert!(
        prompt.contains("No passages from their documents were available"),
        "{prompt}"
    );
    assert!(!prompt.contains("were searched for this turn"), "{prompt}");
}

#[test]
fn the_default_template_has_somewhere_to_put_the_reason() {
    let settings = LLMPromptSettingsDto::default();

    assert!(
        settings.no_context_prompt_template.contains("{context}"),
        "without the slot the reason is computed and dropped, which is the \
             bug this fixes"
    );
    assert!(
        !settings
            .no_context_prompt_template
            .contains("No relevant documents were found"),
        "the default must not assert a search that may not have happened"
    );
}
