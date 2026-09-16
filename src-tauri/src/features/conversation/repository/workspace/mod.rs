//! Conversation workspace persistence. All SQL and transaction ownership stays here.
use super::ConversationRepository;
use crate::features::conversation::dto::{
    ConversationDto, ListConversationsResponseDto, RenameConversationResponseDto,
};
use crate::features::conversation::message_bookmark_dto::*;
use crate::features::conversation::space_dto::*;
use crate::features::conversation::workspace_dto::*;
use crate::shared::error::AppError;
use chrono::Utc;
use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use std::collections::HashSet;
const DEFAULT_SPACE_ID: &str = "space_general";

mod bookmarks;
mod explorer;
mod journals;
mod memberships;
mod retrieval;
mod sources;
mod spaces;
mod state;

use explorer::{build_fts_query, ConversationExplorerRow};
use spaces::ensure_standard_space;

#[cfg(test)]
mod tests;
