//! Conversation Space Repository
//!
//! All SQL for conversation_spaces, conversation_space_members, and the
//! local-owner collaborator_profile row lives here. Tauri commands and
//! services should call this repository instead of inlining sqlx queries.

use chrono::Utc;
use sqlx::SqlitePool;

use crate::features::conversation::space_dto::{
    ConversationSpaceDto, CreateConversationSpaceRequestDto,
};
use crate::shared::error::{AppError, Result};

/// Synthetic single-user owner. Spaces always have an "owner" membership
/// pointing at this profile so the per-space membership table has a
/// non-empty row even before any real collaborators are added.
pub const LOCAL_OWNER_MEMBER_ID: &str = "member_local_owner";

#[derive(Debug, Clone)]
pub struct ConversationSpaceRepository {
    pool: SqlitePool,
}

impl ConversationSpaceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new conversation space with the local owner as a member.
    pub async fn create(
        &self,
        request: CreateConversationSpaceRequestDto,
    ) -> Result<ConversationSpaceDto> {
        let CreateConversationSpaceRequestDto {
            name,
            description,
            icon,
            accent_color,
            space_prompt,
            default_model_name,
            tool_preferences_json,
        } = request;

        let id = format!("space_{}", uuid::Uuid::new_v4().simple());
        let now = Utc::now().to_rfc3339();

        let mut tx = self.pool.begin().await.map_err(|e| {
            AppError::Database(format!("Failed to open tx for space create: {}", e))
        })?;

        sqlx::query(
            r#"
            INSERT INTO conversation_spaces (
                id, name, description, icon, accent_color, space_prompt,
                default_model_name, tool_preferences_json, is_archived, sort_order, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, 0, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&name)
        .bind(&description)
        .bind(&icon)
        .bind(&accent_color)
        .bind(&space_prompt)
        .bind(&default_model_name)
        .bind(&tool_preferences_json)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to create space: {}", e)))?;

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
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to ensure local owner profile while creating space: {}",
                e
            ))
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
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to create default owner membership for new space: {}",
                e
            ))
        })?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit space create: {}", e)))?;

        sqlx::query_as::<_, ConversationSpaceDto>(
            r#"
            SELECT
                id, name, description, icon, accent_color, space_prompt, default_model_name,
                tool_preferences_json, is_archived, sort_order, created_at, updated_at
            FROM conversation_spaces
            WHERE id = ?
            "#,
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch created space: {}", e)))
    }
}
