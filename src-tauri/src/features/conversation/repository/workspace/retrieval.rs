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
);

impl ConversationRepository {
    /// Read only metadata and one bounded opening per document; never load the
    /// entire corpus merely to decide where to search.
    pub async fn retrieval_catalog(
        &self,
        allowed_ids: &HashSet<String>,
    ) -> Result<Vec<crate::application::contracts::search::CorpusDocument>, AppError> {
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
            catalog.push(crate::application::contracts::search::CorpusDocument {
                chapter_number:
                    crate::application::contracts::search::CorpusDocument::opening_chapter_number(
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
            let key = |d: &crate::application::contracts::search::CorpusDocument| {
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

    /// Ordered document openings for overview/learning requests. Both selected
    /// IDs and the hard space scope are checked before any content is read.
    pub async fn retrieval_openings(
        &self,
        selected_ids: &[String],
        allowed_ids: &HashSet<String>,
        chunks_per_document: usize,
    ) -> Result<Vec<crate::application::contracts::search::CorpusPassage>, AppError> {
        let mut passages = Vec::new();
        let mut seen = HashSet::new();
        for id in selected_ids.iter().take(4) {
            if !allowed_ids.contains(id) || !seen.insert(id) {
                continue;
            }
            let rows: Vec<PassageRow> = sqlx::query_as(
                "SELECT c.id, d.id, d.file_name, d.file_path, c.content, c.chunk_index, c.section, c.page_number FROM text_chunks c JOIN documents d ON d.id = c.document_id WHERE c.document_id = ? ORDER BY c.chunk_index LIMIT ?",
            ).bind(id).bind(chunks_per_document.min(8) as i64).fetch_all(&self.pool).await?;
            passages.extend(rows.into_iter().map(
                |(id, document_id, name, path, content, chunk_index, section, page)| {
                    crate::application::contracts::search::CorpusPassage {
                        id,
                        document_id,
                        name,
                        path,
                        content,
                        chunk_index: chunk_index.max(0) as usize,
                        section,
                        page_number: page.and_then(|p| p.try_into().ok()),
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
    ) -> Result<Vec<crate::application::contracts::search::CorpusPassage>, AppError> {
        use crate::domain::value_objects::section_identifier::SectionIdentifier;
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
            let mut q = QueryBuilder::<Sqlite>::new("SELECT c.id, c.document_id, d.file_name, d.file_path, c.content, c.chunk_index, c.section, c.page_number FROM text_chunks c JOIN documents d ON d.id = c.document_id WHERE c.document_id IN (");
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
                    .push(") > 0");
            }
            q.push(") ORDER BY ROW_NUMBER() OVER (PARTITION BY c.document_id ORDER BY c.chunk_index, c.id), c.document_id LIMIT ")
                .push_bind(per_identifier as i64);
            let rows: Vec<PassageRow> = q.build_query_as().fetch_all(&self.pool).await?;
            passages.extend(
                rows.into_iter()
                    .filter(|row| seen.insert(row.0.clone()))
                    .map(
                        |(id, document_id, name, path, content, index, section, page)| {
                            crate::application::contracts::search::CorpusPassage {
                                id,
                                document_id,
                                name,
                                path,
                                content,
                                chunk_index: index.max(0) as usize,
                                section,
                                page_number: page.and_then(|p| p.try_into().ok()),
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
    ) -> Result<Vec<crate::application::contracts::search::CorpusPassage>, AppError> {
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
        let rows: Vec<(String,String,String,String,i64,Option<i64>)> = sqlx::query_as("SELECT c.id, d.file_name, d.file_path, c.content, c.chunk_index, c.page_number FROM text_chunks c JOIN documents d ON d.id = c.document_id WHERE c.document_id = ? AND c.section = ? AND c.chunk_index BETWEEN ? AND ? ORDER BY c.chunk_index LIMIT 3")
            .bind(&doc).bind(&section).bind(index.saturating_sub(1)).bind(index+1).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, path, content, index, page)| {
                crate::application::contracts::search::CorpusPassage {
                    id,
                    document_id: doc.clone(),
                    name,
                    path,
                    content,
                    chunk_index: index.max(0) as usize,
                    section: Some(section.clone()),
                    page_number: page.and_then(|p| p.try_into().ok()),
                }
            })
            .collect())
    }

    /// Resolve one consistent search scope. General searches the vault; named
    /// spaces search only their assigned documents. A stored document must have
    /// searchable text before it contributes to the scope count.
    pub async fn retrieval_document_scope(
        &self,
        conversation_id: &str,
    ) -> Result<Option<(String, HashSet<String>)>, AppError> {
        let mut tx = self.pool.begin().await?;
        let space_id: Option<String> =
            sqlx::query_scalar("SELECT space_id FROM conversations WHERE id = ?")
                .bind(conversation_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(space_id) = space_id.filter(|id| !id.trim().is_empty()) else {
            return Ok(None);
        };
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT d.id FROM documents d
             WHERE EXISTS (SELECT 1 FROM text_chunks c WHERE c.document_id = d.id)
               AND (? = 'space_general' OR EXISTS (
                 SELECT 1 FROM document_space_memberships m
                 WHERE m.document_id = d.id AND m.space_id = ?
               ))",
        )
        .bind(&space_id)
        .bind(&space_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some((space_id, ids.into_iter().collect())))
    }
}

impl ConversationRepository {
    /// Hydrate provenance for lexical/vector hits as well as ordered reads.
    pub async fn retrieval_locations(
        &self,
        chunk_ids: &[String],
    ) -> Result<std::collections::HashMap<String, (Option<String>, Option<u32>)>, AppError> {
        let mut locations = std::collections::HashMap::new();
        for batch in chunk_ids.chunks(100) {
            if batch.is_empty() {
                continue;
            }
            let mut query = QueryBuilder::<Sqlite>::new(
                "SELECT id, section, page_number FROM text_chunks WHERE id IN (",
            );
            let mut separated = query.separated(",");
            for id in batch {
                separated.push_bind(id);
            }
            query.push(")");
            let rows: Vec<(String, Option<String>, Option<i64>)> =
                query.build_query_as().fetch_all(&self.pool).await?;
            for (id, section, page) in rows {
                locations.insert(id, (section, page.and_then(|v| v.try_into().ok())));
            }
        }
        Ok(locations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn retrieval_scope_general_searches_vault_and_named_spaces_stay_scoped() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE conversations (id TEXT, space_id TEXT);
            CREATE TABLE documents (id TEXT);
            CREATE TABLE text_chunks (document_id TEXT);
            CREATE TABLE document_space_memberships (document_id TEXT, space_id TEXT);
            INSERT INTO conversations VALUES ('general', 'space_general'), ('patent', 'patent'), ('empty', 'empty');
            INSERT INTO documents VALUES ('a'), ('b'), ('unfinished');
            INSERT INTO text_chunks VALUES ('a'), ('a'), ('b');
            INSERT INTO document_space_memberships VALUES ('a', 'patent'), ('unfinished', 'patent');")
            .execute(&pool).await.unwrap();
        let repo = ConversationRepository::new(pool);
        let (_, general) = repo
            .retrieval_document_scope("general")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(general, ["a".to_string(), "b".to_string()].into());
        let (_, patent) = repo
            .retrieval_document_scope("patent")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(patent, ["a".to_string()].into());
        let (_, empty) = repo
            .retrieval_document_scope("empty")
            .await
            .unwrap()
            .unwrap();
        assert!(empty.is_empty());
        assert!(repo
            .retrieval_document_scope("missing")
            .await
            .unwrap()
            .is_none());
    }
}
