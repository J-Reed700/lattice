use super::*;
use crate::features::conversation::chat::retrieval::sufficiency_reason as reason;
use crate::features::conversation::chat::retrieval::SufficiencyVerdict;

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

fn weak(reasons: Vec<&'static str>) -> SufficiencyVerdict {
    SufficiencyVerdict {
        sufficient: false,
        reasons,
        ..Default::default()
    }
}

fn prompt_with(
    settings: &LLMPromptSettingsDto,
    kb: Option<&str>,
    web: Option<&str>,
    verdict: Option<&SufficiencyVerdict>,
) -> String {
    PromptMessageBuilder::new(settings, "Does silo hold up?", false, flags())
        .with_kb_context(kb.map(str::to_string))
        .with_web_context(web.map(str::to_string))
        .with_kb_sufficiency(verdict)
        .build()
}

#[test]
fn weak_passages_are_named_as_weak_in_the_prompt() {
    let settings = LLMPromptSettingsDto::default();
    let prompt = prompt_with(
        &settings,
        Some("[1] A passage about something else."),
        None,
        Some(&weak(vec![reason::LOW_TOP_SCORE])),
    );

    assert!(
        prompt.contains("Evidence quality:"),
        "a turn the app judged unanswerable must say so to the model:\n{prompt}"
    );
    assert!(
        prompt.contains("do not fill the gap"),
        "the instruction that stops a confident answer off weak text is the \
             point of the note:\n{prompt}"
    );
}

#[test]
fn sufficient_passages_carry_no_warning() {
    let settings = LLMPromptSettingsDto::default();
    let prompt = prompt_with(
        &settings,
        Some("[1] Directly on point."),
        None,
        Some(&SufficiencyVerdict::default()),
    );

    assert!(!prompt.contains("Evidence quality:"), "{prompt}");
}

#[test]
fn a_turn_with_no_verdict_is_left_alone() {
    let settings = LLMPromptSettingsDto::default();
    let prompt = prompt_with(&settings, Some("[1] A passage."), None, None);

    assert!(!prompt.contains("Evidence quality:"), "{prompt}");
}

#[test]
fn weak_passages_stop_outranking_the_web_by_rule() {
    let settings = LLMPromptSettingsDto::default();
    let strong = prompt_with(
        &settings,
        Some("[1] On point."),
        Some("Web result"),
        Some(&SufficiencyVerdict::default()),
    );
    let thin = prompt_with(
        &settings,
        Some("[1] Off point."),
        Some("Web result"),
        Some(&weak(vec![reason::LOW_TERM_COVERAGE])),
    );

    assert!(
        strong.contains("Prefer Knowledge Base Results first"),
        "{strong}"
    );
    assert!(
        !thin.contains("Prefer Knowledge Base Results first"),
        "ranking known-weak passages above the web is the failure this \
             avoids:\n{thin}"
    );
    assert!(thin.contains("on their merits"), "{thin}");
}

#[test]
fn nothing_is_flagged_when_no_passages_were_retrieved() {
    let settings = LLMPromptSettingsDto::default();
    // The no-context templates already explain an empty knowledge base;
    // adding a note about weak passages would describe passages that do
    // not exist.
    let prompt = prompt_with(&settings, None, None, Some(&weak(vec![reason::NO_RESULTS])));

    assert!(!prompt.contains("Evidence quality:"), "{prompt}");
}

#[test]
fn each_deciding_reason_gets_its_own_words() {
    let low_score = describe_thin_evidence(&[reason::LOW_TOP_SCORE]);
    let low_coverage = describe_thin_evidence(&[reason::LOW_TERM_COVERAGE]);
    let too_few = describe_thin_evidence(&[reason::TOO_FEW_RESULTS]);

    assert_ne!(low_score, low_coverage);
    assert_ne!(low_coverage, too_few);
    for described in [&low_score, &low_coverage, &too_few] {
        assert!(described.ends_with('.'), "{described}");
    }
}

#[test]
fn reasons_about_the_judgement_are_not_reported_as_weak_evidence() {
    // A missing reranker says how the call was made, not that the passages
    // are poor. Describing it to the model would be a false claim about
    // the user's documents.
    let described = describe_thin_evidence(&[reason::NO_RERANKER, reason::FLAT_RERANK_SPREAD]);

    assert!(!described.contains("rerank"), "{described}");
    assert_eq!(
        described, "the retrieved passages were judged too weak to answer from.",
        "with nothing specific to report, the note stays general rather \
             than inventing a cause"
    );
}

#[test]
fn several_reasons_read_as_one_sentence() {
    let described = describe_thin_evidence(&[reason::LOW_TOP_SCORE, reason::LOW_TERM_COVERAGE]);

    assert!(described.contains(", and "), "{described}");
    assert_eq!(described.matches('.').count(), 1, "{described}");
}
