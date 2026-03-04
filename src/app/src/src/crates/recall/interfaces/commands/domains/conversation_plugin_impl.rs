//! Conversation Plugin - Conversation management and conversational Q&A
//!
//! Migrated from ipc/domains/conversation.rs as part of Operation Scorched Earth Batch 4

use crate::application::dtos::conversation_dto::{
    CreateConversationRequestDto, CreateConversationResponseDto, DeleteConversationRequestDto,
    DeleteConversationResponseDto, GetConversationMessagesRequestDto,
    GetConversationMessagesResponseDto, GetConversationRequestDto, GetConversationResponseDto,
    ListConversationsQuery, ListConversationsResponseDto, RenameConversationRequestDto,
    RenameConversationResponseDto,
};
use crate::application::dtos::conversation_message_bookmark_dto::{
    BookmarkConversationMessageRequestDto, ConversationMessageBookmarkDto,
    DeleteConversationMessageRequestDto, ListMessageBookmarksQueryDto,
    ListMessageBookmarksResponseDto, UnbookmarkConversationMessageRequestDto,
};
use crate::application::dtos::conversation_space_dto::{
    AddConversationToJournalRequestDto, ArchiveConversationJournalRequestDto,
    ArchiveConversationSpaceRequestDto, ConversationJournalDto, ConversationSpaceDto,
    ConversationSpaceMemberDto, CreateConversationJournalRequestDto,
    CreateConversationSpaceRequestDto, DeleteConversationJournalRequestDto,
    ListConversationsExplorerQueryDto, ListJournalConversationsQueryDto,
    MoveConversationToSpaceRequestDto, RemoveConversationFromJournalRequestDto,
    RemoveConversationSpaceMemberRequestDto, SetConversationStateRequestDto,
    UpdateConversationJournalRequestDto, UpdateConversationSpaceRequestDto,
    UpsertConversationSpaceMemberRequestDto,
};
use crate::interfaces::commands::conversation;
use crate::interfaces::commands::conversation_chat::{
    chat_with_conversation_impl as run_chat_with_conversation_impl, ChatResponse, ToolPreferences,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use crate::shared::error::AppError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use std::collections::HashSet;

const DEFAULT_SPACE_ID: &str = "space_general";
const LOCAL_OWNER_MEMBER_ID: &str = "member_local_owner";
const JOURNAL_SYNTHESIS_ENTRY_LIMIT_DEFAULT: usize = 12;
const JOURNAL_SYNTHESIS_ENTRY_LIMIT_MAX: usize = 24;
const JOURNAL_SYNTHESIS_MESSAGE_CHAR_LIMIT: usize = 900;
const JOURNAL_SYNTHESIS_ENTRY_CHAR_LIMIT: usize = 6000;
const JOURNAL_SYNTHESIS_CHUNK_CHAR_LIMIT: usize = 14000;

#[derive(Debug, Clone, sqlx::FromRow)]
struct ConversationStateRow {
    space_id: String,
    is_saved: i64,
    is_bookmarked: i64,
    is_pinned: i64,
    is_archived: i64,
    saved_at: Option<String>,
    bookmarked_at: Option<String>,
    pinned_at: Option<String>,
    archived_at: Option<String>,
    last_message_preview: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ConversationExplorerRow {
    id: String,
    title: String,
    model_name: String,
    system_prompt: Option<String>,
    created_at: String,
    updated_at: String,
    message_count: i64,
    total_tokens: i64,
    space_id: String,
    is_saved: i64,
    is_bookmarked: i64,
    is_pinned: i64,
    is_archived: i64,
    saved_at: Option<String>,
    bookmarked_at: Option<String>,
    pinned_at: Option<String>,
    archived_at: Option<String>,
    last_message_preview: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MessageBookmarkRow {
    id: String,
    conversation_id: String,
    conversation_title: String,
    space_id: String,
    message_id: String,
    message_role: String,
    message_preview: String,
    title: Option<String>,
    note: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationLinkedDocumentDto {
    pub document_id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_type: String,
    pub category: String,
    pub indexed_at: String,
    pub last_referenced_at: String,
    pub reference_count: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ConversationLinkedDocumentRow {
    document_id: String,
    file_name: String,
    file_path: String,
    file_type: Option<String>,
    category: String,
    indexed_at: String,
    last_referenced_at: String,
    reference_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationWebSourceDto {
    pub id: String,
    pub url: String,
    pub normalized_url: String,
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub relevance_score: Option<f32>,
    pub added_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ConversationWebSourceRow {
    id: String,
    url: String,
    normalized_url: String,
    title: Option<String>,
    excerpt: Option<String>,
    relevance_score: Option<f32>,
    added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSpaceMembershipDto {
    pub space_id: String,
    pub space_name: String,
    pub is_archived: bool,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct DocumentSpaceMembershipRow {
    space_id: String,
    space_name: String,
    is_archived: i64,
    created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizeJournalEntriesRequestDto {
    pub conversation_ids: Vec<String>,
    pub scope: Option<String>,
    pub max_entries: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizeJournalEntriesResponseDto {
    pub synthesis: String,
    pub scope: String,
    pub entry_count: usize,
    pub chunk_count: usize,
    pub conversation_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct JournalSynthesisEntry {
    conversation_id: String,
    title: String,
    updated_at: String,
    message_count: usize,
    transcript: String,
}

async fn fetch_conversation_state(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Result<Option<ConversationStateRow>, ApiError> {
    let state = sqlx::query_as::<_, ConversationStateRow>(
        r#"
        SELECT
            c.space_id AS space_id,
            c.is_saved AS is_saved,
            c.is_bookmarked AS is_bookmarked,
            c.is_pinned AS is_pinned,
            c.is_archived AS is_archived,
            c.saved_at AS saved_at,
            c.bookmarked_at AS bookmarked_at,
            c.pinned_at AS pinned_at,
            c.archived_at AS archived_at,
            (
                SELECT m.content
                FROM conversation_messages m
                WHERE m.conversation_id = c.id
                ORDER BY m.created_at DESC
                LIMIT 1
            ) AS last_message_preview
        FROM conversations c
        WHERE c.id = ?
        "#,
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to fetch conversation state: {}",
            e
        )))
    })?;

    Ok(state)
}

fn to_conversation_dto(
    c: &crate::domain::conversation::Conversation,
    state: Option<ConversationStateRow>,
) -> crate::application::dtos::conversation_dto::ConversationDto {
    let (
        space_id,
        is_saved,
        is_bookmarked,
        is_pinned,
        is_archived,
        saved_at,
        bookmarked_at,
        pinned_at,
        archived_at,
        last_message_preview,
    ) = if let Some(s) = state {
        (
            Some(s.space_id),
            Some(s.is_saved != 0),
            Some(s.is_bookmarked != 0),
            Some(s.is_pinned != 0),
            Some(s.is_archived != 0),
            s.saved_at,
            s.bookmarked_at,
            s.pinned_at,
            s.archived_at,
            s.last_message_preview,
        )
    } else {
        (
            Some(DEFAULT_SPACE_ID.to_string()),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            None,
            None,
            None,
            None,
            None,
        )
    };

    crate::application::dtos::conversation_dto::ConversationDto {
        id: c.id.to_string(),
        title: c.title.clone(),
        model_name: c.model_name.clone(),
        system_prompt: c.system_prompt.clone(),
        created_at: c.created_at.to_rfc3339(),
        updated_at: c.updated_at.to_rfc3339(),
        message_count: c.message_count,
        total_tokens: c.total_tokens,
        space_id,
        is_saved,
        is_bookmarked,
        is_pinned,
        is_archived,
        saved_at,
        bookmarked_at,
        pinned_at,
        archived_at,
        last_message_preview,
    }
}

fn normalize_web_source_url(url: &str) -> String {
    url.trim().to_ascii_lowercase()
}

fn build_fts_query(raw: &str) -> Option<String> {
    let terms = raw
        .split_whitespace()
        .map(|term| term.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

fn validate_space_role(role: &str) -> Result<&str, ApiError> {
    match role.trim().to_lowercase().as_str() {
        "owner" => Ok("owner"),
        "editor" => Ok("editor"),
        "viewer" => Ok("viewer"),
        _ => Err(ApiError::from(AppError::InvalidInput(
            "Role must be one of: owner, editor, viewer".to_string(),
        ))),
    }
}

fn preferences_declares_journal(raw: Option<&str>) -> bool {
    let Some(raw_json) = raw else {
        return false;
    };

    if raw_json.trim().is_empty() {
        return false;
    }

    let parsed: Result<Value, _> = serde_json::from_str(raw_json);
    let Ok(value) = parsed else {
        return false;
    };

    let kind = value
        .get("spaceType")
        .and_then(Value::as_str)
        .or_else(|| value.get("space_type").and_then(Value::as_str))
        .unwrap_or("standard");

    kind.eq_ignore_ascii_case("journal")
}

fn validate_space_preferences_not_journal(
    raw: Option<&str>,
    context: &str,
) -> Result<(), ApiError> {
    if preferences_declares_journal(raw) {
        return Err(ApiError::from(AppError::InvalidInput(format!(
            "{} cannot declare `spaceType=journal`; journals are a separate entity",
            context
        ))));
    }
    Ok(())
}

async fn ensure_journal_space(pool: &SqlitePool, journal_space_id: &str) -> Result<(), ApiError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM journals WHERE id = ?")
        .bind(journal_space_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            ApiError::from(AppError::Database(format!(
                "Failed to verify journal: {}",
                e
            )))
        })?;

    if exists == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Journal not found: {}",
            journal_space_id
        ))));
    }

    Ok(())
}

async fn ensure_standard_space(pool: &SqlitePool, space_id: &str) -> Result<(), ApiError> {
    let exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversation_spaces WHERE id = ?")
            .bind(space_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                ApiError::from(AppError::Database(format!("Failed to verify space: {}", e)))
            })?;

    if exists == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Space not found: {}",
            space_id
        ))));
    }

    Ok(())
}

fn truncate_for_synthesis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut output = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    output.push('…');
    output
}

fn build_synthesis_transcript(
    messages: &[crate::domain::conversation::ConversationMessage],
) -> String {
    let mut lines = Vec::new();
    for message in messages {
        let content = message.content.trim();
        if content.is_empty() {
            continue;
        }
        lines.push(format!(
            "{}: {}",
            message.role.to_string().to_uppercase(),
            truncate_for_synthesis(content, JOURNAL_SYNTHESIS_MESSAGE_CHAR_LIMIT)
        ));
    }

    truncate_for_synthesis(&lines.join("\n"), JOURNAL_SYNTHESIS_ENTRY_CHAR_LIMIT)
}

fn chunk_journal_synthesis_entries(
    entries: &[JournalSynthesisEntry],
) -> Vec<Vec<JournalSynthesisEntry>> {
    let mut chunks: Vec<Vec<JournalSynthesisEntry>> = Vec::new();
    let mut current: Vec<JournalSynthesisEntry> = Vec::new();
    let mut current_chars: usize = 0;

    for entry in entries {
        let entry_chars = entry.transcript.len() + entry.title.len() + 120;
        if !current.is_empty() && current_chars + entry_chars > JOURNAL_SYNTHESIS_CHUNK_CHAR_LIMIT {
            chunks.push(current);
            current = Vec::new();
            current_chars = 0;
        }
        current.push(entry.clone());
        current_chars += entry_chars;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn format_entries_for_synthesis_prompt(entries: &[JournalSynthesisEntry]) -> String {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            [
                format!("ENTRY {}", index + 1),
                format!("Title: {}", entry.title),
                format!("Updated: {}", entry.updated_at),
                format!("Message Count: {}", entry.message_count),
                "Transcript:".to_string(),
                if entry.transcript.is_empty() {
                    "(No message text captured.)".to_string()
                } else {
                    entry.transcript.clone()
                },
            ]
            .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

fn build_journal_map_prompt(
    entries: &[JournalSynthesisEntry],
    chunk_index: usize,
    chunk_total: usize,
) -> String {
    [
        format!(
            "You are synthesizing journal entry chunk {} of {}.",
            chunk_index, chunk_total
        ),
        "Use only the provided entry transcripts.".to_string(),
        "Goal: capture practical details so the final output can replace re-reading every entry."
            .to_string(),
        "Output markdown with exactly these sections:".to_string(),
        "### Entry Highlights".to_string(),
        "### Practical Details".to_string(),
        "### Decisions & Constraints".to_string(),
        "### Open Questions & Risks".to_string(),
        "### Evidence Notes".to_string(),
        "Rules:".to_string(),
        "- Write concise but information-dense bullets (not generic summaries).".to_string(),
        "- Include concrete values when present (counts, ranges, timing, limits, caveats)."
            .to_string(),
        "- Prefix each bullet with [Entry: <title>].".to_string(),
        "- In Evidence Notes, map each important claim to one or more supporting entry titles."
            .to_string(),
        "- If a section has no data, write one bullet: - None identified.".to_string(),
        "".to_string(),
        "Entries:".to_string(),
        format_entries_for_synthesis_prompt(entries),
    ]
    .join("\n")
}

fn build_journal_reduce_prompt(chunk_outputs: &[String]) -> String {
    let chunks = chunk_outputs
        .iter()
        .enumerate()
        .map(|(index, output)| format!("CHUNK SYNTHESIS {}\n{}", index + 1, output))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    [
        "Merge the chunk syntheses into one final journal brief.".to_string(),
        "Use only the chunk syntheses below.".to_string(),
        "Audience: someone who wants to understand and act quickly without reading every entry."
            .to_string(),
        "Output markdown with exactly these sections (in this order):".to_string(),
        "## Executive Summary".to_string(),
        "## Detailed Synthesis".to_string(),
        "## Action Plan".to_string(),
        "## Decisions and Assumptions".to_string(),
        "## Open Questions and Risks".to_string(),
        "## Evidence Map".to_string(),
        "Formatting rules:".to_string(),
        "- Executive Summary: 1 short paragraph (4-6 sentences), plain language.".to_string(),
        "- Detailed Synthesis: 3-6 subsections with '### <Theme>' headers. Under each theme, write one short paragraph plus bullets for critical specifics.".to_string(),
        "- Action Plan: numbered list (3-7 steps) with enough detail to execute.".to_string(),
        "- Decisions and Assumptions: bullets with explicit rationale when available."
            .to_string(),
        "- Open Questions and Risks: bullets with impact noted briefly.".to_string(),
        "- Evidence Map: markdown table with columns `Claim`, `Supporting entries`, `Confidence`."
            .to_string(),
        "- Keep the brief concise but comprehensive; avoid fluff and repetition.".to_string(),
        "- If data is missing in a section, write `None identified.`".to_string(),
        "".to_string(),
        chunks,
    ]
    .join("\n")
}

fn extract_latest_assistant_message(chat: &ChatResponse) -> Option<String> {
    chat.messages
        .iter()
        .rev()
        .find(|message| message.role == "assistant" && !message.content.trim().is_empty())
        .map(|message| message.content.trim().to_string())
}

pub async fn create_conversation_impl(
    request: CreateConversationRequestDto,
    container: &Container,
) -> Result<CreateConversationResponseDto, ApiError> {
    let conversation = conversation::create_conversation_impl(
        container,
        request.title,
        request.model_name,
        request.system_prompt,
    )
    .await
    .map_err(ApiError::from)?;

    let state = fetch_conversation_state(container.db_pool(), &conversation.id.to_string()).await?;

    Ok(CreateConversationResponseDto {
        conversation: to_conversation_dto(&conversation, state),
        status: "success".to_string(),
    })
}

pub async fn get_conversation_impl(
    request: GetConversationRequestDto,
    container: &Container,
) -> Result<GetConversationResponseDto, ApiError> {
    let conversation = conversation::get_conversation_impl(container, request.conversation_id)
        .await
        .map_err(ApiError::from)?;

    Ok(GetConversationResponseDto {
        conversation: match conversation {
            Some(c) => {
                let state =
                    fetch_conversation_state(container.db_pool(), &c.id.to_string()).await?;
                Some(to_conversation_dto(&c, state))
            }
            None => None,
        },
    })
}

pub async fn list_conversations_impl(
    query: ListConversationsQuery,
    container: &Container,
) -> Result<ListConversationsResponseDto, ApiError> {
    let conversations = conversation::list_conversations_impl(container, query.limit, query.offset)
        .await
        .map_err(ApiError::from)?;

    let mut items = Vec::with_capacity(conversations.len());
    for c in &conversations {
        let state = fetch_conversation_state(container.db_pool(), &c.id.to_string()).await?;
        items.push(to_conversation_dto(c, state));
    }

    Ok(ListConversationsResponseDto {
        conversations: items,
        total: conversations.len(),
    })
}

pub async fn delete_conversation_impl(
    request: DeleteConversationRequestDto,
    container: &Container,
) -> Result<DeleteConversationResponseDto, ApiError> {
    conversation::delete_conversation_impl(container, request.conversation_id)
        .await
        .map_err(ApiError::from)?;

    Ok(DeleteConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn get_conversation_messages_impl(
    request: GetConversationMessagesRequestDto,
    container: &Container,
) -> Result<GetConversationMessagesResponseDto, ApiError> {
    let messages = conversation::get_conversation_messages_impl(container, request.conversation_id)
        .await
        .map_err(ApiError::from)?;

    Ok(GetConversationMessagesResponseDto {
        messages: messages
            .iter()
            .map(|m| crate::application::dtos::conversation_dto::MessageDto {
                id: m.id.to_string(),
                conversation_id: m.conversation_id.to_string(),
                role: m.role.to_string(),
                content: m.content.clone(),
                tokens: m.tokens,
                created_at: m.created_at.to_rfc3339(),
                metadata: m.metadata.clone(),
                status: m.status.clone(),
            })
            .collect(),
        total: messages.len(),
    })
}

pub async fn rename_conversation_impl(
    request: RenameConversationRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation::rename_conversation_impl(container, request.conversation_id, request.new_title)
        .await
        .map_err(ApiError::from)?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn chat_with_conversation_wrapper_impl(
    container: &Container,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    let convo_id_for_log = conversation_id.clone().unwrap_or_else(|| "NEW".to_string());
    let message_len = message.len();
    tracing::info!(
        conversation_id = convo_id_for_log.as_str(),
        message_len = message_len,
        "chat_with_conversation_wrapper: START"
    );

    let fut = run_chat_with_conversation_impl(
        container,
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        window,
    );
    match Box::pin(fut).await {
        Ok(response) => {
            tracing::info!(
                conversation_id = response.conversation_id.as_str(),
                response_len = response.message.len(),
                "chat_with_conversation_wrapper: DONE"
            );
            Ok(response)
        }
        Err(e) => {
            tracing::error!(
                conversation_id = convo_id_for_log.as_str(),
                error = %e,
                "chat_with_conversation_wrapper: FAILED"
            );
            Err(ApiError::from(e))
        }
    }
}

/// Frontend-facing command: apiCall('chat_with_conversation') expects this name.
pub async fn chat_with_conversation_impl(
    container: &Container,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    chat_with_conversation_wrapper_impl(
        container,
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        window,
    )
    .await
}

pub async fn synthesize_journal_entries_impl(
    request: SynthesizeJournalEntriesRequestDto,
    container: &Container,
    window: tauri::Window,
) -> Result<SynthesizeJournalEntriesResponseDto, ApiError> {
    let scope = request
        .scope
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("deck")
        .to_lowercase();

    let normalized_scope = match scope.as_str() {
        "current" | "deck" | "pinned" => scope,
        _ => "deck".to_string(),
    };

    let mut seen = HashSet::new();
    let mut conversation_ids = Vec::new();
    for id in request.conversation_ids {
        let normalized = id.trim();
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.to_string()) {
            conversation_ids.push(normalized.to_string());
        }
    }

    if conversation_ids.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "At least one conversation ID is required for journal synthesis".to_string(),
        )));
    }

    let max_entries = request
        .max_entries
        .unwrap_or(JOURNAL_SYNTHESIS_ENTRY_LIMIT_DEFAULT)
        .clamp(1, JOURNAL_SYNTHESIS_ENTRY_LIMIT_MAX);

    if conversation_ids.len() > max_entries {
        conversation_ids.truncate(max_entries);
    }

    let mut entries = Vec::new();
    for conversation_id in &conversation_ids {
        let conversation = conversation::get_conversation_impl(container, conversation_id.clone())
            .await
            .map_err(ApiError::from)?;
        let Some(conversation) = conversation else {
            continue;
        };

        let messages =
            conversation::get_conversation_messages_impl(container, conversation_id.clone())
                .await
                .map_err(ApiError::from)?;
        let transcript = build_synthesis_transcript(&messages);
        if transcript.is_empty() {
            continue;
        }

        entries.push(JournalSynthesisEntry {
            conversation_id: conversation.id.to_string(),
            title: conversation.title,
            updated_at: conversation.updated_at.to_rfc3339(),
            message_count: messages.len(),
            transcript,
        });
    }

    if entries.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "No synthesizable message content found in selected conversations".to_string(),
        )));
    }

    let chunks = chunk_journal_synthesis_entries(&entries);
    let tool_preferences = ToolPreferences {
        knowledge_base: false,
        web_search: false,
        deep_research_mode: false,
        followup_mode: true,
        turn_mode: Some("followup".to_string()),
        enabled_tools: None,
    };

    let mut synthesis_conversation_id: Option<String> = None;
    let synthesis_result = async {
        let mut map_outputs = Vec::new();

        for (chunk_index, chunk) in chunks.iter().enumerate() {
            let map_prompt = build_journal_map_prompt(chunk, chunk_index + 1, chunks.len());
            let map_response = run_chat_with_conversation_impl(
                container,
                synthesis_conversation_id.clone(),
                map_prompt,
                Some(tool_preferences.clone()),
                None,
                window.clone(),
            )
            .await
            .map_err(ApiError::from)?;

            synthesis_conversation_id = Some(map_response.conversation_id.clone());
            let map_output = extract_latest_assistant_message(&map_response).ok_or_else(|| {
                ApiError::from(AppError::Other(
                    "Synthesis map stage returned no assistant output".to_string(),
                ))
            })?;
            map_outputs.push(map_output);
        }

        let reduce_prompt = build_journal_reduce_prompt(&map_outputs);
        let reduce_response = run_chat_with_conversation_impl(
            container,
            synthesis_conversation_id.clone(),
            reduce_prompt,
            Some(tool_preferences.clone()),
            None,
            window.clone(),
        )
        .await
        .map_err(ApiError::from)?;

        synthesis_conversation_id = Some(reduce_response.conversation_id.clone());
        let synthesis = extract_latest_assistant_message(&reduce_response).ok_or_else(|| {
            ApiError::from(AppError::Other(
                "Synthesis reduce stage returned no assistant output".to_string(),
            ))
        })?;

        Ok::<SynthesizeJournalEntriesResponseDto, ApiError>(SynthesizeJournalEntriesResponseDto {
            synthesis,
            scope: normalized_scope.clone(),
            entry_count: entries.len(),
            chunk_count: chunks.len(),
            conversation_ids: entries
                .iter()
                .map(|entry| entry.conversation_id.clone())
                .collect(),
        })
    }
    .await;

    if let Some(temp_conversation_id) = synthesis_conversation_id {
        let _ = conversation::delete_conversation_impl(container, temp_conversation_id).await;
    }

    synthesis_result
}

pub async fn create_conversation_space_impl(
    request: CreateConversationSpaceRequestDto,
    container: &Container,
) -> Result<ConversationSpaceDto, ApiError> {
    let CreateConversationSpaceRequestDto {
        name,
        description,
        icon,
        accent_color,
        space_prompt,
        default_model_name,
        tool_preferences_json,
    } = request;

    let name = name.trim();
    if name.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "Space name cannot be empty".to_string(),
        )));
    }
    validate_space_preferences_not_journal(tool_preferences_json.as_deref(), "Conversation space")?;

    let id = format!("space_{}", uuid::Uuid::new_v4().simple());
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO conversation_spaces (
            id, name, description, icon, accent_color, space_prompt,
            default_model_name, tool_preferences_json, is_archived, sort_order, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, 0, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(icon)
    .bind(accent_color)
    .bind(space_prompt)
    .bind(default_model_name)
    .bind(tool_preferences_json)
    .bind(&now)
    .bind(&now)
    .execute(container.db_pool())
    .await
    .map_err(|e| ApiError::from(AppError::Database(format!("Failed to create space: {}", e))))?;

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO collaborator_profiles (
            id, display_name, email, avatar_url, created_at, updated_at
        ) VALUES (?, 'Local Owner', NULL, NULL, ?, ?)
        "#,
    )
    .bind(LOCAL_OWNER_MEMBER_ID)
    .bind(&now)
    .bind(&now)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to ensure local owner profile while creating space: {}",
            e
        )))
    })?;

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO conversation_space_members (
            space_id, member_id, role, created_at, updated_at
        ) VALUES (?, ?, 'owner', ?, ?)
        "#,
    )
    .bind(&id)
    .bind(LOCAL_OWNER_MEMBER_ID)
    .bind(&now)
    .bind(&now)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to create default owner membership for new space: {}",
            e
        )))
    })?;

    let created = sqlx::query_as::<_, ConversationSpaceDto>(
        r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM conversation_spaces
        WHERE id = ?
        "#,
    )
    .bind(&id)
    .fetch_one(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to fetch created space: {}",
            e
        )))
    })?;

    Ok(created)
}

pub async fn list_conversation_spaces_impl(
    container: &Container,
) -> Result<Vec<ConversationSpaceDto>, ApiError> {
    let spaces = sqlx::query_as::<_, ConversationSpaceDto>(
        r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM conversation_spaces
        ORDER BY is_archived ASC, sort_order ASC, updated_at DESC
        "#,
    )
    .fetch_all(container.db_pool())
    .await
    .map_err(|e| ApiError::from(AppError::Database(format!("Failed to list spaces: {}", e))))?;

    Ok(spaces)
}

pub async fn create_journal_impl(
    request: CreateConversationJournalRequestDto,
    container: &Container,
) -> Result<ConversationJournalDto, ApiError> {
    let CreateConversationJournalRequestDto {
        name,
        description,
        icon,
        accent_color,
        space_prompt,
        default_model_name,
        tool_preferences_json,
    } = request;

    let name = name.trim();
    if name.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "Journal name cannot be empty".to_string(),
        )));
    }

    let id = format!("journal_{}", uuid::Uuid::new_v4().simple());
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO journals (
            id, name, description, icon, accent_color, space_prompt,
            default_model_name, tool_preferences_json, is_archived, sort_order, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, 0, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(icon)
    .bind(accent_color)
    .bind(space_prompt)
    .bind(default_model_name)
    .bind(tool_preferences_json)
    .bind(&now)
    .bind(&now)
    .execute(container.db_pool())
    .await
    .map_err(|e| ApiError::from(AppError::Database(format!("Failed to create journal: {}", e))))?;

    let created = sqlx::query_as::<_, ConversationJournalDto>(
        r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM journals
        WHERE id = ?
        "#,
    )
    .bind(&id)
    .fetch_one(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to fetch created journal: {}",
            e
        )))
    })?;

    Ok(created)
}

pub async fn list_journals_impl(
    container: &Container,
) -> Result<Vec<ConversationJournalDto>, ApiError> {
    let journals = sqlx::query_as::<_, ConversationJournalDto>(
        r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM journals
        ORDER BY is_archived ASC, sort_order ASC, updated_at DESC
        "#,
    )
    .fetch_all(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to list journals: {}",
            e
        )))
    })?;

    Ok(journals)
}

pub async fn update_journal_impl(
    request: UpdateConversationJournalRequestDto,
    container: &Container,
) -> Result<ConversationJournalDto, ApiError> {
    let now = Utc::now().to_rfc3339();
    let is_archived: Option<i64> = request.is_archived.map(|v| if v { 1 } else { 0 });

    let result = sqlx::query(
        r#"
        UPDATE journals
        SET
            name = COALESCE(?, name),
            description = COALESCE(?, description),
            icon = COALESCE(?, icon),
            accent_color = COALESCE(?, accent_color),
            space_prompt = COALESCE(?, space_prompt),
            default_model_name = COALESCE(?, default_model_name),
            tool_preferences_json = COALESCE(?, tool_preferences_json),
            is_archived = COALESCE(?, is_archived),
            sort_order = COALESCE(?, sort_order),
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(request.name)
    .bind(request.description)
    .bind(request.icon)
    .bind(request.accent_color)
    .bind(request.space_prompt)
    .bind(request.default_model_name)
    .bind(request.tool_preferences_json)
    .bind(is_archived)
    .bind(request.sort_order)
    .bind(&now)
    .bind(&request.journal_id)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to update journal: {}",
            e
        )))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Journal not found: {}",
            request.journal_id
        ))));
    }

    let updated = sqlx::query_as::<_, ConversationJournalDto>(
        r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM journals
        WHERE id = ?
        "#,
    )
    .bind(&request.journal_id)
    .fetch_one(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to fetch updated journal: {}",
            e
        )))
    })?;

    Ok(updated)
}

pub async fn archive_journal_impl(
    request: ArchiveConversationJournalRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let now = Utc::now().to_rfc3339();
    let archived = if request.archived { 1 } else { 0 };

    let result = sqlx::query(
        r#"
        UPDATE journals
        SET is_archived = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(archived)
    .bind(&now)
    .bind(&request.journal_id)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to archive journal: {}",
            e
        )))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Journal not found: {}",
            request.journal_id
        ))));
    }

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn delete_journal_impl(
    request: DeleteConversationJournalRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let journal_id = request.journal_id.trim();
    if journal_id.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "journalId is required".to_string(),
        )));
    }

    let result = sqlx::query(
        r#"
        DELETE FROM journals
        WHERE id = ?
        "#,
    )
    .bind(journal_id)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to delete journal: {}",
            e
        )))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Journal not found: {}",
            journal_id
        ))));
    }

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn list_conversation_space_members_impl(
    space_id: String,
    container: &Container,
) -> Result<Vec<ConversationSpaceMemberDto>, ApiError> {
    let members = sqlx::query_as::<_, ConversationSpaceMemberDto>(
        r#"
        SELECT
            sm.space_id AS space_id,
            sm.member_id AS member_id,
            cp.display_name AS display_name,
            cp.email AS email,
            cp.avatar_url AS avatar_url,
            sm.role AS role,
            sm.created_at AS created_at,
            sm.updated_at AS updated_at
        FROM conversation_space_members sm
        INNER JOIN collaborator_profiles cp ON cp.id = sm.member_id
        WHERE sm.space_id = ?
        ORDER BY
            CASE sm.role
                WHEN 'owner' THEN 0
                WHEN 'editor' THEN 1
                ELSE 2
            END,
            cp.display_name COLLATE NOCASE ASC
        "#,
    )
    .bind(&space_id)
    .fetch_all(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to list conversation space members: {}",
            e
        )))
    })?;

    Ok(members)
}

pub async fn upsert_conversation_space_member_impl(
    request: UpsertConversationSpaceMemberRequestDto,
    container: &Container,
) -> Result<ConversationSpaceMemberDto, ApiError> {
    let role = validate_space_role(&request.role)?;

    let space_exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_spaces WHERE id = ?")
            .bind(&request.space_id)
            .fetch_one(container.db_pool())
            .await
            .map_err(|e| {
                ApiError::from(AppError::Database(format!(
                    "Failed to verify space before upserting member: {}",
                    e
                )))
            })?;
    if space_exists == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Space not found: {}",
            request.space_id
        ))));
    }

    let now = Utc::now().to_rfc3339();
    let display_name = request
        .display_name
        .clone()
        .unwrap_or_else(|| request.member_id.clone());

    let mut tx = container.db_pool().begin().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to begin transaction while upserting space member: {}",
            e
        )))
    })?;

    sqlx::query(
        r#"
        INSERT INTO collaborator_profiles (
            id, display_name, email, avatar_url, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            display_name = excluded.display_name,
            email = COALESCE(excluded.email, collaborator_profiles.email),
            avatar_url = COALESCE(excluded.avatar_url, collaborator_profiles.avatar_url),
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&request.member_id)
    .bind(display_name)
    .bind(request.email)
    .bind(request.avatar_url)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to upsert collaborator profile: {}",
            e
        )))
    })?;

    sqlx::query(
        r#"
        INSERT INTO conversation_space_members (
            space_id, member_id, role, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(space_id, member_id) DO UPDATE SET
            role = excluded.role,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&request.space_id)
    .bind(&request.member_id)
    .bind(role)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to upsert conversation space member: {}",
            e
        )))
    })?;

    tx.commit().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to commit conversation space member transaction: {}",
            e
        )))
    })?;

    let member = sqlx::query_as::<_, ConversationSpaceMemberDto>(
        r#"
        SELECT
            sm.space_id AS space_id,
            sm.member_id AS member_id,
            cp.display_name AS display_name,
            cp.email AS email,
            cp.avatar_url AS avatar_url,
            sm.role AS role,
            sm.created_at AS created_at,
            sm.updated_at AS updated_at
        FROM conversation_space_members sm
        INNER JOIN collaborator_profiles cp ON cp.id = sm.member_id
        WHERE sm.space_id = ? AND sm.member_id = ?
        LIMIT 1
        "#,
    )
    .bind(&request.space_id)
    .bind(&request.member_id)
    .fetch_one(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to fetch upserted conversation space member: {}",
            e
        )))
    })?;

    Ok(member)
}

pub async fn remove_conversation_space_member_impl(
    request: RemoveConversationSpaceMemberRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let current_role = sqlx::query_scalar::<_, String>(
        r#"
        SELECT role
        FROM conversation_space_members
        WHERE space_id = ? AND member_id = ?
        LIMIT 1
        "#,
    )
    .bind(&request.space_id)
    .bind(&request.member_id)
    .fetch_optional(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to verify conversation space member: {}",
            e
        )))
    })?
    .ok_or_else(|| {
        ApiError::from(AppError::NotFound(format!(
            "Member {} is not assigned to space {}",
            request.member_id, request.space_id
        )))
    })?;

    if current_role == "owner" {
        let owner_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversation_space_members WHERE space_id = ? AND role = 'owner'",
        )
        .bind(&request.space_id)
        .fetch_one(container.db_pool())
        .await
        .map_err(|e| {
            ApiError::from(AppError::Database(format!(
                "Failed to validate owner count for space member removal: {}",
                e
            )))
        })?;

        if owner_count <= 1 {
            return Err(ApiError::from(AppError::InvalidInput(
                "Cannot remove the final owner from a space".to_string(),
            )));
        }
    }

    sqlx::query(
        r#"
        DELETE FROM conversation_space_members
        WHERE space_id = ? AND member_id = ?
        "#,
    )
    .bind(&request.space_id)
    .bind(&request.member_id)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to remove conversation space member: {}",
            e
        )))
    })?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn update_conversation_space_impl(
    request: UpdateConversationSpaceRequestDto,
    container: &Container,
) -> Result<ConversationSpaceDto, ApiError> {
    validate_space_preferences_not_journal(
        request.tool_preferences_json.as_deref(),
        "Conversation space",
    )?;

    let now = Utc::now().to_rfc3339();
    let is_archived: Option<i64> = request.is_archived.map(|v| if v { 1 } else { 0 });

    let result = sqlx::query(
        r#"
        UPDATE conversation_spaces
        SET
            name = COALESCE(?, name),
            description = COALESCE(?, description),
            icon = COALESCE(?, icon),
            accent_color = COALESCE(?, accent_color),
            space_prompt = COALESCE(?, space_prompt),
            default_model_name = COALESCE(?, default_model_name),
            tool_preferences_json = COALESCE(?, tool_preferences_json),
            is_archived = COALESCE(?, is_archived),
            sort_order = COALESCE(?, sort_order),
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(request.name)
    .bind(request.description)
    .bind(request.icon)
    .bind(request.accent_color)
    .bind(request.space_prompt)
    .bind(request.default_model_name)
    .bind(request.tool_preferences_json)
    .bind(is_archived)
    .bind(request.sort_order)
    .bind(&now)
    .bind(&request.space_id)
    .execute(container.db_pool())
    .await
    .map_err(|e| ApiError::from(AppError::Database(format!("Failed to update space: {}", e))))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Space not found: {}",
            request.space_id
        ))));
    }

    let updated = sqlx::query_as::<_, ConversationSpaceDto>(
        r#"
        SELECT
            id, name, description, icon, accent_color, space_prompt, default_model_name,
            tool_preferences_json, is_archived, sort_order, created_at, updated_at
        FROM conversation_spaces
        WHERE id = ?
        "#,
    )
    .bind(&request.space_id)
    .fetch_one(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to fetch updated space: {}",
            e
        )))
    })?;

    Ok(updated)
}

pub async fn archive_conversation_space_impl(
    request: ArchiveConversationSpaceRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    if request.space_id == DEFAULT_SPACE_ID && request.archived {
        return Err(ApiError::from(AppError::InvalidInput(
            "The default space cannot be archived".to_string(),
        )));
    }

    let now = Utc::now().to_rfc3339();
    let archived = if request.archived { 1 } else { 0 };

    let result = sqlx::query(
        r#"
        UPDATE conversation_spaces
        SET is_archived = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(archived)
    .bind(&now)
    .bind(&request.space_id)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to archive space: {}",
            e
        )))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Space not found: {}",
            request.space_id
        ))));
    }

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn move_conversation_to_space_impl(
    request: MoveConversationToSpaceRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ensure_standard_space(container.db_pool(), &request.space_id).await?;

    let mut tx = container.db_pool().begin().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to begin transaction: {}",
            e
        )))
    })?;

    let _current_space_id =
        sqlx::query_scalar::<_, String>("SELECT space_id FROM conversations WHERE id = ? LIMIT 1")
            .bind(&request.conversation_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                ApiError::from(AppError::Database(format!(
                    "Failed to resolve current conversation space: {}",
                    e
                )))
            })?
            .ok_or_else(|| {
                ApiError::from(AppError::NotFound(format!(
                    "Conversation not found: {}",
                    request.conversation_id
                )))
            })?;

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE conversations
        SET space_id = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&request.space_id)
    .bind(&now)
    .bind(&request.conversation_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to move conversation: {}",
            e
        )))
    })?;

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at)
        SELECT cd.document_id, ?, ?
        FROM conversation_documents cd
        WHERE cd.conversation_id = ?
        "#,
    )
    .bind(&request.space_id)
    .bind(&now)
    .bind(&request.conversation_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to sync destination space memberships: {}",
            e
        )))
    })?;

    tx.commit().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to commit transaction: {}",
            e
        )))
    })?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn add_conversation_to_journal_impl(
    request: AddConversationToJournalRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let journal_space_id = request.journal_space_id.trim();
    let conversation_id = request.conversation_id.trim();

    if journal_space_id.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "journalSpaceId is required".to_string(),
        )));
    }
    if conversation_id.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "conversationId is required".to_string(),
        )));
    }

    ensure_journal_space(container.db_pool(), journal_space_id).await?;

    let conversation_exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
            .bind(conversation_id)
            .fetch_one(container.db_pool())
            .await
            .map_err(|e| {
                ApiError::from(AppError::Database(format!(
                    "Failed to verify conversation: {}",
                    e
                )))
            })?;

    if conversation_exists == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Conversation not found: {}",
            conversation_id
        ))));
    }

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO journal_conversation_entries (journal_space_id, conversation_id, created_at)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(journal_space_id)
    .bind(conversation_id)
    .bind(Utc::now().to_rfc3339())
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to add conversation to journal: {}",
            e
        )))
    })?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn remove_conversation_from_journal_impl(
    request: RemoveConversationFromJournalRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let journal_space_id = request.journal_space_id.trim();
    let conversation_id = request.conversation_id.trim();

    if journal_space_id.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "journalSpaceId is required".to_string(),
        )));
    }
    if conversation_id.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "conversationId is required".to_string(),
        )));
    }

    ensure_journal_space(container.db_pool(), journal_space_id).await?;

    sqlx::query(
        r#"
        DELETE FROM journal_conversation_entries
        WHERE journal_space_id = ? AND conversation_id = ?
        "#,
    )
    .bind(journal_space_id)
    .bind(conversation_id)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to remove conversation from journal: {}",
            e
        )))
    })?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn list_journal_conversations_impl(
    query: ListJournalConversationsQueryDto,
    container: &Container,
) -> Result<ListConversationsResponseDto, ApiError> {
    let journal_space_id = query.journal_space_id.trim();
    if journal_space_id.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "journalSpaceId is required".to_string(),
        )));
    }

    ensure_journal_space(container.db_pool(), journal_space_id).await?;

    let limit = query.limit.unwrap_or(120).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let include_archived = query.include_archived.unwrap_or(false);
    let search_query = query
        .query
        .as_ref()
        .map(|q| q.trim())
        .filter(|q| !q.is_empty());
    let fts_query = search_query.and_then(build_fts_query);

    let mut qb = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            c.id,
            c.title,
            c.model_name,
            c.system_prompt,
            c.created_at,
            c.updated_at,
            c.message_count,
            c.total_tokens,
            c.space_id,
            c.is_saved,
            c.is_bookmarked,
            c.is_pinned,
            c.is_archived,
            c.saved_at,
            c.bookmarked_at,
            c.pinned_at,
            c.archived_at,
            (
                SELECT m.content
                FROM conversation_messages m
                WHERE m.conversation_id = c.id
                ORDER BY m.created_at DESC
                LIMIT 1
            ) AS last_message_preview
        FROM conversations c
        WHERE (
            EXISTS (
                SELECT 1
                FROM journal_conversation_entries jce
                WHERE jce.journal_space_id = 
        "#,
    );
    qb.push_bind(journal_space_id)
        .push(" AND jce.conversation_id = c.id")
        .push(")");

    if !include_archived {
        qb.push(" AND c.is_archived = 0");
    }

    if let Some(fts) = &fts_query {
        qb.push(
            " AND EXISTS (
                SELECT 1
                FROM conversation_search_fts fts
                WHERE fts.conversation_id = c.id
                  AND fts.content MATCH ",
        )
        .push_bind(fts.clone())
        .push(")");
    }

    if let Some(fts) = &fts_query {
        qb.push(
            " ORDER BY
                c.is_pinned DESC,
                COALESCE((
                    SELECT MIN(bm25(conversation_search_fts))
                    FROM conversation_search_fts
                    WHERE conversation_id = c.id
                      AND content MATCH ",
        )
        .push_bind(fts.clone())
        .push(
            "
                ), 999999.0),
                c.updated_at DESC",
        );
    } else {
        qb.push(" ORDER BY c.is_pinned DESC, c.updated_at DESC");
    }

    qb.push(" LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = qb
        .build_query_as::<ConversationExplorerRow>()
        .fetch_all(container.db_pool())
        .await
        .map_err(|e| {
            ApiError::from(AppError::Database(format!(
                "Failed to list journal conversations: {}",
                e
            )))
        })?;

    let conversations = rows
        .into_iter()
        .map(
            |row| crate::application::dtos::conversation_dto::ConversationDto {
                id: row.id,
                title: row.title,
                model_name: row.model_name,
                system_prompt: row.system_prompt,
                created_at: row.created_at,
                updated_at: row.updated_at,
                message_count: row.message_count,
                total_tokens: row.total_tokens,
                space_id: Some(row.space_id),
                is_saved: Some(row.is_saved != 0),
                is_bookmarked: Some(row.is_bookmarked != 0),
                is_pinned: Some(row.is_pinned != 0),
                is_archived: Some(row.is_archived != 0),
                saved_at: row.saved_at,
                bookmarked_at: row.bookmarked_at,
                pinned_at: row.pinned_at,
                archived_at: row.archived_at,
                last_message_preview: row.last_message_preview,
            },
        )
        .collect::<Vec<_>>();

    Ok(ListConversationsResponseDto {
        total: conversations.len(),
        conversations,
    })
}

pub async fn list_conversation_linked_documents_impl(
    conversation_id: String,
    container: &Container,
) -> Result<Vec<ConversationLinkedDocumentDto>, ApiError> {
    let linked_docs = sqlx::query_as::<_, ConversationLinkedDocumentRow>(
        r#"
        SELECT
            d.id AS document_id,
            d.file_name AS file_name,
            d.file_path AS file_path,
            d.file_type AS file_type,
            d.category AS category,
            d.indexed_at AS indexed_at,
            MAX(cd.added_at) AS last_referenced_at,
            COUNT(*) AS reference_count
        FROM conversation_documents cd
        INNER JOIN documents d ON d.id = cd.document_id
        WHERE cd.conversation_id = ?
        GROUP BY d.id, d.file_name, d.file_path, d.file_type, d.category, d.indexed_at
        ORDER BY last_referenced_at DESC
        "#,
    )
    .bind(&conversation_id)
    .fetch_all(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to list conversation linked documents: {}",
            e
        )))
    })?;

    Ok(linked_docs
        .into_iter()
        .map(|row| ConversationLinkedDocumentDto {
            document_id: row.document_id,
            file_name: row.file_name,
            file_path: row.file_path,
            file_type: row.file_type.unwrap_or_default(),
            category: row.category,
            indexed_at: row.indexed_at,
            last_referenced_at: row.last_referenced_at,
            reference_count: row.reference_count,
        })
        .collect())
}

pub async fn remove_conversation_linked_document_impl(
    conversation_id: String,
    document_id: String,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let mut tx = container.db_pool().begin().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to begin transaction: {}",
            e
        )))
    })?;

    let conversation_exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
            .bind(&conversation_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                ApiError::from(AppError::Database(format!(
                    "Failed to verify conversation: {}",
                    e
                )))
            })?;

    if conversation_exists == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Conversation not found: {}",
            conversation_id
        ))));
    }

    let deleted = sqlx::query(
        r#"
        DELETE FROM conversation_documents
        WHERE conversation_id = ? AND document_id = ?
        "#,
    )
    .bind(&conversation_id)
    .bind(&document_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to remove linked document from conversation: {}",
            e
        )))
    })?;

    if deleted.rows_affected() == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Document {} is not linked to conversation {}",
            document_id, conversation_id
        ))));
    }

    tx.commit().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to commit transaction: {}",
            e
        )))
    })?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn add_conversation_web_source_impl(
    conversation_id: String,
    url: String,
    title: Option<String>,
    excerpt: Option<String>,
    relevance_score: Option<f32>,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let conversation_id = conversation_id.trim().to_string();
    if conversation_id.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "conversationId is required".to_string(),
        )));
    }

    let url = url.trim().to_string();
    if url.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "url is required".to_string(),
        )));
    }

    let normalized_url = normalize_web_source_url(&url);
    let now = Utc::now().to_rfc3339();
    let source_id = format!("cws_{}", uuid::Uuid::new_v4().simple());

    let mut tx = container.db_pool().begin().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to begin transaction: {}",
            e
        )))
    })?;

    let conversation_exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
            .bind(&conversation_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                ApiError::from(AppError::Database(format!(
                    "Failed to verify conversation: {}",
                    e
                )))
            })?;

    if conversation_exists == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Conversation not found: {}",
            conversation_id
        ))));
    }

    sqlx::query(
        r#"
        INSERT INTO conversation_web_sources (
            id,
            conversation_id,
            url,
            normalized_url,
            title,
            excerpt,
            relevance_score,
            added_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(conversation_id, normalized_url) DO UPDATE SET
            url = excluded.url,
            title = COALESCE(excluded.title, conversation_web_sources.title),
            excerpt = COALESCE(excluded.excerpt, conversation_web_sources.excerpt),
            relevance_score = COALESCE(excluded.relevance_score, conversation_web_sources.relevance_score),
            added_at = excluded.added_at
        "#,
    )
    .bind(&source_id)
    .bind(&conversation_id)
    .bind(&url)
    .bind(&normalized_url)
    .bind(title.as_ref().map(|value| value.trim().to_string()))
    .bind(excerpt.as_ref().map(|value| value.trim().to_string()))
    .bind(relevance_score)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to upsert conversation web source: {}",
            e
        )))
    })?;

    tx.commit().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to commit transaction: {}",
            e
        )))
    })?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn list_conversation_web_sources_impl(
    conversation_id: String,
    container: &Container,
) -> Result<Vec<ConversationWebSourceDto>, ApiError> {
    let rows = sqlx::query_as::<_, ConversationWebSourceRow>(
        r#"
        SELECT
            id,
            url,
            normalized_url,
            title,
            excerpt,
            relevance_score,
            added_at
        FROM conversation_web_sources
        WHERE conversation_id = ?
        ORDER BY added_at DESC
        "#,
    )
    .bind(&conversation_id)
    .fetch_all(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to list conversation web sources: {}",
            e
        )))
    })?;

    Ok(rows
        .into_iter()
        .map(|row| ConversationWebSourceDto {
            id: row.id,
            url: row.url,
            normalized_url: row.normalized_url,
            title: row.title,
            excerpt: row.excerpt,
            relevance_score: row.relevance_score,
            added_at: row.added_at,
        })
        .collect())
}

pub async fn remove_conversation_web_source_impl(
    conversation_id: String,
    source_id: String,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let mut tx = container.db_pool().begin().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to begin transaction: {}",
            e
        )))
    })?;

    let deleted = sqlx::query(
        r#"
        DELETE FROM conversation_web_sources
        WHERE conversation_id = ? AND id = ?
        "#,
    )
    .bind(&conversation_id)
    .bind(&source_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to remove conversation web source: {}",
            e
        )))
    })?;

    if deleted.rows_affected() == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Web source {} not found in conversation {}",
            source_id, conversation_id
        ))));
    }

    tx.commit().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to commit transaction: {}",
            e
        )))
    })?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn list_document_space_memberships_impl(
    document_id: String,
    container: &Container,
) -> Result<Vec<DocumentSpaceMembershipDto>, ApiError> {
    let memberships = sqlx::query_as::<_, DocumentSpaceMembershipRow>(
        r#"
        SELECT
            cs.id AS space_id,
            cs.name AS space_name,
            cs.is_archived AS is_archived,
            dsm.created_at AS created_at
        FROM document_space_memberships dsm
        INNER JOIN conversation_spaces cs ON cs.id = dsm.space_id
        WHERE dsm.document_id = ?
        ORDER BY cs.is_archived ASC, cs.name ASC
        "#,
    )
    .bind(&document_id)
    .fetch_all(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to list document space memberships: {}",
            e
        )))
    })?;

    Ok(memberships
        .into_iter()
        .map(|row| DocumentSpaceMembershipDto {
            space_id: row.space_id,
            space_name: row.space_name,
            is_archived: row.is_archived != 0,
            created_at: row.created_at,
        })
        .collect())
}

pub async fn set_document_space_membership_impl(
    document_id: String,
    space_id: String,
    assigned: bool,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let document_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id = ?")
        .bind(&document_id)
        .fetch_one(container.db_pool())
        .await
        .map_err(|e| {
            ApiError::from(AppError::Database(format!(
                "Failed to verify document: {}",
                e
            )))
        })?;

    if document_exists == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Document not found: {}",
            document_id
        ))));
    }

    if assigned {
        let space_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM conversation_spaces WHERE id = ?")
                .bind(&space_id)
                .fetch_one(container.db_pool())
                .await
                .map_err(|e| {
                    ApiError::from(AppError::Database(format!("Failed to verify space: {}", e)))
                })?;

        if space_exists == 0 {
            return Err(ApiError::from(AppError::NotFound(format!(
                "Space not found: {}",
                space_id
            ))));
        }

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(&document_id)
        .bind(&space_id)
        .bind(now)
        .execute(container.db_pool())
        .await
        .map_err(|e| {
            ApiError::from(AppError::Database(format!(
                "Failed to assign document to space: {}",
                e
            )))
        })?;
    } else {
        sqlx::query(
            r#"
            DELETE FROM document_space_memberships
            WHERE document_id = ? AND space_id = ?
            "#,
        )
        .bind(&document_id)
        .bind(&space_id)
        .execute(container.db_pool())
        .await
        .map_err(|e| {
            ApiError::from(AppError::Database(format!(
                "Failed to remove document space membership: {}",
                e
            )))
        })?;
    }

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn set_documents_space_membership_impl(
    document_ids: Vec<String>,
    space_id: String,
    assigned: bool,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    const DOCUMENT_BATCH_SIZE: usize = 250;

    let mut unique_document_ids = document_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    unique_document_ids.sort();
    unique_document_ids.dedup();

    if unique_document_ids.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "At least one document ID is required".to_string(),
        )));
    }

    let space_exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_spaces WHERE id = ?")
            .bind(&space_id)
            .fetch_one(container.db_pool())
            .await
            .map_err(|e| {
                ApiError::from(AppError::Database(format!("Failed to verify space: {}", e)))
            })?;

    if space_exists == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Space not found: {}",
            space_id
        ))));
    }

    let mut existing_document_ids = HashSet::with_capacity(unique_document_ids.len());
    for document_id_batch in unique_document_ids.chunks(DOCUMENT_BATCH_SIZE) {
        let mut qb = QueryBuilder::<Sqlite>::new("SELECT id FROM documents WHERE id IN (");
        {
            let mut separated = qb.separated(", ");
            for document_id in document_id_batch {
                separated.push_bind(document_id.as_str());
            }
        }
        qb.push(")");

        let found = qb
            .build_query_scalar::<String>()
            .fetch_all(container.db_pool())
            .await
            .map_err(|e| {
                ApiError::from(AppError::Database(format!(
                    "Failed to verify documents: {}",
                    e
                )))
            })?;

        for existing_id in found {
            existing_document_ids.insert(existing_id);
        }
    }

    if let Some(missing_document_id) = unique_document_ids
        .iter()
        .find(|document_id| !existing_document_ids.contains(*document_id))
    {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Document not found: {}",
            missing_document_id
        ))));
    }

    if assigned {
        let now = Utc::now().to_rfc3339();
        for document_id_batch in unique_document_ids.chunks(DOCUMENT_BATCH_SIZE) {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at) ",
            );
            qb.push_values(document_id_batch.iter(), |mut builder, document_id| {
                builder
                    .push_bind(document_id.as_str())
                    .push_bind(space_id.as_str())
                    .push_bind(now.as_str());
            });

            qb.build().execute(container.db_pool()).await.map_err(|e| {
                ApiError::from(AppError::Database(format!(
                    "Failed to assign documents to space: {}",
                    e
                )))
            })?;
        }
    } else {
        for document_id_batch in unique_document_ids.chunks(DOCUMENT_BATCH_SIZE) {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "DELETE FROM document_space_memberships WHERE space_id = ",
            );
            qb.push_bind(space_id.as_str())
                .push(" AND document_id IN (");
            {
                let mut separated = qb.separated(", ");
                for document_id in document_id_batch {
                    separated.push_bind(document_id.as_str());
                }
            }
            qb.push(")");

            qb.build().execute(container.db_pool()).await.map_err(|e| {
                ApiError::from(AppError::Database(format!(
                    "Failed to remove documents from space: {}",
                    e
                )))
            })?;
        }
    }

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

async fn set_conversation_state(
    pool: &SqlitePool,
    conversation_id: &str,
    column_name: &str,
    timestamp_column: &str,
    value: bool,
) -> Result<(), ApiError> {
    let now = Utc::now().to_rfc3339();
    let mut qb = QueryBuilder::<Sqlite>::new("UPDATE conversations SET ");
    qb.push(column_name)
        .push(" = ")
        .push_bind(if value { 1_i64 } else { 0_i64 })
        .push(", ")
        .push(timestamp_column)
        .push(" = ")
        .push_bind(if value {
            Some(now.clone())
        } else {
            None::<String>
        })
        .push(", updated_at = ")
        .push_bind(now)
        .push(" WHERE id = ")
        .push_bind(conversation_id);

    let result = qb.build().execute(pool).await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to update conversation state: {}",
            e
        )))
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Conversation not found: {}",
            conversation_id
        ))));
    }

    Ok(())
}

pub async fn set_conversation_saved_impl(
    request: SetConversationStateRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    set_conversation_state(
        container.db_pool(),
        &request.conversation_id,
        "is_saved",
        "saved_at",
        request.value,
    )
    .await?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn set_conversation_bookmarked_impl(
    request: SetConversationStateRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    set_conversation_state(
        container.db_pool(),
        &request.conversation_id,
        "is_bookmarked",
        "bookmarked_at",
        request.value,
    )
    .await?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn set_conversation_pinned_impl(
    request: SetConversationStateRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    set_conversation_state(
        container.db_pool(),
        &request.conversation_id,
        "is_pinned",
        "pinned_at",
        request.value,
    )
    .await?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn set_conversation_archived_impl(
    request: SetConversationStateRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    set_conversation_state(
        container.db_pool(),
        &request.conversation_id,
        "is_archived",
        "archived_at",
        request.value,
    )
    .await?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn bookmark_conversation_message_impl(
    request: BookmarkConversationMessageRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let exists: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM conversation_messages
        WHERE id = ? AND conversation_id = ?
        "#,
    )
    .bind(&request.message_id)
    .bind(&request.conversation_id)
    .fetch_one(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to verify message bookmark target: {}",
            e
        )))
    })?;

    if exists == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Message {} does not belong to conversation {}",
            request.message_id, request.conversation_id
        ))));
    }

    let id = format!("cmb_{}", uuid::Uuid::new_v4().simple());
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO conversation_message_bookmarks (
            id, conversation_id, message_id, title, note, created_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(conversation_id, message_id)
        DO UPDATE SET
            title = excluded.title,
            note = excluded.note,
            created_at = excluded.created_at
        "#,
    )
    .bind(id)
    .bind(&request.conversation_id)
    .bind(&request.message_id)
    .bind(request.title)
    .bind(request.note)
    .bind(now)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to bookmark conversation message: {}",
            e
        )))
    })?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn unbookmark_conversation_message_impl(
    request: UnbookmarkConversationMessageRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    sqlx::query(
        r#"
        DELETE FROM conversation_message_bookmarks
        WHERE conversation_id = ? AND message_id = ?
        "#,
    )
    .bind(&request.conversation_id)
    .bind(&request.message_id)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to remove message bookmark: {}",
            e
        )))
    })?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn delete_conversation_message_impl(
    request: DeleteConversationMessageRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let mut tx = container.db_pool().begin().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to start delete message transaction: {}",
            e
        )))
    })?;

    let exists: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM conversation_messages
        WHERE id = ? AND conversation_id = ?
        "#,
    )
    .bind(&request.message_id)
    .bind(&request.conversation_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to verify message deletion target: {}",
            e
        )))
    })?;

    if exists == 0 {
        return Err(ApiError::from(AppError::NotFound(format!(
            "Message {} does not belong to conversation {}",
            request.message_id, request.conversation_id
        ))));
    }

    sqlx::query(
        r#"
        DELETE FROM conversation_messages
        WHERE id = ? AND conversation_id = ?
        "#,
    )
    .bind(&request.message_id)
    .bind(&request.conversation_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to delete message: {}",
            e
        )))
    })?;

    #[derive(sqlx::FromRow)]
    struct MessageStatsRow {
        message_count: i64,
        total_tokens: i64,
    }

    let stats = sqlx::query_as::<_, MessageStatsRow>(
        r#"
        SELECT
            COUNT(*) AS message_count,
            COALESCE(SUM(tokens), 0) AS total_tokens
        FROM conversation_messages
        WHERE conversation_id = ?
        "#,
    )
    .bind(&request.conversation_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to recalculate conversation message stats: {}",
            e
        )))
    })?;

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE conversations
        SET message_count = ?,
            total_tokens = ?,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(stats.message_count)
    .bind(stats.total_tokens)
    .bind(now)
    .bind(&request.conversation_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to update conversation stats after message deletion: {}",
            e
        )))
    })?;

    tx.commit().await.map_err(|e| {
        ApiError::from(AppError::Database(format!(
            "Failed to commit delete message transaction: {}",
            e
        )))
    })?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn list_message_bookmarks_impl(
    query: ListMessageBookmarksQueryDto,
    container: &Container,
) -> Result<ListMessageBookmarksResponseDto, ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let search_query = query
        .query
        .as_ref()
        .map(|q| q.trim())
        .filter(|q| !q.is_empty());
    let fts_query = search_query.and_then(build_fts_query);

    let mut qb = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            b.id,
            b.conversation_id,
            c.title AS conversation_title,
            c.space_id AS space_id,
            b.message_id,
            m.role AS message_role,
            m.content AS message_preview,
            b.title,
            b.note,
            b.created_at
        FROM conversation_message_bookmarks b
        INNER JOIN conversations c ON c.id = b.conversation_id
        INNER JOIN conversation_messages m ON m.id = b.message_id
        WHERE 1 = 1
        "#,
    );

    if let Some(conversation_id) = &query.conversation_id {
        qb.push(" AND b.conversation_id = ")
            .push_bind(conversation_id);
    }

    if let Some(fts) = &fts_query {
        qb.push(
            " AND EXISTS (
                SELECT 1
                FROM conversation_search_fts fts
                WHERE fts.conversation_id = b.conversation_id
                  AND fts.content MATCH ",
        )
        .push_bind(fts.clone())
        .push(
            "
                  AND (
                    (fts.source = 'bookmark' AND fts.message_id = b.message_id)
                    OR (fts.source = 'message' AND fts.message_id = b.message_id)
                    OR fts.source = 'title'
                  )
            )",
        );
    }

    if let Some(fts) = &fts_query {
        qb.push(
            " ORDER BY
                COALESCE((
                    SELECT MIN(bm25(conversation_search_fts))
                    FROM conversation_search_fts
                    WHERE conversation_id = b.conversation_id
                      AND content MATCH ",
        )
        .push_bind(fts.clone())
        .push(
            "
                ), 999999.0),
                b.created_at DESC",
        );
    } else {
        qb.push(" ORDER BY b.created_at DESC");
    }

    qb.push(" LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = qb
        .build_query_as::<MessageBookmarkRow>()
        .fetch_all(container.db_pool())
        .await
        .map_err(|e| {
            ApiError::from(AppError::Database(format!(
                "Failed to list message bookmarks: {}",
                e
            )))
        })?;

    let bookmarks = rows
        .into_iter()
        .map(|row| ConversationMessageBookmarkDto {
            id: row.id,
            conversation_id: row.conversation_id,
            conversation_title: row.conversation_title,
            space_id: row.space_id,
            message_id: row.message_id,
            message_role: row.message_role,
            message_preview: row.message_preview,
            title: row.title,
            note: row.note,
            created_at: row.created_at,
        })
        .collect::<Vec<_>>();

    Ok(ListMessageBookmarksResponseDto {
        total: bookmarks.len(),
        bookmarks,
    })
}

pub async fn list_conversations_explorer_impl(
    query: ListConversationsExplorerQueryDto,
    container: &Container,
) -> Result<ListConversationsResponseDto, ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let include_archived = query.include_archived.unwrap_or(false);
    let search_query = query
        .query
        .as_ref()
        .map(|q| q.trim())
        .filter(|q| !q.is_empty());
    let fts_query = search_query.and_then(build_fts_query);

    let mut qb = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            c.id,
            c.title,
            c.model_name,
            c.system_prompt,
            c.created_at,
            c.updated_at,
            c.message_count,
            c.total_tokens,
            c.space_id,
            c.is_saved,
            c.is_bookmarked,
            c.is_pinned,
            c.is_archived,
            c.saved_at,
            c.bookmarked_at,
            c.pinned_at,
            c.archived_at,
            (
                SELECT m.content
                FROM conversation_messages m
                WHERE m.conversation_id = c.id
                ORDER BY m.created_at DESC
                LIMIT 1
            ) AS last_message_preview
        FROM conversations c
        WHERE 1 = 1
        "#,
    );

    if let Some(space_id) = &query.space_id {
        ensure_standard_space(container.db_pool(), space_id).await?;
        qb.push(" AND c.space_id = ").push_bind(space_id);
    }

    if query.saved_only.unwrap_or(false) {
        qb.push(" AND c.is_saved = 1");
    }

    if query.bookmarked_only.unwrap_or(false) {
        qb.push(" AND c.is_bookmarked = 1");
    }

    if query.pinned_only.unwrap_or(false) {
        qb.push(" AND c.is_pinned = 1");
    }

    if query.has_message_bookmarks.unwrap_or(false) {
        qb.push(
            " AND EXISTS (SELECT 1 FROM conversation_message_bookmarks b WHERE b.conversation_id = c.id)",
        );
    }

    if !include_archived {
        qb.push(" AND c.is_archived = 0");
    }

    if let Some(fts) = &fts_query {
        qb.push(
            " AND EXISTS (
                SELECT 1
                FROM conversation_search_fts fts
                WHERE fts.conversation_id = c.id
                  AND fts.content MATCH ",
        )
        .push_bind(fts.clone())
        .push(")");
    }

    if let Some(fts) = &fts_query {
        qb.push(
            " ORDER BY
                c.is_pinned DESC,
                COALESCE((
                    SELECT MIN(bm25(conversation_search_fts))
                    FROM conversation_search_fts
                    WHERE conversation_id = c.id
                      AND content MATCH ",
        )
        .push_bind(fts.clone())
        .push(
            "
                ), 999999.0),
                c.updated_at DESC",
        );
    } else {
        qb.push(" ORDER BY c.is_pinned DESC, c.updated_at DESC");
    }

    qb.push(" LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = qb
        .build_query_as::<ConversationExplorerRow>()
        .fetch_all(container.db_pool())
        .await
        .map_err(|e| {
            ApiError::from(AppError::Database(format!(
                "Failed to list explorer conversations: {}",
                e
            )))
        })?;

    let conversations = rows
        .into_iter()
        .map(
            |row| crate::application::dtos::conversation_dto::ConversationDto {
                id: row.id,
                title: row.title,
                model_name: row.model_name,
                system_prompt: row.system_prompt,
                created_at: row.created_at,
                updated_at: row.updated_at,
                message_count: row.message_count,
                total_tokens: row.total_tokens,
                space_id: Some(row.space_id),
                is_saved: Some(row.is_saved != 0),
                is_bookmarked: Some(row.is_bookmarked != 0),
                is_pinned: Some(row.is_pinned != 0),
                is_archived: Some(row.is_archived != 0),
                saved_at: row.saved_at,
                bookmarked_at: row.bookmarked_at,
                pinned_at: row.pinned_at,
                archived_at: row.archived_at,
                last_message_preview: row.last_message_preview,
            },
        )
        .collect::<Vec<_>>();

    Ok(ListConversationsResponseDto {
        total: conversations.len(),
        conversations,
    })
}
