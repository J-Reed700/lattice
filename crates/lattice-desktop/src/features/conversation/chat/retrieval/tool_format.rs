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
use lattice_core::application::services::tool_handles::{CallView, Resume};
use serde_json::json;

mod library_views;

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
            return format!("{}\n{line}", safe_truncate(text, room)).into();
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
                    return "Wikipedia search returned no results.".to_string().into();
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
        .or_else(|| library_views::render(tool_name, &data, call))
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
mod fetched_page_tests;

#[cfg(test)]
mod picture_tests;
