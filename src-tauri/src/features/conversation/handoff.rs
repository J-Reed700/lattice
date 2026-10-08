//! Carrying a conversation on in a new one.
//!
//! A long thread gets slow and its early turns fall out of the model's window.
//! This summarizes it into a handoff note and opens a fresh conversation in the
//! same space whose first message is that note, so the next question starts
//! from what the old thread established rather than from nothing.
use super::branching_dto::{
    ContinueInNewConversationRequestDto, ContinueInNewConversationResponseDto,
};
use super::commands as conversation;
use crate::application::ports::llm_port::{CompletionInput, CompletionRequest, SamplingOverride};
use crate::application::ports::LLMPort;
use crate::features::conversation::repository::ConversationRepository;
use crate::interfaces::di::Container;
use crate::shared::{error::AppError, ipc::ApiError};
use std::collections::HashSet;
use std::time::Duration;

/// One message's share of the transcript. Long answers are cut, not dropped:
/// their opening usually carries the point.
const HANDOFF_MESSAGE_CHAR_LIMIT: usize = 3000;
/// How much transcript one model call reads. A thread longer than this is
/// noted part by part, then the notes are merged.
const HANDOFF_CHUNK_CHAR_LIMIT: usize = 12000;
const TITLE_CHAR_LIMIT: usize = 200;

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut output: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    output.push('…');
    output
}

/// The transcript split into model-sized parts, in order, at message
/// boundaries. Failed and empty turns are left out: they are not part of what
/// the conversation established.
fn transcript_chunks(messages: &[crate::domain::conversation::ConversationMessage]) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for message in messages {
        let content = message.content.trim();
        if content.is_empty() || message.status == "failed" {
            continue;
        }
        let line = format!(
            "{}: {}",
            message.role.to_string().to_uppercase(),
            truncate_chars(content, HANDOFF_MESSAGE_CHAR_LIMIT)
        );
        if !current.is_empty() && current.len() + line.len() + 2 > HANDOFF_CHUNK_CHAR_LIMIT {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(&line);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn build_notes_prompt(title: &str, chunk: &str, index: usize, total: usize) -> String {
    [
        format!(
            "Below is part {} of {} of the conversation \"{}\".",
            index, total, title
        ),
        "Write dense bullet notes of what this part establishes: facts and figures, sources it names, decisions and their reasons, and questions left open.".to_string(),
        "Use only this part. Keep concrete values. No preamble.".to_string(),
        String::new(),
        chunk.to_string(),
    ]
    .join("\n")
}

fn build_handoff_prompt(title: &str, material: &str, from_notes: bool) -> String {
    let what = if from_notes {
        "the notes below, taken from the conversation part by part"
    } else {
        "the conversation transcript below"
    };
    [
        format!(
            "Write a handoff note for the conversation \"{}\" so a new conversation can pick up where it left off.",
            title
        ),
        "The note will be the new conversation's only memory of this one.".to_string(),
        format!("Use only {}. Do not add anything it does not say.", what),
        "Output markdown with exactly these sections:".to_string(),
        "## Where things stand".to_string(),
        "## Established".to_string(),
        "## Decisions".to_string(),
        "## Open threads".to_string(),
        "Rules:".to_string(),
        "- Where things stand: 2-4 sentences on the goal and how far it got.".to_string(),
        "- Established: bullets of findings, with concrete values, and the source named when the conversation names one.".to_string(),
        "- Decisions: bullets, each with its reason when one was given.".to_string(),
        "- Open threads: unanswered questions and next steps.".to_string(),
        "- If a section has nothing, write `None.`".to_string(),
        "- No preamble, no closing remarks. Stay under 450 words.".to_string(),
        String::new(),
        material.to_string(),
    ]
    .join("\n")
}

const SYSTEM: &str = "You summarize conversations faithfully and concisely. You never add facts the material does not contain.";
/// One call's wall-clock limit. A long part on a local model takes a few
/// minutes; past this something is stuck.
const CALL_TIME_BUDGET: Duration = Duration::from_secs(10 * 60);
const CALL_MAX_OUTPUT_TOKENS: u32 = 2_048;

async fn ask_model(llm: &dyn LLMPort, prompt: String) -> Result<String, ApiError> {
    let text = if llm.supports_typed_completions() {
        let request = CompletionRequest {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: SYSTEM.to_string(),
                },
                CompletionInput::Message {
                    role: "user".into(),
                    content: prompt,
                },
            ],
            // A summary needs no hidden chain of thought, and a reasoning model
            // left to it can spend the whole budget before writing a word.
            sampling: Some(SamplingOverride::deterministic()),
            max_output_tokens: Some(CALL_MAX_OUTPUT_TOKENS),
            time_budget: Some(CALL_TIME_BUDGET),
            ..Default::default()
        };
        llm.complete(&request).await.map(|response| response.text)
    } else {
        tokio::time::timeout(
            CALL_TIME_BUDGET,
            llm.generate(&prompt, &[format!("System: {SYSTEM}")], None),
        )
        .await
        .unwrap_or_else(|_| {
            Err(AppError::Other(
                "The summary took too long and was stopped.".to_string(),
            ))
        })
    }
    .map_err(ApiError::from)?;
    let text = text.trim();
    if text.is_empty() {
        return Err(ApiError::from(AppError::Other(
            "The model returned an empty summary.".to_string(),
        )));
    }
    Ok(text.to_string())
}

/// The conversations being summarized right now. The guard releases its id on
/// drop, so an error or a panic cannot leave a conversation locked.
struct InFlight(String);

static IN_FLIGHT: std::sync::LazyLock<std::sync::Mutex<HashSet<String>>> =
    std::sync::LazyLock::new(Default::default);

impl InFlight {
    fn claim(id: &str) -> Option<Self> {
        let mut running = IN_FLIGHT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        running.insert(id.to_string()).then(|| Self(id.to_string()))
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        IN_FLIGHT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.0);
    }
}

pub async fn continue_in_new_conversation_impl(
    request: ContinueInNewConversationRequestDto,
    container: &Container,
) -> Result<ContinueInNewConversationResponseDto, ApiError> {
    let source_id = request.conversation_id.trim().to_string();
    let source = conversation::get_conversation_impl(container, source_id.clone())
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::from(AppError::NotFound(format!(
                "Conversation {} not found",
                source_id
            )))
        })?;
    let messages = conversation::get_conversation_messages_impl(container, source_id.clone())
        .await
        .map_err(ApiError::from)?;
    let chunks = transcript_chunks(&messages);
    if chunks.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "This conversation has nothing to summarize yet.".to_string(),
        )));
    }

    // One summary per conversation at a time. A second click, or a click after
    // a page reload lost the first one's spinner, would otherwise start a
    // parallel run and open a second copy.
    let _running = match InFlight::claim(&source_id) {
        Some(guard) => guard,
        None => {
            return Err(ApiError::from(AppError::InvalidState(
                "This conversation is already being summarized.".to_string(),
            )))
        }
    };

    // Straight to the model, as compaction does: no conversation, retrieval,
    // tools or streaming. Running it as a chat turn created visible scratch
    // chats and let the turn reach for tools.
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    tracing::info!(
        conversation_id = %source_id,
        parts = chunks.len(),
        transcript_chars = chunks.iter().map(String::len).sum::<usize>(),
        provider = llm.provider_name(),
        model = llm.model_name(),
        "Continue in new chat: summarizing"
    );
    let ask = |prompt: String, step: &'static str| {
        let llm = std::sync::Arc::clone(&llm);
        let source_id = source_id.clone();
        async move {
            let started = std::time::Instant::now();
            let result = ask_model(llm.as_ref(), prompt).await;
            match &result {
                Ok(text) => tracing::info!(
                    conversation_id = %source_id,
                    step,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    output_chars = text.len(),
                    "Continue in new chat: model call finished"
                ),
                Err(error) => tracing::warn!(
                    conversation_id = %source_id,
                    step,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    error = ?error,
                    "Continue in new chat: model call failed"
                ),
            }
            result
        }
    };

    let (material, from_notes) = if let [chunk] = chunks.as_slice() {
        (chunk.clone(), false)
    } else {
        let mut notes = Vec::new();
        for (index, chunk) in chunks.iter().enumerate() {
            notes.push(
                ask(
                    build_notes_prompt(&source.title, chunk, index + 1, chunks.len()),
                    "notes",
                )
                .await?,
            );
        }
        (notes.join("\n\n---\n\n"), true)
    };
    let summary = ask(
        build_handoff_prompt(&source.title, &material, from_notes),
        "handoff",
    )
    .await?;

    let new_id = uuid::Uuid::new_v4().to_string();
    let new_title = truncate_chars(&format!("{} · continued", source.title), TITLE_CHAR_LIMIT);
    let metadata = serde_json::json!({
        "continuedFrom": { "conversationId": source_id, "title": source.title },
    })
    .to_string();

    let repo = ConversationRepository::new(container.db_pool().clone());
    repo.create_continuation(&source_id, &new_id, &new_title, &summary, &metadata)
        .await
        .map_err(ApiError::from)?;
    let created = repo
        .find_by_id(&new_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::from(AppError::InvalidState(
                "The new conversation was created but could not be read back.".to_string(),
            ))
        })?;

    Ok(ContinueInNewConversationResponseDto {
        conversation: repo
            .project_conversation(&created)
            .await
            .map_err(ApiError::from)?,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn a_long_answer_is_cut_to_the_message_limit() {
        let cut = truncate_chars(
            &"a".repeat(HANDOFF_MESSAGE_CHAR_LIMIT + 50),
            HANDOFF_MESSAGE_CHAR_LIMIT,
        );
        assert_eq!(cut.chars().count(), HANDOFF_MESSAGE_CHAR_LIMIT);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn a_conversation_cannot_be_summarized_twice_at_once() {
        let first = InFlight::claim("conv-a").unwrap();
        assert!(InFlight::claim("conv-a").is_none());
        assert!(InFlight::claim("conv-b").is_some());
        drop(first);
        assert!(InFlight::claim("conv-a").is_some(), "released on drop");
    }

    #[test]
    fn the_handoff_prompt_names_every_section() {
        let prompt = build_handoff_prompt("Tomatoes", "USER: hi", false);
        for section in [
            "## Where things stand",
            "## Established",
            "## Decisions",
            "## Open threads",
        ] {
            assert!(prompt.contains(section), "missing {section}");
        }
        assert!(prompt.contains("transcript"));
        assert!(build_handoff_prompt("Tomatoes", "- note", true).contains("notes below"));
    }
}
