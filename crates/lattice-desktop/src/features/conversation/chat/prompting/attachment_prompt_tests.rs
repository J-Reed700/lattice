use super::*;

fn web_turn_flags() -> SearchFlags {
    SearchFlags {
        force_kb_search: false,
        force_web_search: true,
        web_fallback: false,
        force_wiki_search: false,
        deep_research_mode: true,
        force_followup_mode: false,
        closed_book: false,
    }
}

fn builder<'a>(settings: &'a LLMPromptSettingsDto) -> PromptMessageBuilder<'a> {
    PromptMessageBuilder::new(
        settings,
        "here it is, are you not receiving it?",
        false,
        web_turn_flags(),
    )
}

/// The turn this was written for. Deep research had the vault switched
/// off, so the attached file reached the model only if something carried
/// it — and nothing did. The answer was "no file came through", with the
/// file indexed in the library.
#[test]
fn an_attachment_reaches_a_forced_web_turn() {
    let settings = LLMPromptSettingsDto::default();

    let prompt = builder(&settings)
        .with_attachment_context(Some(
            "Attached Files:\n[1] Attached file: greens.txt\nContent: kale, chard".to_string(),
        ))
        .with_web_context(Some("Web Results".to_string()))
        .build();

    assert!(prompt.contains("greens.txt"), "{prompt}");
    assert!(prompt.contains("kale, chard"), "{prompt}");
}

/// Attaching a file is the reader saying "this one". A follow-up would
/// otherwise hand the model the *previous* document as the whole prompt
/// and drop the new one.
#[test]
fn an_attachment_outranks_the_document_from_the_last_turn() {
    let settings = LLMPromptSettingsDto::default();

    let prompt = builder(&settings)
        .with_attachment_context(Some("Attached Files:\nthe new file".to_string()))
        .with_followup_context(Some("[1] Document: older.txt\nContent: older".to_string()))
        .build();

    let attachment_at = prompt.find("the new file").expect("attachment is carried");
    let followup_at = prompt
        .find("older.txt")
        .expect("the earlier document stays");
    assert!(
        attachment_at < followup_at,
        "the attached file must lead the context:\n{prompt}"
    );
}

/// Without an attachment the follow-up path is unchanged: the last
/// document is still the whole prompt.
#[test]
fn a_plain_followup_still_reuses_the_last_document_alone() {
    let settings = LLMPromptSettingsDto::default();

    let prompt = builder(&settings)
        .with_followup_context(Some("[1] Document: older.txt\nContent: older".to_string()))
        .with_kb_context(Some("Knowledge Base".to_string()))
        .build();

    assert!(prompt.contains("older.txt"), "{prompt}");
    assert!(!prompt.contains("Knowledge Base"), "{prompt}");
}

#[test]
fn prior_evidence_survives_new_research_and_followup_prompt_paths() {
    let settings = LLMPromptSettingsDto::default();
    for followup in [None, Some("[1] earlier document".to_string())] {
        let prompt = builder(&settings)
            .with_followup_context(followup)
            .with_prior_evidence_context(Some("[11] retained grow light evidence".to_string()))
            .with_web_context(Some("[2] new product specifications".to_string()))
            .build();
        assert!(
            prompt.contains("[11] retained grow light evidence"),
            "{prompt}"
        );
        assert!(
            prompt.contains("[2] new product specifications"),
            "{prompt}"
        );
        assert!(prompt.contains("supplement that evidence"), "{prompt}");
    }
}

/// A file that arrived but could not be read is still a file that
/// arrived; the prompt must never let the model deny it.
#[test]
fn an_attachment_alone_is_enough_context_to_answer_from() {
    let settings = LLMPromptSettingsDto::default();

    let prompt = builder(&settings)
        .with_attachment_context(Some("Attached but not readable yet: scan.pdf".to_string()))
        .build();

    assert!(prompt.contains("scan.pdf"), "{prompt}");
    assert!(
        !prompt.contains("live web retrieval is currently unavailable"),
        "an attachment is context, so the empty-turn templates must not fire:\n{prompt}"
    );
}
