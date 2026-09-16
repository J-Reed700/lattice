//! Assemble chat context through read-only application ports.

use crate::application::ports::conversation_context::ConversationContextPort;
use crate::application::services::context_window_builder::ContextWindowBuilder;
use crate::shared::error::{AppError, Result};
use crate::shared::text_utils::safe_truncate;
use std::sync::Arc;

const LINKED_WEB_SOURCE_PROMPT_MAX_ITEMS: i64 = 10;
const LINKED_WEB_SOURCE_PROMPT_MAX_EXCERPT_CHARS: usize = 280;
const LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_RATIO: f64 = 0.08;
const LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_MIN: usize = 120;
const LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_MAX: usize = 320;

async fn build_linked_web_sources_prompt_context(
    supplemental: &dyn ConversationContextPort,
    conversation_id: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    max_tokens: usize,
) -> Result<Option<String>> {
    let rows = supplemental
        .linked_sources(conversation_id, LINKED_WEB_SOURCE_PROMPT_MAX_ITEMS)
        .await?;

    if rows.is_empty() {
        return Ok(None);
    }

    let raw_budget =
        ((max_tokens as f64) * LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_RATIO).round() as usize;
    let token_budget = raw_budget.clamp(
        LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_MIN,
        LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_MAX,
    );

    let mut entries: Vec<String> = Vec::new();
    let mut used_tokens = 0usize;

    for row in rows {
        let url = row.url.trim();
        if url.is_empty() {
            continue;
        }

        let title = row
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(url);

        let mut entry = format!("- {} ({})", title, url);
        if let Some(excerpt) = row
            .excerpt
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let truncated_excerpt =
                safe_truncate(excerpt, LINKED_WEB_SOURCE_PROMPT_MAX_EXCERPT_CHARS);
            entry.push_str(&format!("\n  Excerpt: {}", truncated_excerpt));
        }

        let entry_tokens = llm.count_tokens(&entry);
        if !entries.is_empty() && used_tokens.saturating_add(entry_tokens) > token_budget {
            break;
        }

        used_tokens = used_tokens.saturating_add(entry_tokens);
        entries.push(entry);
    }

    if entries.is_empty() {
        return Ok(None);
    }

    Ok(Some(format!(
        "User-linked web sources for this conversation (context links, not necessarily indexed):\n{}",
        entries.join("\n")
    )))
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
    if let Some(system_prompt) = effective_system_prompt {
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
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::conversation_context::LinkedConversationSource;
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
            limit: i64,
        ) -> Result<Vec<LinkedConversationSource>> {
            assert_eq!(limit, 10);
            Ok(vec![LinkedConversationSource {
                title: Some(" source ".into()),
                url: " https://example.com ".into(),
                excerpt: Some("excerpt".into()),
            }])
        }
    }

    #[tokio::test]
    async fn prompt_precedence_and_history_use_one_snapshot() {
        let llm: Arc<dyn LLMPort> =
            Arc::new(crate::features::llm::engine::factory::MockLLMPort::new());
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
            let (context, _, links) = build_conversation_context(
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
            assert_eq!(history.reads.load(Ordering::SeqCst), 1);
            assert!(links.unwrap().contains("- source (https://example.com)"));
        }
    }

    #[tokio::test]
    async fn missing_history_and_storage_failure_are_not_hidden() {
        let llm: Arc<dyn LLMPort> =
            Arc::new(crate::features::llm::engine::factory::MockLLMPort::new());
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
}
