use super::retrieval::WEB_SOURCE_PREFIX;
use super::*;
use crate::features::qa::dto::SourceDto;

fn vault_source(document_id: &str, chunk_id: &str) -> SourceDto {
    SourceDto {
        document_id: document_id.to_string(),
        chunk_id: chunk_id.to_string(),
        content: "text".to_string(),
        score: 1.0,
        path: None,
        position: None,
        file_name: "doc.pdf".to_string(),
        file_path: "/doc.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        category: "PDF Document".to_string(),
        file_size_bytes: 1,
        modified_at: String::new(),
        excerpt: None,
        highlights: None,
        section: None,
        chunk_index: None,
        page_number: None,
        chunk_excerpts: None,
        citation_id: None,
        web_snapshot: None,
    }
}

fn web_source(url: &str) -> SourceDto {
    SourceDto {
        document_id: format!("{WEB_SOURCE_PREFIX}{url}"),
        mime_type: "text/html".to_string(),
        category: "Web Article".to_string(),
        ..vault_source("unused", url)
    }
}

/// The turn that started all of this: a chat in a space holding no
/// documents, answered entirely from the web, which reported "10 passages
/// from 10 files" and so looked exactly like a scope leak.
#[test]
fn a_web_only_turn_reports_no_files_and_no_passages() {
    let sources: Vec<SourceDto> = (0..10)
        .map(|index| web_source(&format!("https://example.com/{index}")))
        .collect();
    assert_eq!(
        trace_counts(&sources),
        TraceCounts {
            passages: 0,
            files: 0,
            web_pages: 10
        }
    );
}

#[test]
fn a_vault_turn_counts_documents_and_no_web_pages() {
    let sources = vec![
        vault_source("doc-a", "chunk-1"),
        vault_source("doc-a", "chunk-2"),
        vault_source("doc-b", "chunk-3"),
    ];
    assert_eq!(
        trace_counts(&sources),
        TraceCounts {
            passages: 3,
            files: 2,
            web_pages: 0
        }
    );
}

/// A deep-research turn reads both. Each has to be counted as what it is.
#[test]
fn a_mixed_turn_keeps_the_two_apart() {
    let sources = vec![
        vault_source("doc-a", "chunk-1"),
        web_source("https://example.com/one"),
        web_source("https://example.com/two"),
        web_source("https://example.com/one"),
    ];
    assert_eq!(
        trace_counts(&sources),
        TraceCounts {
            passages: 1,
            files: 1,
            web_pages: 2
        }
    );
}

#[test]
fn a_turn_with_no_sources_counts_nothing() {
    assert_eq!(trace_counts(&[]), TraceCounts::default());
}

/// Zero web pages must not appear on the wire at all, so a trace that read
/// only documents looks the same as one written before the field existed.
#[test]
fn a_trace_without_web_pages_omits_the_field() {
    let trace = RetrievalTraceDto {
        searched_documents: 3,
        passages: 2,
        files: 1,
        scope: "vault".to_string(),
        ..Default::default()
    };
    let json = serde_json::to_value(&trace).expect("serialize");
    assert!(json.get("webPages").is_none(), "got {json}");

    let with_web = RetrievalTraceDto {
        web_pages: 4,
        ..trace
    };
    let json = serde_json::to_value(&with_web).expect("serialize");
    assert_eq!(
        json.get("webPages").and_then(serde_json::Value::as_u64),
        Some(4)
    );
}

/// The sufficiency fields are additive. A trace written before the check
/// existed carries none of them, and a reader that treated a missing
/// verdict as `false` would report every historical turn as insufficient.
#[test]
fn a_trace_without_a_verdict_serializes_without_the_sufficiency_fields() {
    let trace = RetrievalTraceDto {
        searched_documents: 12,
        passages: 3,
        files: 2,
        scope: "vault".to_string(),
        ..Default::default()
    };
    let json = serde_json::to_value(&trace).expect("serialize");
    for absent in [
        "kbSufficient",
        "kbCorrectiveRetries",
        "kbPlannerSkipped",
        "sufficiencyReasons",
        "unavailableReason",
    ] {
        assert!(json.get(absent).is_none(), "{absent} should be omitted");
    }
}

#[test]
fn a_corrective_pass_is_reported_with_its_reasons() {
    let trace = RetrievalTraceDto {
        searched_documents: 12,
        passages: 3,
        files: 2,
        scope: "vault".to_string(),
        kb_sufficient: Some(false),
        kb_corrective_retries: Some(1),
        kb_planner_skipped: Some(true),
        sufficiency_reasons: vec!["low_term_coverage".to_string()],
        ..Default::default()
    };
    let json = serde_json::to_value(&trace).expect("serialize");
    assert_eq!(
        json.get("kbSufficient").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        json.get("kbCorrectiveRetries").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        json.get("kbPlannerSkipped").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        json.get("sufficiencyReasons")
            .and_then(|v| v.as_array())
            .map(Vec::len),
        Some(1)
    );
}

/// Byte-slicing a string at a fixed offset panics when that offset falls
/// inside a multi-byte character. Any first chat message over 50 bytes
/// containing non-ASCII text used to bring down the command handler.
#[test]
fn generate_title_does_not_panic_on_multibyte_input() {
    let cases = [
        // Accents: 'é' is 2 bytes, so byte 50 lands mid-character.
        "Bonjour, je voudrais discuter des propriétés thermodynamiques de ce système.",
        // CJK: every character is 3 bytes.
        "这是一个很长的中文句子用来测试标题生成功能是否会因为多字节字符而崩溃。",
        // Emoji: 4 bytes each, placed to straddle the boundary.
        "Let's talk about the launch 🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀 today",
        // Mixed scripts.
        "Café ☕ meeting notes こんにちは everyone, here is the agenda for today's sync",
    ];

    for message in cases {
        let title = generate_title(message);
        assert!(
            !title.is_empty(),
            "title should not be empty for {:?}",
            message
        );
        // 50 characters plus the ellipsis.
        assert!(
            title.chars().count() <= 53,
            "title {:?} exceeded the character budget",
            title
        );
    }
}

#[test]
fn generate_title_truncates_by_characters_not_bytes() {
    // 60 CJK characters = 180 bytes. A byte-based truncation would cut at
    // ~16 characters (or panic); character-based gives exactly 50.
    let message = "字".repeat(60);
    let title = generate_title(&message);
    assert_eq!(title.chars().count(), 53, "50 chars plus '...'");
    assert!(title.ends_with("..."));
}

#[test]
fn test_generate_title_short() {
    let short_message = "Hello world";
    assert_eq!(generate_title(short_message), "Hello world");
}

#[test]
fn test_generate_title_long() {
    let long_message =
        "This is a very long message that exceeds fifty characters and should be truncated";
    let title = generate_title(long_message);

    assert!(title.len() <= 53); // 50 + "..."
    assert!(title.ends_with("..."));
    assert_eq!(
        title,
        "This is a very long message that exceeds fifty cha..."
    );
}

#[test]
fn test_generate_title_exact_length() {
    let message = "x".repeat(50);
    let title = generate_title(&message);

    assert_eq!(title.len(), 50);
    assert!(!title.ends_with("..."));
}

#[test]
fn test_search_flags_respect_followup_turn_mode() {
    let prefs = ToolPreferences {
        knowledge_base: false,
        web_search: false,
        deep_research_mode: false,
        followup_mode: false,
        turn_mode: Some("followup".to_string()),
        enabled_tools: None,
        focus_document_ids: None,
        explorer_focus: None,
        closed_book: false,
    };

    let flags = SearchFlags::from_preferences(Some(&prefs));
    assert!(!flags.force_kb_search);
    assert!(!flags.force_web_search);
    assert!(flags.force_followup_mode);
}

/// The reported bug: a journal synthesis asked for `knowledge_base: false`
/// and was still handed another space's documents, because that flag only
/// declines to force a search. Closed has to beat every other preference.
#[test]
fn a_closed_book_turn_cannot_be_reopened_by_any_other_preference() {
    let prefs = ToolPreferences {
        knowledge_base: true,
        web_search: true,
        deep_research_mode: true,
        followup_mode: false,
        turn_mode: Some("query".to_string()),
        enabled_tools: Some(vec!["wiki_search".to_string()]),
        focus_document_ids: None,
        explorer_focus: None,
        closed_book: true,
    };

    let flags = SearchFlags::from_preferences(Some(&prefs));

    assert!(flags.closed_book);
    assert!(!flags.force_kb_search);
    assert!(!flags.force_web_search);
    assert!(!flags.force_wiki_search);
    assert!(!flags.deep_research_mode);
}

/// Only backend callers may close a turn. If the frontend could send the
/// flag it could also clear it, so it must not survive deserialization.
#[test]
fn the_frontend_cannot_set_closed_book() {
    let prefs: ToolPreferences =
        serde_json::from_str(r#"{"knowledgeBase":true,"closedBook":true,"closed_book":true}"#)
            .unwrap();

    assert!(!prefs.closed_book);
}

#[test]
fn test_deep_research_gets_a_longer_generation_budget() {
    let mut prefs = ToolPreferences {
        knowledge_base: false,
        web_search: true,
        deep_research_mode: false,
        followup_mode: false,
        turn_mode: None,
        enabled_tools: None,
        focus_document_ids: None,
        explorer_focus: None,
        closed_book: false,
    };
    let chat = generation_time_budget(SearchFlags::from_preferences(Some(&prefs)));
    prefs.deep_research_mode = true;
    let research = generation_time_budget(SearchFlags::from_preferences(Some(&prefs)));
    assert!(chat >= Duration::from_secs(30 * 60));
    assert!(research >= Duration::from_secs(2 * 60 * 60));
    assert!(research > chat);
}

#[test]
fn test_search_flags_query_turn_mode_forces_retrieval() {
    let prefs = ToolPreferences {
        knowledge_base: false,
        web_search: false,
        deep_research_mode: false,
        followup_mode: false,
        turn_mode: Some("query".to_string()),
        enabled_tools: None,
        focus_document_ids: None,
        explorer_focus: None,
        closed_book: false,
    };

    let flags = SearchFlags::from_preferences(Some(&prefs));
    assert!(flags.force_kb_search);
    assert!(flags.force_web_search);
    assert!(!flags.force_followup_mode);
}

#[test]
fn deep_research_keeps_its_searches_after_the_first_round() {
    // An ordinary grounded turn reads what it found; it does not search again.
    assert!(!kept_with_grounded_context("web_search", true, false));
    assert!(!kept_with_grounded_context("wiki_search", true, false));
    assert!(kept_with_grounded_context("fetch_url_content", true, false));
    assert!(kept_with_grounded_context("semantic_search", false, false));
    // Deep research goes back for more.
    assert!(kept_with_grounded_context("web_search", true, true));
    assert!(kept_with_grounded_context("wiki_search", true, true));
    assert!(kept_with_grounded_context("wiki_summary", true, true));
    assert!(!kept_with_grounded_context("list_documents", true, true));
}

#[test]
fn deep_research_is_asked_to_search_again_only_where_it_can() {
    let web_on = ToolPreferences {
        web_search: true,
        deep_research_mode: true,
        enabled_tools: Some(vec!["web_search".into(), "fetch_url_content".into()]),
        ..ToolPreferences::default()
    };
    let flags = SearchFlags::from_preferences(Some(&web_on));
    assert!(deep_research_searches_again(flags, Some(&web_on), true));
    // A model without tool calling cannot follow the instruction.
    assert!(!deep_research_searches_again(flags, Some(&web_on), false));

    // Web search switched off: deep research stays inside what the turn allows.
    let web_off = ToolPreferences {
        deep_research_mode: true,
        enabled_tools: Some(Vec::new()),
        ..ToolPreferences::default()
    };
    let flags = SearchFlags::from_preferences(Some(&web_off));
    assert!(!deep_research_searches_again(flags, Some(&web_off), true));

    // Not deep research: an ordinary web turn is not asked to dig.
    let plain = ToolPreferences {
        web_search: true,
        enabled_tools: Some(vec!["web_search".into()]),
        ..ToolPreferences::default()
    };
    let flags = SearchFlags::from_preferences(Some(&plain));
    assert!(!deep_research_searches_again(flags, Some(&plain), true));
}

#[test]
fn fetch_url_content_is_offered_when_the_turn_already_knows_the_page() {
    let no_allowlist: Option<HashSet<String>> = None;
    // No allowlist at all: still offered, because the conversation carries
    // the page as context and the prompt names this tool for re-reading it.
    assert!(optional_builtin_tool_allowed(
        "fetch_url_content",
        no_allowlist.as_ref(),
        true
    ));
    // The exception covers only re-reading a known page — searching the
    // web stays behind the explicit toggle.
    assert!(!optional_builtin_tool_allowed(
        "web_search",
        no_allowlist.as_ref(),
        true
    ));
    // And without carried pages the old rule applies unchanged.
    assert!(!optional_builtin_tool_allowed(
        "fetch_url_content",
        no_allowlist.as_ref(),
        false
    ));
}

#[test]
fn optional_builtin_tools_require_an_explicit_per_turn_allowlist() {
    let no_preferences = build_optional_tool_allowlist(None);
    assert!(no_preferences.is_none());
    assert!(!no_preferences
        .as_ref()
        .is_some_and(|allowlist| allowlist.contains("fetch_url_content")));

    let preferences = ToolPreferences {
        enabled_tools: Some(vec!["fetch_url_content".to_string()]),
        ..ToolPreferences::default()
    };
    let explicit = build_optional_tool_allowlist(Some(&preferences));
    assert!(explicit
        .as_ref()
        .is_some_and(|allowlist| allowlist.contains("fetch_url_content")));
}

#[test]
fn intent_inference_runs_when_everything_is_on_defaults() {
    let flags = SearchFlags::from_preferences(None);
    assert!(should_infer_turn_intent(None, flags));
    assert!(should_infer_turn_intent(
        Some(&ToolPreferences::default()),
        flags
    ));

    // "auto" (and an empty string) mean the same as unset.
    let auto = ToolPreferences {
        turn_mode: Some("auto".to_string()),
        ..ToolPreferences::default()
    };
    assert!(should_infer_turn_intent(Some(&auto), flags));
    let blank = ToolPreferences {
        turn_mode: Some("  ".to_string()),
        ..ToolPreferences::default()
    };
    assert!(should_infer_turn_intent(Some(&blank), flags));
}

#[test]
fn intent_inference_is_skipped_when_anything_is_explicit() {
    // An explicit turn mode, even one that forces no flag by itself.
    for mode in ["followup", "query"] {
        let prefs = ToolPreferences {
            turn_mode: Some(mode.to_string()),
            ..ToolPreferences::default()
        };
        assert!(!should_infer_turn_intent(
            Some(&prefs),
            SearchFlags::from_preferences(Some(&prefs))
        ));
    }

    // Any single toggle set is enough to stand down.
    for prefs in [
        ToolPreferences {
            knowledge_base: true,
            ..ToolPreferences::default()
        },
        ToolPreferences {
            web_search: true,
            ..ToolPreferences::default()
        },
        ToolPreferences {
            deep_research_mode: true,
            ..ToolPreferences::default()
        },
        ToolPreferences {
            followup_mode: true,
            ..ToolPreferences::default()
        },
        ToolPreferences {
            enabled_tools: Some(vec!["wiki_search".to_string()]),
            ..ToolPreferences::default()
        },
    ] {
        assert!(!should_infer_turn_intent(
            Some(&prefs),
            SearchFlags::from_preferences(Some(&prefs))
        ));
    }

    // A closed book is absolute, and a focus pin counts as an explicit ask.
    let closed = ToolPreferences {
        closed_book: true,
        ..ToolPreferences::default()
    };
    assert!(!should_infer_turn_intent(
        Some(&closed),
        SearchFlags::from_preferences(Some(&closed))
    ));
    let focused = SearchFlags {
        force_kb_search: true,
        ..SearchFlags::from_preferences(None)
    };
    assert!(!should_infer_turn_intent(None, focused));
}

#[test]
fn turn_intent_merge_enables_inferred_retrieval() {
    let flags = SearchFlags::from_preferences(None);
    let intent = TurnIntent {
        needs_knowledge_base: true,
        needs_web: false,
        is_followup: true,
        confidence: 0.9,
    };

    let merged = apply_turn_intent(flags, &intent);

    assert!(merged.force_kb_search);
    assert!(merged.force_followup_mode);
    assert!(!merged.force_web_search);
    assert!(!merged.force_wiki_search);
    assert!(!merged.deep_research_mode);
    assert!(!merged.closed_book);
}

#[test]
fn turn_intent_merge_never_clears_a_set_flag() {
    let every_flag_set = SearchFlags {
        force_kb_search: true,
        force_web_search: true,
        force_wiki_search: true,
        deep_research_mode: true,
        force_followup_mode: true,
        closed_book: false,
    };

    let merged = apply_turn_intent(every_flag_set, &TurnIntent::fallback());

    assert!(merged.force_kb_search);
    assert!(merged.force_web_search);
    assert!(merged.force_wiki_search);
    assert!(merged.deep_research_mode);
    assert!(merged.force_followup_mode);

    // Closed book stays exactly as it was, whatever the model said.
    let closed = SearchFlags {
        closed_book: true,
        ..SearchFlags::from_preferences(None)
    };
    let eager = TurnIntent {
        needs_knowledge_base: true,
        needs_web: true,
        is_followup: true,
        confidence: 1.0,
    };
    let merged = apply_turn_intent(closed, &eager);
    assert!(merged.closed_book);
    assert!(!merged.force_kb_search);
    assert!(!merged.force_web_search);
}
