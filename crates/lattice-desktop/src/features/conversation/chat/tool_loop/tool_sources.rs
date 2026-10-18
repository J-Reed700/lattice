//! The verifiable sources a tool result adds to the turn: a search's results,
//! a page the model read, a Wikipedia summary. Each becomes a citation the
//! answer can point at.

use super::ToolOutputSettingsDto;
use crate::features::conversation::chat::dto::SourceDto;
use crate::features::conversation::chat::retrieval::build_web_source_citations;
use lattice_core::application::contracts::images::FindImageOutput;
use lattice_core::application::contracts::tools::{
    FunctionResult, WikiSearchOutput, WikiSummaryOutput,
};
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, WebSearchOutput, WebSearchResult,
};
use lattice_core::shared::text::build_excerpt;

/// Preview length for a page the model fetched itself, matching the preview a
/// vault passage gets.
const FETCHED_PAGE_EXCERPT_CHARS: usize = 480;

/// The lines that give a result's new sources their citation numbers, and
/// what is left of `allowance` for the result itself.
///
/// The lines are charged to the result's own allowance, so the two together
/// fit it. A source whose text the result already shows (`shown`, the result
/// as it reads with the whole allowance) is named by number, title and
/// address rather than repeated. One whose text the result does not show
/// carries it, each such source an equal share of half the allowance, whole
/// when it fits its share and otherwise cut there and marked.
pub(super) fn citable_passages(
    citable: &[(u32, &SourceDto)],
    shown: &str,
    allowance: usize,
) -> (String, usize) {
    if citable.is_empty() {
        return (String::new(), allowance);
    }
    let unshown = citable
        .iter()
        .filter(|(_, source)| !is_shown(shown, &source.content))
        .count();
    let share = (allowance / 2) / unshown.max(1);
    let mut text = String::new();
    for (number, source) in citable {
        let content = source.content.trim();
        if is_shown(shown, content) {
            text.push_str(&format!(
                "\n\nCitable passage [{number}] — {} ({}): its text is in this result above.",
                source.file_name, source.file_path
            ));
            continue;
        }
        let header = format!("\n\nCitable passage [{number}] — {}\n", source.file_name);
        let room = share.saturating_sub(header.chars().count());
        text.push_str(&header);
        let total = content.chars().count();
        if total <= room {
            text.push_str(content);
        } else {
            // Where the passage stops and how to read on, so the rest is a
            // call away rather than gone: the note is sized with a worst-case
            // offset before the cut is made.
            let mark = |at: usize| continues_note(source, at, total);
            let part = lattice_core::shared::text::split_into_parts(
                content,
                room.saturating_sub(mark(total).chars().count()).max(1),
            );
            let part = part.first().copied().unwrap_or_default();
            text.push_str(part);
            text.push_str(&mark(part.chars().count()));
        }
    }
    let used = text.chars().count();
    (text, allowance.saturating_sub(used))
}

/// The note a cut passage ends with: the character it stops at, and the call
/// that reads on from there.
fn continues_note(source: &SourceDto, at: usize, total: usize) -> String {
    use crate::features::conversation::chat::retrieval::WEB_SOURCE_PREFIX;
    let how = match source.document_id.strip_prefix(WEB_SOURCE_PREFIX) {
        Some(url) => format!("fetch_url_content with url \"{url}\" and offset {at} reads on"),
        None => format!(
            "get_document with document_id \"{}\" reads the document",
            source.document_id
        ),
    };
    format!(" […passage continues at character {at} of {total}; {how}]")
}

/// A tool result and its citation lines, together within `allowance`, the
/// lines decided against the result as it is finally rendered.
///
/// A source counts as already shown only when the text the model receives
/// carries it. Decided against the result rendered with the whole allowance
/// and then rendered smaller to make room for the lines, a source could be
/// named as "in this result above" when the smaller render no longer held it.
/// The room shrinks until the render fits what its lines leave; a smaller
/// render shows less, so the lines only grow and this ends.
pub(super) fn render_with_citations(
    citable: &[(u32, &SourceDto)],
    allowance: usize,
    render: impl Fn(usize) -> String,
) -> String {
    let mut room = allowance;
    loop {
        let rendered = render(room);
        let (citations, left) = citable_passages(citable, &rendered, allowance);
        if room <= left || room == 0 {
            return rendered + &citations;
        }
        room = left;
    }
}

/// Whether `shown` already carries `content`: its opening, whitespace aside.
fn is_shown(shown: &str, content: &str) -> bool {
    let probe: String = content
        .split_whitespace()
        .take(24)
        .collect::<Vec<_>>()
        .join(" ");
    if probe.is_empty() {
        return true;
    }
    let shown: String = shown.split_whitespace().collect::<Vec<_>>().join(" ");
    shown.contains(&probe)
}

/// One web source per page the stored pictures came from, so a picture's
/// caption cites its page like any other web source and the page is in the
/// footnotes. Keyed `web:<url>`, so a page the turn already cites keeps its
/// number. The chunk is the picture's ref, unique within the turn.
fn picture_sources(output: &FindImageOutput) -> Vec<SourceDto> {
    let modified_at = chrono::Utc::now().to_rfc3339();
    let mut sources: Vec<SourceDto> = Vec::new();
    for image in &output.images {
        let document_id = format!("web:{}", image.source_url);
        if sources
            .iter()
            .any(|source| source.document_id == document_id)
        {
            continue;
        }
        let file_name = image
            .source_title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or(&image.source_url)
            .to_string();
        sources.push(SourceDto {
            page_number: None,
            from_ocr: false,
            document_id,
            chunk_id: format!("image:{}", image.r#ref),
            content: image.caption.clone(),
            score: 1.0,
            path: Some(image.source_url.clone()),
            position: Some(1),
            file_name,
            file_path: image.source_url.clone(),
            mime_type: "text/html".to_string(),
            category: "Web image".to_string(),
            file_size_bytes: 0,
            modified_at: modified_at.clone(),
            // The card's preview: the UI refuses an excerpt over its limit.
            excerpt: Some(build_excerpt(
                &image.caption,
                &[],
                FETCHED_PAGE_EXCERPT_CHARS,
            )),
            highlights: None,
            section: None,
            chunk_index: Some(1),
            chunk_excerpts: None,
            citation_id: None,
            web_snapshot: None,
        });
    }
    sources
}

pub(super) fn collect_tool_sources(
    tool_name: &str,
    result: &FunctionResult,
    highlight_terms: &[String],
    tool_output_settings: &ToolOutputSettingsDto,
) -> Vec<SourceDto> {
    if !result.success {
        return Vec::new();
    }
    let data = match result.data.clone() {
        Some(value) => value,
        None => return Vec::new(),
    };

    match tool_name {
        "web_search" => {
            if let Ok(output) = serde_json::from_value::<WebSearchOutput>(data) {
                return build_web_source_citations(
                    &output.results,
                    highlight_terms,
                    tool_output_settings.excerpt_chars as usize,
                );
            }
            Vec::new()
        }
        "fetch_url_content" => {
            if let Ok(output) = serde_json::from_value::<FetchUrlContentOutput>(data) {
                // Stored whole, as a cited web page is archived whole: the
                // grounding check and synthesis read this text, and both
                // read in parts what does not fit them.
                let content = output.content.trim().to_string();
                let title = output
                    .title
                    .unwrap_or_else(|| output.url.clone())
                    .trim()
                    .to_string();
                let modified_at = chrono::Utc::now().to_rfc3339();
                let highlights = if highlight_terms.is_empty() {
                    None
                } else {
                    Some(highlight_terms.to_vec())
                };
                return vec![SourceDto {
                    page_number: None,
                    from_ocr: false,
                    document_id: format!("web:{}", output.url),
                    chunk_id: "web-content-1".to_string(),
                    content: content.clone(),
                    score: 1.0,
                    path: Some(output.url.clone()),
                    position: Some(1),
                    file_name: if title.is_empty() {
                        output.url.clone()
                    } else {
                        title
                    },
                    file_path: output.url.clone(),
                    mime_type: output
                        .content_type
                        .unwrap_or_else(|| "text/html".to_string()),
                    category: "Web Article".to_string(),
                    file_size_bytes: output.word_count as i64,
                    modified_at,
                    // The card's short preview, not the page: the UI refuses an
                    // excerpt over 2000 characters, and a whole-page excerpt here
                    // dropped the source — its `[n]` chip went dead and it was
                    // missing from the list under the answer.
                    excerpt: Some(build_excerpt(
                        &output.content,
                        highlight_terms,
                        FETCHED_PAGE_EXCERPT_CHARS,
                    )),
                    highlights,
                    section: None,
                    chunk_index: Some(1),
                    chunk_excerpts: None,
                    citation_id: None,

                    web_snapshot: None,
                }];
            }
            Vec::new()
        }
        "find_image" => serde_json::from_value::<FindImageOutput>(data)
            .map(|output| picture_sources(&output))
            .unwrap_or_default(),
        "wiki_search" => {
            if let Ok(output) = serde_json::from_value::<WikiSearchOutput>(data) {
                let as_web_results: Vec<_> = output
                    .results
                    .into_iter()
                    .map(|item| WebSearchResult {
                        title: item.title,
                        url: item.url,
                        snippet: item.snippet,
                        published_date: None,
                        source: Some("wikipedia".to_string()),
                    })
                    .collect();
                return build_web_source_citations(
                    &as_web_results,
                    highlight_terms,
                    tool_output_settings.excerpt_chars as usize,
                );
            }
            Vec::new()
        }
        "wiki_summary" => {
            if let Ok(output) = serde_json::from_value::<WikiSummaryOutput>(data) {
                // The whole extract; the card's preview is its own excerpt.
                let content = output.extract.trim().to_string();
                let highlights = if highlight_terms.is_empty() {
                    None
                } else {
                    Some(highlight_terms.to_vec())
                };
                return vec![SourceDto {
                    page_number: None,
                    from_ocr: false,
                    document_id: format!("wiki:{}", output.title.replace(' ', "_")),
                    chunk_id: "wiki-summary-1".to_string(),
                    content: content.clone(),
                    score: 1.0,
                    path: Some(output.url.clone()),
                    position: Some(1),
                    file_name: output.title.clone(),
                    file_path: output.url.clone(),
                    mime_type: "text/plain".to_string(),
                    category: "Wikipedia".to_string(),
                    file_size_bytes: output.extract.chars().count() as i64,
                    modified_at: chrono::Utc::now().to_rfc3339(),
                    excerpt: Some(build_excerpt(
                        &content,
                        highlight_terms,
                        FETCHED_PAGE_EXCERPT_CHARS,
                    )),
                    highlights,
                    section: None,
                    chunk_index: Some(1),
                    chunk_excerpts: None,
                    citation_id: None,

                    web_snapshot: None,
                }];
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod citable_tests {
    use super::*;

    fn source(content: &str) -> SourceDto {
        SourceDto {
            page_number: None,
            from_ocr: false,
            document_id: "web:https://example.com/a".to_string(),
            chunk_id: "web-content-1".to_string(),
            content: content.to_string(),
            score: 1.0,
            path: None,
            position: Some(1),
            file_name: "Page A".to_string(),
            file_path: "https://example.com/a".to_string(),
            mime_type: "text/html".to_string(),
            category: "Web Article".to_string(),
            file_size_bytes: 0,
            modified_at: String::new(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            chunk_excerpts: None,
            citation_id: Some(1),
            web_snapshot: None,
        }
    }

    /// The citation lines used to be added after the result, uncounted, with
    /// up to 4,000 characters of passage each. They come out of the result's
    /// allowance now, and text the result already shows is not repeated.
    #[test]
    fn citable_passages_are_charged_to_the_allowance_and_not_repeated() {
        let page = "The station opened in 1932 and was rebuilt after the fire. ".repeat(20);
        let shown = source(&page);
        let (text, left) = citable_passages(&[(1, &shown)], &format!("Page A\n{page}"), 10_000);
        assert!(text.contains("Citable passage [1] — Page A (https://example.com/a)"));
        assert!(!text.contains("rebuilt after the fire"));
        assert_eq!(left, 10_000 - text.chars().count());

        let unshown = source(&page);
        let (text, left) = citable_passages(&[(1, &unshown)], "something else", 600);
        assert!(text.contains("Citable passage [1] — Page A\n"));
        assert!(
            text.contains("fetch_url_content with url \"https://example.com/a\" and offset "),
            "{text}"
        );
        assert!(text.chars().count() <= 300, "{}", text.chars().count());
        assert_eq!(left, 600 - text.chars().count());
    }

    /// A fetched page and a Wikipedia summary were stored cut at the excerpt
    /// setting, and synthesis read the cut text. They are stored whole, as a
    /// cited web page is archived; the card keeps its short preview.
    #[test]
    fn fetched_and_wiki_sources_are_stored_whole() {
        let body = format!(
            "{}the closing line",
            "A long article sentence. ".repeat(2_000)
        );
        let settings = ToolOutputSettingsDto::default();
        let page = FunctionResult::success(
            serde_json::to_value(FetchUrlContentOutput {
                url: "https://example.com/a".into(),
                title: Some("Page A".into()),
                content: body.clone(),
                content_truncated: false,
                word_count: body.split_whitespace().count(),
                fetch_time_ms: 1.0,
                content_type: None,
                from_cache: false,
            })
            .unwrap(),
        );
        let fetched = collect_tool_sources("fetch_url_content", &page, &[], &settings);
        assert_eq!(fetched[0].content, body.trim());
        assert!(fetched[0].excerpt.as_ref().unwrap().chars().count() < 2_000);

        let summary = FunctionResult::success(
            serde_json::to_value(WikiSummaryOutput {
                title: "Topic".into(),
                url: "https://en.wikipedia.org/wiki/Topic".into(),
                language: "en".into(),
                extract: body.clone(),
            })
            .unwrap(),
        );
        let wiki = collect_tool_sources("wiki_summary", &summary, &[], &settings);
        assert_eq!(wiki[0].content, body.trim());
        assert!(wiki[0].excerpt.as_ref().unwrap().chars().count() < 2_000);
    }

    /// A cut passage says where it stops and how to read on from there, and
    /// the offset it names is where its shown text ends.
    #[test]
    fn a_cut_passage_names_the_offset_to_read_on_from() {
        let page = "word ".repeat(400);
        let cut = source(&page);
        let (text, _) = citable_passages(&[(1, &cut)], "something else", 600);
        let shown = text
            .split("Citable passage [1] — Page A\n")
            .nth(1)
            .and_then(|rest| rest.split(" […passage continues at character ").next())
            .unwrap();
        let offset = shown.chars().count();
        let total = page.trim().chars().count();
        assert!(
            text.contains(&format!("continues at character {offset} of {total};")),
            "{text}"
        );
        assert!(text.contains(&format!("and offset {offset} reads on]")));
    }

    /// Whether a source is already in the result is decided against the
    /// result the model gets. Decided against the render with the whole
    /// allowance, this page counted as shown, and the result was then
    /// rendered smaller, without the page, under a line saying its text was
    /// "in this result above".
    #[test]
    fn shown_is_decided_against_the_final_render() {
        let page = format!("{} and then much more of the page.", "ab ".repeat(24));
        let cited = source(&page);
        let header = "A forty-character header for the result\n";
        let render =
            |room: usize| -> String { format!("{header}{page}").chars().take(room).collect() };
        assert!(
            is_shown(&render(150), &page),
            "the whole allowance shows it"
        );

        let text = render_with_citations(&[(1, &cited)], 150, render);
        assert!(text.chars().count() <= 150, "{}", text.chars().count());
        assert!(!text.contains("its text is in this result above"), "{text}");
        assert!(text.contains("Citable passage [1] — Page A\n"), "{text}");
    }

    /// Each picture's page becomes one web source, so its caption can cite it;
    /// two pictures from one page are one source.
    #[test]
    fn find_image_adds_one_web_source_per_picture_page() {
        let picture = |r: &str, url: &str| {
            serde_json::json!({
                "ref": r, "caption": "Saturn V lifting off", "source_title": "File:Apollo 11 Launch.jpg",
                "source_url": url, "license": "Public domain", "width": 1280, "height": 1600,
                "markdown": format!("![Saturn V lifting off]({r})"),
            })
        };
        let result = FunctionResult::success(serde_json::json!({
            "images": [
                picture("img-1", "https://commons.wikimedia.org/wiki/File:A.jpg"),
                picture("img-2", "https://commons.wikimedia.org/wiki/File:A.jpg"),
                picture("img-3", "https://commons.wikimedia.org/wiki/File:B.jpg"),
            ],
            "note": null,
        }));
        let sources = collect_tool_sources(
            "find_image",
            &result,
            &[],
            &ToolOutputSettingsDto::default(),
        );
        assert_eq!(sources.len(), 2);
        assert_eq!(
            sources[0].document_id,
            "web:https://commons.wikimedia.org/wiki/File:A.jpg"
        );
        assert_eq!(sources[0].chunk_id, "image:img-1");
        assert_eq!(sources[0].category, "Web image");
        assert_eq!(sources[0].file_name, "File:Apollo 11 Launch.jpg");
        assert_eq!(sources[0].content, "Saturn V lifting off");
    }
}
