use super::*;

impl ConversationRepository {
    pub async fn list_conversations_explorer(
        &self,
        query: ListConversationsExplorerQueryDto,
    ) -> Result<ListConversationsResponseDto, AppError> {
        let page = Page::new(query.limit, query.offset)?;

        let mut read = self.db.begin_read().await?;
        if let Some(space_id) = &query.space_id {
            ensure_standard_space(&mut read, space_id).await?;
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
            page,
            ..Default::default()
        }
        .query();

        let rows = qb
            .build_query_as::<ConversationListRow>()
            .fetch_all(&mut *read)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to list explorer conversations: {}", e))
            })?;
        read.commit().await?;

        listing_response(rows)
    }
}
