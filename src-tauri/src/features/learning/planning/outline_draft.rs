//! Resumable outline drafts. A saved draft is not an approval to teach its content.
use crate::features::learning::outline_linking::{CitationReceipts, LinkLog};
use crate::features::learning::outline_references::ReferencePlan;
use crate::features::learning::{
    dto::*, outline_progress::OutlineProgress, repository::LearningRepository,
};
use crate::features::web::traits::WebServiceTrait;
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningOutlineReviewStatus {
    Unchecked,
    NeedsRepair,
    Passed,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningOutlineIssueKind {
    /// An assertion about the world stated as true (a version, behaviour,
    /// number, date, API fact) that the reviewer believes is wrong or unsupported.
    Factual,
    /// Sequencing, prerequisites, duplication, scope, session fit, project
    /// progression or a contradictory requirement.
    Plan,
    /// Course-wide: no references could be gathered, so lessons cannot be prepared.
    References,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineIssueDto {
    /// Stable identity assigned by code when the finding is first raised.
    pub id: String,
    /// JSON pointer into the saved candidate. Empty means the entire curriculum.
    pub path: String,
    pub kind: LearningOutlineIssueKind,
    /// The item text at `path` as of the last review, refreshed by code.
    pub claim: String,
    /// The reviewer's current reason, refreshed on each unresolved verdict.
    pub message: String,
    /// Repair attempts spent on this finding in the current operation.
    pub attempts: u32,
    /// The attempt limit or the module's rewrite budget is spent; waits for the user.
    pub exhausted: bool,
    /// Evidence shown to the user: the contradicting or best passage found.
    pub quote: String,
    pub source_id: Option<String>,
    /// Public searches already run for this finding.
    pub searched: Vec<String>,
}

/// Repair attempts a finding may consume in one operation.
pub(in crate::features::learning) const ATTEMPT_LIMIT: u32 = 2;
/// Rewrites one module may receive in one operation.
pub(in crate::features::learning) const MODULE_REWRITE_BUDGET: u32 = 3;

/// The module a finding's path belongs to; `None` for a course-wide finding.
pub(in crate::features::learning) fn module_index(path: &str) -> Option<usize> {
    path.strip_prefix("/modules/")
        .and_then(|rest| rest.split('/').next())
        .and_then(|index| index.parse().ok())
}

/// Rewrites spent per module in this operation. Held in memory only: a new
/// Repair press is a fresh budget.
#[derive(Debug, Default)]
pub(in crate::features::learning) struct ModuleBudgets {
    spent: BTreeMap<usize, u32>,
    modules: usize,
}
impl ModuleBudgets {
    pub fn new(modules: usize) -> Self {
        Self {
            spent: BTreeMap::new(),
            modules,
        }
    }
    pub fn remaining(&self, module: usize) -> u32 {
        MODULE_REWRITE_BUDGET.saturating_sub(self.spent.get(&module).copied().unwrap_or(0))
    }
    /// Whether a finding at `path` can still be addressed by a rewrite. A
    /// course-wide finding can while any module has budget.
    pub fn can_rewrite(&self, path: &str) -> bool {
        match module_index(path) {
            Some(module) => self.remaining(module) > 0,
            None => (0..self.modules).any(|module| self.remaining(module) > 0),
        }
    }
    /// Spend one rewrite on `module`; false when none is left.
    pub fn spend(&mut self, module: usize) -> bool {
        if self.remaining(module) == 0 {
            return false;
        }
        *self.spent.entry(module).or_default() += 1;
        true
    }
}

impl LearningOutlineIssueDto {
    /// Whether this finding keeps the course from being accepted. A plan
    /// finding that automatic repair could not settle within its attempts is
    /// an editorial suggestion from the same model: it stays visible but does
    /// not block. Factual and missing-reference findings always block.
    pub(in crate::features::learning) fn blocks_acceptance(&self) -> bool {
        !(self.kind == LearningOutlineIssueKind::Plan && self.exhausted)
    }

    /// Whether repair may still act on this finding in this operation. A
    /// factual finding's first attempt is research, which needs no rewrite
    /// budget; everything after that is a rewrite.
    pub(in crate::features::learning) fn actionable(&self, budgets: &ModuleBudgets) -> bool {
        if self.kind == LearningOutlineIssueKind::References || self.attempts >= ATTEMPT_LIMIT {
            return false;
        }
        (self.kind == LearningOutlineIssueKind::Factual && self.attempts == 0)
            || budgets.can_rewrite(&self.path)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineReviewDto {
    pub status: LearningOutlineReviewStatus,
    pub issues: Vec<LearningOutlineIssueDto>,
    pub repair_passes: u32,
    pub note: String,
    pub updated_at: i64,
    pub content_hash: String,
    /// Outline items whose citation is verified: chosen by evidence linking or
    /// attached by a supported factual repair.
    pub citations_verified: u32,
    /// Author citations removed in this operation because they did not match
    /// their source exactly or evidence linking did not choose them.
    pub citations_removed: u32,
    /// Reference pages gathered automatically for this program.
    pub references_gathered: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepairLearningOutlineRequestDto {
    pub program_id: String,
    pub expected_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::features::learning) struct OutlineModuleBinding {
    pub module_id: String,
    pub lesson_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::features::learning) struct OutlineDraft {
    pub request: GenerateLearningProgramRequestDto,
    pub candidate: Value,
    pub source_ids: Vec<String>,
    pub review: LearningOutlineReviewDto,
    #[serde(default)]
    pub completed_review: Option<crate::features::learning::outline_review_scope::ReviewReceipt>,
    /// App-owned identities for candidate positions. Kept outside model JSON
    /// so renaming/reordering the visible course cannot reattach its evidence.
    #[serde(default)]
    pub bindings: Option<Vec<OutlineModuleBinding>>,
    /// The subtopics and public searches the references were gathered for.
    #[serde(default)]
    pub reference_plan: Option<ReferencePlan>,
    /// Evidence-link verdicts, keyed by policy, model, item and passage.
    #[serde(default)]
    pub citation_receipts: CitationReceipts,
    /// Entailment verdicts for factual findings judged during repair.
    #[serde(default)]
    pub claim_receipts: BTreeMap<String, OutlineClaimReceipt>,
    /// Searches already run for a finding, by finding id, so a retry explores
    /// new queries instead of repeating.
    #[serde(default)]
    pub research_log: BTreeMap<String, Vec<String>>,
    /// Findings raised so far; the next finding is `finding-{n+1}`.
    #[serde(default)]
    pub findings_raised: u32,
}

/// A strict-judge verdict on one factual assertion against saved passages.
/// A supported receipt also records where the evidence was attached, so a
/// later review can be told the assertion is verified rather than re-raise it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::features::learning) struct OutlineClaimReceipt {
    pub verdict: lattice_core::application::services::claim_verification::ClaimVerdict,
    pub reason: String,
    pub supporting_quote: Option<String>,
    pub checked_at: i64,
    pub claim: String,
    pub path: String,
    pub source_id: Option<String>,
}
impl OutlineDraft {
    pub fn new(
        request: GenerateLearningProgramRequestDto,
        candidate: Value,
        sources: &[LearningSourceDto],
    ) -> Self {
        let mut draft = Self {
            request,
            candidate,
            source_ids: sources.iter().map(|s| s.id.clone()).collect(),
            completed_review: None,
            bindings: None,
            reference_plan: None,
            citation_receipts: CitationReceipts::new(),
            claim_receipts: BTreeMap::new(),
            research_log: BTreeMap::new(),
            findings_raised: 0,
            review: LearningOutlineReviewDto {
                status: LearningOutlineReviewStatus::Unchecked,
                issues: vec![],
                repair_passes: 0,
                note: "Draft saved. Review has not completed; use Repair to resume if interrupted."
                    .into(),
                updated_at: 0,
                content_hash: String::new(),
                citations_verified: 0,
                citations_removed: 0,
                references_gathered: 0,
            },
        };
        draft.stamp();
        draft
    }

    /// Fold an evidence-linking run into the review counters. `verified` is
    /// the current count of cited items (every surviving citation has a
    /// receipt); removals accumulate over the operation.
    pub fn record_citations(&mut self, log: LinkLog) {
        self.review.citations_verified =
            crate::features::learning::outline_linking::cited_count(&self.candidate);
        self.review.citations_removed += log.removed();
    }

    /// Called only where the candidate and its materialized outline are saved
    /// together. Existing bindings survive later repairs of the same shape.
    pub fn bind_program(&mut self, program: &LearningProgramDto) -> Result<()> {
        if self.bindings.is_some() {
            return Ok(());
        }
        let modules = self
            .candidate
            .get("modules")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::InvalidInput("Outline evidence has no modules.".into()))?;
        if modules.len() != program.modules.len()
            || modules
                .iter()
                .zip(&program.modules)
                .any(|(candidate, module)| {
                    candidate
                        .get("lessons")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        != Some(module.lessons.len())
                })
        {
            return Err(AppError::InvalidInput(
                "Outline evidence does not match the saved course structure.".into(),
            ));
        }
        self.bindings = Some(
            program
                .modules
                .iter()
                .map(|module| OutlineModuleBinding {
                    module_id: module.id.clone(),
                    lesson_ids: module
                        .lessons
                        .iter()
                        .map(|lesson| lesson.id.clone())
                        .collect(),
                })
                .collect(),
        );
        Ok(())
    }

    /// A new, stable finding identity for this draft.
    pub fn next_finding_id(&mut self) -> String {
        self.findings_raised += 1;
        format!("finding-{}", self.findings_raised)
    }

    /// Findings the reviewer and repair act on; the References finding is
    /// derived from the source count, never carried.
    pub fn open_findings(&self) -> impl Iterator<Item = &LearningOutlineIssueDto> {
        self.review
            .issues
            .iter()
            .filter(|issue| issue.kind != LearningOutlineIssueKind::References)
    }

    pub fn stamp(&mut self) {
        self.review.content_hash = format!(
            "{:x}",
            Sha256::digest(self.candidate.to_string().as_bytes())
        );
        self.review.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

/// The one registry of outline drafts being reviewed, repaired or accepted in
/// this process. Built by the composition root and handed to the commands
/// that touch a draft, so concurrent manual repair/acceptance of the same draft
/// is rejected against the same set — and a test never sees a draft another
/// test holds. Held in memory only, so restart recovery stays free of stale
/// locks; database revisions also protect every checkpoint and acceptance.
#[derive(Clone, Default)]
pub struct DraftRunRegistry {
    active: Arc<Mutex<HashSet<String>>>,
}

impl DraftRunRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(in crate::features::learning) fn acquire(&self, id: &str) -> Result<DraftRun> {
        if !self.lock().insert(id.to_owned()) {
            return Err(AppError::InvalidInput(
                "This draft is already being reviewed or repaired.".into(),
            ));
        }
        Ok(DraftRun {
            id: id.into(),
            registry: self.clone(),
        })
    }
}

pub(in crate::features::learning) struct DraftRun {
    id: String,
    registry: DraftRunRegistry,
}
impl Drop for DraftRun {
    fn drop(&mut self) {
        self.registry.lock().remove(&self.id);
    }
}

const NO_REFERENCES: &str = "No references could be gathered. Lessons need saved references before they can be prepared; Repair will retry gathering.";

/// Gather references for a draft that has none, persisting each page with the
/// draft. Reuses the saved plan when there is one so a retry runs the same
/// public searches rather than planning again.
async fn gather_for_draft(
    repo: &LearningRepository,
    llm: &dyn LLMPort,
    web: &dyn WebServiceTrait,
    program: &mut LearningProgramDto,
    draft: &mut OutlineDraft,
    sources: &mut Vec<LearningSourceDto>,
    progress: &OutlineProgress,
) -> Result<()> {
    progress.stage(crate::features::learning::outline_progress::OutlineStage::GatheringReferences);
    let plan = match &draft.reference_plan {
        Some(plan) if plan.query_count() > 0 => plan.clone(),
        _ => {
            let plan = crate::features::learning::outline_references::plan(
                llm,
                &draft.request,
                sources,
                progress,
            )
            .await?;
            draft.reference_plan = Some(plan.clone());
            plan
        }
    };
    let found =
        crate::features::learning::outline_references::gather(llm, web, &plan, sources, progress)
            .await?;
    let saved = found.sources.len();
    if saved == 0 {
        draft.review.note = format!("Reference gathering ran {} searches but saved no usable pages ({} searches or pages were unavailable).", found.searches, found.unavailable);
        repo.checkpoint_outline(program, draft, &[], &[]).await?;
        return Ok(());
    }
    for (source, reference) in found.sources.iter().zip(&found.references) {
        draft.source_ids.push(source.id.clone());
        sources
            .push(crate::features::learning::outline_references::authoring_copy(source, reference));
    }
    draft.review.references_gathered += saved as u32;
    draft.review.status = LearningOutlineReviewStatus::Unchecked;
    draft.review.note = format!(
        "{saved} reference pages saved from {} searches. Citations and the plan still need review.",
        found.searches
    );
    repo.checkpoint_outline(program, draft, &found.sources, &found.references)
        .await?;
    Ok(())
}

/// Where a finding sits, in learner-facing numbering.
pub(in crate::features::learning) fn location(path: &str) -> String {
    let Some(module) = module_index(path) else {
        return "Course".into();
    };
    let mut parts = path.trim_start_matches("/modules/").split('/').skip(1);
    let item = match (
        parts.next(),
        parts.next().and_then(|i| i.parse::<usize>().ok()),
    ) {
        (Some("summary"), _) => " · Summary".to_owned(),
        (Some("outcomes"), Some(index)) => format!(" · Outcome {}", index + 1),
        (Some("lessons"), Some(index)) => format!(" · Lesson {}", index + 1),
        _ => String::new(),
    };
    format!("Module {}{item}", module + 1)
}

fn short(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= 160 {
        return text.into();
    }
    format!("{}…", text.chars().take(159).collect::<String>().trim_end())
}

/// The note shown when findings remain after the allowed attempts: each one
/// in plain language, with what was searched for a factual finding.
fn remaining_note(draft: &OutlineDraft) -> String {
    let remaining: Vec<_> = draft.open_findings().collect();
    let details = remaining
        .iter()
        .map(|issue| {
            let location = location(&issue.path);
            match issue.kind {
                LearningOutlineIssueKind::Factual if issue.source_id.is_some() => format!(
                    "{location}: the claim \"{}\" is contradicted by a saved reference. {}",
                    short(&issue.claim),
                    issue.message
                ),
                LearningOutlineIssueKind::Factual if !issue.searched.is_empty() => format!(
                    "{location}: the claim \"{}\" was not supported by any saved reference after {} {} (queries: {}).",
                    short(&issue.claim),
                    issue.searched.len(),
                    if issue.searched.len() == 1 { "search" } else { "searches" },
                    issue.searched.join("; ")
                ),
                LearningOutlineIssueKind::Factual => format!(
                    "{location}: the claim \"{}\" was not supported by the saved references.",
                    short(&issue.claim)
                ),
                _ => format!("{location}: {}", issue.message),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{} {} remain after the allowed repair attempts. {details} Use Repair to try again with fresh attempts and new searches, or regenerate the outline.",
        remaining.len(),
        if remaining.len() == 1 { "finding" } else { "findings" }
    )
}

/// The note shown when only advisory plan suggestions remain.
fn advisory_note(draft: &OutlineDraft, sources: &[LearningSourceDto]) -> String {
    let suggestions: Vec<_> = draft
        .open_findings()
        .map(|issue| format!("{}: {}", location(&issue.path), issue.message))
        .collect();
    format!(
        "Outline checks passed. {} references saved; {} of {} outline items linked to references. {} plan {} could not be settled automatically and {} listed below; {} not block the course. Accept it, or use Repair to try again. Lesson content is checked separately when prepared. {}",
        sources.len(),
        draft.review.citations_verified,
        crate::features::learning::outline_linking::item_count(&draft.candidate),
        suggestions.len(),
        if suggestions.len() == 1 { "suggestion" } else { "suggestions" },
        if suggestions.len() == 1 { "is" } else { "are" },
        if suggestions.len() == 1 { "it does" } else { "they do" },
        suggestions.join(" ")
    )
}

/// Write the status and note that follow a completed review pass.
fn settle(draft: &mut OutlineDraft, sources: &[LearningSourceDto], references_missing: bool) {
    let open = draft.open_findings().count();
    if references_missing {
        draft.review.issues.push(LearningOutlineIssueDto {
            id: "references".into(),
            path: String::new(),
            kind: LearningOutlineIssueKind::References,
            claim: String::new(),
            message: NO_REFERENCES.into(),
            attempts: 0,
            exhausted: true,
            quote: String::new(),
            source_id: None,
            searched: vec![],
        });
    }
    draft.review.status = if !draft
        .review
        .issues
        .iter()
        .any(LearningOutlineIssueDto::blocks_acceptance)
    {
        LearningOutlineReviewStatus::Passed
    } else {
        LearningOutlineReviewStatus::NeedsRepair
    };
    let linked = format!(
        "{} of {} outline items linked to references",
        draft.review.citations_verified,
        crate::features::learning::outline_linking::item_count(&draft.candidate)
    );
    draft.review.note = if open == 0 && references_missing {
        "The plan passed review, but no references could be gathered. Lessons need saved references; use Repair to retry gathering when the web is available.".into()
    } else if open == 0 {
        format!(
            "Outline checks passed. {} references saved; {linked}. Lesson content is checked separately when prepared.",
            sources.len()
        )
    } else {
        format!(
            "Findings saved; {linked}. Repair will use the remaining issues and source passages."
        )
    };
}

/// Review and repair a saved draft. `gather_if_empty` is set by Repair: a
/// draft that still has no references is gathered for first. Generation has
/// already gathered once and passes false, so a failed gathering is reported
/// as the References finding rather than retried in the same operation.
///
/// The loop terminates by construction: each pass either resolves findings or
/// spends attempts (at most `ATTEMPT_LIMIT` per finding) and module rewrites
/// (at most `MODULE_REWRITE_BUDGET` per module), and new findings can only
/// arise at paths the last pass changed.
pub(in crate::features::learning) async fn finish(
    repo: &LearningRepository,
    llm: &dyn LLMPort,
    mut program: LearningProgramDto,
    mut draft: OutlineDraft,
    mut sources: Vec<LearningSourceDto>,
    progress: &OutlineProgress,
    web: Option<&dyn WebServiceTrait>,
    gather_if_empty: bool,
) -> Result<LearningProgramDto> {
    use crate::features::learning::outline_progress::OutlineStage;
    use crate::features::learning::outline_review_scope::{ReviewReceipt, ReviewScope};
    let work: Result<()> = async {
        // The course-wide References finding is re-derived each pass from the
        // actual source count, never carried over.
        draft.review.issues.retain(|issue| issue.kind != LearningOutlineIssueKind::References);
        if gather_if_empty && sources.is_empty() {
            if let Some(web) = web {
                gather_for_draft(repo, llm, web, &mut program, &mut draft, &mut sources, progress).await?;
            }
        }
        // Evidence linking: code retrieves candidate passages per item and the
        // model chooses one or none; the quote is always the saved page's own
        // text. Author citations that do not match or are not chosen are removed
        // here, so the reviewer never sees a doubtful citation.
        let log = crate::features::learning::outline_linking::link(llm, &mut draft, &sources, None, progress).await?;
        draft.record_citations(log);
        tracing::info!(program_id = %program.summary.id, linked = log.linked, items = log.items, removed_unmatched = log.removed_unmatched, removed_unchosen = log.removed_unchosen, "Linked outline items to references");
        draft.review.status = LearningOutlineReviewStatus::Unchecked;
        repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
        let references_missing = sources.is_empty() && web.is_some();
        let modules = draft.candidate.get("modules").and_then(Value::as_array).map_or(0, Vec::len);
        let mut budgets = ModuleBudgets::new(modules);
        loop {
            progress.check_cancelled()?;
            draft.review.issues.retain(|issue| issue.kind != LearningOutlineIssueKind::References);
            // Skip rule: a draft whose completed review still matches and has
            // no open findings is passed without a model call.
            if draft.open_findings().next().is_none()
                && draft.completed_review.as_ref() == Some(&ReviewReceipt::new(&draft, llm))
            {
                tracing::info!(program_id = %program.summary.id, "Outline review receipt matches; no review call needed");
                settle(&mut draft, &sources, references_missing);
                repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
                break;
            }
            progress.stage(if draft.review.repair_passes == 0 { OutlineStage::Reviewing } else { OutlineStage::CheckingRepair });
            let scope = ReviewScope::new(&draft, llm);
            draft.review.status = LearningOutlineReviewStatus::Unchecked;
            draft.review.note = if scope.incremental {
                format!("Rechecking {} corrected or unresolved {} and connections across the course. Completed checks for unchanged material are retained; open findings await this review's verdict.", scope.indices.len(), if scope.indices.len() == 1 { "module" } else { "modules" })
            } else {
                "Reviewing the full outline against the saved references. Open findings await this review's verdict.".into()
            };
            repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
            crate::features::learning::outline_review::review(llm, &mut draft, &sources, progress, scope).await?;
            for issue in &mut draft.review.issues {
                issue.exhausted = !issue.actionable(&budgets);
            }
            settle(&mut draft, &sources, references_missing);
            repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
            let open = draft.open_findings().count();
            if open == 0 {
                break;
            }
            if draft.open_findings().all(|issue| issue.exhausted) {
                draft.review.note = if draft.review.status == LearningOutlineReviewStatus::Passed {
                    advisory_note(&draft, &sources)
                } else {
                    remaining_note(&draft)
                };
                repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
                break;
            }
            progress.stage(OutlineStage::Repairing);
            let outcome = crate::features::learning::outline_repair::repair(repo, llm, &mut program, &mut draft, &mut sources, web, &mut budgets, progress).await?;
            if !outcome.changed && !outcome.evidence_added {
                // Plan findings an unchanged last attempt exhausted no longer
                // block; only a blocking finding left uncorrected stops here.
                if draft.review.issues.iter().any(LearningOutlineIssueDto::blocks_acceptance) {
                    draft.review.note = "The model returned no usable correction. Your draft, findings and saved references are kept; Repair can retry checking.".into();
                } else {
                    draft.review.status = LearningOutlineReviewStatus::Passed;
                    draft.review.note = advisory_note(&draft, &sources);
                }
                repo.checkpoint_outline(&mut program, &mut draft, &[], &[]).await?;
                break;
            }
        }
        Ok(())
    }.await;
    if let Err(error) = work {
        tracing::warn!(program_id = %program.summary.id, %error, "Outline retained for repair");
        // Only decorate the committed state; a failed transaction may have
        // left tentative edits or new source IDs in the local working copy.
        program = repo.get(&program.summary.id).await?;
        draft = repo
            .outline_draft(&program.summary.id)
            .await?
            .ok_or_else(|| AppError::InvalidInput("The saved draft is unavailable.".into()))?;
        // A failed or interrupted check can never inherit a previous approval.
        draft.review.status = LearningOutlineReviewStatus::Unchecked;
        let safe = lattice_core::shared::ipc::ApiError::from(error).message;
        draft.review.note = format!(
            "Review or repair stopped: {safe} Completed work is saved. Use Repair to continue."
        );
        repo.checkpoint_outline(&mut program, &mut draft, &[], &[])
            .await?;
    }
    Ok(program)
}

pub(in crate::features::learning) async fn repair_saved(
    repo: &LearningRepository,
    drafts: &DraftRunRegistry,
    llm: &dyn LLMPort,
    request: &RepairLearningOutlineRequestDto,
    progress: &OutlineProgress,
    web: Option<&dyn WebServiceTrait>,
) -> Result<LearningProgramDto> {
    let _run = drafts.acquire(&request.program_id)?;
    let program = repo.get(&request.program_id).await?;
    if program.summary.status != LearningProgramStatus::Draft
        || program.summary.revision != request.expected_revision
    {
        return Err(AppError::InvalidInput(
            "The draft changed. Reload it before repairing.".into(),
        ));
    }
    let mut draft = repo
        .outline_draft(&request.program_id)
        .await?
        .ok_or_else(|| {
            AppError::InvalidInput("This course has no resumable outline draft.".into())
        })?;
    // Removal counts describe one operation; a repair starts a new one, and
    // the user asked for another try: every open finding gets fresh attempts.
    // The searches already run stay in the research log, so the planner is
    // told what was tried and explores new queries.
    draft.review.citations_removed = 0;
    for issue in &mut draft.review.issues {
        issue.attempts = 0;
        issue.exhausted = false;
    }
    let saved = repo.verification_sources(&request.program_id).await?;
    let sources = draft
        .source_ids
        .iter()
        .map(|id| {
            saved.iter().find(|s| &s.id == id).cloned().ok_or_else(|| {
                AppError::InvalidInput(
                    "A saved reference is unavailable. Restore it before repairing.".into(),
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    progress.saved(&program.summary.id);
    finish(repo, llm, program, draft, sources, progress, web, true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_draft_rejects_a_second_run_until_released() {
        let drafts = DraftRunRegistry::new();
        let id = uuid::Uuid::new_v4().to_string();
        let held = drafts.acquire(&id).unwrap();
        assert!(drafts.acquire(&id).is_err());
        // A separate registry is a separate process-worth of state.
        assert!(DraftRunRegistry::new().acquire(&id).is_ok());
        drop(held);
        assert!(drafts.acquire(&id).is_ok());
    }
}
