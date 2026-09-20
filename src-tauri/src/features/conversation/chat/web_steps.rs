//! How a web step is recorded, for both places a turn reaches the web.
//!
//! The retrieval pipeline searches before the model runs and the tool loop
//! searches whenever the model asks to. They are the same act to a reader, and
//! when each recorded it its own way the first search named its pages and the
//! second, third and fourth — the ones a long research turn is mostly made of —
//! were a bare "Searching the web" with nothing under it.

use crate::features::function_calling::dto::{FetchUrlContentOutput, WebSearchOutput};

use super::fetch_memory;
use super::turn_record::{StepGuard, TurnRecorder, TurnStepKind, TurnStepLinkDto};

/// The pages a search handed back, in the order the model will see them.
fn search_links(output: &WebSearchOutput) -> Vec<TurnStepLinkDto> {
    output
        .results
        .iter()
        .map(|result| TurnStepLinkDto::new(result.url.clone(), Some(&result.title)))
        .collect()
}

pub(super) fn result_count_line(count: usize) -> String {
    match count {
        0 => "no results".to_string(),
        1 => "1 result".to_string(),
        many => format!("{many} results"),
    }
}

/// Finish a search with where it led, then account for the searches inside it.
///
/// A deep search runs follow-up queries of its own before it returns. They are
/// over by the time anything can be said about them, so each is noted rather
/// than timed — but each is said, because the results belong to those queries
/// as much as to the one that was asked.
pub(super) fn finish_search(
    step: StepGuard<'_>,
    recorder: &TurnRecorder,
    output: &WebSearchOutput,
) {
    step.done_with_links(
        Some(result_count_line(output.results.len())),
        search_links(output),
    );
    for query in &output.followup_queries {
        recorder.note(
            TurnStepKind::WebSearch,
            "Followed up with another search",
            Some(query.clone()),
            None,
        );
    }
}

/// What a page read came to.
pub(super) fn page_result_line(page: &FetchUrlContentOutput) -> String {
    if page.from_cache {
        // A page that cost no request says so, so the timeline shows the cache
        // working rather than looking implausibly fast.
        format!("{} words · cached", page.word_count)
    } else {
        format!("{} words", page.word_count)
    }
}

/// Finish a page read under the address it was asked for.
///
/// The requested address, not the one a redirect ended at: it is the one the
/// search listed, and the reader matches a page to its search by it.
pub(super) fn finish_page(step: StepGuard<'_>, requested_url: &str, page: &FetchUrlContentOutput) {
    step.done_with_links(
        Some(page_result_line(page)),
        vec![TurnStepLinkDto::new(requested_url, page.title.as_deref())],
    );
}

/// The one argument of a web tool call worth showing, bare — the same form the
/// pipeline records, so a query reads as a query wherever it was issued.
pub(super) fn tool_detail(tool: &str, arguments: &serde_json::Value) -> Option<String> {
    match tool {
        "web_search" => arguments
            .get("query")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        "fetch_url_content" => fetch_memory::fetch_target(arguments).map(str::to_owned),
        _ => None,
    }
}

/// A page read knows its address before it starts.
pub(super) fn links_for_call(tool: &str, arguments: &serde_json::Value) -> Vec<TurnStepLinkDto> {
    match tool {
        "fetch_url_content" => fetch_memory::fetch_target(arguments)
            .map(|url| vec![TurnStepLinkDto::new(url, None)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Finish a successful tool call's step. A web tool says where it went; every
/// other tool finishes as it always did.
pub(super) fn finish_tool_step(
    step: StepGuard<'_>,
    recorder: &TurnRecorder,
    tool: &str,
    arguments: &serde_json::Value,
    data: Option<&serde_json::Value>,
) {
    match (tool, data) {
        ("web_search", Some(data)) => {
            match serde_json::from_value::<WebSearchOutput>(data.clone()) {
                Ok(output) => finish_search(step, recorder, &output),
                Err(_) => step.done(None),
            }
        }
        ("fetch_url_content", Some(data)) => {
            match (
                fetch_memory::fetch_target(arguments),
                serde_json::from_value::<FetchUrlContentOutput>(data.clone()),
            ) {
                (Some(url), Ok(page)) => finish_page(step, url, &page),
                _ => step.done(None),
            }
        }
        _ => step.done(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::conversation::chat::turn_record::TurnStepState;

    fn search_output(followups: &[&str]) -> WebSearchOutput {
        serde_json::from_value(serde_json::json!({
            "results": [
                { "title": "First", "url": "https://a.example/1", "snippet": "" },
                { "title": "Second", "url": "https://b.example/2", "snippet": "" },
            ],
            "query": "root question",
            "result_count": 2,
            "followup_queries": followups,
        }))
        .unwrap()
    }

    /// The model's second search is the one a reader is waiting on. It has to
    /// name its pages exactly as the first one did.
    #[test]
    fn a_search_the_model_asked_for_names_the_pages_it_found() {
        let recorder = TurnRecorder::silent("c", "r");
        let arguments = serde_json::json!({ "query": "root question" });
        let step = recorder.begin_guarded_with_links(
            TurnStepKind::WebSearch,
            "Searching the web",
            tool_detail("web_search", &arguments),
            links_for_call("web_search", &arguments),
        );
        let data = serde_json::to_value(search_output(&[])).unwrap();

        finish_tool_step(step, &recorder, "web_search", &arguments, Some(&data));

        let steps = recorder.steps();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].detail.as_deref(), Some("root question"));
        assert_eq!(steps[0].result.as_deref(), Some("2 results"));
        let urls: Vec<_> = steps[0].links.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(urls, ["https://a.example/1", "https://b.example/2"]);
    }

    /// Deep research searches several times inside one call. Each of those
    /// queries shaped the results, so each is on the record.
    #[test]
    fn every_follow_up_query_inside_a_deep_search_is_on_the_record() {
        let recorder = TurnRecorder::silent("c", "r");
        let step = recorder.begin_guarded(TurnStepKind::WebSearch, "Searching the web", None);

        finish_search(
            step,
            &recorder,
            &search_output(&["root question pricing", "root question reviews"]),
        );

        let steps = recorder.steps();
        let details: Vec<_> = steps.iter().map(|s| s.detail.as_deref()).collect();
        assert_eq!(
            details,
            [
                None,
                Some("root question pricing"),
                Some("root question reviews")
            ]
        );
        assert!(steps
            .iter()
            .all(|s| s.kind == TurnStepKind::WebSearch && s.state == TurnStepState::Done));
    }

    /// The search listed one address and a redirect landed on another. Filing
    /// the read under the second would leave the listed page looking unread.
    #[test]
    fn a_page_read_is_filed_under_the_address_it_was_asked_for() {
        let recorder = TurnRecorder::silent("c", "r");
        let arguments = serde_json::json!({ "url": "https://a.example/1" });
        let step = recorder.begin_guarded_with_links(
            TurnStepKind::ReadPage,
            "Reading a.example",
            tool_detail("fetch_url_content", &arguments),
            links_for_call("fetch_url_content", &arguments),
        );
        let data = serde_json::json!({
            "url": "https://www.a.example/moved/1",
            "title": "First",
            "content": "text",
            "content_truncated": false,
            "word_count": 412,
            "fetch_time_ms": 12.5,
        });

        finish_tool_step(
            step,
            &recorder,
            "fetch_url_content",
            &arguments,
            Some(&data),
        );

        let steps = recorder.steps();
        assert_eq!(steps[0].links[0].url, "https://a.example/1");
        assert_eq!(steps[0].links[0].title.as_deref(), Some("First"));
        assert_eq!(steps[0].result.as_deref(), Some("412 words"));
    }

    #[test]
    fn a_tool_that_is_not_a_web_tool_finishes_as_it_always_did() {
        let recorder = TurnRecorder::silent("c", "r");
        let step = recorder.begin_guarded(TurnStepKind::Tool, "Running something", None);

        finish_tool_step(
            step,
            &recorder,
            "list_documents",
            &serde_json::json!({}),
            None,
        );

        let steps = recorder.steps();
        assert!(steps[0].links.is_empty());
        assert_eq!(steps[0].result, None);
    }
}
