use lattice_core::application::contracts::images::{FindImageOutput, NO_PICTURE_NOTE};
use lattice_core::application::contracts::settings::ToolOutputSettingsDto;
use lattice_core::application::contracts::tools::FunctionResult;
use lattice_core::application::contracts::tools::{
    CustomQueryToolOutput, GetDocumentOutput, SemanticSearchOutput, WikiSearchOutput,
    WikiSummaryOutput,
};
use lattice_core::application::contracts::web::{FetchUrlContentOutput, WebSearchOutput};
use lattice_core::application::ports::turn_source::SourcePassages;
use lattice_core::shared::text::safe_truncate;
use std::collections::HashMap;

use super::rendered::{Passage, Rendered};
use super::source_citations::library_chunk_id;
use super::source_names::marker;
use crate::features::conversation::chat::history_tools::view as history_view;
use crate::features::function_calling::handles::{CallView, Resume};
use crate::features::function_calling::views as tool_views;
use serde_json::json;

/// Room kept at the end of a result for the line saying what had to be left
/// out of it, so that line always fits.
const OMISSION_NOTE_CHARS: usize = 240;

/// The least a result is given, however full the window: room for the note
/// saying what was left out and where it is.
const MIN_RESULT_CHARS: usize = 2 * OMISSION_NOTE_CHARS;

/// A tool's result as the model reads it, in at most `allowance` characters
/// (its share of the window; see `tool_result_allowance`). When it does not
/// fit, it says what was left out and how to reach it. `page_offset` is where
/// a fetched page's text starts. `citations` holds the turn's numbers by chunk:
/// a library result is shown under its own, since a list numbered from one
/// would read as the passages the turn already holds. `call` names what the
/// result shows and where it stopped, so the model never reads an id or an offset.
/// Besides the text, it gives where in it each labelled source's own text is.
pub(super) fn format_tool_result(
    tool_name: &str,
    result: &FunctionResult,
    settings: &ToolOutputSettingsDto,
    allowance: usize,
    page_offset: usize,
    source_text: bool,
    citations: &HashMap<String, u32>,
    call: &CallView<'_>,
) -> Rendered {
    let allowance = allowance.max(MIN_RESULT_CHARS);
    if !result.success {
        return format!(
            "Error: {}",
            result.error_message.as_deref().unwrap_or("Unknown error")
        )
        .into();
    }

    let data = match result.data.clone() {
        Some(value) => value,
        None => return "No data returned.".to_string().into(),
    };

    // A turn source's tools already fit their text to this call's allowance,
    // cut at a line and saying where to read on. Through the JSON fallback
    // below the numbered lines would arrive as one escaped string.
    if source_text {
        if let Some(text) = data.as_str() {
            return safe_truncate(text, allowance).into();
        }
        if let Some(text) = data.get("text").and_then(serde_json::Value::as_str) {
            let resume = data
                .get("read_on")
                .and_then(|next| serde_json::from_value::<Resume>(next.clone()).ok());
            let Some(name) = resume.and_then(|resume| call.handles().more(resume)) else {
                return safe_truncate(text, allowance).into();
            };
            let line = call.read_on_line(&name);
            let room = allowance.saturating_sub(line.chars().count() + 1);
            return format!(
                "{}\n{line}",
                safe_truncate(text, room)
            )
            .into();
        }
        if let Ok(mut output) = serde_json::from_value::<SourcePassages>(data.clone()) {
            let read_on = data
                .get("read_on")
                .and_then(|next| serde_json::from_value::<Resume>(next.clone()).ok())
                .and_then(|resume| call.handles().more(resume))
                .map(|name| call.read_on_line(&name));
            if let Some(line) = read_on {
                output.note = Some(match output.note {
                    Some(note) => format!("{note} {line}"),
                    None => line,
                });
            }
            return render_source_passages(&output, allowance, citations);
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
                let name = call
                    .handles()
                    .document(&doc.document_id)
                    .unwrap_or_default();
                let read_on_from = |at: usize| {
                    call.read_on(&[("offset", json!(at)), ("max_content_length", json!(null))])
                };
                let mut pagination_line = String::new();
                if doc.offset > 0 || doc.next_offset.is_some() {
                    pagination_line = format!(
                        "\nCharacters {} to {shown_end} of {}",
                        doc.offset, doc.total_chars
                    );
                    if let Some(next_offset) = doc.next_offset {
                        pagination_line.push_str(&format!(
                            "\n{}",
                            call.read_on_line(&read_on_from(next_offset))
                        ));
                    }
                }
                let render = |excerpt: &str| {
                    let mut rendered = templates
                        .get_document_template
                        .replace("{document_id}", &name)
                        .replace("{title}", title)
                        .replace("{chunk_count}", &chunk_count.to_string())
                        .replace("{excerpt}", excerpt)
                        .replace("{truncated}", truncated_note);
                    rendered.push_str(&pagination_line);
                    rendered
                };
                let rendered = render(&doc.content);
                if rendered.chars().count() <= allowance {
                    return rendered.into();
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
                    "\n[Only characters {} to {next} of this part fit this result. {}]",
                    doc.offset,
                    call.read_on_line(&read_on_from(next))
                ));
                return keep_note(&rendered, allowance).into();
            }
        }
        "semantic_search" => {
            if let Ok(output) = serde_json::from_value::<SemanticSearchOutput>(data.clone()) {
                let blocks: Vec<Block> = output
                    .results
                    .iter()
                    .map(|hit| {
                        let number = citations.get(&library_chunk_id(hit));
                        let label = number.map_or_else(|| "-".to_owned(), |number| marker(*number));
                        let name = call
                            .handles()
                            .document(&hit.document_id)
                            .unwrap_or_default();
                        Block::new(
                            format!("{label} {} {name} (score: {:.3})", hit.filename, hit.score),
                            format!("Excerpt: {}", hit.snippet.trim()),
                            number.is_some(),
                        )
                    })
                    .collect();
                if blocks.is_empty() {
                    return "Search returned no results.".to_string().into();
                }
                let template = |results: Rendered, shown: usize| -> Rendered {
                    let filled = templates
                        .semantic_search_template
                        .replace("{total_found}", &output.total_found.to_string())
                        .replace("{shown}", &shown.to_string());
                    match filled.split_once("{results}") {
                        Some((before, after)) => {
                            let after = after.replace("{results}", &results.text);
                            results.wrapped(before, &after)
                        }
                        None => filled.into(),
                    }
                };
                let framing = template(Rendered::default(), blocks.len())
                    .text
                    .chars()
                    .count();
                // Results past this page are a call away, and the result says
                // which: without an offset, the 51st hit could never be read.
                let more = output.next_offset.map_or_else(String::new, |next| {
                    format!(
                        "\n[More results may follow. {}]",
                        call.read_on_line(&call.read_on(&[("offset", json!(next))]))
                    )
                });
                let (results, shown) = fit_labelled(
                    &blocks,
                    allowance.saturating_sub(framing + more.chars().count()),
                    "Call get_document with a name above to read a document, or search again more narrowly.",
                );
                let mut rendered = template(results, shown);
                rendered.append(more.into());
                return keep_labelled_note(rendered, allowance);
            }
        }
        "wiki_search" => {
            if let Ok(output) = serde_json::from_value::<WikiSearchOutput>(data.clone()) {
                if output.results.is_empty() {
                    return "Wikipedia search returned no results."
                        .to_string()
                        .into();
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
                return keep_note(&text, allowance).into();
            }
        }
        "wiki_summary" => {
            if let Ok(output) = serde_json::from_value::<WikiSummaryOutput>(data.clone()) {
                let rendered = format!(
                    "{} ({})\nLanguage: {}\nSummary: {}",
                    output.title, output.url, output.language, output.extract
                );
                return fit_text(&rendered, allowance).into();
            }
        }
        "fetch_url_content" => {
            if let Ok(page) = serde_json::from_value::<FetchUrlContentOutput>(data.clone()) {
                return render_fetched_page(&page, allowance, page_offset, call).into();
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
                let more = more_results_note(&output, call);
                let (text, _) = fit_blocks(
                    &blocks,
                    allowance.saturating_sub(heading.chars().count() + 2 + more.chars().count()),
                    "Search again more narrowly to see them.",
                );
                return keep_note(&format!("{heading}\n\n{text}{more}"), allowance).into();
            }
        }
        "find_image" => {
            if let Ok(output) = serde_json::from_value::<FindImageOutput>(data.clone()) {
                let mut lines: Vec<String> = output
                    .images
                    .iter()
                    .map(|image| {
                        let name = call.handles().picture(&image.markdown).unwrap_or_default();
                        let dimensions = format!("{}×{}", image.width, image.height);
                        let details = [
                            image.caption.as_str(),
                            image.source_title.as_deref().unwrap_or_default(),
                            image.license.as_deref().unwrap_or_default(),
                            dimensions.as_str(),
                        ]
                        .iter()
                        .filter(|part| !part.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" · ");
                        format!("{name} {details}").trim().to_owned()
                    })
                    .collect();
                if let Some(note) = output.note.as_deref().filter(|note| !note.is_empty()) {
                    lines.push(note.to_owned());
                }
                let text = if lines.is_empty() {
                    NO_PICTURE_NOTE.to_owned()
                } else {
                    lines.join("\n")
                };
                return text.into();
            }
        }
        _ => {}
    }

    if let Some(text) = history_view::render(tool_name, &data, call)
        .or_else(|| tool_views::render(tool_name, &data, call))
    {
        return fit_text(&text, allowance).into();
    }

    if let Ok(output) = serde_json::from_value::<CustomQueryToolOutput>(data.clone()) {
        let rendered = format!(
            "Custom tool '{}' response from {}:\n{}",
            output.tool_name,
            output.request_url,
            serde_json::to_string(&output.data).unwrap_or_else(|_| "{}".to_string())
        );
        return fit_text(&rendered, allowance).into();
    }

    let fallback = serde_json::to_string(&data).unwrap_or_else(|_| "null".to_string());
    let rendered = templates.default_template.replace("{json}", &fallback);
    let more = match (tool_name, serde_json::from_value::<WebSearchOutput>(data)) {
        ("find_sources", Ok(output)) => more_results_note(&output, call),
        _ => String::new(),
    };
    let body = fit_text(&rendered, allowance.saturating_sub(more.chars().count()));
    format!("{body}{more}").into()
}

/// The line that names how to read the next page of a web search's results,
/// when there is one.
fn more_results_note(output: &WebSearchOutput, call: &CallView<'_>) -> String {
    if !output.has_more {
        return String::new();
    }
    let next = call.read_on(&[("page", json!(output.page + 1))]);
    format!(
        "\n[More results are available. {}]",
        call.read_on_line(&next)
    )
}

/// A turn source's passages, each under the turn's number for it as a library
/// result is, as many as fit, closed by the source's note.
fn render_source_passages(
    output: &SourcePassages,
    allowance: usize,
    citations: &HashMap<String, u32>,
) -> Rendered {
    let note = output
        .note
        .as_deref()
        .map_or_else(String::new, |note| format!("\n[{note}]"));
    let blocks: Vec<Block> = output
        .passages
        .iter()
        .map(|passage| {
            let number = citations.get(&passage.source.chunk_id);
            let label = number.map_or_else(|| "-".to_owned(), |number| marker(*number));
            Block::new(
                format!("{label} {}", passage.label),
                passage.source.content.trim().to_owned(),
                number.is_some(),
            )
        })
        .collect();
    if blocks.is_empty() {
        return note.trim_start().to_string().into();
    }
    let (mut rendered, _) = fit_labelled(
        &blocks,
        allowance.saturating_sub(note.chars().count()),
        "Search again more narrowly to see them.",
    );
    rendered.append(note.into());
    keep_labelled_note(rendered, allowance)
}

/// One result of a list: the heading line the app wrote, and the text under
/// it. A labelled block's text is one source's own text.
struct Block {
    text: String,
    heading_len: usize,
    labelled: bool,
}

impl Block {
    fn new(heading: String, body: String, labelled: bool) -> Self {
        Self {
            heading_len: heading.len(),
            text: format!("{heading}\n{body}"),
            labelled,
        }
    }

    fn plain(text: String) -> Self {
        Self {
            text,
            heading_len: 0,
            labelled: false,
        }
    }

    /// This block as a result of its own, with its passage when it is labelled.
    fn rendered(&self) -> Rendered {
        let mut rendered = Rendered::from(self.text.clone());
        let heading = self.text.get(..self.heading_len);
        if let (true, Some(heading)) = (self.labelled, heading) {
            rendered.passages.push(Passage {
                heading: heading.to_owned(),
                line: 0..self.heading_len,
                body: self.heading_len + 1..self.text.len(),
                legend: Vec::new(),
            });
        }
        rendered
    }
}

/// `blocks` joined by blank lines, as many from the front as fit `room`
/// characters, and how many that was. When some do not fit, a closing line
/// says how many were left out and `how_to_reach` says how to see them: the
/// count comes from the allowance, and the model is told so.
fn fit_blocks(blocks: &[String], room: usize, how_to_reach: &str) -> (String, usize) {
    let blocks: Vec<Block> = blocks.iter().cloned().map(Block::plain).collect();
    let (rendered, shown) = fit_labelled(&blocks, room, how_to_reach);
    (rendered.text, shown)
}

/// `fit_blocks` for blocks some of which are one labelled source's text: the
/// result says where each shown one's text is.
fn fit_labelled(blocks: &[Block], room: usize, how_to_reach: &str) -> (Rendered, usize) {
    let room_for_blocks = room.saturating_sub(OMISSION_NOTE_CHARS);
    let mut rendered = Rendered::default();
    let mut used = 0usize;
    let mut shown = 0usize;
    for block in blocks {
        let cost = block.text.chars().count() + if shown > 0 { 2 } else { 0 };
        let fits_whole_result = shown + 1 == blocks.len() && used + cost <= room;
        if used + cost > room_for_blocks && !fits_whole_result {
            break;
        }
        if shown > 0 {
            rendered.append("\n\n".to_owned().into());
        }
        rendered.append(block.rendered());
        used += cost;
        shown += 1;
    }
    if shown == 0 {
        if let Some(first) = blocks.first() {
            // Not even one whole result fits: show the start of the best one
            // rather than nothing, and say it was cut.
            rendered = first.rendered().cut(room_for_blocks);
            shown = 1;
            rendered.append(
                "\n[This result was cut to fit the model's context.]"
                    .to_owned()
                    .into(),
            );
        }
    }
    let left_out = blocks.len().saturating_sub(shown);
    if left_out > 0 {
        rendered.append(
            format!(
                "\n\n[{left_out} more result(s) did not fit in the room this result has in the model's context. {how_to_reach}]"
            )
            .into(),
        );
    }
    (rendered, shown)
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
    keep_labelled_note(text.to_owned().into(), allowance).text
}

/// `keep_note` for a result that knows where its labelled text is: the ranges
/// follow the cut.
fn keep_labelled_note(rendered: Rendered, allowance: usize) -> Rendered {
    if rendered.text.chars().count() <= allowance {
        return rendered;
    }
    let Some(at) = rendered.text.rfind("\n[") else {
        return rendered.cut(allowance);
    };
    let note = rendered.text.get(at..).unwrap_or_default().to_owned();
    let mut body = rendered;
    body.truncate_bytes(at);
    let mut kept = body.cut(allowance.saturating_sub(note.chars().count()));
    kept.append(note.into());
    kept
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

/// A fetched page as prose rather than escaped JSON, from character `offset`
/// on, as much as a result of `max_chars` holds. A longer page names the
/// offset the next part starts at.
fn render_fetched_page(
    page: &FetchUrlContentOutput,
    max_chars: usize,
    offset: usize,
    call: &CallView<'_>,
) -> String {
    let content = page.content.trim();
    let room = fetched_page_text_room(max_chars);
    let total = content.chars().count();
    let start = offset.min(total);
    let shown: String = content.chars().skip(start).take(room).collect();
    let end = start + shown.chars().count();
    let extent = if end == start && start < total {
        format!(
            "{} words, {total} characters; the model's context has no room left for this page's text — characters {start} to {total} are unread. {}",
            page.word_count,
            call.read_on_line(&call.read_on(&[("offset", json!(start))]))
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
            "{} words, {total} characters; the first {end} characters are shown, as much as this result has room for. {}",
            page.word_count,
            call.read_on_line(&call.read_on(&[("offset", json!(end))]))
        )
    } else {
        format!(
            "{} words, {total} characters; characters {start} to {end} are shown, as much as this result has room for. {}",
            page.word_count,
            call.read_on_line(&call.read_on(&[("offset", json!(end))]))
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
    use crate::features::function_calling::handles::ToolHandles;
    use lattice_core::application::contracts::tools::FunctionResult;
    use lattice_core::application::ports::turn_source::SourcePassage;

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
            !render(&search_output(2, 1, "text"), "semantic_search", 4_000, 0)
                .contains("read_more")
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
}

#[cfg(test)]
mod picture_tests;
