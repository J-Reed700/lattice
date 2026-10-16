//! Restore original source passages from completed turns, not the assistant's
//! prose. Citation numbers belong to an answer, so recalled sources receive
//! fresh numbers alongside this turn's new evidence.

use std::collections::HashSet;
use std::sync::Arc;

use crate::features::conversation::chat::ports::ChatStorage;
use crate::features::qa::dto::SourceDto;
use crate::features::search::engine::query_expansion::dictionaries::select_informative_terms;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::conversation::{ConversationMessage, MessageRole};
use lattice_core::shared::error::Result;
use lattice_core::shared::text::{build_excerpt, safe_truncate};

use super::retrieval::WEB_SOURCE_PREFIX;

/// Below this many characters a recalled passage is too short to carry a
/// claim (the grounding check reads sources in windows of this size). It
/// decides how many sources the budget can carry, not how long any one may
/// be: a source the budget has room for is recalled whole.
const MIN_RECALLED_PASSAGE_CHARS: usize = 1_200;

/// What a recall brought back, and how many more earlier sources matched the
/// question but had no room.
#[derive(Debug, Default)]
pub(super) struct Recalled {
    pub(super) sources: Vec<SourceDto>,
    pub(super) left_out: usize,
}

/// Search the entire saved source history before applying a prompt budget.
/// An old citation is not excluded merely because newer research followed it.
/// Document evidence still obeys the current scope and explicit focus.
///
/// Sources are read from the archive one at a time, best first, and reading
/// stops at the first the budget cannot give a useful share: a long history
/// does not load every page it ever cited to recall a few of them. The ones
/// left out are counted, and the prompt says so.
pub(super) async fn recall(
    container: &dyn ChatStorage,
    conversation_id: &str,
    question: &str,
    allowed_documents: &HashSet<String>,
    existing: &[SourceDto],
    token_budget: usize,
    llm: &Arc<dyn LLMPort>,
) -> Result<Recalled> {
    let Some(aggregate) = container
        .conversation_history()
        .get_conversation(conversation_id)
        .await?
    else {
        return Ok(Recalled::default());
    };
    let mut terms = query_terms(question);
    if terms.len() < 3 {
        if let Some(previous) = aggregate
            .messages()
            .iter()
            .rev()
            .find(|message| message.is_completed() && message.role == MessageRole::User)
        {
            terms.extend(query_terms(&previous.content));
        }
    }
    let candidates = rank_sources(aggregate.messages(), &terms, allowed_documents, existing);
    let matched = candidates.len();
    if token_budget == 0 {
        return Ok(Recalled {
            sources: Vec::new(),
            left_out: matched,
        });
    }
    let budget_chars = lattice_core::application::ports::llm_port::chars_within_tokens(
        token_budget,
        llm.chars_per_token(),
    );
    let mut loaded = Vec::new();
    for mut source in candidates {
        if source.document_id.starts_with(WEB_SOURCE_PREFIX) {
            let url = source.path.as_deref().unwrap_or(&source.file_path);
            if let Some(page) =
                super::source_snapshots::archived_page(container, conversation_id, url).await
            {
                source.content = page.content;
            }
        }
        loaded.push(source);
        if useful_count(&loaded, budget_chars) < loaded.len() {
            loaded.pop();
            break;
        }
    }
    let mut recalled = Vec::new();
    let mut used = 0;
    for source in fit_to_budget(loaded, &terms, budget_chars) {
        // The character shares err short; the count is the backend's own.
        let cost = llm.count_tokens(&render_source(&source));
        if used + cost > token_budget {
            continue;
        }
        used += cost;
        recalled.push(source);
    }
    Ok(Recalled {
        left_out: matched - recalled.len(),
        sources: recalled,
    })
}

/// How many of `sources`, from the front, `budget_chars` gives every one a
/// useful share of.
///
/// Adding a source never raises another's share, so once a prefix leaves one
/// short every longer prefix does too, and the longest that works is found by
/// halving: a few allocations, not one for every prefix (which made recalling
/// a long research chat's sources cubic in their number).
fn useful_count(sources: &[SourceDto], budget_chars: usize) -> usize {
    let lengths: Vec<usize> = sources
        .iter()
        .map(|source| source.content.trim().chars().count())
        .collect();
    let all_useful = |count: usize| {
        let prefix = lengths.get(..count).unwrap_or_default();
        super::retrieval::page_budget::allocate(prefix, budget_chars)
            .iter()
            .zip(prefix)
            .all(|(share, length)| *share >= (*length).clamp(1, MIN_RECALLED_PASSAGE_CHARS))
    };
    let (mut works, mut fails) = (0usize, lengths.len() + 1);
    while fails - works > 1 {
        let middle = works + (fails - works) / 2;
        if all_useful(middle) {
            works = middle;
        } else {
            fails = middle;
        }
    }
    works
}

/// Every ranked source the budget can carry, best first, each given a fair
/// share of `budget_chars`: a source shorter than its share is recalled whole
/// and leaves the rest to the others, a longer one is cut to the stretch that
/// best matches `terms` and marked as an excerpt. Sources are taken in rank
/// order while every one taken still gets a useful share, so how many are
/// recalled follows the budget rather than a fixed count.
fn fit_to_budget(sources: Vec<SourceDto>, terms: &[String], budget_chars: usize) -> Vec<SourceDto> {
    let taken = useful_count(&sources, budget_chars);
    let lengths: Vec<usize> = sources
        .iter()
        .take(taken)
        .map(|source| source.content.trim().chars().count())
        .collect();
    let shares = super::retrieval::page_budget::allocate(&lengths, budget_chars);
    sources
        .into_iter()
        .take(taken)
        .zip(shares)
        .map(|(mut source, share)| {
            let content = source.content.trim();
            if content.chars().count() > share {
                // The excerpt rounds out to a sentence, which can run past
                // the share; the share is what was budgeted, so it binds.
                let room = share.saturating_sub(4);
                let mut excerpt = safe_truncate(&build_excerpt(content, terms, room), room);
                excerpt.push_str(" […]");
                source.content = excerpt;
            } else {
                source.content = content.to_string();
            }
            source.excerpt = Some(source.content.clone());
            source
        })
        .collect()
}

fn query_terms(question: &str) -> Vec<String> {
    let cleaned = super::retrieval::search_text_without_url_tracking(question);
    let terms = cleaned
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|term| term.len() >= 3)
        .map(str::to_lowercase)
        .collect();
    select_informative_terms(terms, 24)
}

fn rank_sources(
    messages: &[ConversationMessage],
    terms: &[String],
    allowed_documents: &HashSet<String>,
    existing: &[SourceDto],
) -> Vec<SourceDto> {
    let seen: HashSet<String> = existing.iter().map(source_key).collect();
    let mut positions = std::collections::HashMap::<String, usize>::new();
    let mut ranked: Vec<(usize, SourceDto)> = Vec::new();
    for message in messages
        .iter()
        .rev()
        .filter(|message| message.is_completed() && message.role == MessageRole::Assistant)
    {
        let Some(metadata) = message
            .metadata
            .as_deref()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        else {
            continue;
        };
        let Some(sources) = metadata
            .get("sources")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        for value in sources {
            let Ok(mut source) = serde_json::from_value::<SourceDto>(value.clone()) else {
                continue;
            };
            if !source.document_id.starts_with(WEB_SOURCE_PREFIX)
                && !allowed_documents.contains(&source.document_id)
            {
                continue;
            }
            if source.content.trim().is_empty() {
                continue;
            }
            let title = source.file_name.to_lowercase();
            let body = source.content.to_lowercase();
            let score: usize = terms
                .iter()
                .map(|term| {
                    usize::from(title.contains(term)) * 3 + usize::from(body.contains(term))
                })
                .sum();
            let key = source_key(&source);
            if score == 0 || seen.contains(&key) {
                continue;
            }
            source.citation_id = None;
            // Old web-result-N ids are reused by every search. Give recalled
            // passages a distinct identity before the shared citation map runs.
            if !source.chunk_id.starts_with("recalled:") {
                source.chunk_id = format!("recalled:{}:{}", source.document_id, source.chunk_id);
            }
            if let Some(index) = positions.get(&key) {
                if let Some(existing) = ranked.get_mut(*index) {
                    // A later answer may carry only a clipped excerpt. Keep
                    // the original, fuller evidence when it is still saved.
                    if source.content.len() > existing.1.content.len() {
                        *existing = (score, source);
                    }
                }
            } else {
                positions.insert(key, ranked.len());
                ranked.push((score, source));
            }
        }
    }
    // Stable ties retain newest-first order; relevance beats recency.
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    ranked.into_iter().map(|(_, source)| source).collect()
}

pub(super) fn source_key(source: &SourceDto) -> String {
    if source.document_id.starts_with(WEB_SOURCE_PREFIX) {
        source.document_id.clone()
    } else {
        // Strip the recall wrapper so carrying a passage again cannot multiply it.
        let prefix = format!("recalled:{}:", source.document_id);
        format!(
            "{}:{}",
            source.document_id,
            source
                .chunk_id
                .strip_prefix(&prefix)
                .unwrap_or(&source.chunk_id)
        )
    }
}

fn render_source(source: &SourceDto) -> String {
    format!(
        "[{}] {}\nDocument ID: {}\nLocation: {}\nPreviously read source passage: {}",
        source.citation_id.unwrap_or(9999),
        source.file_name,
        source.document_id,
        source.path.as_deref().unwrap_or(&source.file_path),
        source.content
    )
}

pub(super) fn render(sources: &[SourceDto], left_out: usize) -> Option<String> {
    if sources.is_empty() && left_out == 0 {
        return None;
    }
    let mut rendered = format!(
        "Evidence retained from earlier in this conversation. These are original source passages, \
         citable with the numbers below. Combine them with new evidence; a new search does not \
         by itself invalidate prior findings. Apply later corrections and reconcile conflicting evidence. Do not reuse citation numbers from earlier assistant messages.\n\n{}",
        sources.iter().map(render_source).collect::<Vec<_>>().join("\n\n")
    );
    if left_out > 0 {
        rendered.push_str(&format!(
            "\n\n[{left_out} more source(s) cited earlier in this conversation match this question but had no room here. Search or read them again if the passages above are not enough.]"
        ));
    }
    Some(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::domain::conversation::ConversationAggregate;

    fn source(id: &str, body: &str) -> SourceDto {
        SourceDto {
            document_id: id.into(),
            chunk_id: "web-result-1".into(),
            content: body.into(),
            score: 1.0,
            file_name: body.into(),
            file_path: "https://example.com/growing".into(),
            path: None,
            mime_type: "text/html".into(),
            category: "Web Article".into(),
            file_size_bytes: 0,
            modified_at: String::new(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            chunk_excerpts: None,
            citation_id: Some(1),
            page_number: None,
            from_ocr: false,
            position: None,

            web_snapshot: None,
        }
    }

    fn message(source: SourceDto) -> ConversationMessage {
        let mut aggregate = ConversationAggregate::new("test".into(), None).unwrap();
        aggregate
            .add_message(MessageRole::Assistant, "An earlier answer [1]".into(), 10)
            .unwrap();
        let mut message = aggregate.messages().last().unwrap().clone();
        message.metadata = Some(serde_json::json!({"sources": [source]}).to_string());
        message
    }

    #[test]
    fn recalls_old_original_evidence_past_ten_newer_searches_with_new_citation_ids() {
        let old = source(
            "web:https://example.com/grow-lights",
            "Blueberry grow light requirements: original evidence.",
        );
        let mut messages = vec![message(old.clone())];
        for i in 0..30 {
            messages.push(message(source(
                &format!("web:https://example.com/solar/{i}"),
                "Solar panels for electricity",
            )));
        }
        let current = source("web:https://example.com/current", "new source");
        let recalled = rank_sources(
            &messages,
            &query_terms("blueberry grow lights"),
            &HashSet::new(),
            std::slice::from_ref(&current),
        );
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].content, old.content);
        assert_eq!(recalled[0].citation_id, None);
        assert_ne!(recalled[0].chunk_id, current.chunk_id);
        let mut combined = vec![current];
        combined.extend(recalled);
        super::super::retrieval::assign_citation_ids(&mut combined);
        assert_eq!(combined[1].citation_id, Some(2));
        let prompt = render(&combined[1..], 0).unwrap();
        assert!(prompt.contains("[2]"));
        assert!(prompt.contains("original evidence"));
        assert!(!prompt.contains("An earlier answer"));
    }

    #[test]
    fn scoped_documents_failed_answers_and_current_duplicates_are_excluded() {
        let web = source(
            "web:https://example.com/grow-lights",
            "Blueberry grow light evidence",
        );
        let doc = source("private-doc", "Blueberry grow light evidence");
        let mut failed = message(source(
            "web:https://example.com/failed",
            "Blueberry evidence",
        ));
        failed.status = "failed".into();
        let messages = vec![message(web.clone()), message(doc), failed];
        let terms = query_terms("blueberry light");
        assert!(rank_sources(
            &messages,
            &terms,
            &HashSet::new(),
            std::slice::from_ref(&web)
        )
        .is_empty());
        let allowed = ["private-doc".to_string()].into_iter().collect();
        let recalled = rank_sources(&messages, &terms, &allowed, &[web]);
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].document_id, "private-doc");
    }

    #[test]
    fn a_later_excerpt_does_not_erase_the_original_document_passage() {
        let original = source(
            "doc",
            "Blueberry light requirements and the complete original growing instructions.",
        );
        let mut clipped = original.clone();
        clipped.content = "Blueberry light requirements".into();
        clipped.chunk_id = format!("recalled:{}:{}", original.document_id, original.chunk_id);
        let messages = vec![message(original.clone()), message(clipped)];
        let allowed = ["doc".to_string()].into_iter().collect();
        let recalled = rank_sources(&messages, &query_terms("blueberry light"), &allowed, &[]);
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].content, original.content);
    }

    #[test]
    fn recurring_recall_does_not_duplicate_passage_identity() {
        let original = source("doc", "Blueberry light evidence");
        let mut recalled = original.clone();
        recalled.chunk_id = format!("recalled:{}:{}", original.document_id, original.chunk_id);
        assert_eq!(source_key(&original), source_key(&recalled));
    }

    fn recalled(count: usize, body: &str) -> Vec<SourceDto> {
        (0..count)
            .map(|i| source(&format!("doc-{i}"), body))
            .collect()
    }

    /// The old recall took eight sources and cut each to 2,400 characters
    /// whatever the budget. With room, a long archived page comes back whole,
    /// and the ninth source is not dropped.
    #[test]
    fn a_budget_with_room_recalls_every_source_whole() {
        let page = format!("{}the closing finding", "Long page text. ".repeat(1_000));
        let mut sources = vec![source("web:https://example.com/a", &page)];
        sources.extend(recalled(11, &"A vault passage. ".repeat(100)));
        let fitted = fit_to_budget(sources, &["finding".to_string()], 1_000_000);
        assert_eq!(fitted.len(), 12);
        assert!(fitted[0].content.ends_with("the closing finding"));
        assert!(fitted.iter().all(|source| !source.content.ends_with("[…]")));
    }

    /// A tight budget shares itself out: a long source is cut to its share and
    /// says so, and only as many sources are taken as can each be given a
    /// passage worth citing.
    #[test]
    fn a_tight_budget_shares_out_excerpts_and_stops_where_shares_get_too_small() {
        let page = "Long page text. ".repeat(1_000);
        let mut sources = vec![source("web:https://example.com/a", &page)];
        sources.extend(recalled(20, &"A vault passage. ".repeat(200)));
        let budget = 6_000;
        let fitted = fit_to_budget(sources, &[], budget);
        assert_eq!(fitted.len(), budget / MIN_RECALLED_PASSAGE_CHARS);
        assert!(
            fitted[0].content.ends_with("[…]"),
            "the long page is an excerpt"
        );
        let total: usize = fitted.iter().map(|s| s.content.chars().count()).sum();
        assert!(total <= budget, "{total} characters in a {budget} budget");
    }

    #[test]
    fn short_sources_are_never_cut() {
        let fitted = fit_to_budget(recalled(3, "A short finding."), &[], 1_000);
        assert_eq!(fitted.len(), 3);
        assert!(fitted.iter().all(|s| s.content == "A short finding."));
    }

    /// Sources are read in rank order and reading stops at the first the
    /// budget has no useful share for; those not recalled are counted in the
    /// prompt instead of vanishing.
    #[test]
    fn reading_stops_at_the_first_source_without_room_and_the_rest_are_counted() {
        let sources = recalled(10, &"A long finding. ".repeat(300));
        let budget = 3 * MIN_RECALLED_PASSAGE_CHARS;
        assert_eq!(useful_count(&sources, budget), 3);
        let note = render(&fit_to_budget(sources, &[], budget), 7).unwrap();
        assert!(note.contains("7 more source(s)"), "{note}");
        assert!(render(&[], 2).unwrap().contains("2 more source(s)"));
        assert_eq!(render(&[], 0), None);
    }

    /// The longest prefix every source of which gets a useful share, found
    /// by halving, is the one trying every prefix in turn found; and two
    /// thousand sources take a handful of allocations, not two thousand.
    #[test]
    fn the_useful_count_is_found_by_halving_and_matches_every_prefix() {
        let every_prefix =
            |lengths: &[usize], budget: usize| {
                let mut taken = 0;
                for count in 1..=lengths.len() {
                    let prefix = &lengths[..count];
                    let shares = super::super::retrieval::page_budget::allocate(prefix, budget);
                    if !shares.iter().zip(prefix).all(|(share, length)| {
                        *share >= (*length).clamp(1, MIN_RECALLED_PASSAGE_CHARS)
                    }) {
                        break;
                    }
                    taken = count;
                }
                taken
            };
        let mut seed = 7u64;
        for case in 0..60 {
            let count = (case % 12) + 1;
            let lengths: Vec<usize> = (0..count)
                .map(|_| {
                    seed = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    (seed >> 33) as usize % 4_000 + 1
                })
                .collect();
            let sources: Vec<SourceDto> = lengths
                .iter()
                .map(|length| source("doc", &"x".repeat(*length)))
                .collect();
            for budget in [0, 500, 2_400, 9_000, 40_000] {
                assert_eq!(
                    useful_count(&sources, budget),
                    every_prefix(&lengths, budget),
                    "{lengths:?} in {budget}"
                );
            }
        }
        let many: Vec<SourceDto> = (0..2_000).map(|_| source("doc", "short note")).collect();
        assert_eq!(useful_count(&many, 2_000 * 10), 2_000);
    }
}
