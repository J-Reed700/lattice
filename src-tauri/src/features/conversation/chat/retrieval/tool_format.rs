use crate::features::function_calling::domain::FunctionResult;
use crate::features::function_calling::dto::{
    CustomQueryToolOutput, FetchUrlContentOutput, GetDocumentOutput, SemanticSearchOutput,
    WebSearchOutput, WikiSearchOutput, WikiSummaryOutput,
};
use crate::features::settings::dto::ToolOutputSettingsDto;
use crate::shared::text_utils::{build_excerpt, safe_truncate};

pub(super) fn format_tool_result(
    tool_name: &str,
    result: &FunctionResult,
    highlight_terms: &[String],
    settings: &ToolOutputSettingsDto,
) -> String {
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

    let max_chars = settings.max_chars as usize;
    let excerpt_chars = settings.excerpt_chars as usize;
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
                let excerpt = {
                    let candidate = build_excerpt(&doc.content, highlight_terms, excerpt_chars);
                    if candidate.trim().is_empty() {
                        safe_truncate(&doc.content, excerpt_chars)
                    } else {
                        candidate
                    }
                };
                let truncated_note = if doc.content_truncated {
                    "\n(Note: content truncated)"
                } else {
                    ""
                };
                let mut rendered = templates
                    .get_document_template
                    .replace("{document_id}", &doc.document_id)
                    .replace("{title}", title)
                    .replace("{chunk_count}", &chunk_count.to_string())
                    .replace("{excerpt}", &excerpt)
                    .replace("{truncated}", truncated_note);
                if doc.total_pages > 1 {
                    let mut pagination_line = format!(
                        "\nPage {}/{} ({} chars total)",
                        doc.page, doc.total_pages, doc.total_chars
                    );
                    if let Some(next_page) = doc.next_page {
                        pagination_line.push_str(&format!(
                            "\nTo continue, call get_document with document_id=\"{}\" and page={}.",
                            doc.document_id, next_page
                        ));
                    }
                    rendered.push_str(&pagination_line);
                }
                return safe_truncate(&rendered, max_chars);
            }
        }
        "semantic_search" => {
            if let Ok(output) = serde_json::from_value::<SemanticSearchOutput>(data.clone()) {
                let limit = (settings.max_results as usize).max(12);
                let mut lines = Vec::new();
                if !output.documents.is_empty() {
                    for (idx, doc) in output.documents.iter().take(limit).enumerate() {
                        let mut block = format!(
                            "[{}] {} (doc_id: {}, best_score: {:.3}, matches: {})",
                            idx + 1,
                            doc.filename,
                            doc.document_id,
                            doc.max_score,
                            doc.match_count
                        );
                        for (match_idx, evidence) in doc.matches.iter().take(3).enumerate() {
                            let chunk_label = evidence
                                .chunk_index
                                .map(|v| format!("#{}", v))
                                .unwrap_or_else(|| "n/a".to_string());
                            let excerpt =
                                build_excerpt(&evidence.excerpt, highlight_terms, excerpt_chars);
                            block.push_str(&format!(
                                "\n  [{}] chunk {} score {:.3}: {}",
                                match_idx + 1,
                                chunk_label,
                                evidence.score,
                                excerpt
                            ));
                        }
                        lines.push(block);
                    }
                } else {
                    for (idx, res) in output.results.iter().take(limit).enumerate() {
                        let snippet = build_excerpt(&res.snippet, highlight_terms, excerpt_chars);
                        lines.push(format!(
                            "[{}] {} (doc_id: {}, score: {:.3})\nExcerpt: {}",
                            idx + 1,
                            res.filename,
                            res.document_id,
                            res.score,
                            snippet
                        ));
                    }
                }
                if lines.is_empty() {
                    return "Search returned no results.".to_string();
                }
                let results_text = lines.join("\n\n");
                let rendered = templates
                    .semantic_search_template
                    .replace("{results}", &results_text)
                    .replace("{total_found}", &output.total_found.to_string())
                    .replace("{shown}", &lines.len().to_string());
                return safe_truncate(&rendered, max_chars);
            }
        }
        "wiki_search" => {
            if let Ok(output) = serde_json::from_value::<WikiSearchOutput>(data.clone()) {
                if output.results.is_empty() {
                    return "Wikipedia search returned no results.".to_string();
                }
                let lines = output
                    .results
                    .iter()
                    .take((settings.max_results as usize).max(5))
                    .enumerate()
                    .map(|(idx, item)| {
                        format!(
                            "[{}] {} ({})\nSnippet: {}",
                            idx + 1,
                            item.title,
                            item.url,
                            build_excerpt(&item.snippet, highlight_terms, excerpt_chars)
                        )
                    })
                    .collect::<Vec<_>>();
                return safe_truncate(&lines.join("\n\n"), max_chars);
            }
        }
        "wiki_summary" => {
            if let Ok(output) = serde_json::from_value::<WikiSummaryOutput>(data.clone()) {
                let excerpt = build_excerpt(&output.extract, highlight_terms, excerpt_chars);
                let rendered = format!(
                    "{} ({})\nLanguage: {}\nSummary: {}",
                    output.title, output.url, output.language, excerpt
                );
                return safe_truncate(&rendered, max_chars);
            }
        }
        "fetch_url_content" => {
            if let Ok(page) = serde_json::from_value::<FetchUrlContentOutput>(data.clone()) {
                return render_fetched_page(&page, max_chars);
            }
        }
        "web_search" => {
            if let Ok(output) = serde_json::from_value::<WebSearchOutput>(data.clone()) {
                let mut rendered = format!(
                    "Web results for \"{}\" ({}):",
                    output.query,
                    output.results.len()
                );
                for (index, result) in output.results.iter().enumerate() {
                    rendered.push_str(&format!(
                        "\n\n[{}] {}\nURL: {}\n{}",
                        index + 1,
                        result.title,
                        result.url,
                        safe_truncate(result.snippet.trim(), excerpt_chars)
                    ));
                }
                return safe_truncate(&rendered, max_chars);
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
        return safe_truncate(&rendered, max_chars);
    }

    let fallback = serde_json::to_string(&data).unwrap_or_else(|_| "null".to_string());
    let rendered = templates.default_template.replace("{json}", &fallback);
    safe_truncate(&rendered, max_chars)
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

/// A fetched page as prose.
///
/// It used to reach the model through the generic fallback: the whole output
/// struct as one line of JSON, every quotation mark and line break in the
/// article escaped, with the fetch timing and content type along for the ride.
/// That spends tokens on backslashes and makes an article harder to read than
/// it was on the page.
fn render_fetched_page(page: &FetchUrlContentOutput, max_chars: usize) -> String {
    let content = page.content.trim();
    let room = fetched_page_text_room(max_chars);
    let total = content.chars().count();
    let extent = if total <= room {
        format!(
            "{} words — complete; this is the whole page",
            page.word_count
        )
    } else {
        format!(
            "{} words; the first {room} characters are shown — request this URL again for the rest",
            page.word_count
        )
    };
    format!(
        "{}\nURL: {}\n({extent})\n\n{}",
        page.title.as_deref().unwrap_or("Untitled page"),
        page.url,
        safe_truncate(content, room)
    )
}

#[cfg(test)]
mod fetched_page_tests {
    use super::*;
    use crate::features::function_calling::domain::FunctionResult;

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
            })
            .unwrap_or_default(),
        )
    }

    fn settings(max_chars: u32) -> ToolOutputSettingsDto {
        ToolOutputSettingsDto {
            max_chars,
            ..ToolOutputSettingsDto::default()
        }
    }

    /// As JSON the article's own punctuation came through escaped.
    #[test]
    fn a_fetched_page_reads_as_prose_not_as_escaped_json() {
        let article = "Bernard said \"better tape\".\nThat was the mistake.";
        let rendered =
            format_tool_result("fetch_url_content", &page(article), &[], &settings(50_000));

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

    #[test]
    fn a_page_longer_than_its_room_says_how_much_is_shown() {
        let article = "word ".repeat(2_000);
        let rendered =
            format_tool_result("fetch_url_content", &page(&article), &[], &settings(3_000));

        let room = fetched_page_text_room(3_000);
        assert!(
            rendered.contains(&format!("first {room} characters")),
            "{rendered}"
        );
        assert!(!rendered.contains("complete"), "{rendered}");
        assert!(
            rendered.chars().count() <= 3_000,
            "got {}",
            rendered.chars().count()
        );
    }
}
