use super::*;
use crate::features::conversation::chat::retrieval::WEB_SOURCE_PREFIX;

fn web(url: &str, title: &str, excerpt: &str) -> SourceDto {
    SourceDto {
        document_id: format!("{WEB_SOURCE_PREFIX}{url}"),
        chunk_id: "web-result-1".to_string(),
        content: excerpt.to_string(),
        score: 0.5,
        path: Some(url.to_string()),
        position: Some(1),
        file_name: title.to_string(),
        file_path: url.to_string(),
        mime_type: "text/html".to_string(),
        category: "Web Article".to_string(),
        file_size_bytes: 0,
        modified_at: "2026-09-21T00:00:00Z".to_string(),
        excerpt: None,
        highlights: None,
        section: None,
        chunk_index: None,
        page_number: None,
        from_ocr: false,
        chunk_excerpts: None,
        citation_id: None,

        web_snapshot: None,
    }
}

fn document(id: &str) -> SourceDto {
    SourceDto {
        document_id: id.to_string(),
        chunk_id: format!("{id}-c1"),
        content: "A passage from a file on disk.".to_string(),
        score: 0.9,
        path: Some(format!("/vault/{id}.md")),
        position: Some(1),
        file_name: format!("{id}.md"),
        file_path: format!("/vault/{id}.md"),
        mime_type: "text/markdown".to_string(),
        category: "Markdown".to_string(),
        file_size_bytes: 0,
        modified_at: "2026-09-21T00:00:00Z".to_string(),
        excerpt: None,
        highlights: None,
        section: None,
        chunk_index: None,
        page_number: None,
        from_ocr: false,
        chunk_excerpts: None,
        citation_id: None,

        web_snapshot: None,
    }
}

#[test]
fn a_turns_web_citations_are_what_gets_kept() {
    let kept = cited_web_sources(&[
        document("doc-1"),
        web(
            "https://example.com/broccolini",
            "Growing broccolini",
            "Broccolini tolerates light frost.",
        ),
        document("doc-2"),
    ]);

    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].url, "https://example.com/broccolini");
    assert_eq!(kept[0].title.as_deref(), Some("Growing broccolini"));
    assert_eq!(
        kept[0].excerpt.as_deref(),
        Some("Broccolini tolerates light frost.")
    );
}

/// The store is keyed by URL, so a second row for a page the turn cited
/// twice would only overwrite the first.
#[test]
fn one_page_cited_twice_is_kept_once() {
    let kept = cited_web_sources(&[
        web("https://example.com/a", "A", "first"),
        web("https://EXAMPLE.com/a", "A again", "second"),
    ]);

    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].excerpt.as_deref(), Some("first"));
}

/// A deep-research turn can cite dozens of pages. The prompt window that
/// reads these takes ten, so one turn must not fill it on its own and push
/// every earlier round of the conversation out.
#[test]
fn one_turn_cannot_crowd_out_every_earlier_round() {
    let sources: Vec<SourceDto> = (0..40)
        .map(|i| web(&format!("https://example.com/{i}"), "Page", "text"))
        .collect();

    assert_eq!(
        cited_web_sources(&sources).len(),
        CITED_WEB_SOURCES_KEPT_PER_TURN
    );
}

/// A result with no title carries its URL as its file name. Stored as a
/// title, the prompt entry would read `- <url> (<url>)`.
#[test]
fn a_url_standing_in_for_a_missing_title_is_not_stored_as_one() {
    let url = "https://example.com/untitled";
    let kept = cited_web_sources(&[web(url, url, "")]);

    assert_eq!(kept[0].title, None, "{kept:?}");
    assert_eq!(kept[0].excerpt, None, "{kept:?}");
}

#[test]
fn a_turn_that_cited_no_web_page_stores_nothing() {
    assert!(cited_web_sources(&[document("doc-1")]).is_empty());
    assert!(cited_web_sources(&[]).is_empty());
}
