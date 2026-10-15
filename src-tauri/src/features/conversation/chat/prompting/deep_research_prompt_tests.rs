use super::*;

fn flags() -> SearchFlags {
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

fn prompt(web: Option<&str>, searches: Option<Vec<String>>) -> String {
    let settings = LLMPromptSettingsDto::default();
    PromptMessageBuilder::new(
        &settings,
        "what is going on with the weather in China?",
        false,
        flags(),
    )
    .with_web_context(web.map(str::to_string))
    .with_deep_research_searches(searches)
    .build()
}

/// The turn this was written for: a wide first round, then one page read
/// and an answer, because nothing asked the model to look further and it
/// had no search left to look with.
#[test]
fn deep_research_sends_the_model_after_what_the_first_round_missed() {
    let searches = vec![
        "China extreme weather events 2026".to_string(),
        "China typhoon track October 2026".to_string(),
    ];
    let text = prompt(Some("[1] Storms test resilience"), Some(searches));
    let results = text.find("Web Results").unwrap();
    let instruction = text.find("Deep research:").unwrap();
    assert!(
        instruction > results,
        "the instruction follows the results it is about:\n{text}"
    );
    assert!(text
        .contains("\"China extreme weather events 2026\"; \"China typhoon track October 2026\""));
    assert!(text.contains("call web_search"));
    assert!(text.contains("several calls in one reply"));
    assert!(text.contains("Do not repeat a search listed here"));
}

#[test]
fn no_instruction_without_a_first_round_or_a_way_to_search() {
    // The turn may not search again (web search off, or no tool calling).
    assert!(!prompt(Some("[1] Storms"), None).contains("Deep research:"));
    // No web results: the empty-turn templates speak for the turn instead.
    assert!(!prompt(None, Some(vec!["q".to_string()])).contains("Deep research:"));
}

#[test]
fn the_list_of_searches_is_bounded() {
    let many: Vec<String> = (0..40)
        .map(|n| format!("query {n} {}", "x".repeat(300)))
        .collect();
    let text = deep_research_instruction(&many);
    assert_eq!(text.matches("\"query ").count(), LISTED_SEARCHES);
    assert!(!text.contains(&"x".repeat(LISTED_SEARCH_CHARS)));
    assert!(!deep_research_instruction(&[]).contains("Searches already run"));
}
