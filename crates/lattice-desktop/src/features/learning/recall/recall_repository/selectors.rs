//! Source selectors: creating, reading and re-matching them, and searching program sources.
use super::*;

#[derive(Debug, Clone)]
struct SourceSelectorRecord {
    id: String,
    source_id: String,
    source_version_id: String,
    selector: crate::features::learning::source_selector::LearningTextQuoteSelector,
    match_status: crate::features::learning::source_selector::LearningQuoteMatchStatus,
    start_byte: Option<usize>,
    end_byte: Option<usize>,
    candidate_count: usize,
    created_at: i64,
}
impl SourceSelectorRecord {
    fn dto(self) -> LearningSourceSelectorDto {
        LearningSourceSelectorDto {
            id: self.id,
            source_id: self.source_id,
            source_version_id: self.source_version_id,
            selector: self.selector,
            match_status: self.match_status,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
            candidate_count: self.candidate_count,
            created_at: self.created_at,
        }
    }
}

impl LearningRecallRepository {
    pub async fn create_selector(
        &self,
        req: &CreateLearningSourceSelectorRequestDto,
    ) -> Result<LearningSourceSelectorDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.selector_id, "selector")?;
        validate_uuid(&req.program_id, "program")?;
        validate_owned_id(&req.source_id, "source")?;
        validate_owned_id(&req.source_version_id, "source version")?;
        let hash = payload_hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &req.program_id).await?;
        let operation = Operation {
            id: &req.operation_id,
            scope: "portability",
            kind: "create_selector",
            program_id: Some(&req.program_id),
            subject_id: None,
            payload_hash: &hash,
            conflict: "Operation ID was reused with different recall data.",
        };
        if let Some(selector_id) = operation.replay::<String, _>(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.get_selector(&req.program_id, &selector_id).await;
        }
        let version = sqlx::query("SELECT full_text FROM learning_source_versions WHERE program_id=? AND source_id=? AND id=?")
            .bind(&req.program_id)
            .bind(&req.source_id)
            .bind(&req.source_version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Source version not found in this program.".into()))?;
        let text: String = version.get("full_text");
        let selector = create_quote_selector(&text, req.start_byte, req.end_byte)?;
        let matched = match_quote_selector(&text, &selector)?;
        let timestamp = now();
        sqlx::query("INSERT INTO learning_source_selectors(id,program_id,source_id,source_version_id,exact_text,prefix_text,suffix_text,start_byte,end_byte,candidate_count,status,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&req.selector_id)
            .bind(&req.program_id)
            .bind(&req.source_id)
            .bind(&req.source_version_id)
            .bind(&selector.exact)
            .bind(&selector.prefix)
            .bind(&selector.suffix)
            .bind(matched.start_byte.map(|x| x as i64))
            .bind(matched.end_byte.map(|x| x as i64))
            .bind(matched.candidate_count as i64)
            .bind(selector_status_name(&matched.status))
            .bind(timestamp)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        operation.record(&mut *tx, &req.selector_id).await?;
        tx.commit().await.map_err(db)?;
        self.get_selector(&req.program_id, &req.selector_id).await
    }

    pub async fn get_selector(
        &self,
        program_id: &str,
        selector_id: &str,
    ) -> Result<LearningSourceSelectorDto> {
        validate_uuid(program_id, "program")?;
        validate_uuid(selector_id, "selector")?;
        let row =
            sqlx::query("SELECT * FROM learning_source_selectors WHERE program_id=? AND id=?")
                .bind(program_id)
                .bind(selector_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Source selector not found.".into()))?;
        parse_selector(&row).map(SourceSelectorRecord::dto)
    }

    pub async fn match_selector(
        &self,
        req: &MatchLearningSourceSelectorRequestDto,
    ) -> Result<LearningQuoteMatch> {
        validate_uuid(&req.program_id, "program")?;
        validate_uuid(&req.selector_id, "selector")?;
        validate_uuid(&req.target_version_id, "source version")?;
        let row = sqlx::query("SELECT s.source_id,s.exact_text,s.prefix_text,s.suffix_text,v.full_text FROM learning_source_selectors s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.source_id=s.source_id WHERE s.program_id=? AND s.id=? AND v.id=?")
            .bind(&req.program_id)
            .bind(&req.selector_id)
            .bind(&req.target_version_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Selector or source version not found.".into()))?;
        let selector = crate::features::learning::source_selector::LearningTextQuoteSelector {
            exact: row.get("exact_text"),
            prefix: row.get("prefix_text"),
            suffix: row.get("suffix_text"),
        };
        let text: String = row.get("full_text");
        match_quote_selector(&text, &selector)
    }

    pub async fn search_sources(
        &self,
        req: &SearchLearningSourcesSemanticallyRequestDto,
        courses: Option<&std::sync::Arc<CourseIndexes>>,
    ) -> Result<Vec<LearningSourceSemanticSearchResultDto>> {
        validate_uuid(&req.program_id, "program")?;
        let query = req.query.trim();
        if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
            return Err(invalid(
                "Source search query must contain 1–240 characters.",
            ));
        }
        let snapshots = LearningSourceLibraryRepository::new(self.pool.clone());
        let sources = snapshots.verification_sources(&req.program_id).await?;
        // The course's index with its rows brought up to date, never waiting
        // for embedding; by words alone when there is none.
        let course = match courses {
            Some(courses) => courses
                .search(&req.program_id, tokio_util::sync::CancellationToken::new())
                .await
                .inspect_err(|error| tracing::warn!(%error, "Course index could not be opened"))
                .ok(),
            None => None,
        };
        let collection =
            crate::features::learning::reference_collection::ReferenceCollection::searchable_now(
                course.as_ref(),
                &sources,
            )
            .await?;
        let passages = collection.retrieve(query, req.limit).await?;
        let workspace = snapshots.workspace(&req.program_id).await?;
        let mut results = Vec::new();
        for passage in passages {
            let item = workspace
                .sources
                .iter()
                .find(|s| s.active_version_id.as_deref() == Some(&passage.source_id))
                .ok_or_else(|| {
                    invalid("The reference collection changed during search. Search again.")
                })?;
            let version = item
                .active_version
                .clone()
                .ok_or_else(|| invalid("The source version is unavailable."))?;
            let source = sources
                .iter()
                .find(|s| s.id == passage.source_id)
                .ok_or_else(|| invalid("The source version is unavailable."))?;
            results.push(LearningSourceSemanticSearchResultDto {
                source_id: item.id.clone(),
                version,
                excerpt: passage.text.clone(),
                score: passage.score,
                retrieval_kind: passage.retrieval_kind,
                selector: create_quote_selector(
                    &source.excerpt,
                    passage.start_byte,
                    passage.end_byte,
                )?,
            });
        }
        Ok(results)
    }
}

fn parse_selector(row: &sqlx::sqlite::SqliteRow) -> Result<SourceSelectorRecord> {
    let status = match row.get::<String, _>("status").as_str() {
        "exact" => crate::features::learning::source_selector::LearningQuoteMatchStatus::Exact,
        "contextual" => crate::features::learning::source_selector::LearningQuoteMatchStatus::ContextDisambiguated,
        "ambiguous" => crate::features::learning::source_selector::LearningQuoteMatchStatus::Ambiguous,
        "not_found" => crate::features::learning::source_selector::LearningQuoteMatchStatus::NotFound,
        _ => {
            return Err(AppError::InvalidState(
                "Invalid stored source selector status.".into(),
            ))
        }
    };
    let exact: String = row.get("exact_text");
    let prefix: String = row.get("prefix_text");
    let suffix: String = row.get("suffix_text");
    let start: Option<i64> = row.get("start_byte");
    let end: Option<i64> = row.get("end_byte");
    Ok(SourceSelectorRecord {
        id: row.get("id"),
        source_id: row.get("source_id"),
        source_version_id: row.get("source_version_id"),
        selector: crate::features::learning::source_selector::LearningTextQuoteSelector {
            exact,
            prefix,
            suffix,
        },
        match_status: status,
        start_byte: start.map(|value| value.max(0) as usize),
        end_byte: end.map(|value| value.max(0) as usize),
        candidate_count: row.get::<i64, _>("candidate_count").max(0) as usize,
        created_at: row.get("created_at"),
    })
}

fn selector_status_name(
    status: &crate::features::learning::source_selector::LearningQuoteMatchStatus,
) -> &'static str {
    match status {
        crate::features::learning::source_selector::LearningQuoteMatchStatus::Exact => "exact",
        crate::features::learning::source_selector::LearningQuoteMatchStatus::ContextDisambiguated => "contextual",
        crate::features::learning::source_selector::LearningQuoteMatchStatus::Ambiguous => "ambiguous",
        crate::features::learning::source_selector::LearningQuoteMatchStatus::NotFound => "not_found",
    }
}
