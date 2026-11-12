use super::*;

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct ConversationListRow {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) model_choice: Option<String>,
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
    pub(super) explorer_root: Option<String>,
    pub(super) program_id: Option<String>,
    pub(super) lesson_id: Option<String>,
}

impl ConversationListRow {
    pub(super) fn into_dto(self) -> Result<ConversationDto, AppError> {
        Ok(ConversationDto {
            model_choice: lattice_core::domain::conversation::ChatModelChoice::from_column(
                &self.id,
                self.model_choice.as_deref(),
            )?,
            id: self.id,
            title: self.title,
            system_prompt: self.system_prompt,
            created_at: self.created_at,
            updated_at: self.updated_at,
            message_count: self.message_count,
            total_tokens: self.total_tokens,
            space_id: Some(self.space_id),
            is_saved: Some(self.is_saved != 0),
            is_bookmarked: Some(self.is_bookmarked != 0),
            is_pinned: Some(self.is_pinned != 0),
            is_archived: Some(self.is_archived != 0),
            saved_at: self.saved_at,
            bookmarked_at: self.bookmarked_at,
            pinned_at: self.pinned_at,
            archived_at: self.archived_at,
            last_message_preview: self.last_message_preview,
            compaction: None,
            forked_from_conversation_id: self.forked_from_conversation_id,
            forked_from_message_id: self.forked_from_message_id,
            explorer_root: self.explorer_root,
            program_id: self.program_id,
            lesson_id: self.lesson_id,
            tangent_parent_id: None,
        })
    }
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

/// `rank` rather than `bm25()`: an auxiliary function cannot run inside `MIN`.
fn push_conversation_matches(qb: &mut QueryBuilder<'_, Sqlite>, fts: &str) {
    qb.push(
        " FROM conversation_search_fts fts
        WHERE fts.conversation_id = c.id
          AND fts.content MATCH ",
    )
    .push_bind(fts.to_string());
}

/// Which conversations to list, shared by the explorer and the journal listing.
#[derive(Debug, Default)]
pub(super) struct ConversationListing<'a> {
    pub(super) journal_space_id: Option<&'a str>,
    pub(super) space_id: Option<&'a str>,
    pub(super) program_id: Option<&'a str>,
    pub(super) saved_only: bool,
    pub(super) bookmarked_only: bool,
    pub(super) pinned_only: bool,
    pub(super) has_message_bookmarks: bool,
    pub(super) include_archived: bool,
    pub(super) search: Option<&'a str>,
    pub(super) page: Page,
}

impl<'a> ConversationListing<'a> {
    pub(super) fn query(&self) -> QueryBuilder<'a, Sqlite> {
        let fts_query = self.search.and_then(build_fts_query);

        let mut qb = QueryBuilder::<Sqlite>::new(
            r#"
        SELECT
            c.id,
            c.title,
            c.model_choice,
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
            c.explorer_root,
            c.program_id,
            c.lesson_id,
            (
                SELECT m.content
                FROM conversation_messages m
                WHERE m.conversation_id = c.id
                ORDER BY m.created_at DESC
                LIMIT 1
            ) AS last_message_preview
        FROM conversations c
        WHERE c.tangent_parent_id IS NULL
        "#,
        );

        if let Some(journal_space_id) = self.journal_space_id {
            qb.push(
                " AND EXISTS (
                SELECT 1
                FROM journal_conversation_entries jce
                WHERE jce.journal_space_id = ",
            )
            .push_bind(journal_space_id)
            .push(" AND jce.conversation_id = c.id)");
        }

        if let Some(space_id) = self.space_id {
            qb.push(" AND c.space_id = ").push_bind(space_id);
        }

        if let Some(program_id) = self.program_id {
            qb.push(" AND c.program_id = ").push_bind(program_id);
        }

        if self.saved_only {
            qb.push(" AND c.is_saved = 1");
        }

        if self.bookmarked_only {
            qb.push(" AND c.is_bookmarked = 1");
        }

        if self.pinned_only {
            qb.push(" AND c.is_pinned = 1");
        }

        if self.has_message_bookmarks {
            qb.push(
                " AND EXISTS (SELECT 1 FROM conversation_message_bookmarks b WHERE b.conversation_id = c.id)",
            );
        }

        if !self.include_archived {
            qb.push(" AND c.is_archived = 0");
        }

        if let Some(fts) = &fts_query {
            qb.push(" AND EXISTS (SELECT 1");
            push_conversation_matches(&mut qb, fts);
            qb.push(") ORDER BY c.is_pinned DESC, (SELECT MIN(fts.rank)");
            push_conversation_matches(&mut qb, fts);
            qb.push("), c.updated_at DESC");
        } else {
            qb.push(" ORDER BY c.is_pinned DESC, c.updated_at DESC");
        }

        qb.push(" LIMIT ")
            .push_bind(self.page.sql_limit())
            .push(" OFFSET ")
            .push_bind(self.page.offset());
        qb
    }
}

pub(super) fn listing_response(
    rows: Vec<ConversationListRow>,
) -> Result<ListConversationsResponseDto, AppError> {
    let conversations = rows
        .into_iter()
        .map(ConversationListRow::into_dto)
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(ListConversationsResponseDto { conversations })
}
