//! Reading source workspaces, versions, search hits and prepared sources.
use super::*;

impl LearningSourceLibraryRepository {
    pub async fn workspace(&self, program_id: &str) -> Result<LearningSourceWorkspaceDto> {
        self.workspace_with_deleted(program_id, false).await
    }

    /// Build a source snapshot for portable exports, including tombstones whose
    /// immutable versions and refresh history must remain recoverable.
    #[cfg(test)]
    pub(crate) async fn workspace_including_deleted(
        &self,
        program_id: &str,
    ) -> Result<LearningSourceWorkspaceDto> {
        self.workspace_with_deleted(program_id, true).await
    }

    async fn workspace_with_deleted(
        &self,
        program_id: &str,
        include_deleted: bool,
    ) -> Result<LearningSourceWorkspaceDto> {
        Self::validate_program_id(program_id)?;
        self.backfill_legacy(program_id).await?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let workspace = Self::workspace_on(&mut tx, program_id, include_deleted).await?;
        tx.commit().await.map_err(db)?;
        Ok(workspace)
    }

    /// Read source metadata and history through an existing database snapshot.
    pub(in crate::features::learning) async fn workspace_on(
        connection: &mut sqlx::SqliteConnection,
        program_id: &str,
        include_deleted: bool,
    ) -> Result<LearningSourceWorkspaceDto> {
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Learning program not found".into()));
        }
        let query = if include_deleted {
            "SELECT * FROM learning_source_library WHERE program_id=? ORDER BY created_at,id"
        } else {
            "SELECT * FROM learning_source_library WHERE program_id=? AND deleted_at IS NULL ORDER BY created_at,id"
        };
        let rows = sqlx::query(query)
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
        let mut sources = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.get("id");
            let versions = Self::version_summaries(connection, program_id, &id).await?;
            let active_id: Option<String> = row.try_get("active_version_id").map_err(db)?;
            let pending_id: Option<String> = row.try_get("pending_version_id").map_err(db)?;
            let checks = Self::checks(connection, program_id, &id).await?;
            sources.push(LearningSourceLibraryItemDto {
                id: id.clone(),
                kind: parse_kind(row.get("kind"))?,
                added_by: parse_added_by(row.get("added_by"))?.into(),
                origin: row.get("origin"),
                requested_url: row.get("requested_url"),
                freshness_policy: parse_policy(row.get("freshness_policy"))?,
                active_version_id: active_id.clone(),
                pending_version_id: pending_id.clone(),
                revision: row.get("revision"),
                deleted_at: row.get("deleted_at"),
                deletion_reason: row.get("deletion_reason"),
                active_version: active_id
                    .as_deref()
                    .and_then(|vid| versions.iter().find(|v| v.id == vid).cloned()),
                pending_version: pending_id
                    .as_deref()
                    .and_then(|vid| versions.iter().find(|v| v.id == vid).cloned()),
                versions,
                latest_check: checks.first().cloned(),
                checks,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }
        Ok(LearningSourceWorkspaceDto {
            program_id: program_id.into(),
            sources,
        })
    }

    async fn version_summaries(
        connection: &mut sqlx::SqliteConnection,
        program_id: &str,
        source_id: &str,
    ) -> Result<Vec<LearningSourceVersionSummaryDto>> {
        let rows = sqlx::query("SELECT * FROM learning_source_versions WHERE program_id=? AND source_id=? ORDER BY version_number DESC")
            .bind(program_id).bind(source_id).fetch_all(&mut *connection).await.map_err(db)?;
        rows.into_iter().map(|r| Ok(summary(&r))).collect()
    }
    async fn checks(
        connection: &mut sqlx::SqliteConnection,
        program_id: &str,
        source_id: &str,
    ) -> Result<Vec<LearningSourceCheckDto>> {
        let rows = sqlx::query("SELECT * FROM learning_source_refresh_checks WHERE program_id=? AND source_id=? ORDER BY checked_at DESC,rowid DESC LIMIT ?")
            .bind(program_id).bind(source_id).bind(MAX_CHECKS_IN_WORKSPACE).fetch_all(&mut *connection).await.map_err(db)?;
        rows.into_iter()
            .map(|r| {
                Ok(LearningSourceCheckDto {
                    operation_id: r.get("operation_id"),
                    status: parse_check(r.get("status"))?,
                    checked_at: r.get("checked_at"),
                    active_digest: r.get("active_digest"),
                    pending_version_id: r.get("pending_version_id"),
                    message: r.get("message"),
                })
            })
            .collect()
    }

    pub async fn get_version(
        &self,
        req: &GetLearningSourceVersionRequestDto,
    ) -> Result<LearningSourceVersionDto> {
        Self::validate_program_id(&req.program_id)?;
        for (v, label) in [
            (&req.source_id, "source"),
            (&req.version_id, "source version"),
        ] {
            if v.trim().is_empty() || v.len() > 256 {
                return Err(invalid(format!("Invalid {label} ID")));
            }
        }
        self.backfill_legacy(&req.program_id).await?;
        let row = sqlx::query(
            "SELECT * FROM learning_source_versions WHERE program_id=? AND source_id=? AND id=?",
        )
        .bind(&req.program_id)
        .bind(&req.source_id)
        .bind(&req.version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Source version not found in this program".into()))?;
        let usage = self.version_usage(&req.program_id, &req.version_id).await?;
        Ok(LearningSourceVersionDto {
            source_id: req.source_id.clone(),
            version: summary(&row),
            full_text: row.get("full_text"),
            usage,
        })
    }

    async fn version_usage(
        &self,
        program_id: &str,
        version_id: &str,
    ) -> Result<Vec<LearningSourceUsageDto>> {
        let mut out = Vec::new();
        let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,b.title ref_title FROM learning_blocks b JOIN learning_lessons l ON l.id=b.lesson_id WHERE l.program_id=? AND EXISTS(SELECT 1 FROM json_each(b.source_ids_json) WHERE value=?) ORDER BY l.ordinal,b.ordinal")
            .bind(program_id).bind(version_id).fetch_all(&self.pool).await.map_err(db)?;
        for r in rows {
            out.push(LearningSourceUsageDto {
                lesson_id: r.get("lesson_id"),
                lesson_title: r.get("lesson_title"),
                reference_kind: "lesson_block".into(),
                reference_title: r.get("ref_title"),
            });
        }
        let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,q.prompt ref_title FROM learning_questions q JOIN learning_lessons l ON l.id=q.lesson_id WHERE l.program_id=? AND EXISTS(SELECT 1 FROM json_each(q.source_ids_json) WHERE value=?) ORDER BY l.ordinal,q.ordinal")
            .bind(program_id).bind(version_id).fetch_all(&self.pool).await.map_err(db)?;
        for r in rows {
            out.push(LearningSourceUsageDto {
                lesson_id: r.get("lesson_id"),
                lesson_title: r.get("lesson_title"),
                reference_kind: "assessment_question".into(),
                reference_title: r.get("ref_title"),
            });
        }
        let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,d.question ref_title FROM learning_card_drafts d JOIN learning_lessons l ON l.id=d.lesson_id WHERE d.program_id=? AND d.status='pending' AND EXISTS(SELECT 1 FROM json_each(d.source_ids_json) WHERE value=?) ORDER BY l.ordinal,d.created_at")
            .bind(program_id).bind(version_id).fetch_all(&self.pool).await.map_err(db)?;
        for r in rows {
            out.push(LearningSourceUsageDto {
                lesson_id: r.get("lesson_id"),
                lesson_title: r.get("lesson_title"),
                reference_kind: "recall_draft".into(),
                reference_title: r.get("ref_title"),
            });
        }
        let rows=sqlx::query("SELECT l.id lesson_id,l.title lesson_title,c.question ref_title FROM learning_card_origins o JOIN learning_lessons l ON l.id=o.lesson_id JOIN study_cards c ON c.id=o.card_id WHERE o.program_id=? AND EXISTS(SELECT 1 FROM json_each(o.source_ids_json) WHERE value=?) ORDER BY l.ordinal,o.accepted_at")
            .bind(program_id).bind(version_id).fetch_all(&self.pool).await.map_err(db)?;
        for r in rows {
            out.push(LearningSourceUsageDto {
                lesson_id: r.get("lesson_id"),
                lesson_title: r.get("lesson_title"),
                reference_kind: "accepted_recall_card".into(),
                reference_title: r.get("ref_title"),
            });
        }
        Ok(out)
    }

    pub async fn search(
        &self,
        req: &SearchLearningSourcesRequestDto,
    ) -> Result<Vec<LearningSourceSearchResultDto>> {
        Self::validate_program_id(&req.program_id)?;
        let query = req.query.trim();
        if query.is_empty() || query.chars().count() > 200 {
            return Err(invalid("Search query must contain 1–200 characters"));
        }
        let limit = req.limit.clamp(1, 50) as i64;
        let mut rows=sqlx::query("SELECT source_id,id,title,full_text FROM learning_source_versions WHERE program_id=? ORDER BY acquired_at DESC,id")
            .bind(&req.program_id).fetch(&self.pool);
        let mut results = Vec::new();
        while let Some(r) = rows.try_next().await.map_err(db)? {
            let text: String = r.get("full_text");
            if let Some(index) = find_case_insensitive(&text, query) {
                results.push(LearningSourceSearchResultDto {
                    source_id: r.get("source_id"),
                    version_id: r.get("id"),
                    title: r.get("title"),
                    excerpt: context_excerpt(&text, index),
                });
                if results.len() as i64 >= limit {
                    break;
                }
            }
        }
        Ok(results)
    }

    pub async fn prepare_sources(&self, program_id: &str) -> Result<Vec<LearningSourceDto>> {
        self.backfill_legacy(program_id).await?;
        let rows=sqlx::query("SELECT v.id,v.title,v.resolved_url,v.requested_url,v.excerpt,v.acquired_at FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL ORDER BY s.created_at,s.id")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| LearningSourceDto {
                id: r.get("id"),
                title: r.get("title"),
                url: r
                    .get::<Option<String>, _>("resolved_url")
                    .or_else(|| r.get("requested_url")),
                excerpt: r.get("excerpt"),
                acquired_at: r.get("acquired_at"),
            })
            .collect())
    }
    /// The library document each of the program's document snapshots was
    /// captured from, keyed by snapshot version ID.
    pub async fn library_documents(&self, program_id: &str) -> Result<HashMap<String, String>> {
        let rows = sqlx::query("SELECT v.id,l.origin FROM learning_source_versions v JOIN learning_source_library l ON l.program_id=v.program_id AND l.id=v.source_id WHERE v.program_id=? AND l.kind='document'")
            .bind(program_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("id"), row.get("origin")))
            .collect())
    }
    /// The whole captured text of one version of the program's sources,
    /// deleted ones included.
    pub async fn version_text(&self, program_id: &str, version_id: &str) -> Result<Option<String>> {
        sqlx::query_scalar(
            "SELECT full_text FROM learning_source_versions WHERE program_id=? AND id=?",
        )
        .bind(program_id)
        .bind(version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    /// The whole captured text of every version in the program, by version
    /// ID: what a model receives in place of a stored preview.
    pub async fn version_texts(&self, program_id: &str) -> Result<HashMap<String, String>> {
        let rows =
            sqlx::query("SELECT id,full_text FROM learning_source_versions WHERE program_id=?")
                .bind(program_id)
                .fetch_all(&self.pool)
                .await
                .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("id"), r.get("full_text")))
            .collect())
    }

    pub async fn verification_sources(&self, program_id: &str) -> Result<Vec<LearningSourceDto>> {
        self.backfill_legacy(program_id).await?;
        let rows=sqlx::query("SELECT v.id,v.title,v.resolved_url,v.requested_url,v.full_text AS excerpt,v.acquired_at FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL ORDER BY s.created_at,s.id")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| LearningSourceDto {
                id: r.get("id"),
                title: r.get("title"),
                url: r
                    .get::<Option<String>, _>("resolved_url")
                    .or_else(|| r.get("requested_url")),
                excerpt: r.get("excerpt"),
                acquired_at: r.get("acquired_at"),
            })
            .collect())
    }
}
