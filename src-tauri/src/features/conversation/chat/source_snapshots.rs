//! Permanent per-conversation archives of the pages a conversation read.
//!
//! The global page cache expires after a day and a live page can change or
//! vanish. A citation is a record of what the user was *shown*, so every page
//! a conversation reads is archived beside its citation — once, first text
//! wins — and later reads of the same URL in the same conversation are served
//! from the archive instead of the network.

use crate::features::conversation::repository::ConversationRepository;
use crate::features::function_calling::dto::FetchUrlContentOutput;
use crate::features::qa::dto::{SourceDto, WebSnapshotDto};
use crate::interfaces::di::Container;
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use std::time::Duration;
use tracing::warn;

/// The most page text archived beside one citation.
///
/// A snapshot is not a prompt ingredient — the prompt keeps its short excerpt
/// pointer — so the cap only stops a pathological page from becoming a blob
/// dump. 64k chars is far past any real article.
const SNAPSHOT_MAX_CHARS: usize = 64_000;
const CITATION_CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

/// The page as the conversation archived it, or `None` when it never read it.
pub(super) async fn archived_page(
    container: &Container,
    conversation_id: &str,
    url: &str,
) -> Option<FetchUrlContentOutput> {
    let repository = ConversationRepository::new(container.db_pool().clone());
    let snapshot = match repository
        .conversation_web_source_snapshot(conversation_id.to_string(), url.to_string())
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
    container: &Container,
    conversation_id: &str,
    url: &str,
) -> Option<WebSnapshotDto> {
    let repository = ConversationRepository::new(container.db_pool().clone());
    let snapshot = match repository
        .conversation_web_source_snapshot(conversation_id.to_string(), url.to_string())
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
    container: &Container,
    conversation_id: &str,
    citation_url: &str,
    page: &FetchUrlContentOutput,
) {
    let Some((content, clipped)) = capped_snapshot_text(&page.content) else {
        return;
    };
    let repository = ConversationRepository::new(container.db_pool().clone());
    if let Err(error) = repository
        .store_conversation_web_source_snapshot(
            conversation_id.to_string(),
            citation_url.to_string(),
            page.title.clone(),
            content,
            page.content_truncated || clipped,
        )
        .await
    {
        warn!(
            error = %error,
            url = page.url.as_str(),
            "Failed to archive a read page beside its citation"
        );
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
    repository: &ConversationRepository,
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
        let snapshot = repository
            .conversation_web_source_snapshot(conversation_id.to_string(), url)
            .await;
        if let Ok(Some(snapshot)) = snapshot {
            if !snapshot.content.trim().is_empty() {
                source.content = snapshot.content;
            }
        }
    }
    evidence
}

/// Attach immutable page evidence to sources that the assistant actually
/// cited. If a search snippet was cited without a page read, make one bounded
/// capture attempt now; the answer remains successful if capture fails.
pub(super) async fn attach_cited_web_snapshots(
    container: &Container,
    conversation_id: &str,
    response: &str,
    sources: &mut [SourceDto],
) {
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

    // Concurrent bounded reads keep capture latency close to one read rather
    // than one timeout per citation. Every distinct cited URL gets an
    // attempt, while at most four requests are active at once. `read_page`
    // validates URLs and applies its normal redirect and network policy.
    let reads = stream::iter(pending.into_iter().map(|(url, indices)| async move {
        let result = tokio::time::timeout(
            CITATION_CAPTURE_TIMEOUT,
            container.web_service().read_page(&url),
        )
        .await;
        (url, indices, result)
    }))
    .buffer_unordered(4)
    .collect::<Vec<_>>()
    .await;
    for (url, indices, result) in reads {
        let Ok(Ok(read)) = result else {
            continue;
        };
        archive_page_for_url(container, conversation_id, &url, &read.output).await;
        if let Some(snapshot) = web_snapshot(container, conversation_id, &url).await {
            for index in indices {
                if let Some(source) = sources.get_mut(index) {
                    source.web_snapshot = Some(snapshot.clone());
                }
            }
        }
    }
}

/// Add snapshots to older message metadata on read. This only consults the
/// requested conversation's archive and never reaches the network.
pub(crate) async fn hydrate_message_metadata(
    container: &Container,
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

/// The text worth archiving, capped, plus whether the cap cut anything.
/// `None` for a page with no text worth keeping.
fn capped_snapshot_text(content: &str) -> Option<(String, bool)> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    if content.chars().count() > SNAPSHOT_MAX_CHARS {
        Some((
            crate::shared::text_utils::safe_truncate(content, SNAPSHOT_MAX_CHARS),
            true,
        ))
    } else {
        Some((content.to_string(), false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_archive_keeps_the_whole_article_and_marks_only_a_genuine_cut() {
        let (kept, clipped) = capped_snapshot_text("  an article body  ").unwrap();
        assert_eq!(kept, "an article body");
        assert!(!clipped);

        let long = "word ".repeat(SNAPSHOT_MAX_CHARS);
        let (kept, clipped) = capped_snapshot_text(&long).unwrap();
        assert!(kept.chars().count() <= SNAPSHOT_MAX_CHARS);
        assert!(clipped);

        assert!(capped_snapshot_text("   ").is_none());
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
