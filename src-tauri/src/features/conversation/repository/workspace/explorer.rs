use super::*;

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct ConversationExplorerRow {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) model_name: String,
    pub(super) system_prompt: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) message_count: i64,
    pub(super) total_tokens: i64,
    pub(super) space_id: String,
    pub(super) is_saved: i64,
    pub(super) is_bookmarked: i64,
    pub(super) is_pinned: i64,
    pub(super) is_archived: i64,
    pub(super) saved_at: Option<String>,
    pub(super) bookmarked_at: Option<String>,
    pub(super) pinned_at: Option<String>,
    pub(super) archived_at: Option<String>,
    pub(super) last_message_preview: Option<String>,
    pub(super) forked_from_conversation_id: Option<String>,
    pub(super) forked_from_message_id: Option<String>,
}
pub(super) fn build_fts_query(raw: &str) -> Option<String> {
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

impl ConversationRepository {
    pub async fn list_conversations_explorer(
        &self,
        query: ListConversationsExplorerQueryDto,
    ) -> Result<ListConversationsResponseDto, AppError> {
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
            c.forked_from_conversation_id,
            c.forked_from_message_id,
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
            ensure_standard_space(&self.pool, space_id).await?;
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
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to list explorer conversations: {}", e))
            })?;

        let conversations = rows
            .into_iter()
            .map(|row| crate::features::conversation::dto::ConversationDto {
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
                compaction: None,
                forked_from_conversation_id: row.forked_from_conversation_id,
                forked_from_message_id: row.forked_from_message_id,
            })
            .collect::<Vec<_>>();

        Ok(ListConversationsResponseDto {
            total: conversations.len(),
            conversations,
        })
    }
}
