use super::*;

type PassageRow = (
    String,
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<i64>,
    bool,
);

/// One neighbouring chunk of a section: id, file name, file path, content, chunk
/// index, actual section, page number, and whether OCR read it.
type SectionNeighbourRow = (
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<i64>,
    bool,
);

impl ConversationRepository {
    /// Read only metadata and one bounded opening per document; never load the
    /// entire corpus merely to decide where to search.
    pub async fn retrieval_catalog(
        &self,
        allowed_ids: &HashSet<String>,
    ) -> Result<Vec<lattice_core::application::contracts::search::CorpusDocument>, AppError> {
        if allowed_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT d.id, d.file_name, COALESCE((SELECT substr(c.content, 1, 600) FROM text_chunks c WHERE c.document_id = d.id ORDER BY c.chunk_index LIMIT 1), ''), d.source_context FROM documents d WHERE d.id IN (",
        );
        let mut ids: Vec<_> = allowed_ids.iter().collect();
        ids.sort();
        let mut separated = query.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        query.push(") ORDER BY d.file_name, d.id");
        let rows: Vec<(String, String, String, Option<String>)> =
            query.build_query_as().fetch_all(&self.pool).await?;
        let mut catalog = Vec::new();
        for (id, name, opening, source_context) in rows {
            let sections: Vec<String> = sqlx::query_scalar("SELECT section FROM text_chunks WHERE document_id = ? AND section IS NOT NULL GROUP BY section ORDER BY MIN(chunk_index) LIMIT 24")
                .bind(&id).fetch_all(&self.pool).await?;
            catalog.push(lattice_core::application::contracts::search::CorpusDocument {
                chapter_number:
                    lattice_core::application::contracts::search::CorpusDocument::opening_chapter_number(
                        &opening,
                    ),
                id,
                name,
                opening,
                source_context: source_context
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()?,
                sections,
            });
        }
        catalog.sort_by(|a, b| {
            let key = |d: &lattice_core::application::contracts::search::CorpusDocument| {
                (
                    d.source_context
                        .as_ref()
                        .map(|s| s.group.id.clone())
                        .unwrap_or_default(),
                    d.source_context
                        .as_ref()
                        .filter(|s| s.group.ordered)
                        .map(|s| s.position)
                        .unwrap_or(u32::MAX),
                    d.name.clone(),
                )
            };
            key(a).cmp(&key(b))
        });
        Ok(catalog)
    }

    /// The mean length, in characters, of the indexed passages of
    /// `allowed_ids`: what one retrieved passage costs a prompt, measured on
    /// this library rather than assumed. `None` when the scope has none.
    ///
    /// Read from the per-document totals the chunk triggers keep, so a turn
    /// costs one row per document in scope rather than a pass over every
    /// passage, and the scope is one bound value however many documents it
    /// holds.
    pub async fn retrieval_mean_chunk_chars(
        &self,
        allowed_ids: &HashSet<String>,
    ) -> Result<Option<usize>, AppError> {
        if allowed_ids.is_empty() {
            return Ok(None);
        }
        let scope = serde_json::to_string(allowed_ids)
            .map_err(|error| AppError::InternalError(format!("scope is not JSON: {error}")))?;
        let (chars, chunks): (Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT SUM(content_chars), SUM(chunks) FROM document_chunk_stats \
             WHERE document_id IN (SELECT value FROM json_each(?))",
        )
        .bind(scope)
        .fetch_one(&self.pool)
        .await?;
        let (Some(chars), Some(chunks)) = (chars, chunks) else {
            return Ok(None);
        };
        if chunks <= 0 || chars <= 0 {
            return Ok(None);
        }
        Ok(usize::try_from((chars + chunks - 1) / chunks).ok())
    }

    /// Ordered document openings for overview/learning requests, up to
    /// `chunks_per_document` passages of each, in reading order. Both selected
    /// IDs and the hard space scope are checked before any content is read.
    pub async fn retrieval_openings(
        &self,
        selected_ids: &[String],
        allowed_ids: &HashSet<String>,
        chunks_per_document: usize,
    ) -> Result<Vec<lattice_core::application::contracts::search::CorpusPassage>, AppError> {
        let mut passages = Vec::new();
        let mut seen = HashSet::new();
        for id in selected_ids {
            if !allowed_ids.contains(id) || !seen.insert(id) {
                continue;
            }
            let rows: Vec<PassageRow> = sqlx::query_as(
                "SELECT c.id, d.id, d.file_name, d.file_path, c.content, c.chunk_index, c.section, c.page_number, c.from_ocr FROM text_chunks c JOIN documents d ON d.id = c.document_id WHERE c.document_id = ? ORDER BY c.chunk_index LIMIT ?",
            ).bind(id).bind(i64::try_from(chunks_per_document).unwrap_or(i64::MAX)).fetch_all(&self.pool).await?;
            passages.extend(rows.into_iter().map(
                |(id, document_id, name, path, content, chunk_index, section, page, from_ocr)| {
                    lattice_core::application::contracts::search::CorpusPassage {
                        id,
                        document_id,
                        name,
                        path,
                        content,
                        chunk_index: chunk_index.max(0) as usize,
                        section,
                        page_number: page.and_then(|p| p.try_into().ok()),
                        from_ocr,
                    }
                },
            ));
        }
        Ok(passages)
    }

    pub async fn retrieval_section_passages(
        &self,
        identifiers: &[String],
        allowed: &HashSet<String>,
    ) -> Result<Vec<lattice_core::application::contracts::search::CorpusPassage>, AppError> {
        use lattice_core::domain::value_objects::section_identifier::SectionIdentifier;
        let mut seen_identifiers = HashSet::new();
        let identifiers: Vec<_> = identifiers
            .iter()
            .filter_map(|id| SectionIdentifier::parse(id))
            .filter(|id| seen_identifiers.insert(id.text))
            .take(4)
            .collect();
        if identifiers.is_empty() || allowed.is_empty() {
            return Ok(Vec::new());
        }
        // Reserve evidence for each requested identifier. One long section or
        // edition must not consume the entire budget of a comparison request.
        let per_identifier = 12 / identifiers.len();
        let mut passages = Vec::new();
        let mut seen = HashSet::new();
        for identifier in identifiers {
            let mut q = QueryBuilder::<Sqlite>::new("SELECT c.id, c.document_id, d.file_name, d.file_path, c.content, c.chunk_index, c.section, c.page_number, c.from_ocr FROM text_chunks c JOIN documents d ON d.id = c.document_id WHERE c.document_id IN (");
            let mut ids: Vec<_> = allowed.iter().collect();
            ids.sort();
            for (i, id) in ids.into_iter().enumerate() {
                if i > 0 {
                    q.push(",");
                }
                q.push_bind(id);
            }
            q.push(") AND (");
            for (i, prefix) in [
                "", "§", "§ ", "Section ", "SECTION ", "section ", "Chapter ", "CHAPTER ",
                "chapter ",
            ]
            .into_iter()
            .enumerate()
            {
                if i > 0 {
                    q.push(" OR ");
                }
                // Padding gives both ends of a breadcrumb an exact boundary,
                // including headings consisting of only an identifier. INSTR
                // preserves case in labels such as (a) versus (A).
                q.push("INSTR(' > ' || c.section || ' ', ")
                    .push_bind(format!(" > {prefix}{} ", identifier.text))
                    .push(") > 0 OR INSTR(' > ' || c.section || ' ', ")
                    .push_bind(format!(" > {prefix}{}. ", identifier.text))
                    .push(") > 0");
            }
            if identifier.text.contains('.') {
                q.push(" OR INSTR('\n' || c.content, ")
                    .push_bind(format!("\n{} ", identifier.text))
                    .push(") > 0 OR INSTR(' ' || c.content, ")
                    .push_bind(format!(" {} ", identifier.text))
                    .push(") > 0");
            }
            q.push(") ORDER BY ROW_NUMBER() OVER (PARTITION BY c.document_id ORDER BY c.chunk_index, c.id), c.document_id LIMIT ")
                .push_bind(per_identifier as i64);
            let rows: Vec<PassageRow> = q.build_query_as().fetch_all(&self.pool).await?;
            passages.extend(
                rows.into_iter()
                    .filter(|row| seen.insert(row.0.clone()))
                    .map(
                        |(id, document_id, name, path, content, index, section, page, from_ocr)| {
                            lattice_core::application::contracts::search::CorpusPassage {
                                id,
                                document_id,
                                name,
                                path,
                                content,
                                chunk_index: index.max(0) as usize,
                                section,
                                page_number: page.and_then(|p| p.try_into().ok()),
                                from_ocr,
                            }
                        },
                    ),
            );
        }
        Ok(passages)
    }

    /// Expand a hit to nearby evidence within the same detected section, bounded
    /// by both chunk count and the original document scope. Never joins books.
    pub async fn retrieval_neighbors(
        &self,
        chunk_id: &str,
        allowed: &HashSet<String>,
    ) -> Result<Vec<lattice_core::application::contracts::search::CorpusPassage>, AppError> {
        let anchor: Option<(String, i64, Option<String>)> = sqlx::query_as(
            "SELECT document_id, chunk_index, section FROM text_chunks WHERE id = ?",
        )
        .bind(chunk_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some((doc, index, Some(section))) = anchor else {
            return Ok(Vec::new());
        };
        if !allowed.contains(&doc) {
            return Ok(Vec::new());
        }
        let rows: Vec<SectionNeighbourRow> = sqlx::query_as("SELECT c.id, d.file_name, d.file_path, c.content, c.chunk_index, c.section, c.page_number, c.from_ocr FROM text_chunks c JOIN documents d ON d.id = c.document_id WHERE c.document_id = ? AND (c.section = ? OR INSTR(c.section, ? || ' > ') = 1) AND c.chunk_index BETWEEN ? AND ? ORDER BY c.chunk_index LIMIT 3")
            .bind(&doc).bind(&section).bind(&section).bind(index.saturating_sub(1)).bind(index+1).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, name, path, content, index, actual_section, page, from_ocr)| {
                    lattice_core::application::contracts::search::CorpusPassage {
                        id,
                        document_id: doc.clone(),
                        name,
                        path,
                        content,
                        chunk_index: index.max(0) as usize,
                        section: actual_section,
                        page_number: page.and_then(|p| p.try_into().ok()),
                        from_ocr,
                    }
                },
            )
            .collect())
    }

    /// Resolve one consistent search scope. Every space, General included,
    /// searches only the documents assigned to it: a document filed into a named
    /// space must never surface in a chat belonging to another one. General also
    /// picks up documents that were never filed anywhere, so an unassigned import
    /// stays findable instead of silently disappearing from every search.
    ///
    /// General used to mean "the whole vault", which let a document the user had
    /// deliberately filed into one space leak into unrelated chats. This is the
    /// single chokepoint every retrieval path derives its allow-list from, so the
    /// rule is enforced here rather than trusted to each caller.
    pub async fn retrieval_document_scope(
        &self,
        conversation_id: &str,
    ) -> Result<Option<(String, HashSet<String>)>, AppError> {
        let space_id: Option<String> =
            sqlx::query_scalar("SELECT space_id FROM conversations WHERE id = ?")
                .bind(conversation_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some(space_id) = space_id.filter(|id| !id.trim().is_empty()) else {
            return Ok(None);
        };
        let mut ids = self.space_document_scope(&space_id).await?;
        // Files attached to *this* chat. They are in no space — that is what
        // keeps them out of the library and out of everyone else's searches —
        // so the space query above cannot return them, and without this the
        // conversation that owns an attachment could not retrieve from it on
        // any turn after the one that carried it in.
        let attachments = sqlx::query_scalar::<_, String>(
            "SELECT d.id FROM documents d
             WHERE d.owner_conversation_id = ?
               AND EXISTS (SELECT 1 FROM text_chunks c WHERE c.document_id = d.id)",
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;
        ids.extend(attachments);
        Ok(Some((space_id, ids)))
    }

    /// The same rule, asked by space instead of by conversation.
    ///
    /// A file attached to a chat belongs to no space and is excluded here, in
    /// the one statement every space-scoped path reads. `retrieval_document_scope`
    /// adds back the asking conversation's own attachments.
    ///
    /// The chat starters need a space's documents before any conversation is
    /// involved — they used to read the whole vault, so a Movies chat opened
    /// with questions about a patent library filed somewhere else. Rather than
    /// let that caller write its own membership SQL and drift, the rule stays
    /// in this one statement and `retrieval_document_scope` reads it too.
    pub async fn space_document_scope(&self, space_id: &str) -> Result<HashSet<String>, AppError> {
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT d.id FROM documents d
             WHERE EXISTS (SELECT 1 FROM text_chunks c WHERE c.document_id = d.id)
               AND d.owner_conversation_id IS NULL
               AND (
                 EXISTS (
                   SELECT 1 FROM document_space_memberships m
                   WHERE m.document_id = d.id AND m.space_id = ?
                 )
                 OR (? = 'space_general' AND NOT EXISTS (
                   SELECT 1 FROM document_space_memberships m
                   WHERE m.document_id = d.id
                 ))
               )",
        )
        .bind(space_id)
        .bind(space_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids.into_iter().collect())
    }

    /// The documents a chat may name, newest first.
    ///
    /// Derived from the chat's retrieval scope and then narrowed — never
    /// widened — by a case-insensitive substring match on the file name. The
    /// composer's `@` picker is the one place the user is told "these are the
    /// documents this chat can use", so it has to be exactly the set retrieval
    /// would search. A blank or unknown space means General, matching the
    /// conversation default.
    ///
    /// With a `conversation_id` the answer comes from
    /// [`Self::retrieval_document_scope`], so a chat can name the files
    /// attached to it as well as the ones filed in its space. Without one it
    /// falls back to the space alone, and attachments — which belong to no
    /// space — are not on offer.
    pub async fn space_documents(
        &self,
        space_id: Option<&str>,
        conversation_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SpaceDocumentDto>, AppError> {
        /// Ids bound per statement. SQLite's oldest variable ceiling is 999 and
        /// a General space in a real vault is far larger than that.
        const ID_BIND_BATCH: usize = 900;
        const MAX_RESULTS: usize = 50;

        let space_id = space_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or(DEFAULT_SPACE_ID);
        let limit = limit.clamp(1, MAX_RESULTS);
        let conversation_id = conversation_id.map(str::trim).filter(|id| !id.is_empty());
        let allowed = match conversation_id {
            Some(conversation) => match self.retrieval_document_scope(conversation).await? {
                Some((_, ids)) => ids,
                // A conversation the caller named but that no longer exists:
                // answer for the space rather than for nothing.
                None => self.space_document_scope(space_id).await?,
            },
            None => self.space_document_scope(space_id).await?,
        };
        if allowed.is_empty() {
            return Ok(Vec::new());
        }

        let needle = query.trim().to_lowercase();
        let mut ids: Vec<_> = allowed.into_iter().collect();
        ids.sort();

        let mut rows: Vec<(String, String, Option<String>, Option<String>)> = Vec::new();
        for batch in ids.chunks(ID_BIND_BATCH) {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "SELECT d.id, d.file_name, d.category, d.modified_at FROM documents d WHERE d.id IN (",
            );
            let mut separated = qb.separated(", ");
            for id in batch {
                separated.push_bind(id);
            }
            qb.push(")");
            if !needle.is_empty() {
                qb.push(" AND INSTR(LOWER(d.file_name), ")
                    .push_bind(needle.clone())
                    .push(") > 0");
            }
            // Newest first, then by name so an unset or shared timestamp still
            // gives a stable order rather than whatever SQLite happens to emit.
            qb.push(" ORDER BY d.modified_at DESC, d.file_name ASC LIMIT ")
                .push_bind(limit as i64);
            rows.extend(qb.build_query_as().fetch_all(&self.pool).await?);
        }

        // Each batch was ordered and capped on its own, so the batches are
        // merged and capped again here.
        rows.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.1.cmp(&b.1)));
        rows.truncate(limit);
        Ok(rows
            .into_iter()
            .map(
                |(document_id, file_name, category, modified_at)| SpaceDocumentDto {
                    document_id,
                    file_name,
                    category,
                    modified_at,
                },
            )
            .collect())
    }
}

impl ConversationRepository {
    /// Hydrate provenance for lexical/vector hits as well as ordered reads:
    /// section, page, and whether OCR read the text off a scan.
    pub async fn retrieval_locations(
        &self,
        chunk_ids: &[String],
    ) -> Result<std::collections::HashMap<String, (Option<String>, Option<u32>, bool)>, AppError>
    {
        let mut locations = std::collections::HashMap::new();
        for batch in chunk_ids.chunks(100) {
            if batch.is_empty() {
                continue;
            }
            let mut query = QueryBuilder::<Sqlite>::new(
                "SELECT id, section, page_number, from_ocr FROM text_chunks WHERE id IN (",
            );
            let mut separated = query.separated(",");
            for id in batch {
                separated.push_bind(id);
            }
            query.push(")");
            let rows: Vec<(String, Option<String>, Option<i64>, bool)> =
                query.build_query_as().fetch_all(&self.pool).await?;
            for (id, section, page, from_ocr) in rows {
                locations.insert(
                    id,
                    (section, page.and_then(|v| v.try_into().ok()), from_ocr),
                );
            }
        }
        Ok(locations)
    }
}

#[cfg(test)]
mod tests;
