//! Permanent per-conversation archives of the pages a conversation read.
//!
//! The global page cache expires after a day and a live page can change or
//! vanish. A citation is a record of what the user was *shown*, so every page
//! a conversation reads is archived beside its citation — once, first text
//! wins — and later reads of the same URL in the same conversation are served
//! from the archive instead of the network.

use crate::features::conversation::chat::ports::{
    ChatRecords, ChatRuntime, ChatStorage, FetchUrlContentOutput,
};
use lattice_core::application::contracts::sources::{SourceDto, WebSnapshotDto};
use std::collections::HashMap;
use tracing::warn;

mod capture;
pub use self::capture::CapturedSourcesDto;
pub(super) use self::capture::{pending_capture, CitationCapture, UncapturedPage, CAPTURE_KEY};

/// The page as the conversation archived it, or `None` when it never read it.
pub(super) async fn archived_page(
    container: &dyn ChatStorage,
    conversation_id: &str,
    url: &str,
) -> Option<FetchUrlContentOutput> {
    let snapshot = match container
        .chat_records()
        .web_source_snapshot(conversation_id, url)
        .await
    {
        Ok(snapshot) => snapshot?,
        Err(error) => {
            // A broken archive must not break the read: fall through to the
            // live fetch as if nothing were stored.
            warn!(error = %error, url, "Could not read archived page; fetching live");
            return None;
        }
    };
    Some(FetchUrlContentOutput {
        url: url.to_string(),
        title: snapshot.title,
        word_count: snapshot.content.split_whitespace().count(),
        content: snapshot.content,
        content_truncated: snapshot.truncated,
        fetch_time_ms: 0.0,
        content_type: Some("text/html".to_string()),
        from_cache: true,
    })
}

/// Read a snapshot in the citation DTO shape without changing source excerpts.
pub(super) async fn web_snapshot(
    container: &dyn ChatStorage,
    conversation_id: &str,
    url: &str,
) -> Option<WebSnapshotDto> {
    let snapshot = match container
        .chat_records()
        .web_source_snapshot(conversation_id, url)
        .await
    {
        Ok(snapshot) => snapshot?,
        Err(error) => {
            warn!(error = %error, url, "Could not read archived citation snapshot");
            return None;
        }
    };
    Some(WebSnapshotDto {
        url: url.to_string(),
        title: snapshot.title,
        text: snapshot.content,
        fetched_at: snapshot.fetched_at,
        truncated: snapshot.truncated,
    })
}

/// Store a page against the URL the citation points to. Fetches can redirect;
/// retaining the requested URL lets a later citation resolve its own archive.
pub(super) async fn archive_page_for_url(
    container: &dyn ChatStorage,
    conversation_id: &str,
    citation_url: &str,
    page: &FetchUrlContentOutput,
) {
    // The whole page. The archive is a prompt ingredient: later reads of the
    // URL, recalled prior evidence and the grounding check all take their text
    // from it, and each fits what it takes to its own budget. Cutting here
    // would lose the tail for every one of them. The fetch's 5 MiB body bound
    // is what keeps a pathological page from becoming a blob dump.
    let Some(content) = snapshot_text(&page.content) else {
        return;
    };
    match container
        .chat_records()
        .store_web_source_snapshot(
            conversation_id,
            citation_url,
            page.title.clone(),
            content,
            page.content_truncated,
        )
        .await
    {
        // The first text snapshot also asks for the page's frozen copy: a row
        // insert and a job submit, the capture itself runs as a job.
        Ok(()) => {
            container
                .ensure_page_snapshot(conversation_id, citation_url)
                .await
        }
        Err(error) => warn!(
            error = %error,
            url = page.url.as_str(),
            "Failed to archive a read page beside its citation"
        ),
    }
}

/// The turn's sources as the answering model read them.
///
/// A web citation carries the search engine's snippet — a sentence or two —
/// while the prompt carried the whole page. Checking the answer against the
/// snippet marked nearly every well-cited sentence unsupported, which reads to
/// the user as invented citations. Each web source whose page this
/// conversation archived is given that page's text for verification; the
/// sources persisted with the message are left as they were.
pub(super) async fn with_archived_page_text(
    repository: &dyn ChatRecords,
    conversation_id: &str,
    sources: &[SourceDto],
) -> Vec<SourceDto> {
    let mut evidence = sources.to_vec();
    for source in &mut evidence {
        let url = source.path.as_deref().unwrap_or(source.file_path.as_str());
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            continue;
        }
        let url = url.to_string();
        let snapshot = repository.web_source_snapshot(conversation_id, &url).await;
        if let Ok(Some(snapshot)) = snapshot {
            if !snapshot.content.trim().is_empty() {
                source.content = snapshot.content;
            }
        }
    }
    evidence
}

/// Attach immutable page evidence to sources that the assistant actually
/// cited, from what this conversation already archived. A cited search
/// snippet whose page was never read is returned instead of read here: the
/// read is a network call, so it runs after the answer is saved
/// ([`CitationCapture`]).
pub(super) async fn attach_archived_snapshots(
    container: &dyn ChatStorage,
    conversation_id: &str,
    response: &str,
    sources: &mut [SourceDto],
) -> Vec<UncapturedPage> {
    let cited_ids: std::collections::HashSet<u32> = regex::Regex::new(r"\[(\d+)\]")
        .ok()
        .into_iter()
        .flat_map(|regex| {
            regex
                .captures_iter(response)
                .filter_map(|capture| capture.get(1)?.as_str().parse::<u32>().ok())
                .collect::<Vec<_>>()
        })
        .collect();
    let mut pending: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, source) in sources.iter_mut().enumerate() {
        if source.web_snapshot.is_some() {
            continue;
        }
        let url = source.file_path.trim();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            continue;
        }
        if let Some(snapshot) = web_snapshot(container, conversation_id, url).await {
            source.web_snapshot = Some(snapshot);
        } else if source.citation_id.is_some_and(|id| cited_ids.contains(&id)) {
            pending.entry(url.to_string()).or_default().push(index);
        }
    }
    pending
        .into_iter()
        .map(|(url, sources)| UncapturedPage { url, sources })
        .collect()
}

/// Add snapshots to older message metadata on read. This only consults the
/// requested conversation's archive and never reaches the network.
pub(crate) async fn hydrate_message_metadata(
    container: &dyn ChatRuntime,
    conversation_id: &str,
    metadata: Option<String>,
) -> Option<String> {
    let raw = metadata.as_deref()?;
    let Some(mut value) = metadata_value_with_sources(raw) else {
        return metadata;
    };
    let Some(sources) = value
        .get_mut("sources")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return metadata;
    };
    let mut changed = false;
    for item in sources {
        let Ok(mut source) = serde_json::from_value::<SourceDto>(item.clone()) else {
            continue;
        };
        if source.web_snapshot.is_some() {
            continue;
        }
        let url = source.file_path.trim();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            continue;
        }
        if let Some(snapshot) = web_snapshot(container, conversation_id, url).await {
            source.web_snapshot = Some(snapshot);
            if let Ok(enriched) = serde_json::to_value(source) {
                *item = enriched;
                changed = true;
            }
        }
    }
    changed.then(|| value.to_string()).or(metadata)
}

fn metadata_value_with_sources(raw: &str) -> Option<serde_json::Value> {
    let value = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    value.get("sources")?.as_array()?;
    Some(value)
}

/// The text worth archiving, whole. `None` for a page with no text worth
/// keeping.
fn snapshot_text(content: &str) -> Option<String> {
    let content = content.trim();
    (!content.is_empty()).then(|| content.to_string())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod capture_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_archive_keeps_the_whole_page_however_long() {
        assert_eq!(
            snapshot_text("  an article body  ").as_deref(),
            Some("an article body")
        );

        // Far past the old 64,000-character cut: every word is kept, so a
        // later read, a recalled source and the grounding check see the tail.
        let long = format!("{}the closing sentence", "word ".repeat(100_000));
        let kept = snapshot_text(&long).unwrap();
        assert_eq!(kept.chars().count(), long.trim().chars().count());
        assert!(kept.ends_with("the closing sentence"));

        assert!(snapshot_text("   ").is_none());
    }

    #[test]
    fn source_snapshot_serializes_as_optional_camel_case_evidence() {
        let mut source: SourceDto = serde_json::from_value(serde_json::json!({
            "documentId": "web:https://example.com/a",
            "chunkId": "web-a",
            "content": "The original short search excerpt.",
            "score": 0.8,
            "path": "https://example.com/a",
            "position": null,
            "fileName": "Example",
            "filePath": "https://example.com/a",
            "mimeType": "text/html",
            "category": "Web Page",
            "fileSizeBytes": 0,
            "modifiedAt": "",
            "citationId": 1
        }))
        .unwrap();
        assert!(source.web_snapshot.is_none());
        source.web_snapshot = Some(WebSnapshotDto {
            url: "https://example.com/a".into(),
            title: Some("Example page".into()),
            text: "Captured full page text.".into(),
            fetched_at: Some("2026-09-28T12:00:00Z".into()),
            truncated: false,
        });
        let json = serde_json::to_value(&source).unwrap();
        assert_eq!(source.content, "The original short search excerpt.");
        assert_eq!(json["webSnapshot"]["text"], "Captured full page text.");
        assert!(json.get("web_snapshot").is_none());
    }

    #[test]
    fn old_or_non_source_metadata_is_preserved_as_unhydratable() {
        assert!(metadata_value_with_sources("not json").is_none());
        assert!(metadata_value_with_sources(r#"{"attachments":["a.pdf"]}"#).is_none());
        assert!(metadata_value_with_sources(r#"{"sources":{}}"#).is_none());
        assert!(metadata_value_with_sources(r#"{"sources":[]}"#).is_some());
    }
}
