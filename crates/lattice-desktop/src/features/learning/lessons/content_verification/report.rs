//! Binding, validating and persisting the verification report at publication.
use super::resolution::Resolution;
use super::*;
use crate::features::learning::review_findings::{FindingKind, FindingSite, ReviewFinding};

pub(super) fn content_hash(lesson: &PreparedLearningLesson) -> Result<String> {
    Ok(digest(&serde_json::to_string(&(
        &lesson.blocks,
        &lesson.questions,
        &lesson.keys,
    ))?))
}
/// How a lesson's preparation ends when `count` of its statements are
/// published not verified; `None` when none are.
pub(in crate::features::learning) fn not_verified_line(count: usize) -> Option<String> {
    (count > 0).then(|| {
        format!(
            "{count} statement{} not verified",
            if count == 1 { "" } else { "s" }
        )
    })
}

impl LessonVerificationReport {
    /// The statements this lesson publishes not verified.
    pub(in crate::features::learning) fn unverified_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| matches!(finding.resolution, Some(Resolution::Unverified { .. })))
            .filter(|finding| {
                finding.class.is_none_or(|classed| {
                    classed.class != lattice_core::domain::learning::claim_review::Class::Internal
                })
            })
            .count()
    }

    pub(in crate::features::learning) fn reusable(
        &self,
        lesson_id: &str,
        lesson: &PreparedLearningLesson,
        sources: &[crate::features::learning::dto::LearningSourceDto],
        llm: &dyn LLMPort,
    ) -> bool {
        self.refusal(lesson_id, lesson, sources, llm).is_none()
    }

    /// Why this report cannot stand for `lesson` while the program holds
    /// `sources` and `llm` checks lessons; `None` when it can.
    pub(in crate::features::learning) fn refusal(
        &self,
        lesson_id: &str,
        lesson: &PreparedLearningLesson,
        sources: &[crate::features::learning::dto::LearningSourceDto],
        llm: &dyn LLMPort,
    ) -> Option<String> {
        if self.checker_model != llm.model_name() {
            return Some(format!(
                "it was checked by {}, and {} checks lessons now",
                self.checker_model,
                llm.model_name()
            ));
        }
        match self.unpublishable(lesson_id, lesson) {
            Ok(Some(reason)) => return Some(format!("its verification report {reason}")),
            Ok(None) => {}
            Err(error) => return Some(error.to_string()),
        }
        let changed = !self.sources.iter().all(|binding| {
            sources
                .iter()
                .any(|source| source.id == binding.id && digest(&source.excerpt) == binding.sha256)
        });
        changed.then(|| {
            "a source it was checked against changed while it was being prepared; prepare it again once the sources stop changing".to_owned()
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
    /// What the audit found the claim inventory leaves out, as findings for
    /// the teaching review: one for each section or question it names, with
    /// every assertion it lists. They are not claims: nothing is checked
    /// against the sources for them, and the finding says so. `blocks` is the
    /// number of sections, which come before the questions in the audit's
    /// order (claim classes design §3.4).
    pub(in crate::features::learning) fn coverage_findings(
        &self,
        blocks: usize,
    ) -> Vec<ReviewFinding> {
        self.coverage_audit
            .iter()
            .filter(|unit| !unit.missing_assertions.is_empty())
            .map(|unit| {
                let site = if unit.index < blocks {
                    FindingSite::Section(unit.index)
                } else {
                    FindingSite::Question(unit.index - blocks)
                };
                let statements: Vec<&str> = unit
                    .missing_assertions
                    .iter()
                    .map(|missing| missing.statement.as_str())
                    .collect();
                ReviewFinding::new(
                    FindingKind::Teaching,
                    Some(site),
                    format!(
                        "The claim audit found assertions here that were not extracted for checking, so no source has confirmed them: {}",
                        statements.join("; ")
                    ),
                )
            })
            .collect()
    }

    /// The user confirmed this example although it did not complete (§5.5).
    pub(super) fn example_confirmed(&self, observation_id: &str) -> bool {
        self.confirmed_examples
            .as_ref()
            .is_some_and(|ids| ids.iter().any(|id| id == observation_id))
    }

    pub(super) fn validate(&self, lesson_id: &str, lesson: &PreparedLearningLesson) -> Result<()> {
        match self.unpublishable(lesson_id, lesson)? {
            Some(reason) => Err(invalid(format!(
                "This lesson cannot be published: its verification report {}.",
                reason.trim_end_matches('.')
            ))),
            None => Ok(()),
        }
    }

    /// Why this report may not publish `lesson`; `None` when it is a complete
    /// report of this exact revision whose every statement a passage supports
    /// or the user resolved.
    fn unpublishable(
        &self,
        lesson_id: &str,
        lesson: &PreparedLearningLesson,
    ) -> Result<Option<String>> {
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
        let reason = if unit_count == 0
            || self.coverage.len() != unit_count
            || coverage_ids.len() != unit_count
            || self.coverage_audit.len() != unit_count
            || audit_ids != coverage_ids
            || coverage_ids.iter().any(|index| *index >= unit_count)
        {
            "does not cover each section and question of the lesson once"
        } else if self.policy != POLICY {
            "was made under another verification policy"
        } else if self.lesson_id != lesson_id || self.content_sha256 != content_hash(lesson)? {
            "was made for another revision of the lesson"
        } else if self.findings.is_empty() || !findings_complete {
            "does not answer for every statement its claim inventory lists"
        } else if !self.issues.is_empty() {
            return Ok(Some(format!(
                "leaves {} open: {}",
                if self.issues.len() == 1 {
                    "1 issue".to_owned()
                } else {
                    format!("{} issues", self.issues.len())
                },
                self.issues.join("; ")
            )));
        } else if self.findings.iter().any(|f| !f.publishable()) {
            "holds a statement no passage supports and you have not resolved"
        } else if self
            .executions
            .iter()
            .any(|e| !e.passed() && !self.example_confirmed(&e.id))
        {
            "holds a code example that did not complete"
        } else {
            return Ok(None);
        };
        Ok(Some(reason.to_owned()))
    }
}

/// The publication gate's refusals when a source the lesson was verified
/// against moved on. A source added since is none of them (claim classes design
/// §3.2). The job verifies again, once, instead of failing (§12).
const SOURCE_INACTIVE: &str = "A verified source is no longer active; verify this lesson again.";
const SOURCE_CONTENT_CHANGED: &str =
    "Verification source content changed; verify this lesson again.";

/// Whether `error` is the publication gate finding the references changed
/// since verification.
pub(in crate::features::learning) fn sources_changed(error: &AppError) -> bool {
    matches!(error, AppError::InvalidInput(message)
        if [SOURCE_INACTIVE, SOURCE_CONTENT_CHANGED].contains(&message.as_str()))
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
    for source in &report.sources {
        let row = sqlx::query("SELECT v.full_text FROM learning_source_versions v JOIN learning_source_library s ON s.active_version_id=v.id AND s.program_id=v.program_id WHERE v.id=? AND v.program_id=? AND s.deleted_at IS NULL")
            .bind(&source.id).bind(program_id).fetch_optional(&mut **tx).await.map_err(|e| AppError::Database(e.to_string()))?.ok_or_else(|| invalid(SOURCE_INACTIVE))?;
        if digest(&row.get::<String, _>("full_text")) != source.sha256 {
            return Err(invalid(SOURCE_CONTENT_CHANGED));
        }
    }
    sqlx::query("INSERT INTO learning_lesson_verifications(lesson_id,program_id,policy,content_sha256,report_json,checked_at) VALUES(?,?,?,?,?,?)")
        .bind(lesson_id).bind(program_id).bind(&report.policy).bind(&report.content_sha256).bind(serde_json::to_string(report)?).bind(report.checked_at).execute(&mut **tx).await.map_err(|e| AppError::Database(e.to_string()))?;
    // The lesson's claim review, when it has one, ends with the publication.
    sqlx::query("UPDATE learning_lesson_reviews SET state='ready',revision=revision+1,updated_at=? WHERE lesson_id=?")
        .bind(chrono::Utc::now().timestamp_millis()).bind(lesson_id).execute(&mut **tx).await.map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}
