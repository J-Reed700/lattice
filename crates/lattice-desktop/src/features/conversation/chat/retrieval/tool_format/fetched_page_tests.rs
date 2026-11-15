use super::*;
use lattice_core::application::contracts::tools::FunctionResult;
use lattice_core::application::ports::turn_source::SourcePassage;
use lattice_core::application::services::tool_handles::ToolHandles;

fn page(content: &str) -> FunctionResult {
    FunctionResult::success(
        serde_json::to_value(FetchUrlContentOutput {
            url: "https://example.test/recap".to_string(),
            title: Some("Silo Season 2, Explained".to_string()),
            content: content.to_string(),
            content_truncated: false,
            word_count: content.split_whitespace().count(),
            fetch_time_ms: 137.67,
            content_type: Some("text/html".to_string()),
            from_cache: false,
            read_via: Default::default(),
        })
        .unwrap_or_default(),
    )
}

fn render(result: &FunctionResult, tool: &str, allowance: usize, offset: usize) -> String {
    let handles = ToolHandles::new();
    let arguments = json!({});
    format_tool_result(
        tool,
        result,
        &ToolOutputSettingsDto::default(),
        allowance,
        offset,
        false,
        &HashMap::new(),
        &CallView::new(tool, &arguments, &handles),
    )
    .text
}

/// As JSON the article's own punctuation came through escaped.
#[test]
fn a_fetched_page_reads_as_prose_not_as_escaped_json() {
    let article = "Bernard said \"better tape\".\nThat was the mistake.";
    let rendered = render(&page(article), "fetch_url_content", 50_000, 0);

    assert!(
        rendered.contains("Bernard said \"better tape\".\nThat was the mistake."),
        "{rendered}"
    );
    assert!(!rendered.contains("\\\""), "{rendered}");
    assert!(!rendered.contains("fetch_time_ms"), "{rendered}");
    assert!(rendered.contains("Silo Season 2, Explained"), "{rendered}");
    assert!(
        rendered.contains("https://example.test/recap"),
        "{rendered}"
    );
    assert!(rendered.contains("complete"), "{rendered}");
}

/// The window-derived allowance alone binds; there is no configured ceiling.
#[test]
fn a_page_far_past_the_old_ceiling_is_whole_when_the_allowance_holds_it() {
    let article = format!("{}the last line", "word ".repeat(40_000));
    let rendered = render(&page(&article), "fetch_url_content", 400_000, 0);
    assert!(rendered.contains("complete"), "whole page expected");
    assert!(rendered.ends_with("the last line"));
}

#[test]
fn a_page_longer_than_its_room_says_where_the_next_part_starts() {
    let article = "word ".repeat(2_000);
    let rendered = render(&page(&article), "fetch_url_content", 3_000, 0);

    let room = fetched_page_text_room(3_000);
    assert!(
        rendered.contains(&format!("the first {room} characters")),
        "{rendered}"
    );
    assert!(
        rendered.contains("To read on, call read_more with [m1]."),
        "{rendered}"
    );
    assert!(!rendered.contains("offset"), "{rendered}");
    assert!(!rendered.contains("complete"), "{rendered}");
    assert!(
        rendered.chars().count() <= 3_000,
        "got {}",
        rendered.chars().count()
    );
}

/// Reading a page on from the offset the last part named reaches its
/// tail: the parts together are the whole page, nothing skipped.
#[test]
fn reading_on_by_offset_reaches_the_end_of_the_page() {
    let article: String = (0..3_000).map(|i| format!("w{i} ")).collect();
    let total = article.trim().chars().count();
    let room = fetched_page_text_room(2_000);
    let mut offset = 0;
    let mut parts = 0;
    while offset < total {
        let rendered = render(&page(&article), "fetch_url_content", 2_000, offset);
        assert!(rendered.chars().count() <= 2_000);
        parts += 1;
        offset += room;
    }
    let last = render(&page(&article), "fetch_url_content", 2_000, offset - room);
    assert!(last.contains("the end of the page"), "{last}");
    assert!(last.ends_with(article.trim().split(' ').next_back().unwrap()));
    assert!(parts > 1);
}

fn search_output(documents: usize, matches: usize, excerpt: &str) -> FunctionResult {
    use lattice_core::application::contracts::tools::DocumentResult;
    let results = (0..documents)
        .flat_map(|d| (0..matches).map(move |m| (d, m)))
        .map(|(d, m)| DocumentResult {
            document_id: format!("doc-{d}"),
            filename: format!("file-{d}.pdf"),
            file_path: format!("/vault/file-{d}.pdf"),
            mime_type: "application/pdf".to_string(),
            score: 0.5,
            snippet: format!("{excerpt} ({d}/{m})"),
            chunk_index: Some(m),
            chunk_id: Some(format!("chunk-{d}-{m}")),
            modified_at: chrono::Utc::now(),
            size_bytes: 1,
        })
        .collect();
    FunctionResult::success(
        serde_json::to_value(SemanticSearchOutput {
            results,
            documents: Vec::new(),
            total_found: 0,
            search_time_ms: 1.0,
            query: "q".to_string(),
            next_offset: None,
        })
        .unwrap_or_default(),
    )
}

fn source_passage(chunk: &str, content: &str) -> SourcePassage {
    let source = serde_json::from_value(serde_json::json!({
        "documentId": "course:version-1",
        "chunkId": chunk,
        "content": content,
        "score": 1.0,
        "fileName": "Search notes",
        "filePath": "",
        "mimeType": "text/plain",
        "category": "Course source",
        "fileSizeBytes": 0,
        "modifiedAt": "",
    }))
    .unwrap();
    SourcePassage {
        label: format!("Ranking › Fusion › Search notes (source_id: {chunk})"),
        source,
    }
}

fn source_result(passages: Vec<SourcePassage>, note: Option<&str>) -> FunctionResult {
    FunctionResult::success(
        serde_json::to_value(SourcePassages {
            passages,
            note: note.map(str::to_string),
        })
        .unwrap(),
    )
}

fn render_source(
    result: &FunctionResult,
    allowance: usize,
    citations: &HashMap<String, u32>,
) -> String {
    let handles = ToolHandles::new();
    let arguments = json!({});
    format_tool_result(
        "search_course",
        result,
        &ToolOutputSettingsDto::default(),
        allowance,
        0,
        true,
        citations,
        &CallView::new("search_course", &arguments, &handles),
    )
    .text
}

/// Each passage reads under the turn's number for it, so the model cites
/// it as it cites a library passage, and what closes the result stays.
#[test]
fn a_source_s_passages_are_shown_under_the_turn_s_numbers() {
    let citations = HashMap::from([("a".to_owned(), 7), ("b".to_owned(), 8)]);
    let result = source_result(
        vec![
            source_passage("a", "Fusion sums reciprocal ranks."),
            source_passage("b", "A larger k flattens them."),
        ],
        Some("Call read_course_source to read on."),
    );

    let rendered = render_source(&result, 4_000, &citations);

    assert!(
        rendered.contains(
            "[s7] Ranking › Fusion › Search notes (source_id: a)\nFusion sums reciprocal ranks."
        ),
        "{rendered}"
    );
    assert!(rendered.contains("[s8] Ranking"), "{rendered}");
    assert!(
        rendered.ends_with("\n[Call read_course_source to read on.]"),
        "{rendered}"
    );
}

/// With no room for every passage, the note still closes the result and
/// says how many were left out.
#[test]
fn a_source_s_passages_past_the_allowance_are_counted() {
    let long = "word ".repeat(400);
    let result = source_result(
        vec![source_passage("a", &long), source_passage("b", &long)],
        Some("Read on."),
    );
    let citations = HashMap::from([("a".to_owned(), 1), ("b".to_owned(), 2)]);

    let rendered = render_source(&result, 2_800, &citations);

    assert!(rendered.chars().count() <= 2_800, "{}", rendered.len());
    assert!(
        rendered.contains("1 more result(s) did not fit"),
        "{rendered}"
    );
    assert!(rendered.ends_with("[Read on.]"), "{rendered}");
}

#[test]
fn a_source_with_nothing_found_says_why() {
    let result = source_result(Vec::new(), Some("No passage matched."));
    assert_eq!(
        render_source(&result, 4_000, &HashMap::new()),
        "[No passage matched.]"
    );
}

/// A folder's tools return plain text, and it stays what they wrote.
#[test]
fn a_source_s_plain_text_is_left_as_it_is() {
    let result = FunctionResult::success(serde_json::Value::String("  1 fn main() {}".into()));
    assert_eq!(
        render_source(&result, 4_000, &HashMap::new()),
        "  1 fn main() {}"
    );
}

/// A number of the result's own would read as the passage the turn holds
/// under that number, and the citation would point at the wrong document.
#[test]
fn a_library_result_is_shown_under_the_turn_s_number_for_it() {
    let citations = HashMap::from([("chunk-0-0".to_owned(), 41), ("chunk-1-0".to_owned(), 7)]);

    let handles = ToolHandles::new();
    let arguments = json!({"query": "q"});
    let rendered = format_tool_result(
        "semantic_search",
        &search_output(2, 1, "text"),
        &ToolOutputSettingsDto::default(),
        4_000,
        0,
        false,
        &citations,
        &CallView::new("semantic_search", &arguments, &handles),
    )
    .text;

    assert!(rendered.contains("[s41] file-0.pdf"), "{rendered}");
    assert!(rendered.contains("[s7] file-1.pdf"), "{rendered}");
    assert!(!rendered.contains("[s1]"), "{rendered}");
}

/// A search page that may have more names the point the next starts at,
/// and that name runs the same search from the next result.
#[test]
fn a_full_search_page_names_the_point_of_the_next() {
    let mut result = search_output(2, 1, "text");
    if let Some(data) = result.data.as_mut() {
        data["next_offset"] = serde_json::json!(20);
    }
    let handles = ToolHandles::new();
    let arguments = json!({"query": "q"});
    let rendered = format_tool_result(
        "semantic_search",
        &result,
        &ToolOutputSettingsDto::default(),
        4_000,
        0,
        false,
        &HashMap::new(),
        &CallView::new("semantic_search", &arguments, &handles),
    )
    .text;
    assert!(
        rendered.contains("To read on, call read_more with [m1]."),
        "{rendered}"
    );
    assert!(!rendered.contains("offset"), "{rendered}");
    let resumed = handles
        .read_call("read_more", &json!({"handle": "[m1]"}))
        .unwrap();
    assert_eq!(resumed.tool, "semantic_search");
    assert_eq!(resumed.arguments, json!({"query": "q", "offset": 20}));
    assert!(
        !render(&search_output(2, 1, "text"), "semantic_search", 4_000, 0).contains("read_more")
    );
}

/// A search result names each document by a handle and shows no id.
#[test]
fn a_search_result_shows_documents_by_name_and_never_by_id() {
    let rendered = render(&search_output(2, 1, "text"), "semantic_search", 4_000, 0);
    assert!(rendered.contains("file-0.pdf [d1] (score"), "{rendered}");
    assert!(rendered.contains("file-1.pdf [d2] (score"), "{rendered}");
    assert!(!rendered.contains("doc-0"), "{rendered}");
    assert!(!rendered.contains("doc_id"), "{rendered}");
}

/// A web search with more results says how to read them, once.
#[test]
fn a_web_search_with_more_results_names_the_next_page() {
    let output = lattice_core::application::contracts::web::WebSearchOutput {
        results: Vec::new(),
        query: "tide tables".to_owned(),
        result_count: 0,
        page: 2,
        offset: 0,
        total_results: 30,
        has_more: true,
        ..serde_json::from_value(json!({
            "results": [], "query": "tide tables", "result_count": 0
        }))
        .unwrap()
    };
    let handles = ToolHandles::new();
    let arguments = json!({"query": "tide tables"});
    let rendered = format_tool_result(
        "web_search",
        &FunctionResult::success(serde_json::to_value(output).unwrap()),
        &ToolOutputSettingsDto::default(),
        4_000,
        0,
        false,
        &HashMap::new(),
        &CallView::new("web_search", &arguments, &handles),
    )
    .text;
    assert!(rendered.contains("call read_more with [m1]"), "{rendered}");
    let resumed = handles
        .read_call("read_more", &json!({"handle": "m1"}))
        .unwrap();
    assert_eq!(
        resumed.arguments,
        json!({"query": "tide tables", "page": 3})
    );
}

/// With room, every match is shown whole.
#[test]
fn every_search_match_is_shown_whole_when_the_allowance_holds_them() {
    let excerpt = "a long passage ".repeat(400);
    let result = search_output(3, 6, excerpt.trim());
    let rendered = render(&result, "semantic_search", 1_000_000, 0);
    assert!(
        rendered.contains("(2/5)"),
        "the sixth match of the third document"
    );
    assert!(rendered.contains(excerpt.trim()), "excerpts are not cut");
    assert!(!rendered.contains("did not fit"), "{rendered}");
}

/// Without room for all of them, matches are shown until the allowance is
/// spent and the result says how many were left out.
#[test]
fn search_matches_past_the_allowance_are_counted_not_silently_dropped() {
    let result = search_output(10, 2, &"text ".repeat(200));
    let rendered = render(&result, "semantic_search", 4_000, 0);
    assert!(rendered.chars().count() <= 4_000);
    assert!(
        rendered.contains("more result(s) did not fit"),
        "{rendered}"
    );
    assert!(rendered.contains("- file-0.pdf"), "{rendered}");
}

/// Every kind of result keeps the line saying what was left out and how to
/// reach it, however full the window.
#[test]
fn a_result_in_a_full_window_still_says_what_was_left_out() {
    let page_note = render(&page(&"word ".repeat(5_000)), "fetch_url_content", 0, 120);
    assert!(page_note.contains("characters 120 to"), "{page_note}");
    assert!(page_note.contains("call read_more with"), "{page_note}");

    let document = FunctionResult::success(serde_json::json!({
        "document_id": "doc-7", "content": "x".repeat(5_000),
        "content_truncated": true, "offset": 0, "next_offset": 5_000,
        "total_chars": 9_000
    }));
    let document_note = render(&document, "get_document", 0, 0);
    assert!(document_note.contains("[d1]"), "{document_note}");
    assert!(!document_note.contains("doc-7"), "{document_note}");
    assert!(
        document_note.contains("call read_more with"),
        "{document_note}"
    );
    assert!(document_note.chars().count() <= MIN_RESULT_CHARS);

    let search = render(
        &search_output(10, 2, &"text ".repeat(200)),
        "semantic_search",
        0,
        0,
    );
    assert!(search.contains("did not fit"), "{search}");

    let fallback = FunctionResult::success(serde_json::json!({"rows": "y".repeat(5_000)}));
    let fallback_note = render(&fallback, "list_documents", 0, 0);
    assert!(fallback_note.contains("[Cut to fit"), "{fallback_note}");
}

/// A part larger than the result's room names the point the rest starts at.
#[test]
fn an_oversized_document_part_names_the_point_to_read_on_from() {
    let document = FunctionResult::success(serde_json::json!({
        "document_id": "doc-7", "content": "§".repeat(5_000),
        "content_truncated": false, "offset": 1_000, "total_chars": 6_000
    }));
    let handles = ToolHandles::new();
    let arguments = json!({"document_id": "doc-7"});
    let rendered = format_tool_result(
        "get_document",
        &document,
        &ToolOutputSettingsDto::default(),
        2_000,
        0,
        false,
        &HashMap::new(),
        &CallView::new("get_document", &arguments, &handles),
    )
    .text;
    assert!(rendered.chars().count() <= 2_000);
    let shown = rendered.matches('§').count();
    let resumed = handles
        .read_call("read_more", &json!({"handle": "[m1]"}))
        .unwrap();
    assert_eq!(resumed.arguments["offset"], json!(1_000 + shown));
    assert!(!rendered.contains("offset"), "{rendered}");
}

fn render_labelled(
    result: &FunctionResult,
    tool: &str,
    source_text: bool,
    allowance: usize,
    citations: &HashMap<String, u32>,
) -> Rendered {
    let handles = ToolHandles::new();
    let arguments = json!({"query": "q"});
    format_tool_result(
        tool,
        result,
        &ToolOutputSettingsDto::default(),
        allowance,
        0,
        source_text,
        citations,
        &CallView::new(tool, &arguments, &handles),
    )
}

fn line_and_body<'a>(rendered: &'a Rendered, at: usize) -> (&'a str, &'a str) {
    let passage = &rendered.passages[at];
    (
        rendered.text.get(passage.line.clone()).unwrap_or_default(),
        rendered.text.get(passage.body.clone()).unwrap_or_default(),
    )
}

#[test]
fn a_search_result_says_where_each_cited_hit_s_text_is() {
    let citations = HashMap::from([("chunk-0-0".to_owned(), 41)]);
    let rendered = render_labelled(
        &search_output(2, 1, "text"),
        "semantic_search",
        false,
        4_000,
        &citations,
    );
    assert_eq!(rendered.passages.len(), 1, "{}", rendered.text);
    assert_eq!(
        line_and_body(&rendered, 0),
        (
            "[s41] file-0.pdf [d1] (score: 0.500)",
            "Excerpt: text (0/0)"
        )
    );
    assert_eq!(
        rendered.passages[0].heading,
        "[s41] file-0.pdf [d1] (score: 0.500)"
    );
    assert!(rendered.text.contains("file-1.pdf [d2]"));
}

#[test]
fn the_ranges_still_cut_each_hit_after_the_result_is_cut_for_length() {
    let citations: HashMap<String, u32> = (0..10)
        .flat_map(|d| (0..2).map(move |m| (format!("chunk-{d}-{m}"), 1 + d * 2 + m)))
        .collect();
    let rendered = render_labelled(
        &search_output(10, 2, &"text ".repeat(200)),
        "semantic_search",
        false,
        4_000,
        &citations,
    );
    assert!(rendered.text.contains("more result(s) did not fit"));
    assert!(!rendered.passages.is_empty());
    for at in 0..rendered.passages.len() {
        let (line, body) = line_and_body(&rendered, at);
        assert_eq!(line, rendered.passages[at].heading);
        assert!(body.starts_with("Excerpt: text text"), "{body}");
        assert!(!body.contains("[s"), "{body}");
    }
}

#[test]
fn a_cut_first_hit_keeps_the_range_of_what_is_shown() {
    let citations = HashMap::from([("chunk-0-0".to_owned(), 3)]);
    let rendered = render_labelled(
        &search_output(1, 1, &"word ".repeat(2_000)),
        "semantic_search",
        false,
        MIN_RESULT_CHARS,
        &citations,
    );
    assert!(rendered.text.chars().count() <= MIN_RESULT_CHARS);
    let (line, body) = line_and_body(&rendered, 0);
    assert_eq!(line, "[s3] file-0.pdf [d1] (score: 0.500)");
    assert!(body.starts_with("Excerpt: word word"), "{body}");
    assert!(rendered.text[rendered.passages[0].body.end..]
        .starts_with("\n[This result was cut to fit the model's context.]"));
}

#[test]
fn a_source_s_passage_is_one_labelled_text_and_an_unlabelled_one_is_none() {
    let citations = HashMap::from([("a".to_owned(), 7)]);
    let result = source_result(
        vec![
            source_passage("a", "Fusion sums reciprocal ranks."),
            source_passage("b", "A larger k flattens them."),
        ],
        Some("Call read_course_source to read on."),
    );
    let rendered = render_labelled(&result, "search_course", true, 4_000, &citations);
    assert_eq!(rendered.passages.len(), 1);
    assert_eq!(
        line_and_body(&rendered, 0),
        (
            "[s7] Ranking › Fusion › Search notes (source_id: a)",
            "Fusion sums reciprocal ranks."
        )
    );
    assert!(rendered
        .text
        .ends_with("\n[Call read_course_source to read on.]"));
}
