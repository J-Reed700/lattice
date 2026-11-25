use super::*;

fn flags() -> SearchFlags {
    SearchFlags::default()
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

#[test]
fn hybrid_user_requested_kb_without_hits_still_allows_general_knowledge() {
    let settings = LLMPromptSettingsDto::default();
    let flags = SearchFlags {
        force_kb_search: true,
        user_requested_kb: true,
        strict_sources: false,
        ..SearchFlags::default()
    };
    let prompt = PromptMessageBuilder::new(&settings, "What is TCP?", false, flags)
        .with_kb_attempted(true)
        .build();

    assert!(
        prompt.contains("Answer naturally from general knowledge"),
        "{prompt}"
    );
    assert!(
        !prompt.contains("Do not answer from general knowledge"),
        "{prompt}"
    );
}

#[test]
fn a_requested_search_that_found_nothing_says_the_answer_is_not_backed_by_their_documents() {
    let settings = LLMPromptSettingsDto::default();
    let flags = SearchFlags {
        force_kb_search: true,
        user_requested_kb: true,
        strict_sources: false,
        ..SearchFlags::default()
    };
    let prompt = PromptMessageBuilder::new(&settings, "What is TCP?", false, flags)
        .with_kb_attempted(true)
        .build();

    assert!(prompt.contains("not backed by their documents"), "{prompt}");
}

#[test]
fn a_requested_search_that_could_not_run_says_why_and_that_the_answer_is_not_backed() {
    let settings = LLMPromptSettingsDto::default();
    let flags = SearchFlags {
        user_requested_kb: true,
        ..SearchFlags::default()
    };
    let prompt = PromptMessageBuilder::new(&settings, "What is TCP?", false, flags)
        .with_kb_unavailable_reason(Some("the vault is still indexing".to_string()))
        .build();

    assert!(prompt.contains("the vault is still indexing"), "{prompt}");
    assert!(prompt.contains("not backed by their documents"), "{prompt}");
}

#[test]
fn a_turn_that_did_not_ask_for_its_documents_is_not_told_to_say_so() {
    let settings = LLMPromptSettingsDto::default();
    let prompt = PromptMessageBuilder::new(&settings, "What is TCP?", false, flags())
        .with_kb_attempted(true)
        .build();

    assert!(
        !prompt.contains("not backed by their documents"),
        "{prompt}"
    );
}

#[test]
fn strict_mode_without_hits_refuses_general_knowledge() {
    let settings = LLMPromptSettingsDto::default();
    let flags = SearchFlags {
        strict_sources: true,
        ..SearchFlags::default()
    };
    let prompt = PromptMessageBuilder::new(&settings, "What is TCP?", false, flags)
        .with_kb_attempted(true)
        .build();

    assert!(
        prompt.contains("Do not answer from general knowledge"),
        "{prompt}"
    );
    assert!(prompt.contains("turn off Sources only"), "{prompt}");
}

#[test]
fn strict_mode_with_failed_forced_web_refuses_general_knowledge() {
    let settings = LLMPromptSettingsDto::default();
    let strict_flags = SearchFlags {
        strict_sources: true,
        force_web_search: true,
        ..SearchFlags::default()
    };
    let strict_prompt = PromptMessageBuilder::new(&settings, "Latest news?", false, strict_flags)
        .with_web_search_error(Some("network offline".to_string()))
        .build();
    assert!(strict_prompt.contains("network offline"), "{strict_prompt}");
    assert!(
        strict_prompt.contains("Do not answer from general knowledge"),
        "{strict_prompt}"
    );
    assert!(
        strict_prompt.contains("turn off Sources only"),
        "{strict_prompt}"
    );

    let hybrid_flags = SearchFlags {
        strict_sources: false,
        force_web_search: true,
        ..SearchFlags::default()
    };
    let hybrid_prompt = PromptMessageBuilder::new(&settings, "Latest news?", false, hybrid_flags)
        .with_web_search_error(Some("network offline".to_string()))
        .build();
    assert!(
        hybrid_prompt.contains("Give your best-effort answer from your built-in knowledge"),
        "{hybrid_prompt}"
    );
}

#[test]
fn strict_mode_with_evidence_cut_never_falls_through_to_general_knowledge() {
    let settings = LLMPromptSettingsDto::default();
    let strict_flags = SearchFlags {
        strict_sources: true,
        ..SearchFlags::default()
    };
    let prompt = PromptMessageBuilder::new(&settings, "What is in my notes?", false, strict_flags)
        .with_evidence_cut(true)
        .build();

    assert!(prompt.contains("Sources-only mode is on"), "{prompt}");
    assert!(prompt.contains("context window had no room"), "{prompt}");
    assert!(
        prompt.contains("Do not answer from general knowledge"),
        "{prompt}"
    );
    assert!(
        !prompt.contains("Answer naturally from general knowledge"),
        "{prompt}"
    );
}
