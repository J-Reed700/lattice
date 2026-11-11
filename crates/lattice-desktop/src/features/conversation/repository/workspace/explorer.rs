use super::*;

impl ConversationRepository {
    pub async fn list_conversations_explorer(
        &self,
        query: ListConversationsExplorerQueryDto,
    ) -> Result<ListConversationsResponseDto, AppError> {
        let (limit, offset) = ConversationListing::page(query.limit, query.offset, 100);

        if let Some(space_id) = &query.space_id {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|e| AppError::Database(format!("Failed to verify space: {}", e)))?;
            ensure_standard_space(&mut connection, space_id).await?;
        }

        let mut qb = ConversationListing {
            space_id: query.space_id.as_deref(),
            program_id: query.program_id.as_deref(),
            saved_only: query.saved_only.unwrap_or(false),
            bookmarked_only: query.bookmarked_only.unwrap_or(false),
            pinned_only: query.pinned_only.unwrap_or(false),
            has_message_bookmarks: query.has_message_bookmarks.unwrap_or(false),
            include_archived: query.include_archived.unwrap_or(false),
            search: query.query.as_deref(),
            limit,
            offset,
            ..Default::default()
        }
        .query();

        let rows = qb
            .build_query_as::<ConversationExplorerRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to list explorer conversations: {}", e))
            })?;

        listing_response(rows)
    }
}
