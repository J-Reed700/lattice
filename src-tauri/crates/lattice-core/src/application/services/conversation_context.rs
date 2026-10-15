//! Assemble chat context through read-only application ports.

use crate::application::ports::conversation_context::ConversationContextPort;
use crate::application::services::context_window_builder::ContextWindowBuilder;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;

/// Share of the window the list of pages linked to the chat may take. Every
/// page is listed by title and address while that share holds them, so the
/// model knows each one exists and can fetch it; excerpts fill what is left,
/// newest first.
const LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_RATIO: f64 = 0.08;

/// Linked sources read per query while the list is being filled.
const LINKED_SOURCE_PAGE_ROWS: usize = 64;

async fn build_linked_web_sources_prompt_context(
    supplemental: &dyn ConversationContextPort,
    conversation_id: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    max_tokens: usize,
) -> Result<Option<String>> {
    let token_budget =
        ((max_tokens as f64) * LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_RATIO).round() as usize;
    let count = |text: &str| llm.count_tokens(text);
    // Read a page of rows at a time, newest first, only until the list no
    // longer fits: a long-running research chat links hundreds of pages, and
    // every turn loaded all of them to show the few its share holds.
    let mut rows = Vec::new();
    let total = loop {
        let page = supplemental
            .linked_sources(conversation_id, rows.len(), LINKED_SOURCE_PAGE_ROWS)
            .await?;
        let fetched = page.len();
        rows.extend(page);
        if fetched < LINKED_SOURCE_PAGE_ROWS {
            break rows.len();
        }
        if linked_source_entries(&rows, token_budget, count).1 > 0 {
            break supplemental.linked_source_count(conversation_id).await?;
        }
    };
    let (entries, _) = linked_source_entries(&rows, token_budget, count);
    let unlisted = total.saturating_sub(entries.len());

    if entries.is_empty() {
        return Ok(None);
    }

    // Spelled out because these entries carry no passage numbers while the RAG
    // template, which renders them, instructs the model to cite numerically.
    // Left implicit, a model handed a titled URL and told to cite will invent a
    // number for it, and an invented number is worse than an uncited sentence.
    let unlisted_note = if unlisted > 0 {
        format!(
            "\n- …and {unlisted} older linked page{} not listed here for lack of room.",
            if unlisted == 1 { "" } else { "s" }
        )
    } else {
        String::new()
    };
    Ok(Some(format!(
        "Web sources on record for this conversation (links and short excerpts \
that earlier turns cited or the user attached — background, not numbered \
evidence):\n{}{unlisted_note}\n\nThese have no citation numbers, so do not cite them with \
brackets. Use them to stay consistent with what this conversation has already \
established. To rely on one for a new claim, open it with fetch_url_content \
first and cite what you read.",
        entries.join("\n")
    )))
}

/// The listed entries, newest first, and how many pages had no room.
///
/// Every page's title and address come first, newest first, while the budget
/// holds them; excerpts are then added whole, newest first, to the pages
/// whose excerpt still fits. An excerpt that does not fit leaves its page
/// listed without one, never cut mid-sentence.
fn linked_source_entries(
    rows: &[crate::application::ports::conversation_context::LinkedConversationSource],
    token_budget: usize,
    count: impl Fn(&str) -> usize,
) -> (Vec<String>, usize) {
    let listed: Vec<(String, Option<String>)> = rows
        .iter()
        .filter_map(|row| {
            let url = row.url.trim();
            if url.is_empty() {
                return None;
            }
            let title = row
                .title
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(url);
            let excerpt = row
                .excerpt
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|excerpt| format!("\n  Excerpt: {excerpt}"));
            Some((format!("- {title} ({url})"), excerpt))
        })
        .collect();

    let mut used = 0usize;
    let mut entries: Vec<String> = Vec::new();
    for (line, _) in &listed {
        let cost = count(line);
        if !entries.is_empty() && used.saturating_add(cost) > token_budget {
            break;
        }
        used = used.saturating_add(cost);
        entries.push(line.clone());
    }
    let unlisted = listed.len() - entries.len();
    for (entry, (_, excerpt)) in entries.iter_mut().zip(&listed) {
        let Some(excerpt) = excerpt else { continue };
        let cost = count(excerpt);
        if used.saturating_add(cost) <= token_budget {
            used = used.saturating_add(cost);
            entry.push_str(excerpt);
        }
    }
    (entries, unlisted)
}

pub async fn build_conversation_context(
    supplemental: Arc<dyn ConversationContextPort>,
    history: Arc<dyn crate::application::ports::ConversationHistoryPort>,
    conversation_id: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    max_tokens: usize,
    global_system_prompt: &str,
) -> Result<(
    Vec<String>,
    Vec<crate::domain::conversation::DocumentReference>,
    Option<String>,
    // The effective system prompt on its own, for callers that assemble the
    // prompt from typed parts instead of the `System:` entry in the context.
    Option<String>,
)> {
    let token_counter = {
        let llm = Arc::clone(llm);
        Arc::new(move |text: &str| llm.count_tokens(text))
    };

    let context_builder = ContextWindowBuilder::new(history.clone(), max_tokens, token_counter);

    let conversation_aggregate = history.get_conversation(conversation_id).await?;
    let conversation_system_prompt = conversation_aggregate
        .as_ref()
        .and_then(|aggregate| aggregate.system_prompt().map(|s| s.to_string()))
        .and_then(|prompt| {
            let trimmed = prompt.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
    let conversation_document_context = conversation_aggregate
        .as_ref()
        .map(|aggregate| aggregate.document_context().to_vec())
        .unwrap_or_default();
    let space_system_prompt =
        supplemental
            .space_prompt(conversation_id)
            .await?
            .and_then(|prompt: String| {
                let trimmed = prompt.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            });

    let aggregate = conversation_aggregate.as_ref().ok_or_else(|| {
        AppError::NotFound(format!("Conversation not found: {}", conversation_id))
    })?;
    let mut context = context_builder.build_from_aggregate(aggregate);
    let global_prompt = global_system_prompt.trim();
    let effective_system_prompt = conversation_system_prompt
        .or(space_system_prompt)
        .or_else(|| (!global_prompt.is_empty()).then(|| global_prompt.to_string()));
    if let Some(system_prompt) = &effective_system_prompt {
        let entry = format!("System: {}", system_prompt);
        if !context
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&entry))
        {
            context.insert(0, entry);
        }
    }

    let linked_web_sources_context = build_linked_web_sources_prompt_context(
        supplemental.as_ref(),
        conversation_id,
        llm,
        max_tokens,
    )
    .await?;

    Ok((
        context,
        conversation_document_context,
        linked_web_sources_context,
        effective_system_prompt,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::conversation_context::LinkedConversationSource;
    use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use crate::application::ports::{ConversationHistoryPort, LLMPort};
    use crate::domain::conversation::{ConversationAggregate, MessageRole};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct History {
        aggregate: Option<ConversationAggregate>,
        reads: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl ConversationHistoryPort for History {
        async fn get_conversation(&self, id: &str) -> Result<Option<ConversationAggregate>> {
            assert_eq!(id, "chat");
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.aggregate.clone())
        }
    }
    /// A model that is only asked to count tokens: the context builder
    /// never sends it a completion.
    struct Counter;
    #[async_trait::async_trait]
    impl LLMPort for Counter {
        async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
            Err(AppError::InvalidState(
                "the context builder never completes".into(),
            ))
        }
        fn model_name(&self) -> &str {
            "counter"
        }
        fn max_context_tokens(&self) -> usize {
            4096
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    struct Supplemental {
        prompt: Option<String>,
        fail: bool,
    }
    #[async_trait::async_trait]
    impl ConversationContextPort for Supplemental {
        async fn space_prompt(&self, _: &str) -> Result<Option<String>> {
            if self.fail {
                return Err(AppError::Database("context unavailable".into()));
            }
            Ok(self.prompt.clone())
        }
        async fn linked_sources(
            &self,
            _: &str,
            offset: usize,
            _: usize,
        ) -> Result<Vec<LinkedConversationSource>> {
            Ok(vec![LinkedConversationSource {
                title: Some(" source ".into()),
                url: " https://example.com ".into(),
                excerpt: Some("excerpt".into()),
            }]
            .into_iter()
            .skip(offset)
            .collect())
        }
        async fn linked_source_count(&self, _: &str) -> Result<usize> {
            Ok(1)
        }
    }

    #[tokio::test]
    async fn prompt_precedence_and_history_use_one_snapshot() {
        let llm: Arc<dyn LLMPort> = Arc::new(Counter);
        for (conversation, space, global, expected) in [
            (
                Some(" conversation "),
                Some("space"),
                "global",
                "System: conversation",
            ),
            (Some(" "), Some(" space "), "global", "System: space"),
            (None, Some(" "), " global ", "System: global"),
        ] {
            let mut aggregate = ConversationAggregate::new(
                "Title".into(),
                "model".into(),
                conversation.map(str::to_string),
            )
            .unwrap();
            aggregate
                .add_message(MessageRole::User, "hello".into(), 1)
                .unwrap();
            aggregate
                .add_message_with_status(
                    MessageRole::Assistant,
                    "unfinished".into(),
                    1,
                    "pending".into(),
                )
                .unwrap();
            let history = Arc::new(History {
                aggregate: Some(aggregate),
                reads: AtomicUsize::new(0),
            });
            let (context, _, links, system_prompt) = build_conversation_context(
                Arc::new(Supplemental {
                    prompt: space.map(str::to_string),
                    fail: false,
                }),
                history.clone(),
                "chat",
                &llm,
                1000,
                global,
            )
            .await
            .unwrap();
            assert_eq!(context, vec![expected, "User: hello"]);
            assert_eq!(
                system_prompt
                    .map(|prompt| format!("System: {prompt}"))
                    .as_deref(),
                Some(expected)
            );
            assert_eq!(history.reads.load(Ordering::SeqCst), 1);
            let links = links.unwrap();
            assert!(links.contains("- source (https://example.com)"));
            // These entries carry no passage numbers, but the template that
            // renders them orders numeric citations. Without saying so, a model
            // handed a titled URL and told to cite invents a number for it.
            assert!(
                links.contains("do not cite them with brackets"),
                "carried sources must be marked uncitable:\n{links}"
            );
            assert!(
                links.contains("fetch_url_content"),
                "and must name the way to make one citable:\n{links}"
            );
        }
    }

    #[tokio::test]
    async fn missing_history_and_storage_failure_are_not_hidden() {
        let llm: Arc<dyn LLMPort> = Arc::new(Counter);
        let history = Arc::new(History {
            aggregate: None,
            reads: AtomicUsize::new(0),
        });
        let result = build_conversation_context(
            Arc::new(Supplemental {
                prompt: None,
                fail: false,
            }),
            history.clone(),
            "chat",
            &llm,
            100,
            "",
        )
        .await;
        assert!(matches!(result, Err(AppError::NotFound(_))));
        let result = build_conversation_context(
            Arc::new(Supplemental {
                prompt: None,
                fail: true,
            }),
            history,
            "chat",
            &llm,
            100,
            "",
        )
        .await;
        assert!(
            matches!(result, Err(AppError::Database(message)) if message == "context unavailable")
        );
    }

    fn row(index: usize, excerpt: &str) -> LinkedConversationSource {
        LinkedConversationSource {
            title: Some(format!("page {index}")),
            url: format!("https://example.com/{index}"),
            excerpt: Some(excerpt.to_string()),
        }
    }

    /// Sources past the tenth used to be invisible. Every page is listed by
    /// title and address while the share of the window holds them.
    #[test]
    fn every_linked_page_is_listed_when_the_window_holds_it() {
        let rows: Vec<_> = (0..30).map(|index| row(index, "short note")).collect();
        let (entries, unlisted) =
            linked_source_entries(&rows, 10_000, |text| text.chars().count() / 4 + 1);
        assert_eq!(entries.len(), 30);
        assert_eq!(unlisted, 0);
        assert!(entries
            .iter()
            .all(|entry| entry.contains("Excerpt: short note")));
    }

    /// When the share cannot hold them all, the newest are listed and the rest
    /// are counted rather than dropped without a word; an excerpt that does
    /// not fit leaves its page listed without one, never cut.
    #[test]
    fn pages_without_room_are_counted_and_excerpts_are_whole_or_absent() {
        let long = "x".repeat(400);
        let rows: Vec<_> = (0..10).map(|index| row(index, &long)).collect();
        let (entries, unlisted) =
            linked_source_entries(&rows, 60, |text| text.chars().count() / 4 + 1);
        assert!(entries.len() < 10, "{entries:?}");
        assert_eq!(unlisted, 10 - entries.len());
        assert!(entries[0].starts_with("- page 0 ("));
        assert!(entries.iter().all(|entry| !entry.contains("Excerpt")));
    }

    /// Hundreds of linked pages, a share that lists a few: the rows are read a
    /// page at a time until the list is full, not all of them every turn, and
    /// the ones left out are still counted.
    #[tokio::test]
    async fn linked_sources_are_read_only_as_far_as_the_share_lists() {
        struct Many(std::sync::Mutex<usize>);
        #[async_trait::async_trait]
        impl ConversationContextPort for Many {
            async fn space_prompt(&self, _: &str) -> Result<Option<String>> {
                Ok(None)
            }
            async fn linked_sources(
                &self,
                _: &str,
                offset: usize,
                limit: usize,
            ) -> Result<Vec<LinkedConversationSource>> {
                *self.0.lock().unwrap() += limit.min(500usize.saturating_sub(offset));
                Ok((offset..500.min(offset + limit))
                    .map(|index| row(index, "note"))
                    .collect())
            }
            async fn linked_source_count(&self, _: &str) -> Result<usize> {
                Ok(500)
            }
        }
        let source = Many(std::sync::Mutex::new(0));
        let llm: Arc<dyn crate::application::ports::LLMPort> = Arc::new(Counter);
        let text = build_linked_web_sources_prompt_context(&source, "chat", &llm, 4_096)
            .await
            .unwrap()
            .expect("some are listed");
        let read = *source.0.lock().unwrap();
        assert!(read < 500, "read {read} of 500 rows");
        let listed = text
            .lines()
            .filter(|line| line.starts_with("- page "))
            .count();
        assert!(
            text.contains(&format!("…and {} older linked pages", 500 - listed)),
            "{text}"
        );
    }
}
