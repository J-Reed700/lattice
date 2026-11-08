//! Binding, validating and persisting the verification report at publication.
use super::resolution::Resolution;
use super::*;
use crate::review_findings::{FindingKind, FindingSite, ReviewFinding};

pub(super) fn content_hash(lesson: &PreparedLearningLesson) -> Result<String> {
    Ok(digest(&serde_json::to_string(&(
        &lesson.blocks,
        &lesson.questions,
        &lesson.keys,
    ))?))
}
/// A sentence of a published section the user wrote over, and the claim that
/// stands for what they wrote.
pub(crate) struct WrittenOver<'a> {
    pub unit: usize,
    /// The section's text before the edit, and the span the edit replaced.
    pub old_body: &'a str,
    pub range: (usize, usize),
    pub new_body: &'a str,
    /// The claims that stood on the replaced text.
    pub replaced: &'a HashSet<String>,
    pub after: &'a str,
    pub successor: &'a ClaimId,
    pub at: i64,
}

/// A sentence put back into a published section, or the original put in
/// place of a rewrite, and the claim that stands for it.
pub(crate) struct WrittenBack<'a> {
    pub unit: usize,
    /// The section's text before, and the span the sentence replaced (empty
    /// where it was written in).
    pub old_body: &'a str,
    pub range: (usize, usize),
    pub new_body: &'a str,
    /// The claims that stood on the replaced text.
    pub replaced: &'a HashSet<String>,
    pub claim: &'a ClaimId,
    pub sentence: &'a str,
    pub statement: &'a str,
    pub reason: &'a str,
    pub at: i64,
}

/// What a published edit or restore took out of the report's findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ReplacedFindings {
    /// The claim whose finding took their place.
    by: String,
    /// Where the first of them stood among the findings.
    #[serde(default)]
    at: usize,
    findings: Vec<Finding>,
}

/// A sentence of a published section the user took out, with the claims
/// that stood on it.
pub(crate) struct TakenOut<'a> {
    pub unit: usize,
    /// The section's text before the removal, and the span it took out.
    pub old_body: &'a str,
    pub range: (usize, usize),
    pub new_body: &'a str,
    /// The claims that stood on the removed text.
    pub removed: &'a HashSet<String>,
    /// The claim the user removed, whose id keeps what the removal dropped.
    pub claim: &'a ClaimId,
}

/// Whether `finding` stood on the text at `range` of `old_body` in `unit`:
/// its claim is in `claims`, or it names no claim and quotes that text.
fn stood_on(
    finding: &Finding,
    unit: usize,
    claims: &HashSet<String>,
    old_body: &str,
    range: (usize, usize),
) -> bool {
    finding.unit == unit
        && match &finding.claim_id {
            Some(id) => claims.contains(id),
            None => lattice_core::domain::learning::claim_review::span_of(old_body, &finding.quote)
                .is_some_and(|(start, end)| start < range.1 && range.0 < end),
        }
}

impl LessonVerificationReport {
    pub(crate) fn content_sha256(&self) -> &str {
        &self.content_sha256
    }

    /// The report after the user took out the text the findings in
    /// `removal.removed` stood on: those findings are gone, every other
    /// finding as it was.
    pub(crate) fn taken_out(&mut self, removal: &TakenOut<'_>) {
        let stood = |finding: &Finding| {
            stood_on(
                finding,
                removal.unit,
                removal.removed,
                removal.old_body,
                removal.range,
            )
        };
        let at = self.findings.iter().position(stood).unwrap_or(0);
        let (gone, kept): (Vec<Finding>, Vec<Finding>) = std::mem::take(&mut self.findings)
            .into_iter()
            .partition(|finding| stood(finding));
        self.findings = kept;
        if !gone.is_empty() {
            self.replaced_findings.push(ReplacedFindings {
                by: removal.claim.0.clone(),
                at,
                findings: gone,
            });
        }
        self.content_sha256 =
            digest(&json!([self.content_sha256, removal.unit, removal.new_body]).to_string());
    }

    /// The report after the user's sentence took the place of the text the
    /// findings in `edit.replaced` stood on: one finding for the user's
    /// sentence, confirmed by them, in the place of those, every other
    /// finding as it was.
    pub(crate) fn written_over(&mut self, edit: &WrittenOver<'_>) {
        let replaced = |finding: &Finding| {
            stood_on(finding, edit.unit, edit.replaced, edit.old_body, edit.range)
        };
        let first = self.findings.iter().position(replaced);
        let (gone, kept): (Vec<Finding>, Vec<Finding>) = std::mem::take(&mut self.findings)
            .into_iter()
            .partition(|finding| replaced(finding));
        self.findings = kept;
        if !gone.is_empty() {
            self.replaced_findings.push(ReplacedFindings {
                by: edit.successor.0.clone(),
                at: first.unwrap_or(0),
                findings: gone,
            });
        }
        let finding = Finding {
            unit: edit.unit,
            quote: edit.after.to_owned(),
            statement: edit.after.to_owned(),
            verdict: ClaimVerdict::Unverified,
            reason: "You wrote this sentence; it was not checked against the sources.".into(),
            evidence: Vec::new(),
            supporting_quote: None,
            resolution: Some(Resolution::Confirmed {
                at: edit.at,
                note: None,
            }),
            claim_id: Some(edit.successor.0.clone()),
            trust: None,
            label: None,
            class: None,
            two_pass: Default::default(),
        };
        let at = first.unwrap_or(self.findings.len());
        self.findings.insert(at, finding);
        self.content_sha256 =
            digest(&json!([self.content_sha256, edit.unit, edit.new_body]).to_string());
    }
}

impl LessonVerificationReport {
    /// The report after a sentence was written into the text for `back.claim`:
    /// one finding for it, confirmed by the user, in the place of the findings
    /// of the claims that stood on the replaced text.
    pub(crate) fn written_back(&mut self, back: &WrittenBack<'_>) {
        let replaced = |finding: &Finding| match &finding.claim_id {
            Some(id) => id == &back.claim.0 || back.replaced.contains(id),
            None => {
                back.range.0 < back.range.1
                    && finding.unit == back.unit
                    && lattice_core::domain::learning::claim_review::span_of(
                        back.old_body,
                        &finding.quote,
                    )
                    .is_some_and(|(start, end)| start < back.range.1 && back.range.0 < end)
            }
        };
        let first = self.findings.iter().position(replaced);
        let (gone, kept): (Vec<Finding>, Vec<Finding>) = std::mem::take(&mut self.findings)
            .into_iter()
            .partition(|finding| replaced(finding));
        self.findings = kept;
        if !gone.is_empty() {
            self.replaced_findings.push(ReplacedFindings {
                by: back.claim.0.clone(),
                at: first.unwrap_or(0),
                findings: gone,
            });
        }
        let finding = Finding {
            unit: back.unit,
            quote: back.sentence.to_owned(),
            statement: back.statement.to_owned(),
            verdict: ClaimVerdict::Unverified,
            reason: back.reason.to_owned(),
            evidence: Vec::new(),
            supporting_quote: None,
            resolution: Some(Resolution::Confirmed {
                at: back.at,
                note: None,
            }),
            claim_id: Some(back.claim.0.clone()),
            trust: None,
            label: None,
            class: None,
            two_pass: Default::default(),
        };
        let at = first.unwrap_or_else(|| {
            self.findings
                .iter()
                .position(|other| other.unit > back.unit)
                .unwrap_or(self.findings.len())
        });
        self.findings.insert(at, finding);
        self.content_sha256 =
            digest(&json!([self.content_sha256, back.unit, back.new_body]).to_string());
    }

    /// The report after the sentence the claims in `claims` stood for was
    /// written out of the text again: their findings go, and the findings
    /// they replaced come back.
    pub(crate) fn written_out(&mut self, unit: usize, new_body: &str, claims: &HashSet<String>) {
        let own = |finding: &Finding| {
            finding
                .claim_id
                .as_ref()
                .is_some_and(|id| claims.contains(id))
        };
        let at = self.findings.iter().position(own);
        self.findings.retain(|finding| !own(finding));
        self.restore_replaced(claims, at);
        self.content_sha256 = digest(&json!([self.content_sha256, unit, new_body]).to_string());
    }

    /// The report after the user's removal was undone: the findings it
    /// dropped are back where they stood.
    pub(crate) fn taken_back(&mut self, unit: usize, new_body: &str, claims: &HashSet<String>) {
        self.restore_replaced(claims, None);
        self.content_sha256 = digest(&json!([self.content_sha256, unit, new_body]).to_string());
    }

    /// Puts back the findings kept for the claims in `claims`, at `at` or
    /// where they stood: the latest entry only, since an older one holds
    /// findings a later change already replaced.
    fn restore_replaced(&mut self, claims: &HashSet<String>, at: Option<usize>) {
        if let Some(held) = self
            .replaced_findings
            .iter()
            .rposition(|replaced| claims.contains(&replaced.by))
        {
            let replaced = self.replaced_findings.remove(held);
            let place = at.unwrap_or(replaced.at).min(self.findings.len());
            self.findings.splice(place..place, replaced.findings);
        }
    }
}

/// How a lesson's preparation ends when `count` of its statements are
/// published not verified; `None` when none are.
pub(crate) fn not_verified_line(count: usize) -> Option<String> {
    (count > 0).then(|| {
        format!(
            "{count} statement{} not verified",
            if count == 1 { "" } else { "s" }
        )
    })
}

impl LessonVerificationReport {
    /// The statements this lesson publishes not verified.
    pub(crate) fn unverified_count(&self) -> usize {
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

    pub(crate) fn reusable(
        &self,
        lesson_id: &str,
        lesson: &PreparedLearningLesson,
        sources: &[crate::dto::LearningSourceDto],
        llm: &dyn LLMPort,
    ) -> bool {
        self.refusal(lesson_id, lesson, sources, llm).is_none()
    }

    /// Why this report cannot stand for `lesson` while the program holds
    /// `sources` and `llm` checks lessons; `None` when it can.
    pub(crate) fn refusal(
        &self,
        lesson_id: &str,
        lesson: &PreparedLearningLesson,
        sources: &[crate::dto::LearningSourceDto],
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

    pub(crate) fn bind(mut self, lesson_id: &str, lesson: &PreparedLearningLesson) -> Result<Self> {
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
    pub(crate) fn coverage_findings(&self, blocks: usize) -> Vec<ReviewFinding> {
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
                let quoted: Vec<String> = statements
                    .iter()
                    .map(|statement| format!("\u{201c}{statement}\u{201d}"))
                    .collect();
                let summary = match quoted.as_slice() {
                    [only] => format!(
                        "A statement here was never checked against a source. The checker read it as: {only}."
                    ),
                    _ => format!(
                        "Some statements here were never checked against a source. The checker read them as: {}.",
                        quoted.join(", ")
                    ),
                };
                ReviewFinding::new(
                    FindingKind::Teaching,
                    Some(site),
                    format!(
                        "The claim audit found assertions here that were not extracted for checking, so no source has confirmed them: {}",
                        statements.join("; ")
                    ),
                )
                .with_summary(summary)
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
pub(crate) fn sources_changed(error: &AppError) -> bool {
    matches!(error, AppError::InvalidInput(message)
        if [SOURCE_INACTIVE, SOURCE_CONTENT_CHANGED].contains(&message.as_str()))
}

/// Called inside both publication transactions, before marking anything ready.
pub(crate) async fn persist_report(
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
