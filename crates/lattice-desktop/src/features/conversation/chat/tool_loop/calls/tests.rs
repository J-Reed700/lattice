use super::*;

fn call(id: &str, name: &str, arguments: serde_json::Value) -> RequestedCall {
    RequestedCall {
        id: id.into(),
        name: name.into(),
        arguments,
    }
}

fn fetch(id: &str, url: &str) -> RequestedCall {
    call(id, "fetch_url_content", serde_json::json!({ "url": url }))
}

/// The registry is where new tools appear. Each has to be placed on one
/// side or the other on purpose.
#[test]
fn every_registered_tool_is_classified() {
    use crate::features::function_calling::trait_def::FunctionRegistryTrait;
    let registry = crate::features::function_calling::registry::init_function_registry().unwrap();
    let mut names: Vec<String> = registry
        .list_tools()
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "fetch_url_content",
            "find_image",
            "find_sources",
            "follow_links",
            "get_document",
            "list_attachments",
            "list_documents",
            "semantic_search",
            "web_search",
            "wiki_search",
            "wiki_summary",
        ],
        "a tool was added or removed: decide whether it only reads, then \
         update `runs_alongside_others` and this list together"
    );
    // `find_image` stores pictures and numbers them after the turn's last.
    assert!(names
        .iter()
        .all(|name| runs_alongside_others(name) == (name != "find_image")));
}

#[test]
fn folder_reads_run_together_and_everything_else_runs_alone() {
    for tool in [
        "read_file",
        "list_directory",
        "search_files",
        "find_files",
        "search_folder",
    ] {
        assert!(runs_alongside_others(tool), "{tool}");
    }
    for tool in [
        history_tools::TOOL_SEARCH_CONVERSATION_HISTORY,
        history_tools::TOOL_READ_CONVERSATION_HISTORY,
        history_tools::TOOL_SEARCH_SAVED_KNOWLEDGE,
        "my_company_wiki",
        "capture_into_journal",
    ] {
        assert!(!runs_alongside_others(tool), "{tool}");
    }
}

#[test]
fn reads_share_a_batch_and_a_tool_that_may_write_stands_alone() {
    let calls = [
        fetch("1", "https://a.example/"),
        call("2", "web_search", serde_json::json!({"query": "x"})),
        call("3", "capture_into_journal", serde_json::json!({})),
        fetch("4", "https://b.example/"),
    ];
    assert_eq!(batch_end(&calls, 0), 2);
    assert_eq!(batch_end(&calls, 2), 3);
    assert_eq!(batch_end(&calls, 3), 4);
}

/// Two reads of one page in one round: the second is answered from the first.
#[test]
fn a_second_read_of_a_page_waits_for_the_first() {
    let calls = [
        fetch("1", "https://a.example/story"),
        fetch("2", "https://b.example/"),
        fetch("3", "https://A.example/story#top"),
    ];
    assert_eq!(batch_end(&calls, 0), 2);
    assert_eq!(batch_end(&calls, 2), 3);
}

/// Stop is honoured while a tool is still running, not after it returns.
#[tokio::test]
async fn a_stopped_turn_does_not_wait_for_a_pending_tool() {
    let started = Instant::now();
    let outcome = run_tool_bounded(
        std::future::pending::<()>(),
        Duration::from_secs(600),
        tokio::time::sleep(Duration::from_millis(20)),
    )
    .await;
    assert!(matches!(outcome, ToolRun::Cancelled));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn a_tool_that_outlives_the_turn_is_cut_off_at_the_deadline() {
    let outcome = run_tool_bounded(
        std::future::pending::<()>(),
        Duration::from_millis(20),
        std::future::pending::<()>(),
    )
    .await;
    assert!(matches!(outcome, ToolRun::TimedOut));
    let outcome = run_tool_bounded(
        async { 7 },
        Duration::from_secs(1),
        std::future::pending::<()>(),
    )
    .await;
    assert!(matches!(outcome, ToolRun::Finished(7)));
}

/// A part is sized to each call's own room, with no size locked in from an
/// earlier read: parts are addressed by offset, so a larger part after a
/// smaller one carries on where the smaller one stopped instead of rereading.
#[test]
fn a_document_part_follows_each_calls_room_and_keeps_its_offset() {
    let args = serde_json::json!({"document_id": "doc", "offset": 4_000});
    let large = document_part_sized_to(&args, 400_000);
    let small = document_part_sized_to(&args, 700);
    let size = |value: &serde_json::Value| value["max_content_length"].as_u64().unwrap();
    assert!(
        size(&large) > 100_000,
        "a large room is not held to an old maximum"
    );
    assert!(size(&small) < 1_000, "a small room still reads something");
    assert!(size(&small) >= 1);
    assert_eq!(large["offset"], 4_000);
    assert_eq!(
        size(&document_part_sized_to(&large, 400_000)),
        size(&large),
        "nothing is remembered between calls"
    );
    let asked_less = serde_json::json!({"document_id": "doc", "max_content_length": 50});
    assert_eq!(size(&document_part_sized_to(&asked_less, 400_000)), 50);
}
