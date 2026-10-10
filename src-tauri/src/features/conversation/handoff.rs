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
use crate::application::ports::llm_port::{
    chars_within_tokens, InferencePriority, SamplingOverride,
};
use crate::application::ports::LLMPort;
use crate::application::services::grounded_generation::{
    self, CallOptions, EvidencePassage, EvidenceSelection, GroundedRequest,
};
use crate::domain::conversation::ConversationMessage;
use crate::features::conversation::repository::ConversationRepository;
use crate::interfaces::di::Container;
use crate::shared::{error::AppError, ipc::ApiError};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const TITLE_CHAR_LIMIT: usize = 200;

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut output: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    output.push('…');
    output
}

/// The transcript as evidence, one passage per message under its id. Failed
/// and empty turns are left out: they are not part of what the conversation
/// established.
fn transcript_passages(messages: &[ConversationMessage]) -> Vec<EvidencePassage> {
    messages
        .iter()
        .filter(|message| !message.content.trim().is_empty() && message.status != "failed")
        .map(|message| EvidencePassage {
            id: message.id.clone(),
            label: format!("{}:", message.role.to_string().to_uppercase()),
            text: message.content.trim().to_string(),
            rank: 1.0,
        })
        .collect()
}

/// The transcript in parts that each fit `room` tokens, in order, at message
/// boundaries. A message longer than a whole part is carried across several,
/// every piece labelled, so nothing the conversation said is cut.
fn transcript_parts(
    llm: &dyn LLMPort,
    passages: Vec<EvidencePassage>,
    room: usize,
) -> Vec<Vec<EvidencePassage>> {
    let piece_chars = chars_within_tokens(room, llm.chars_per_token()).max(1);
    let mut parts: Vec<Vec<EvidencePassage>> = Vec::new();
    let mut current: Vec<EvidencePassage> = Vec::new();
    let mut used = 0usize;
    for passage in passages {
        for piece in split_passage(passage, piece_chars) {
            let cost = grounded_generation::passage_tokens(llm, &piece);
            if !current.is_empty() && used + cost > room {
                parts.push(std::mem::take(&mut current));
                used = 0;
            }
            used += cost;
            current.push(piece);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// One passage as pieces of at most `max_chars`, cut at whitespace where one
/// is near; the second and later pieces say they continue the message.
fn split_passage(passage: EvidencePassage, max_chars: usize) -> Vec<EvidencePassage> {
    if passage.text.chars().count() <= max_chars {
        return vec![passage];
    }
    let mut pieces = Vec::new();
    let mut rest = passage.text.as_str();
    while !rest.is_empty() {
        let end = rest
            .char_indices()
            .nth(max_chars)
            .map_or(rest.len(), |(index, _)| index);
        let cut = if end == rest.len() {
            end
        } else {
            rest.get(..end)
                .and_then(|head| head.rfind(char::is_whitespace))
                .filter(|space| *space > end / 2)
                .unwrap_or(end)
        };
        let (head, tail) = rest.split_at(cut);
        rest = tail.trim_start();
        let head = head.trim();
        if head.is_empty() {
            continue;
        }
        let label = if pieces.is_empty() {
            passage.label.clone()
        } else {
            format!("{} (continued)", passage.label.trim_end_matches(':'))
        };
        pieces.push(EvidencePassage {
            id: format!("{}#{}", passage.id, pieces.len()),
            label,
            text: head.to_string(),
            rank: passage.rank,
        });
    }
    pieces
}

fn notes_task(title: &str, index: usize, total: usize) -> String {
    [
        format!(
            "Below is part {} of {} of the conversation \"{}\".",
            index, total, title
        ),
        "Write dense bullet notes of what this part establishes: facts and figures, sources it names, decisions and their reasons, and questions left open.".to_string(),
        "Use only this part. Keep concrete values. No preamble.".to_string(),
    ]
    .join("\n")
}

fn handoff_task(title: &str, from_notes: bool) -> String {
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
    ]
    .join("\n")
}

const SYSTEM: &str = "You summarize conversations faithfully and concisely. You never add facts the material does not contain.";
/// One call's wall-clock limit. A long part on a local model takes a few
/// minutes; past this something is stuck.
const CALL_TIME_BUDGET: Duration = Duration::from_secs(10 * 60);
const CALL_MAX_OUTPUT_TOKENS: usize = 2_048;

/// One summarizing call that carries every passage it is given whole.
fn summary_request(
    task: String,
    evidence: Vec<EvidencePassage>,
    conversation_id: &str,
) -> GroundedRequest {
    let mut request = GroundedRequest::new(SYSTEM, task);
    request.evidence = evidence;
    request.selection = EvidenceSelection::All;
    request.output_tokens = CALL_MAX_OUTPUT_TOKENS;
    request.call = CallOptions {
        // The user is waiting for the new chat to open, and every part
        // reads the same conversation, so it returns to the slot that holds
        // it.
        priority: InferencePriority::Interactive,
        cancel: None,
        cache_key: Some(conversation_id.to_string()),
        // A summary needs no hidden chain of thought, and a reasoning model
        // left to it can spend the whole budget before writing a word.
        sampling: Some(SamplingOverride::deterministic()),
        time_budget: Some(CALL_TIME_BUDGET),
        ..Default::default()
    };
    request
}

async fn ask_model(llm: &dyn LLMPort, request: GroundedRequest) -> Result<String, ApiError> {
    let output = grounded_generation::generate(llm, request)
        .await
        .map_err(|error| ApiError::from(AppError::from(error)))?;
    tracing::debug!(
        input_tokens = output.accounting.total_input,
        input_budget = output.accounting.input_budget,
        "Continue in new chat: request planned"
    );
    let text = output.text.trim();
    if text.is_empty() {
        return Err(ApiError::from(AppError::Other(
            "The model returned an empty summary.".to_string(),
        )));
    }
    Ok(text.to_string())
}

/// The conversations being summarized right now. Owned by the conversation
/// DI, so every handoff in the process claims from one set.
#[derive(Default)]
pub struct HandoffsInFlight(std::sync::Mutex<HashSet<String>>);

/// A claimed conversation. The guard releases its id on drop, so an error or
/// a panic cannot leave a conversation locked.
struct InFlight {
    set: Arc<HandoffsInFlight>,
    id: String,
}

impl InFlight {
    fn claim(set: &Arc<HandoffsInFlight>, id: &str) -> Option<Self> {
        let mut running = set
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        running.insert(id.to_string()).then(|| Self {
            set: Arc::clone(set),
            id: id.to_string(),
        })
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.set
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.id);
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
    let passages = transcript_passages(&messages);
    if passages.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "This conversation has nothing to summarize yet.".to_string(),
        )));
    }

    // One summary per conversation at a time. A second click, or a click after
    // a page reload lost the first one's spinner, would otherwise start a
    // parallel run and open a second copy.
    let _running = match InFlight::claim(&container.handoffs_in_flight(), &source_id) {
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
    // The whole transcript in one call when the window holds it; otherwise it
    // is noted part by part, each part as much as the window holds, and the
    // notes are merged.
    let whole_room = grounded_generation::evidence_room(
        llm.as_ref(),
        &summary_request(handoff_task(&source.title, false), Vec::new(), &source_id),
    )
    .map_err(|error| ApiError::from(AppError::from(error)))?;
    let transcript_tokens: usize = passages
        .iter()
        .map(|passage| grounded_generation::passage_tokens(llm.as_ref(), passage))
        .sum();
    let parts = if transcript_tokens <= whole_room {
        vec![passages]
    } else {
        let part_room = grounded_generation::evidence_room(
            llm.as_ref(),
            &summary_request(
                notes_task(&source.title, usize::MAX, usize::MAX),
                Vec::new(),
                &source_id,
            ),
        )
        .map_err(|error| ApiError::from(AppError::from(error)))?;
        transcript_parts(llm.as_ref(), passages, part_room)
    };
    tracing::info!(
        conversation_id = %source_id,
        parts = parts.len(),
        transcript_tokens,
        provider = llm.provider_name(),
        model = llm.model_name(),
        "Continue in new chat: summarizing"
    );
    let ask = |request: GroundedRequest, step: &'static str| {
        let llm = std::sync::Arc::clone(&llm);
        let source_id = source_id.clone();
        async move {
            let started = std::time::Instant::now();
            let result = ask_model(llm.as_ref(), request).await;
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

    let total = parts.len();
    let summary = if total == 1 {
        let transcript = parts.into_iter().flatten().collect();
        ask(
            summary_request(handoff_task(&source.title, false), transcript, &source_id),
            "handoff",
        )
        .await?
    } else {
        let mut notes = Vec::with_capacity(total);
        for (index, part) in parts.into_iter().enumerate() {
            notes.push(
                ask(
                    summary_request(
                        notes_task(&source.title, index + 1, total),
                        part,
                        &source_id,
                    ),
                    "notes",
                )
                .await?,
            );
        }
        let material = EvidencePassage {
            id: "notes".to_string(),
            label: String::new(),
            text: notes.join("\n\n---\n\n"),
            rank: 1.0,
        };
        ask(
            summary_request(
                handoff_task(&source.title, true),
                vec![material],
                &source_id,
            ),
            "handoff",
        )
        .await?
    };

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

    fn passage(id: &str, label: &str, text: &str) -> EvidencePassage {
        EvidencePassage {
            id: id.into(),
            label: label.into(),
            text: text.into(),
            rank: 1.0,
        }
    }

    #[test]
    fn a_message_longer_than_a_part_is_carried_across_parts_not_cut() {
        let llm = crate::features::llm::engine::factory::MockLLMPort::new();
        let long = "word ".repeat(2_000);
        let parts = transcript_parts(
            &llm,
            vec![
                passage("m1", "USER:", "short question"),
                passage("m2", "ASSISTANT:", &long),
            ],
            300,
        );

        assert!(parts.len() > 2, "{} parts", parts.len());
        let carried: String = parts
            .iter()
            .flatten()
            .filter(|piece| piece.id.starts_with("m2"))
            .map(|piece| piece.text.split_whitespace().count().to_string() + " ")
            .collect();
        let words: usize = carried
            .split_whitespace()
            .map(|n| n.parse::<usize>().unwrap())
            .sum();
        assert_eq!(words, 2_000, "every word of the long answer is carried");
        assert!(parts
            .iter()
            .flatten()
            .any(|piece| piece.label == "ASSISTANT (continued)"));
        for part in &parts {
            let cost: usize = part
                .iter()
                .map(|piece| grounded_generation::passage_tokens(&llm, piece))
                .sum();
            assert!(
                cost <= 300 || part.len() == 1,
                "a part overran its room: {cost}"
            );
        }
    }

    #[test]
    fn failed_and_empty_messages_are_not_evidence() {
        use crate::domain::conversation::MessageRole;
        use crate::shared::types::ConversationId;
        let message = |id: &str, content: &str, status: &str| ConversationMessage {
            id: id.into(),
            conversation_id: ConversationId::new(),
            role: MessageRole::User,
            content: content.into(),
            tokens: 0,
            created_at: chrono::Utc::now(),
            metadata: None,
            status: status.into(),
        };
        let passages = transcript_passages(&[
            message("kept", "  a question  ", "completed"),
            message("failed", "lost", "failed"),
            message("empty", "   ", "completed"),
        ]);
        assert_eq!(passages.len(), 1);
        assert_eq!(passages[0].id, "kept");
        assert_eq!(passages[0].label, "USER:");
        assert_eq!(passages[0].text, "a question");
    }

    #[test]
    fn a_conversation_cannot_be_summarized_twice_at_once() {
        let set = Arc::new(HandoffsInFlight::default());
        let first = InFlight::claim(&set, "conv-a").unwrap();
        assert!(InFlight::claim(&set, "conv-a").is_none());
        assert!(InFlight::claim(&set, "conv-b").is_some());
        drop(first);
        assert!(
            InFlight::claim(&set, "conv-a").is_some(),
            "released on drop"
        );
    }

    #[test]
    fn the_handoff_prompt_names_every_section() {
        let prompt = handoff_task("Tomatoes", false);
        for section in [
            "## Where things stand",
            "## Established",
            "## Decisions",
            "## Open threads",
        ] {
            assert!(prompt.contains(section), "missing {section}");
        }
        assert!(prompt.contains("transcript"));
        assert!(handoff_task("Tomatoes", true).contains("notes below"));
    }
}
