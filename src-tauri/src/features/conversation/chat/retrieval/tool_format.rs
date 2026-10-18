use lattice_core::application::contracts::settings::ToolOutputSettingsDto;
use lattice_core::application::contracts::tools::FunctionResult;
use lattice_core::application::contracts::tools::{
    CustomQueryToolOutput, GetDocumentOutput, SemanticSearchOutput, WikiSearchOutput,
    WikiSummaryOutput,
};
use lattice_core::application::contracts::web::{FetchUrlContentOutput, WebSearchOutput};
use lattice_core::shared::text::safe_truncate;

/// Room kept at the end of a result for the line saying what had to be left
/// out of it, so that line always fits.
const OMISSION_NOTE_CHARS: usize = 240;

/// The least a result is given, however full the window: room for the note
/// saying what was left out and where it is. Without it a full window turned a
/// result into an empty string that told the model nothing.
const MIN_RESULT_CHARS: usize = 2 * OMISSION_NOTE_CHARS;

/// A tool's result as the model reads it, in at most `allowance` characters.
///
/// `allowance` is the result's share of what is left of the model's window
/// (see the tool loop's `tool_result_allowance`); there is no configured
/// ceiling on top of it. Everything the tool returned is shown when it fits.
/// When it does not, the result says what was left out and how to reach it,
/// so a long result is read in parts rather than silently cut. `page_offset`
/// is where a fetched page's text starts, for a model reading a page on.
pub(super) fn format_tool_result(
    tool_name: &str,
    result: &FunctionResult,
    settings: &ToolOutputSettingsDto,
    allowance: usize,
    page_offset: usize,
) -> String {
    let allowance = allowance.max(MIN_RESULT_CHARS);
    if !result.success {
        return format!(
            "Error: {}",
            result.error_message.as_deref().unwrap_or("Unknown error")
        );
    }

    let data = match result.data.clone() {
        Some(value) => value,
        None => return "No data returned.".to_string(),
    };

    // The folder tools already fit their text to this call's allowance, cut at
    // a line and saying where to read on. Through the JSON fallback below the
    // numbered lines would arrive as one escaped string.
    if lattice_core::application::contracts::tools::explorer::is_explorer_tool(tool_name) {
        if let Some(text) = data.as_str() {
            return safe_truncate(text, allowance);
        }
    }
    let templates = &settings.templates;

    match tool_name {
        "get_document" => {
            if let Ok(doc) = serde_json::from_value::<GetDocumentOutput>(data.clone()) {
                let title = doc
                    .metadata
                    .as_ref()
                    .map(|m| m.filename.as_str())
                    .unwrap_or("Unknown");
                let chunk_count = doc.metadata.as_ref().map(|m| m.chunk_count).unwrap_or(0);
                // The part as the tool returned it. The tool loop sizes a part
                // to this call's allowance before it runs, so the part is whole
                // here and the next one carries on from `next_offset`.
                let truncated_note = if doc.content_truncated {
                    "\n(Note: the document continues past this part)"
                } else {
                    ""
                };
                let shown_end = doc.offset + doc.content.chars().count();
                let mut pagination_line = String::new();
                if doc.offset > 0 || doc.next_offset.is_some() {
                    pagination_line = format!(
                        "\nCharacters {} to {shown_end} of {}",
                        doc.offset, doc.total_chars
                    );
                    if let Some(next_offset) = doc.next_offset {
                        pagination_line.push_str(&format!(
                            "\nTo continue, call get_document with document_id=\"{}\" and offset={next_offset}.",
                            doc.document_id
                        ));
                    }
                }
                let render = |excerpt: &str| {
                    let mut rendered = templates
                        .get_document_template
                        .replace("{document_id}", &doc.document_id)
                        .replace("{title}", title)
                        .replace("{chunk_count}", &chunk_count.to_string())
                        .replace("{excerpt}", excerpt)
                        .replace("{truncated}", truncated_note);
                    rendered.push_str(&pagination_line);
                    rendered
                };
                let rendered = render(&doc.content);
                if rendered.chars().count() <= allowance {
                    return rendered;
                }
                // A part larger than this call's room: the model asked for more
                // than the window holds. Show what fits and say where the rest
                // starts, so it is the next read rather than lost.
                let framing = render("").chars().count() + OMISSION_NOTE_CHARS;
                let room = allowance.saturating_sub(framing);
                let shown = safe_truncate(&doc.content, room);
                let next = doc.offset + shown.chars().count();
                let mut rendered = render(&shown);
                rendered.push_str(&format!(
                    "\n[Only characters {} to {next} of this part fit this result. Call get_document with document_id=\"{}\" and offset={next} to read on.]",
                    doc.offset, doc.document_id
                ));
                return keep_note(&rendered, allowance);
            }
        }
        "semantic_search" => {
            if let Ok(output) = serde_json::from_value::<SemanticSearchOutput>(data.clone()) {
                let blocks: Vec<String> = if !output.documents.is_empty() {
                    output
                        .documents
                        .iter()
                        .enumerate()
                        .map(|(idx, doc)| {
                            let mut block = format!(
                                "[{}] {} (doc_id: {}, best_score: {:.3}, matches: {})",
                                idx + 1,
                                doc.filename,
                                doc.document_id,
                                doc.max_score,
                                doc.match_count
                            );
                            for (match_idx, evidence) in doc.matches.iter().enumerate() {
                                let chunk_label = evidence
                                    .chunk_index
                                    .map(|v| format!("#{}", v))
                                    .unwrap_or_else(|| "n/a".to_string());
                                block.push_str(&format!(
                                    "\n  [{}] chunk {} score {:.3}: {}",
                                    match_idx + 1,
                                    chunk_label,
                                    evidence.score,
                                    evidence.excerpt.trim()
                                ));
                            }
                            block
                        })
                        .collect()
                } else {
                    output
                        .results
                        .iter()
                        .enumerate()
                        .map(|(idx, res)| {
                            format!(
                                "[{}] {} (doc_id: {}, score: {:.3})\nExcerpt: {}",
                                idx + 1,
                                res.filename,
                                res.document_id,
                                res.score,
                                res.snippet.trim()
                            )
                        })
                        .collect()
                };
                if blocks.is_empty() {
                    return "Search returned no results.".to_string();
                }
                let template = |results: &str, shown: usize| {
                    templates
                        .semantic_search_template
                        .replace("{results}", results)
                        .replace("{total_found}", &output.total_found.to_string())
                        .replace("{shown}", &shown.to_string())
                };
                let framing = template("", blocks.len()).chars().count();
                // Results past this page are a call away, and the result says
                // which: without an offset, the 51st hit could never be read.
                let more = output.next_offset.map_or_else(String::new, |next| {
                    format!(
                        "\n[More results may follow: call semantic_search with the same query and offset={next}.]"
                    )
                });
                let (results_text, shown) = fit_blocks(
                    &blocks,
                    allowance.saturating_sub(framing + more.chars().count()),
                    "Call get_document with a doc_id above to read a document, or search again more narrowly.",
                );
                return keep_note(
                    &format!("{}{more}", template(&results_text, shown)),
                    allowance,
                );
            }
        }
        "wiki_search" => {
            if let Ok(output) = serde_json::from_value::<WikiSearchOutput>(data.clone()) {
                if output.results.is_empty() {
                    return "Wikipedia search returned no results.".to_string();
                }
                let blocks: Vec<String> = output
                    .results
                    .iter()
                    .enumerate()
                    .map(|(idx, item)| {
                        format!(
                            "[{}] {} ({})\nSnippet: {}",
                            idx + 1,
                            item.title,
                            item.url,
                            item.snippet.trim()
                        )
                    })
                    .collect();
                let (text, _) = fit_blocks(
                    &blocks,
                    allowance,
                    "Search again more narrowly to see them.",
                );
                return keep_note(&text, allowance);
            }
        }
        "wiki_summary" => {
            if let Ok(output) = serde_json::from_value::<WikiSummaryOutput>(data.clone()) {
                let rendered = format!(
                    "{} ({})\nLanguage: {}\nSummary: {}",
                    output.title, output.url, output.language, output.extract
                );
                return fit_text(&rendered, allowance);
            }
        }
        "fetch_url_content" => {
            if let Ok(page) = serde_json::from_value::<FetchUrlContentOutput>(data.clone()) {
                return render_fetched_page(&page, allowance, page_offset);
            }
        }
        "web_search" => {
            if let Ok(output) = serde_json::from_value::<WebSearchOutput>(data.clone()) {
                let heading = format!(
                    "Web results for \"{}\" ({}):",
                    output.query,
                    output.results.len()
                );
                let blocks: Vec<String> = output
                    .results
                    .iter()
                    .enumerate()
                    .map(|(index, result)| {
                        format!(
                            "[{}] {}\nURL: {}\n{}",
                            index + 1,
                            result.title,
                            result.url,
                            result.snippet.trim()
                        )
                    })
                    .collect();
                let (text, _) = fit_blocks(
                    &blocks,
                    allowance.saturating_sub(heading.chars().count() + 2),
                    "Search again more narrowly to see them.",
                );
                return keep_note(&format!("{heading}\n\n{text}"), allowance);
            }
        }
        _ => {}
    }

    if let Ok(output) = serde_json::from_value::<CustomQueryToolOutput>(data.clone()) {
        let rendered = format!(
            "Custom tool '{}' response from {}:\n{}",
            output.tool_name,
            output.request_url,
            serde_json::to_string(&output.data).unwrap_or_else(|_| "{}".to_string())
        );
        return fit_text(&rendered, allowance);
    }

    let fallback = serde_json::to_string(&data).unwrap_or_else(|_| "null".to_string());
    let rendered = templates.default_template.replace("{json}", &fallback);
    fit_text(&rendered, allowance)
}

/// `blocks` joined by blank lines, as many from the front as fit `room`
/// characters, and how many that was. When some do not fit, a closing line
/// says how many were left out and `how_to_reach` says how to see them: the
/// count comes from the allowance, and the model is told so.
fn fit_blocks(blocks: &[String], room: usize, how_to_reach: &str) -> (String, usize) {
    let room_for_blocks = room.saturating_sub(OMISSION_NOTE_CHARS);
    let mut text = String::new();
    let mut used = 0usize;
    let mut shown = 0usize;
    for block in blocks {
        let cost = block.chars().count() + if shown > 0 { 2 } else { 0 };
        let fits_whole_result = shown + 1 == blocks.len() && used + cost <= room;
        if used + cost > room_for_blocks && !fits_whole_result {
            break;
        }
        if shown > 0 {
            text.push_str("\n\n");
        }
        text.push_str(block);
        used += cost;
        shown += 1;
    }
    if shown == 0 {
        if let Some(first) = blocks.first() {
            // Not even one whole result fits: show the start of the best one
            // rather than nothing, and say it was cut.
            text = safe_truncate(first, room_for_blocks);
            shown = 1;
            text.push_str("\n[This result was cut to fit the model's context.]");
        }
    }
    let left_out = blocks.len().saturating_sub(shown);
    if left_out > 0 {
        text.push_str(&format!(
            "\n\n[{left_out} more result(s) did not fit in the room this result has in the model's context. {how_to_reach}]"
        ));
    }
    (text, shown)
}

/// `text` whole when it fits `allowance`, otherwise its start with a closing
/// line saying how much was shown. Never silent.
fn fit_text(text: &str, allowance: usize) -> String {
    let total = text.chars().count();
    if total <= allowance {
        return text.to_string();
    }
    let room = allowance.saturating_sub(OMISSION_NOTE_CHARS);
    let mut shown = safe_truncate(text, room);
    shown.push_str(&format!(
        "\n[Cut to fit the model's context: the first {room} of {total} characters are shown.]"
    ));
    keep_note(&shown, allowance)
}

/// `text` within `allowance`, cutting its body rather than the closing note
/// that says what was left out, so a result never loses the line telling the
/// model where the rest is.
fn keep_note(text: &str, allowance: usize) -> String {
    if text.chars().count() <= allowance {
        return text.to_string();
    }
    let Some(at) = text.rfind("\n[") else {
        return safe_truncate(text, allowance);
    };
    let note = &text[at..];
    let body = safe_truncate(&text[..at], allowance.saturating_sub(note.chars().count()));
    format!("{body}{note}")
}

/// Room kept for a fetched page's title, URL and the line saying how much of
/// it is shown, so that the text is what gets cut and never the framing.
const FETCHED_PAGE_FRAMING_CHARS: usize = 400;

/// How many characters of `page`'s text fit a result of `max_chars`.
///
/// Shared with the tool loop, which records this figure so that a later request
/// for the same page can be given exactly the part that was left out.
pub(in crate::features::conversation::chat) fn fetched_page_text_room(max_chars: usize) -> usize {
    max_chars.saturating_sub(FETCHED_PAGE_FRAMING_CHARS)
}

/// Room kept around a document page for its template, title and the lines
/// saying which page it is and how to read on.
const DOCUMENT_PAGE_FRAMING_CHARS: usize = 600;

/// How many characters of a document page fit a result of `allowance`. The
/// tool loop sizes `get_document` pages to this, so a page is shown whole.
pub(in crate::features::conversation::chat) fn document_page_room(allowance: usize) -> usize {
    allowance.saturating_sub(DOCUMENT_PAGE_FRAMING_CHARS)
}

/// A fetched page as prose, from character `offset` on, as much as a result
/// of `max_chars` holds.
///
/// It used to reach the model through the generic fallback: the whole output
/// struct as one line of JSON, every quotation mark and line break in the
/// article escaped, with the fetch timing and content type along for the ride.
/// That spends tokens on backslashes and makes an article harder to read than
/// it was on the page. A page longer than the result's room is shown in part,
/// with the offset the next part starts at, so the rest is a further call
/// rather than lost.
fn render_fetched_page(page: &FetchUrlContentOutput, max_chars: usize, offset: usize) -> String {
    let content = page.content.trim();
    let room = fetched_page_text_room(max_chars);
    let total = content.chars().count();
    let start = offset.min(total);
    let shown: String = content.chars().skip(start).take(room).collect();
    let end = start + shown.chars().count();
    let extent = if end == start && start < total {
        format!(
            "{} words, {total} characters; the model's context has no room left for this page's text — characters {start} to {total} are unread; call fetch_url_content with this url and offset={start} once there is room",
            page.word_count
        )
    } else if start == 0 && end >= total {
        format!(
            "{} words — complete; this is the whole page",
            page.word_count
        )
    } else if end >= total {
        format!(
            "{} words, {total} characters; characters {start} to {total} are shown, the end of the page",
            page.word_count
        )
    } else if start == 0 {
        format!(
            "{} words, {total} characters; the first {end} characters are shown, as much as this result has room for — call fetch_url_content with this url and offset={end} for the next part",
            page.word_count
        )
    } else {
        format!(
            "{} words, {total} characters; characters {start} to {end} are shown, as much as this result has room for — call fetch_url_content with this url and offset={end} for the next part",
            page.word_count
        )
    };
    format!(
        "{}\nURL: {}\n({extent})\n\n{}",
        page.title.as_deref().unwrap_or("Untitled page"),
        page.url,
        shown
    )
}

#[cfg(test)]
mod fetched_page_tests {
    use super::*;
    use lattice_core::application::contracts::tools::FunctionResult;

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
        format_tool_result(
            tool,
            result,
            &ToolOutputSettingsDto::default(),
            allowance,
            offset,
        )
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

    /// The old configured ceiling cut every result at 50,000 characters even
    /// when the window-derived allowance was larger. The allowance alone binds.
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
        assert!(rendered.contains(&format!("offset={room}")), "{rendered}");
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
        use lattice_core::application::contracts::tools::{
            DocumentEvidence, DocumentEvidenceMatch,
        };
        let documents = (0..documents)
            .map(|d| DocumentEvidence {
                document_id: format!("doc-{d}"),
                filename: format!("file-{d}.pdf"),
                file_path: format!("/vault/file-{d}.pdf"),
                mime_type: "application/pdf".to_string(),
                max_score: 0.9,
                match_count: matches,
                matches: (0..matches)
                    .map(|m| DocumentEvidenceMatch {
                        chunk_index: Some(m),
                        score: 0.5,
                        excerpt: format!("{excerpt} ({d}/{m})"),
                    })
                    .collect(),
            })
            .collect();
        FunctionResult::success(
            serde_json::to_value(SemanticSearchOutput {
                results: Vec::new(),
                documents,
                total_found: 0,
                search_time_ms: 1.0,
                query: "q".to_string(),
                next_offset: None,
            })
            .unwrap_or_default(),
        )
    }

    /// A search held at 50 results a page had no way past the 50th. A page
    /// that may have more says the offset the next starts at.
    #[test]
    fn a_full_search_page_names_the_offset_of_the_next() {
        let mut result = search_output(2, 1, "text");
        if let Some(data) = result.data.as_mut() {
            data["next_offset"] = serde_json::json!(20);
        }
        let rendered = render(&result, "semantic_search", 4_000, 0);
        assert!(
            rendered.contains("call semantic_search with the same query and offset=20"),
            "{rendered}"
        );
        assert!(
            !render(&search_output(2, 1, "text"), "semantic_search", 4_000, 0).contains("offset=")
        );
    }

    /// Search results used to show at most three matches a document, each cut
    /// to an excerpt length. With room, every match is shown whole.
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
        assert!(rendered.contains("[1] file-0.pdf"), "{rendered}");
    }

    /// A full window used to turn a result into an empty string: the model
    /// was told nothing, not even that there was more. Every kind of result
    /// now keeps the line saying what was left out and how to reach it.
    #[test]
    fn a_result_in_a_full_window_still_says_what_was_left_out() {
        let page_note = render(&page(&"word ".repeat(5_000)), "fetch_url_content", 0, 120);
        assert!(page_note.contains("characters 120 to"), "{page_note}");
        assert!(page_note.contains("offset="), "{page_note}");

        let document = FunctionResult::success(serde_json::json!({
            "document_id": "doc-7", "content": "x".repeat(5_000),
            "content_truncated": true, "offset": 0, "next_offset": 5_000,
            "total_chars": 9_000
        }));
        let document_note = render(&document, "get_document", 0, 0);
        assert!(document_note.contains("doc-7"), "{document_note}");
        assert!(document_note.contains("offset="), "{document_note}");
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

    /// A part larger than the result's room names the offset the rest starts
    /// at; it used to name only a page size and no page.
    #[test]
    fn an_oversized_document_part_names_the_offset_to_read_on_from() {
        let document = FunctionResult::success(serde_json::json!({
            "document_id": "doc-7", "content": "§".repeat(5_000),
            "content_truncated": false, "offset": 1_000, "total_chars": 6_000
        }));
        let rendered = render(&document, "get_document", 2_000, 0);
        assert!(rendered.chars().count() <= 2_000);
        let shown = rendered.matches('§').count();
        assert!(
            rendered.contains(&format!("offset={}", 1_000 + shown)),
            "{rendered}"
        );
    }
}
