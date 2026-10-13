//! Binding, validating and persisting the verification report at publication.
use super::*;

pub(super) fn content_hash(lesson: &PreparedLearningLesson) -> Result<String> {
    Ok(digest(&serde_json::to_string(&(
        &lesson.blocks,
        &lesson.questions,
        &lesson.keys,
    ))?))
}
impl LessonVerificationReport {
    pub(in crate::features::learning) fn reusable(
        &self,
        lesson_id: &str,
        lesson: &PreparedLearningLesson,
        sources: &[crate::features::learning::dto::LearningSourceDto],
        llm: &dyn LLMPort,
    ) -> bool {
        self.checker_model == llm.model_name()
            && self.validate(lesson_id, lesson).is_ok()
            && self.sources.len() == sources.len()
            && self.sources.iter().all(|binding| {
                sources.iter().any(|source| {
                    source.id == binding.id && digest(&source.excerpt) == binding.sha256
                })
            })
    }

    pub(in crate::features::learning) fn bind(
        mut self,
        lesson_id: &str,
        lesson: &PreparedLearningLesson,
    ) -> Result<Self> {
        self.lesson_id = lesson_id.into();
        self.content_sha256 = content_hash(lesson)?;
        Ok(self)
    }
    pub(super) fn validate(&self, lesson_id: &str, lesson: &PreparedLearningLesson) -> Result<()> {
        let unit_count = lesson.blocks.len() + lesson.questions.len();
        let coverage_ids = self
            .coverage
            .iter()
            .map(|unit| unit.index)
            .collect::<HashSet<_>>();
        let audit_ids = self
            .coverage_audit
            .iter()
            .map(|unit| unit.index)
            .collect::<HashSet<_>>();
        // A deserialized final checkpoint must account for every extracted
        // assertion exactly once. A nonempty subset of supported findings is
        // not a complete report, even when the content hash still matches.
        let mut outstanding = HashMap::new();
        for unit in &self.coverage {
            for claim in &unit.claims {
                *outstanding
                    .entry((unit.index, claim.quote.as_str(), claim.statement.as_str()))
                    .or_insert(0usize) += 1;
            }
        }
        let findings_complete = self.findings.iter().all(|finding| {
            let Some(count) = outstanding.get_mut(&(
                finding.unit,
                finding.quote.as_str(),
                finding.statement.as_str(),
            )) else {
                return false;
            };
            if *count == 0 {
                return false;
            }
            *count -= 1;
            true
        }) && outstanding.values().all(|count| *count == 0);
        if unit_count == 0
            || self.coverage.len() != unit_count
            || coverage_ids.len() != unit_count
            || self.coverage_audit.len() != unit_count
            || audit_ids != coverage_ids
            || coverage_ids.iter().any(|index| *index >= unit_count)
            || self.coverage_audit.iter().any(|unit| !unit.complete)
            || self.policy != POLICY
            || self.lesson_id != lesson_id
            || self.content_sha256 != content_hash(lesson)?
            || !self.issues.is_empty()
            || self.findings.is_empty()
            || !findings_complete
            || self
                .findings
                .iter()
                .any(|f| f.verdict != ClaimVerdict::Supported)
            || self.executions.iter().any(|e| !e.passed())
        {
            return Err(invalid("A complete verification report for this exact lesson revision is required before publication."));
        }
        Ok(())
    }
}

/// Called inside both publication transactions, before marking anything ready.
pub(in crate::features::learning) async fn persist_report(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    lesson_id: &str,
    lesson: &PreparedLearningLesson,
) -> Result<()> {
    use sqlx::Row;
    let report = lesson.verification.as_ref().ok_or_else(|| {
        invalid("This lesson has no verification report. Prepare it again before publication.")
    })?;
    report.validate(lesson_id, lesson)?;
    if !report.sources.is_empty() {
        let active: Vec<String> = sqlx::query_scalar("SELECT active_version_id FROM learning_source_library WHERE program_id=? AND deleted_at IS NULL AND active_version_id IS NOT NULL").bind(program_id).fetch_all(&mut **tx).await.map_err(|e| AppError::Database(e.to_string()))?;
        let expected: HashSet<_> = report.sources.iter().map(|s| s.id.as_str()).collect();
        if active.iter().map(String::as_str).collect::<HashSet<_>>() != expected {
            return Err(invalid(
                "The reference collection changed during verification. Prepare the lesson again.",
            ));
        }
    }
    for source in &report.sources {
        let row = sqlx::query("SELECT v.full_text FROM learning_source_versions v JOIN learning_source_library s ON s.active_version_id=v.id AND s.program_id=v.program_id WHERE v.id=? AND v.program_id=? AND s.deleted_at IS NULL")
            .bind(&source.id).bind(program_id).fetch_optional(&mut **tx).await.map_err(|e| AppError::Database(e.to_string()))?.ok_or_else(|| invalid("A verified source is no longer active; verify this lesson again."))?;
        if digest(&row.get::<String, _>("full_text")) != source.sha256 {
            return Err(invalid(
                "Verification source content changed; verify this lesson again.",
            ));
        }
    }
    sqlx::query("INSERT INTO learning_lesson_verifications(lesson_id,program_id,policy,content_sha256,report_json,checked_at) VALUES(?,?,?,?,?,?)")
        .bind(lesson_id).bind(program_id).bind(&report.policy).bind(&report.content_sha256).bind(serde_json::to_string(report)?).bind(report.checked_at).execute(&mut **tx).await.map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}
