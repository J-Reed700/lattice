//! Rows of the space, journal and member queries, apart from their IPC DTOs.

use crate::features::conversation::space_dto::{
    ConversationJournalDto, ConversationSpaceDto, ConversationSpaceMemberDto,
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ConversationSpaceRow {
    id: String,
    name: String,
    description: Option<String>,
    icon: Option<String>,
    accent_color: Option<String>,
    space_prompt: Option<String>,
    tool_preferences_json: Option<String>,
    is_archived: bool,
    sort_order: i64,
    created_at: String,
    updated_at: String,
}

impl From<ConversationSpaceRow> for ConversationSpaceDto {
    fn from(row: ConversationSpaceRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            icon: row.icon,
            accent_color: row.accent_color,
            space_prompt: row.space_prompt,
            tool_preferences_json: row.tool_preferences_json,
            is_archived: row.is_archived,
            sort_order: row.sort_order,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ConversationJournalRow {
    id: String,
    name: String,
    description: Option<String>,
    icon: Option<String>,
    accent_color: Option<String>,
    space_prompt: Option<String>,
    tool_preferences_json: Option<String>,
    is_archived: bool,
    sort_order: i64,
    created_at: String,
    updated_at: String,
}

impl From<ConversationJournalRow> for ConversationJournalDto {
    fn from(row: ConversationJournalRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            icon: row.icon,
            accent_color: row.accent_color,
            space_prompt: row.space_prompt,
            tool_preferences_json: row.tool_preferences_json,
            is_archived: row.is_archived,
            sort_order: row.sort_order,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ConversationSpaceMemberRow {
    space_id: String,
    member_id: String,
    display_name: String,
    email: Option<String>,
    avatar_url: Option<String>,
    role: String,
    created_at: String,
    updated_at: String,
}

impl From<ConversationSpaceMemberRow> for ConversationSpaceMemberDto {
    fn from(row: ConversationSpaceMemberRow) -> Self {
        Self {
            space_id: row.space_id,
            member_id: row.member_id,
            display_name: row.display_name,
            email: row.email,
            avatar_url: row.avatar_url,
            role: row.role,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
