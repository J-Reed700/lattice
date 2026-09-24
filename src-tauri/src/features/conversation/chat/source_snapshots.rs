//! Permanent per-conversation archives of the pages a conversation read.
//!
//! The global page cache expires after a day and a live page can change or
//! vanish. A citation is a record of what the user was *shown*, so every page
//! a conversation reads is archived beside its citation — once, first text
//! wins — and later reads of the same URL in the same conversation are served
//! from the archive instead of the network.

use crate::features::conversation::repository::ConversationRepository;
use crate::features::function_calling::dto::FetchUrlContentOutput;
use crate::features::qa::dto::SourceDto;
use crate::interfaces::di::Container;
use tracing::warn;

/// The most page text archived beside one citation.
///
/// A snapshot is not a prompt ingredient — the prompt keeps its short excerpt
/// pointer — so the cap only stops a pathological page from becoming a blob
/// dump. 64k chars is far past any real article.
const SNAPSHOT_MAX_CHARS: usize = 64_000;

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

/// Archive a page the conversation just read. First snapshot wins — see the
/// migration note. A failure here is logged and dropped: the page was read
/// and the answer stands; the archive is a bonus, not a requirement.
pub(super) async fn archive_page(
    container: &Container,
    conversation_id: &str,
    page: &FetchUrlContentOutput,
) {
    let Some((content, clipped)) = capped_snapshot_text(&page.content) else {
        return;
    };
    let repository = ConversationRepository::new(container.db_pool().clone());
    if let Err(error) = repository
        .store_conversation_web_source_snapshot(
            conversation_id.to_string(),
            page.url.clone(),
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
    container: &Container,
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
        if let Some(page) = archived_page(container, conversation_id, &url).await {
            if !page.content.trim().is_empty() {
                source.content = page.content;
            }
        }
    }
    evidence
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
}
